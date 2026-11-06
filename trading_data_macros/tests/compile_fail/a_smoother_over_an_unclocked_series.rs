//! A count-weighted body over a series with no clock: its weights say how the feed arrived rather
//! than anything about time, so `#[node(clocked)]` refuses the wiring.
use trading_data_dag::{Cell, DepReads, Env, Scans, Slots, Vars, Witness, slice_nudge};
use trading_data_macros::{graph, node};

struct Src;
impl Cell for Src {
	type Out<'t> = &'t [f64];
}
slice_nudge!(Src, f64);

#[derive(Clone, Default)]
struct Smooth;
impl Cell for Smooth {
	type Out<'t> = &'t [f64];
}
#[node(clocked)]
impl Scans for Smooth {
	type Deps = (Src,);

	fn read<W: Witness>((xs,): DepReads<'_, Self>, i: usize, env: &mut Env<'_, W>) -> Option<i64> {
		env.put(xs.at(i)?);
		Some(0)
	}

	fn body(&self, v: Vars) -> impl Slots {
		v.get::<0>()
	}
}
slice_nudge!(Smooth, f64);

graph! { //~ ERROR: is `#[node(clocked)]` and its leading dep has no clock
	struct G;
	batches Batches;
	roots { src: Src[f64] };
	out GOut;
	outputs { smooth: Smooth }
}

fn main() {}
