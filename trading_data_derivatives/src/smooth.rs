//! Exponential smoothers over a [`Reading`] series.

use core::marker::PhantomData;

use trading_data_dag::{
	Buffering, Carried, Cell, DepReads, Elems, Env, Folding, Folds, Horizon, Nudge, Present, Reach, Reading, Rows, Series, Slots, Tag, Unbounded, Vars, Witness, absent, constant, lt, min,
	node, select, slice_nudge,
};

/// A smoothing length — a number an app reading it from a config file has no const to give.
pub trait Length: 'static {
	/// How a smoother spells this length in its own [`Cell::NAME`].
	const NAME: &'static str;

	fn len() -> usize;
}

macro_rules! held {
	($ty:ident [$($gen:tt)*] [$($arg:tt)*]) => {
		impl<$($gen)*> Clone for $ty<$($arg)*> {
			fn clone(&self) -> Self {
				Self(self.0, PhantomData)
			}
		}
		impl<$($gen)*> Default for $ty<$($arg)*> {
			fn default() -> Self {
				Self(Carried::default(), PhantomData)
			}
		}
	};
}

/// Wilder's running mean: an SMA over the first `len` samples, then `(prev·(len-1) + x)/len`.
/// Absent until warm.
pub struct Wilder<X, L>(Carried, PhantomData<(X, L)>);
held!(Wilder [X, L] [X, L]);
impl<X: Series<Item = Reading> + Nudge, L: Length> Wilder<X, L> {
	const TAG: Tag = Tag::of("Wilder", &[X::NAME, L::NAME]);
}
impl<X: Series<Item = Reading> + Nudge, L: Length> Cell for Wilder<X, L> {
	type Out<'t> = &'t [Reading];

	const NAME: &'static str = Self::TAG.as_str();
}
#[node(clocked)]
impl<X: Series<Item = Reading> + Nudge, L: Length> Folds for Wilder<X, L> {
	type Deps = (Folding<X, Unbounded>,);

	/// The running mean, and how many samples have gone into it.
	const STATE: usize = 2;

	/// An average is not advanced by an absence, which is exactly what declining leaves the state doing.
	fn read<W: Witness>((xs,): DepReads<'_, Self>, i: usize, env: &mut Env<'_, W>) -> Option<i64> {
		let x = xs.at(i)?;
		x.present()?;
		env.put(x);
		Some(0)
	}

	/// `warmed` is carried for the SMA warmup, and `warmed < len` is equally how a reader of the state
	/// knows it is still cold.
	fn step(&self, v: Vars) -> impl Slots {
		let (x, value, warmed) = (v.get::<0>(), v.get::<1>(), v.get::<2>());
		let n = L::len() as f64;
		let cold = value + x;
		(
			select(
				lt(warmed, constant(n)),
				select(lt(warmed, constant(n - 1.0)), cold, cold / constant(n)),
				(value * constant(n - 1.0) + x) / constant(n),
			),
			min(warmed + constant(1.0), constant(n)),
		)
	}

	fn value(&self, v: Vars) -> impl Slots {
		select(lt(v.get::<2>(), constant(L::len() as f64)), absent(), v.get::<1>())
	}

	fn carried(&self) -> &Carried {
		&self.0
	}

	fn carried_mut(&mut self) -> &mut Carried {
		&mut self.0
	}
}
slice_nudge!([X: Series<Item = Reading> + Nudge, L: Length] Wilder<X, L>, Reading);

/// Nautilus's `ExponentialMovingAverage`: `α = 2/(len+1)`, seeded on the first sample, absent until
/// `len` samples have reached it.
pub struct Ema<X, L>(Carried, PhantomData<(X, L)>);
held!(Ema [X, L] [X, L]);
impl<X: Series<Item = Reading> + Nudge, L: Length> Ema<X, L> {
	const TAG: Tag = Tag::of("Ema", &[X::NAME, L::NAME]);
}
impl<X: Series<Item = Reading> + Nudge, L: Length> Cell for Ema<X, L> {
	type Out<'t> = &'t [Reading];

	const NAME: &'static str = Self::TAG.as_str();
}
#[node(clocked)]
impl<X: Series<Item = Reading> + Nudge, L: Length> Folds for Ema<X, L> {
	type Deps = (Folding<X, Unbounded>,);

	/// The running value, and how many samples have reached it.
	const STATE: usize = 2;

	fn read<W: Witness>((xs,): DepReads<'_, Self>, i: usize, env: &mut Env<'_, W>) -> Option<i64> {
		let x = xs.at(i)?;
		x.present()?;
		env.put(x);
		Some(0)
	}

	fn step(&self, v: Vars) -> impl Slots {
		let (x, ema, seen) = (v.get::<0>(), v.get::<1>(), v.get::<2>());
		let alpha = 2.0 / (L::len() as f64 + 1.0);
		(
			select(lt(seen, constant(1.0)), x, constant(alpha) * x + constant(1.0 - alpha) * ema),
			min(seen + constant(1.0), constant(L::len() as f64)),
		)
	}

	fn value(&self, v: Vars) -> impl Slots {
		select(lt(v.get::<2>(), constant(L::len() as f64)), absent(), v.get::<1>())
	}

	fn carried(&self) -> &Carried {
		&self.0
	}

	fn carried_mut(&mut self) -> &mut Carried {
		&mut self.0
	}
}
slice_nudge!([X: Series<Item = Reading> + Nudge, L: Length] Ema<X, L>, Reading);

