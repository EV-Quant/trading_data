//@error-in-other-file: an `Over` reach over a series whose items carry no time
//@error-in-other-file: an `Over` reach over a series whose items carry no time
//! A `Reading` is an element of a series and carries no time, so it is retained by count. A span
//! over one has nothing to be located by — the reach to spell is `Elems<N>`.
use trading_data_dag::{Buffering, Cell, DepOuts, Over, Reading, Runs, slice_nudge};
use trading_data_macros::{graph, node};
use v_utils::Timeframe;

struct Src;
impl Cell for Src {
	type Out<'t> = &'t [Reading];
}
slice_nudge!(Src, Reading);

#[derive(Clone, Default)]
struct Last;
impl Cell for Last {
	type Out<'t> = &'t [Reading];
}
#[node]
impl Runs for Last {
	type Deps = (Buffering<Src, Over<{ Timeframe(60_000) }>>,);

	const WHY: &'static str = "a retention fixture";

	fn emit(&mut self, (xs,): DepOuts<'_, Self>, out: &mut Vec<Reading>) {
		out.extend(xs.past().last());
	}
}
slice_nudge!(Last, Reading);

graph! {
	struct G;
	batches Batches;
	roots { src: Src[Reading] };
	out GOut;
	outputs { last: Last }
}

fn main() {
	G::default().tick(0, Batches { src: &[] });
}
