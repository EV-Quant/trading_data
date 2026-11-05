//! Where an RSI chain starts having a number, read off the nodes.
//!
//! That boundary is the thing a change of absence representation could move without any golden
//! noticing, so it is pinned here: absent for exactly the cold elements, a number from the first warm
//! one, and the stages compose — an EMA over the RSI starts counting where the RSI warms.

use trading_data_core::Ts;
use trading_data_dag::{Cell, Elems, Present, graph, slice_nudge};
#[allow(
	unused_imports,
	reason = "rust#52234: `graph!` reaches a node's shim textually, so the import is the only thing that puts it in scope"
)]
use trading_data_derivatives::{__td_node_Ema, __td_node_Gains, __td_node_Losses, __td_node_Rsi, __td_node_RsiDelta, __td_node_Wilder};
use trading_data_derivatives::{Bar, Ema, Gains, Length, Losses, Rsi, RsiDelta, Wilder};

/// The bars arrive from the test rather than from a `Closes` over trades: what is under test is the
/// chain above them, and a root series is the shortest thing that feeds it.
struct Closes;
impl Cell for Closes {
	type Out<'t> = &'t [Bar];
}
slice_nudge!(Closes, Bar);

/// Short on purpose: the boundary is `len` deltas, and 14 would need more bars to say the same thing.
struct Three;
impl Length for Three {
	const NAME: &'static str = "3";

	fn len() -> usize {
		3
	}
}
struct Two;
impl Length for Two {
	const NAME: &'static str = "2";

	fn len() -> usize {
		2
	}
}
struct Fourteen;
impl Length for Fourteen {
	const NAME: &'static str = "14";

	fn len() -> usize {
		14
	}
}

graph! {
	struct G;
	batches Batches;
	roots { bars: Closes[Bar] };
	out GOut;
	outputs {
		delta: RsiDelta<Closes>,
		gain: Wilder<Gains<RsiDelta<Closes>>, Three>,
		loss: Wilder<Losses<RsiDelta<Closes>>, Three>,
		rsi: Rsi<Wilder<Gains<RsiDelta<Closes>>, Three>, Wilder<Losses<RsiDelta<Closes>>, Three>>,
		smooth: Ema<Rsi<Wilder<Gains<RsiDelta<Closes>>, Three>, Wilder<Losses<RsiDelta<Closes>>, Three>>, Two>,
		rsi14: Rsi<Wilder<Gains<RsiDelta<Closes>>, Fourteen>, Wilder<Losses<RsiDelta<Closes>>, Fourteen>>
	}
}

fn bar(i: i64, close: f64) -> Bar {
	Bar {
		ts_close: Ts::from_nanos((i + 1) * 60_000_000_000),
		open: close,
		high: close,
		low: close,
		close,
		vol_base: 1.0,
	}
}

/// A rising-then-falling walk, so both averages see samples and neither ratio is degenerate — the
/// numbers are not what is asserted, but a leg that only ever saw zeros would warm on values that
/// could not tell a bug from a boundary.
fn walk(n: usize) -> Vec<Bar> {
	(0..n).map(|i| bar(i as i64, 100.0 + (i as f64 * 0.7).sin() * 5.0)).collect()
}

/// One bar per tick, and every out concatenated back into the run the chain published.
#[derive(Default)]
struct Runs {
	delta: Vec<Option<f64>>,
	gain: Vec<Option<f64>>,
	loss: Vec<Option<f64>>,
	rsi: Vec<Option<f64>>,
	smooth: Vec<Option<f64>>,
	rsi14: Vec<Option<f64>>,
}

fn drive(closes: &[f64]) -> Runs {
	let mut g = G::default();
	let mut r = Runs::default();
	for (i, c) in closes.iter().enumerate() {
		let b = bar(i as i64, *c);
		let o = g.tick(0, Batches { bars: core::slice::from_ref(&b) });
		r.delta.extend(o.delta.iter().map(|v| v.present()));
		r.gain.extend(o.gain.iter().map(|v| v.present()));
		r.loss.extend(o.loss.iter().map(|v| v.present()));
		r.rsi.extend(o.rsi.iter().map(|v| v.present()));
		r.smooth.extend(o.smooth.iter().map(|v| v.present()));
		r.rsi14.extend(o.rsi14.iter().map(|v| v.present()));
	}
	r
}