/// The element a `K`-long window drops as the next one enters: `K` back from the newest, so `K + 1`
/// retained — a sum a braced const argument may not spell over its impl's own generics.
#[doc(hidden)]
pub struct Leaving<const K: usize>;
impl<const K: usize> Reach for Leaving<K> {
	const HORIZON: Horizon = Horizon::Elems(K + 1);
}

/// An exponential smoother over the last `K` elements only, normalised by the weight present in
/// them: `Σ α(1-α)^m x_{j-m} / Σ α(1-α)^m p_{j-m}` over `m < K`, `p` an element's presence. A pure
/// function of its last `K` elements, so two runs agree once both have seen `K` of them — to
/// rounding, not bit for bit: each step adds the entering term and subtracts the leaving one, and
/// the subtraction carries residue.
///
/// Absent until `K` elements have passed since a reset, and wherever the window holds no sample.
macro_rules! windowed {
	($ty:ident, $name:literal, $alpha:expr, $doc:literal) => {
		#[doc = $doc]
		pub struct $ty<X, L, const K: usize>(Carried, PhantomData<(X, L)>);
		held!($ty [X, L, const K: usize] [X, L, K]);
		impl<X: Series<Item = Reading> + Nudge, L: Length, const K: usize> $ty<X, L, K> {
			const TAG: Tag = Tag::of($name, &[X::NAME, L::NAME]).count(K);
		}
		impl<X: Series<Item = Reading> + Nudge, L: Length, const K: usize> Cell for $ty<X, L, K> {
			type Out<'t> = &'t [Reading];

			const NAME: &'static str = Self::TAG.as_str();
		}
		#[node(clocked)]
		/// `Rows` because the leaving element is read off the retention's own history.
		impl<X: Series<Item = Reading, Batch = Rows<Reading>> + Nudge, L: Length, const K: usize> Folds for $ty<X, L, K> {
			/// The fold states the window the value is a function of; the retention hands back the
			/// element leaving it.
			type Deps = (Folding<X, Elems<K>>, Buffering<X, crate::Leaving<K>>);

			/// Elements folded since a reset (saturating at `K`), samples present in the window, the
			/// weighted sum, and the weight.
			const STATE: usize = 4;

			/// Never declines: an absent element still moves the window, and must push its leaver out.
			/// Slots past the state: the entering presence, then the leaving element and its presence.
			fn read<W: Witness>((xs, held): DepReads<'_, Self>, i: usize, env: &mut Env<'_, W>) -> Option<i64> {
				let x = xs.at(i).expect("element i of this tick's own run");
				let present = x.present().is_some();
				match present {
					true => {
						env.put(x);
						// the retention's own copy of the same element, so its column stands for what it
						// says; the body reads the lead's.
						env.put(held.at(i).expect("the retention's fresh run is the lead's"));
					}
					false => {
						env.attr(0.0);
						env.attr(0.0);
					}
				}
				env.attr(f64::from(u8::from(present)));
				// read only once `K` elements were folded, and by then the retention — which steps on every
				// tick the fold does — holds the one `K` back.
				match held.lagged_at(i, K).filter(|p| p.present().is_some()) {
					Some(out) => {
						env.put(out);
						env.attr(1.0);
					}
					None => {
						env.attr(0.0);
						env.attr(0.0);
					}
				}
				Some(0)
			}

			fn step(&self, v: Vars) -> impl Slots {
				let (x, folded, count, sum, weight) = (v.get::<0>(), v.get::<2>(), v.get::<3>(), v.get::<4>(), v.get::<5>());
				let (p, x_out, p_out) = (v.get::<6>(), v.get::<7>(), v.get::<8>());
				let (k, alpha) = (K as f64, $alpha);
				let decay = (1.0 - alpha).powi(K as i32);
				// before `K` were folded the window has dropped nothing it ever added — which is also what
				// keeps a reset from subtracting the elements before it.
				let full = select(lt(folded, constant(k)), constant(0.0), constant(1.0));
				(
					min(folded + constant(1.0), constant(k)),
					count + p - full * p_out,
					constant(1.0 - alpha) * sum + constant(alpha) * x - constant(alpha * decay) * full * x_out,
					constant(1.0 - alpha) * weight + constant(alpha) * p - constant(alpha * decay) * full * p_out,
				)
			}

			fn value(&self, v: Vars) -> impl Slots {
				let (folded, count, sum, weight) = (v.get::<2>(), v.get::<3>(), v.get::<4>(), v.get::<5>());
				select(
					lt(folded, constant(K as f64)),
					absent(),
					select(lt(count, constant(0.5)), absent(), sum / weight),
				)
			}

			fn carried(&self) -> &Carried {
				&self.0
			}

			fn carried_mut(&mut self) -> &mut Carried {
				&mut self.0
			}
		}
		slice_nudge!([X: Series<Item = Reading> + Nudge, L: Length, const K: usize] $ty<X, L, K>, Reading);
	};
}
windowed!(
	WilderWindow,
	"WilderWindow",
	1.0 / L::len() as f64,
	"[`Wilder`]'s weights, `α = 1/len`, over the last `K` elements — see the windowed smoothers above."
);
windowed!(
	EmaWindow,
	"EmaWindow",
	2.0 / (L::len() as f64 + 1.0),
	"[`Ema`]'s weights, `α = 2/(len+1)`, over the last `K` elements — see the windowed smoothers above."
);
