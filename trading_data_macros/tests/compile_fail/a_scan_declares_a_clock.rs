//! A rate-preserving node has its leading dep's rate by construction (`rates.node.declared`), so a
//! `Cell::CLOCK` on one is a second statement of a fact the walk already fixes.
use trading_data_dag::{Cell, DepReads, Env, Scans, Slots, Vars, Witness, slice_nudge};
use trading_data_macros::{graph, node};
use v_utils::Timeframe;

struct Src;
impl Cell for Src {
	type Out<'t> = &'t [f64];

	const CLOCK: Option<Timeframe> = Some(Timeframe(60_000));
}
slice_nudge!(Src, f64);

#[derive(Clone, Default)]
struct Double;
impl Cell for Double {
	type Out<'t> = &'t [f64];

	const CLOCK: Option<Timeframe> = Some(Timeframe(60_000));
}
#[node]
impl Scans for Double {
	type Deps = (Src,);

	fn read<W: Witness>((xs,): DepReads<'_, Self>, i: usize, env: &mut Env<'_, W>) -> Option<i64> {
		env.put(xs.at(i)?);
		Some(0)
	}

	fn body(&self, v: Vars) -> impl Slots {
		v.get::<0>() * trading_data_dag::constant(2.0)
	}
}
slice_nudge!(Double, f64);

graph! { //~ ERROR: declares a `Cell::CLOCK` on a `Scans`/`Folds`/`Closes`
	struct G;
	batches Batches;
	roots { src: Src[f64] };
	out GOut;
	outputs { double: Double }
}

fn main() {}
