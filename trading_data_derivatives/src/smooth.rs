//! Exponential smoothers over a [`Reading`] series.

use core::marker::PhantomData;

use trading_data_dag::{
	Carried, Cell, DepReads, Env, Folding, Folds, Nudge, Present, Reading, Series, Slots, Tag, Unbounded, Vars, Witness, absent, constant, lt, min, node, select, slice_nudge,
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
#[node]
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
#[node]
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
