use core::marker::PhantomData;

use trading_data_dag::{
	Buffering, Cell, DepReads, Elems, Env, Ex, Expr, Nudge, Plot, Present, Reading, Rows, Scans, Series, Slots, Stamped, Tag, Vars, Witness, constant, gt, max, node, select, slice_nudge,
};

use crate::bar::Bar;

/// Close-to-close change on `B` — the series RSI's up and down moves are taken of.
///
/// Rate-*preserving*, and the first bar declines: a change needs two closes, and the bar that has
/// only one is an element that carried nothing rather than an element that is not there. The lag is
/// a reading off the retention (`Buffering<B, Elems<2>>`) rather than a close the node remembers,
/// which is what leaves nothing for a state to be carried in.
pub struct RsiDelta<B>(PhantomData<B>);
impl<B> Clone for RsiDelta<B> {
	fn clone(&self) -> Self {
		Self(PhantomData)
	}
}
impl<B> Default for RsiDelta<B> {
	fn default() -> Self {
		Self(PhantomData)
	}
}
impl<B: Series<Item = Bar>> RsiDelta<B> {
	const TAG: Tag = Tag::of("RsiDelta", &[B::NAME]);
}
impl<B: Series<Item = Bar>> Cell for RsiDelta<B> {
	type Out<'t> = &'t [Reading];

	const NAME: &'static str = Self::TAG.as_str();
}
#[node]
/// `Rows` because the lag is read off the retention's own history, which is what a row-keeping
/// batch is; a bar series that folded its rows would have none to look one back through.
impl<B: Series<Item = Bar, Batch = Rows<Bar>>> Scans for RsiDelta<B> {
	/// Two elements, because the lag reaches one behind the element being computed — and it is the
	/// engine's retention rather than the node's, so nothing here survives a tick.
	type Deps = (Buffering<B, Elems<2>>,);

	fn read<W: Witness>((bars,): DepReads<'_, Self>, i: usize, env: &mut Env<'_, W>) -> Option<i64> {
		let b = bars.at(i).expect("element i of this tick's own fresh run");
		// the first bar of a run has nothing behind it, and an absence is declined rather than put:
		// a NaN in the env is an operand, where a decline is an out.
		let p = bars.lagged_at(i, 1)?;
		env.put(b);
		// the one field this reading takes, so the partial lands in that field's own column of the
		// lagged element rather than in a column of its own.
		env.put(p.slot(Bar::CLOSE));
		Some(b.ts_ns())
	}

	fn body(&self, v: Vars) -> impl Slots {
		v.get::<3>() - v.get::<5>()
	}
}
slice_nudge!([B: Series<Item = Bar>] RsiDelta<B>, Reading);

macro_rules! half {
	($ty:ident, $sign:literal, $doc:literal) => {
		#[doc = $doc]
		pub struct $ty<X>(PhantomData<X>);
		impl<X> Clone for $ty<X> {
			fn clone(&self) -> Self {
				Self(PhantomData)
			}
		}
		impl<X> Default for $ty<X> {
			fn default() -> Self {
				Self(PhantomData)
			}
		}
		impl<X: Series<Item = Reading> + Nudge> $ty<X> {
			const TAG: Tag = Tag::of(stringify!($ty), &[X::NAME]);
		}
		impl<X: Series<Item = Reading> + Nudge> Cell for $ty<X> {
			type Out<'t> = &'t [Reading];

			const NAME: &'static str = Self::TAG.as_str();
		}
		#[node]
		impl<X: Series<Item = Reading> + Nudge> Scans for $ty<X> {
			type Deps = (X,);

			fn read<W: Witness>((xs,): DepReads<'_, Self>, i: usize, env: &mut Env<'_, W>) -> Option<i64> {
				let x = xs.at(i)?;
				x.present()?;
				env.put(x);
				Some(0)
			}

			fn body(&self, v: Vars) -> impl Slots {
				max(constant($sign) * v.get::<0>(), constant(0.0))
			}
		}
		slice_nudge!([X: Series<Item = Reading> + Nudge] $ty<X>, Reading);
	};
}
half!(Gains, 1.0, "The up moves of `X`, and zero where it fell.");
half!(Losses, -1.0, "The down moves of `X` as a positive magnitude, and zero where it rose.");

/// `100 - 100/(1 + U/D)` over an average of a series' [`Gains`] and one of its [`Losses`]. Which
/// average is the wiring's to say: Wilder's is the classic RSI.
pub struct Rsi<U, D>(PhantomData<(U, D)>);
impl<U, D> Clone for Rsi<U, D> {
	fn clone(&self) -> Self {
		Self(PhantomData)
	}
}
impl<U, D> Default for Rsi<U, D> {
	fn default() -> Self {
		Self(PhantomData)
	}
}
impl<U: Series<Item = Reading> + Nudge, D: Series<Item = Reading> + Nudge> Rsi<U, D> {
	const TAG: Tag = Tag::of("Rsi", &[U::NAME, D::NAME]);
}
impl<U: Series<Item = Reading> + Nudge, D: Series<Item = Reading> + Nudge> Cell for Rsi<U, D> {
	type Out<'t> = &'t [Reading];

	const NAME: &'static str = Self::TAG.as_str();
}
#[node]
impl<U: Series<Item = Reading> + Nudge, D: Series<Item = Reading> + Nudge> Scans for Rsi<U, D> {
	type Deps = (U, D);

	// No threshold guide: whoever wires this owns the trigger and `Plot` is a const, so drawing one
	// here would pin a number the wiring is free to move.
	const PLOTS: &'static [Plot] = &[Plot {
		range: Some((0.0, 100.0)),
		..Plot::DEFAULT
	}];

	fn read<W: Witness>((up, down): DepReads<'_, Self>, i: usize, env: &mut Env<'_, W>) -> Option<i64> {
		assert_eq!(up.len(), down.len(), "up/down averages rate mismatch");
		let (u, d) = (up.at(i)?, down.at(i)?);
		u.present()?;
		d.present()?;
		env.put(u);
		env.put(d);
		Some(0)
	}

	fn body(&self, v: Vars) -> impl Slots {
		ratio(v)
	}
}
slice_nudge!([U: Series<Item = Reading> + Nudge, D: Series<Item = Reading> + Nudge] Rsi<U, D>, Reading);

/// A loss of zero is every gain and no loss, which is the top of the scale rather than a division.
fn ratio(v: Vars) -> Ex<impl Expr> {
	let (up, down) = (v.get::<0>(), v.get::<1>());
	select(gt(down, constant(0.0)), constant(100.0) - constant(100.0) / (constant(1.0) + up / down), constant(100.0))
}