fn drive_walk(n: usize) -> Runs {
	drive(&walk(n).iter().map(|b| b.close).collect::<Vec<_>>())
}

/// The index of the first element carrying a number, and the claim that everything before it carries
/// none — a single `position` would pass on a run that came back absent afterwards.
fn boundary(run: &[Option<f64>]) -> usize {
	let first = run.iter().position(Option::is_some).unwrap_or_else(|| panic!("never warmed over {} elements", run.len()));
	assert!(run[..first].iter().all(Option::is_none), "absence is a prefix, not a scatter: {run:?}");
	assert!(run[first..].iter().all(Option::is_some), "warm is forever: {run:?}");
	assert!(run[first..].iter().flatten().all(|v| v.is_finite()), "a warm element is a number: {run:?}");
	first
}

/// Rate-preserving throughout: every stage publishes one element per bar, cold ones included. That is
/// what makes "absent for exactly the cold elements" a statement about *which* elements rather than
/// about how many there are.
#[test]
fn every_stage_publishes_one_element_per_bar() {
	let r = drive_walk(12);
	assert_eq!((r.delta.len(), r.gain.len(), r.loss.len(), r.rsi.len(), r.smooth.len()), (12, 12, 12, 12, 12));
}

/// A change needs two closes, so the first bar of the run carries nothing and every bar after it
/// carries a number.
#[test]
fn a_delta_is_absent_only_for_the_bar_that_has_nothing_behind_it() {
	assert_eq!(boundary(&drive_walk(12).delta), 1);
}

/// `len` deltas warm a Wilder average, and the first bar contributed none — so the first three
/// deltas land on bars 1..=3 and bar 3 is the one that has a value.
#[test]
fn a_wilder_average_is_absent_for_exactly_its_cold_bars() {
	let r = drive_walk(12);
	assert_eq!(boundary(&r.gain), Three::len());
	assert_eq!(boundary(&r.loss), Three::len());
	assert_eq!(boundary(&r.rsi), Three::len());
}

/// The EMA sees its first value where the RSI warms, and needs `len` of them.
#[test]
fn an_ema_over_the_rsi_counts_from_where_the_rsi_warms() {
	assert_eq!(boundary(&drive_walk(12).smooth), Three::len() + Two::len() - 1);
}

// StockCharts' classic Wilder RSI-14 worked example; goldens recomputed independently and they match
// the published 70.53 / 66.32 / … sequence.
#[test]
fn rsi14_golden_sequence() {
	let closes = [
		44.3389, 44.0902, 44.1497, 43.6124, 44.3278, 44.8264, 45.0955, 45.4245, 45.8433, 46.0826, 45.8931, 46.0328, 45.6140, 46.2820, 46.2820, 46.0028, 46.0328, 46.4116, 46.2222, 45.6439,
		46.2122, 46.2521, 45.7137, 46.4515, 45.7835, 45.3548, 44.0288, 44.1783, 44.2181, 44.5672, 43.4205, 42.6628, 43.1314,
	];
	let golden = [
		70.5328, 66.3186, 66.5498, 69.4063, 66.3552, 57.9749, 62.9296, 63.2571, 56.0593, 62.3771, 54.7076, 50.4228, 39.9898, 41.4605, 41.8689, 45.4632, 37.3040, 33.0795, 37.7730,
	];
	let rsi = drive(&closes).rsi14;
	assert_eq!(boundary(&rsi), 14, "warm on the 14th delta, which lands on bar 14");
	for (got, want) in rsi[14..].iter().zip(golden) {
		let got = got.expect("warm from the 14th delta on");
		assert!((got - want).abs() < 5e-4, "{got} vs {want}");
	}
}
