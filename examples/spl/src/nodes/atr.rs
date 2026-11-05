use trading_data::{Bar, Bars, Buffering, Cell, DepReads, Elems, Env, Length, Reading, Scans, Slots, Stamped, Tag, Timeframe, Vars, Witness, abs, max, node, slice_nudge};

use crate::config::strategy;

/// The bar's range, widened to the previous close where the price gapped past it.
#[derive(Clone, Default)]
pub struct TrueRange<const TF: Timeframe>;
impl<const TF: Timeframe> TrueRange<TF> {
	const TAG: Tag = Tag::new("TrueRange:", TF);
}
impl<const TF: Timeframe> Cell for TrueRange<TF> {
	type Out<'t> = &'t [Reading];

	const NAME: &'static str = Self::TAG.as_str();
}
#[node]
impl<const TF: Timeframe> Scans for TrueRange<TF> {
	type Deps = (Buffering<Bars<TF>, Elems<2>>,);

	fn read<W: Witness>((bars,): DepReads<'_, Self>, i: usize, env: &mut Env<'_, W>) -> Option<i64> {
		let b = bars.at(i)?;
		env.put(b);
		// the first bar has no previous close; its own close lies inside its range, so standing in for
		// one leaves the true range the range.
		env.put(bars.lagged_at(i, 1).unwrap_or(b).slot(Bar::CLOSE));
		Some(b.ts_ns())
	}

	fn body(&self, v: Vars) -> impl Slots {
		let (high, low, prev) = (v.get::<1>(), v.get::<2>(), v.get::<5>());
		max(max(high - low, abs(high - prev)), abs(low - prev))
	}
}
slice_nudge!([const TF: Timeframe] TrueRange<TF>, Reading);

/// `indies.atr.period`: the length of the Wilder average over [`TrueRange`] that is ATR.
pub struct AtrPeriod;
impl Length for AtrPeriod {
	const NAME: &'static str = "AtrPeriod";

	fn len() -> usize {
		strategy().indies.atr.period
	}
}
