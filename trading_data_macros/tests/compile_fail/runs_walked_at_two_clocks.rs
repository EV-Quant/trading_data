//! A per-element kernel reads element `i` of each bare run as one instant, which two runs at
//! different clocks never share. The slower one is a cross-rate read, and says so through a wrapper.
use trading_data_dag::{Cell, DepReads, Env, Scans, Slots, Vars, Witness, slice_nudge};
use trading_data_macros::{graph, node};
use v_utils::Timeframe;

struct Fast;
impl Cell for Fast {
	type Out<'t> = &'t [f64];

	const CLOCK: Option<Timeframe> = Some(Timeframe(60_000));
}
slice_nudge!(Fast, f64);

struct Slow;
impl Cell for Slow {
	type Out<'t> = &'t [f64];

	const CLOCK: Option<Timeframe> = Some(Timeframe(300_000));
}
slice_nudge!(Slow, f64);

#[derive(Clone, Default)]
struct Spread;
impl Cell for Spread {
	type Out<'t> = &'t [f64];
}
#[node]
impl Scans for Spread {
	type Deps = (Fast, Slow);

	fn read<W: Witness>((fast, slow): DepReads<'_, Self>, i: usize, env: &mut Env<'_, W>) -> Option<i64> {
		env.put(fast.at(i)?);
		env.put(slow.at(i)?);
		Some(0)
	}

	fn body(&self, v: Vars) -> impl Slots {
		v.get::<0>() - v.get::<1>()
	}
}
slice_nudge!(Spread, f64);

graph! { //~ ERROR: walks bare runs at different clocks
	struct G;
	batches Batches;
	roots { fast: Fast[f64], slow: Slow[f64] };
	out GOut;
	outputs { spread: Spread }
}

fn main() {}
