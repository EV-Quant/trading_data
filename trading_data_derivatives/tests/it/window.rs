//! What a windowed smoother is, read off its outs: the weighted mean of its last `K` elements and
//! nothing older — so where a run started stops mattering once `K` elements have passed.

use trading_data_dag::{Cell, Reading, graph, slice_nudge};
#[allow(
	unused_imports,
	reason = "rust#52234: `graph!` reaches a node's shim textually, so the import is the only thing that puts it in scope"
)]
use trading_data_derivatives::{__td_node_EmaWindow, __td_node_Wilder, __td_node_WilderWindow};
use trading_data_derivatives::{EmaWindow, Length, Wilder, WilderWindow};
use v_utils::Timeframe;

/// One sample per period, as a root of bars would publish.
struct Xs;
impl Cell for Xs {
	type Out<'t> = &'t [Reading];

	const CLOCK: Option<Timeframe> = Some(Timeframe(60_000));
}
slice_nudge!(Xs, Reading);

struct Three;
impl Length for Three {
	const NAME: &'static str = "3";

	fn len() -> usize {
		3
	}
}
struct Fourteen;
impl Length for Fourteen {
	const NAME: &'static str = "14";

	fn len() -> usize {
		14
	}
}

const K: usize = 5;

graph! {
	struct G;
	batches Batches;
	roots { xs: Xs[Reading] };
	out GOut;
	outputs {
		wilder: WilderWindow<Xs, Three, 5>,
		ema: EmaWindow<Xs, Three, 5>,
		long: WilderWindow<Xs, Fourteen, 400>,
		classic: Wilder<Xs, Fourteen>
	}
}

/// Every out over one series, fed in uneven batches so a window's edge falls inside a tick as often
/// as on one.
struct Outs {
	wilder: Vec<Option<f64>>,
	ema: Vec<Option<f64>>,
	long: Vec<Option<f64>>,
	classic: Vec<Option<f64>>,
}

fn run(xs: &[Option<f64>]) -> Outs {
	let xs: Vec<Reading> = xs.iter().map(|x| x.map_or(Reading::ABSENT, Reading::from)).collect();
	let mut g = G::default();
	let mut o = Outs {
		wilder: Vec::new(),
		ema: Vec::new(),
		long: Vec::new(),
		classic: Vec::new(),
	};
	let (mut at, mut tick) = (0, 0);
	while at < xs.len() {
		let end = (at + 1 + tick % 3).min(xs.len());
		let out = g.tick(tick as i64 * 60_000_000_000, Batches { xs: &xs[at..end] });
		o.wilder.extend(out.wilder.iter().map(|r| r.get()));
		o.ema.extend(out.ema.iter().map(|r| r.get()));
		o.long.extend(out.long.iter().map(|r| r.get()));
		o.classic.extend(out.classic.iter().map(|r| r.get()));
		(at, tick) = (end, tick + 1);
	}
	o
}

/// The definition, summed outright over the window ending at each element.
fn oracle(xs: &[Option<f64>], alpha: f64, k: usize) -> Vec<Option<f64>> {
	(0..xs.len())
		.map(|j| {
			if j + 1 < k {
				return None;
			}
			let (mut sum, mut weight) = (0.0, 0.0);
			for m in 0..k {
				if let Some(x) = xs[j - m] {
					let w = alpha * (1.0 - alpha).powi(m as i32);
					sum += w * x;
					weight += w;
				}
			}
			(weight > 0.0).then(|| sum / weight)
		})
		.collect()
}

fn walk(n: usize) -> Vec<Option<f64>> {
	(0..n).map(|i| Some(100.0 + (i as f64 * 0.7).sin() * 5.0 + i as f64 * 0.01)).collect()
}

/// Gaps, a lone sample, and a stretch longer than the window with nothing in it.
fn holed() -> Vec<Option<f64>> {
	let mut xs = walk(40);
	for i in [3, 7, 8, 15] {
		xs[i] = None;
	}
	for x in &mut xs[20..27] {
		*x = None;
	}
	xs[23] = Some(42.0);
	for x in &mut xs[30..37] {
		*x = None;
	}
	xs
}

#[track_caller]
fn close(a: &[Option<f64>], b: &[Option<f64>], tol: f64) {
	assert_eq!(a.len(), b.len());
	for (i, (x, y)) in a.iter().zip(b).enumerate() {
		match (x, y) {
			(Some(x), Some(y)) => assert!((x - y).abs() <= tol * y.abs().max(1.0), "element {i}: {x} vs {y}"),
			(None, None) => {}
			_ => panic!("element {i}: {x:?} vs {y:?}"),
		}
	}
}

#[test]
fn a_window_is_its_definition() {
	for xs in [walk(60), holed()] {
		let o = run(&xs);
		close(&o.wilder, &oracle(&xs, 1.0 / 3.0, K), 1e-12);
		close(&o.ema, &oracle(&xs, 2.0 / 4.0, K), 1e-12);
	}
}

/// The property the window exists for: the second run started `m` elements late, and from its
/// `K`-th element on the two cannot tell where either began.
#[test]
fn two_starts_agree_once_both_have_seen_k() {
	for xs in [walk(60), holed()] {
		for m in [1, 4, 11, 22] {
			let (whole, late) = (run(&xs), run(&xs[m..]));
			close(&whole.wilder[m + K - 1..], &late.wilder[K - 1..], 1e-12);
			close(&whole.ema[m + K - 1..], &late.ema[K - 1..], 1e-12);
			assert!(late.wilder[..K - 1].iter().all(Option::is_none), "absent until K elements have passed");
		}
	}
}

#[test]
fn a_constant_in_is_the_constant_out() {
	let o = run(&vec![Some(7.25); 50]);
	for v in o.wilder[K - 1..].iter().chain(&o.ema[K - 1..]) {
		assert!((v.expect("warm") - 7.25).abs() < 1e-12);
	}
}

/// Wilder's own SMA seed is forgotten at `(1-α)^n`, and so is everything past a long window: deep
/// into a run the two are one average.
#[test]
fn a_long_window_tracks_wilder() {
	let xs = walk(1500);
	let o = run(&xs);
	close(&o.long[1000..], &o.classic[1000..], 1e-12);
}
