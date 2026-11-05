//! What a [`Zipf`] average is, read off its outs: the Zipf-weighted mean of what it has seen, exact
//! while history fits the head, and normalised by `ζ(s)` once history outgrows any head.

use trading_data_dag::{Cell, Present, Reading, graph, slice_nudge};
#[allow(
	unused_imports,
	reason = "rust#52234: `graph!` reaches a node's shim textually, so the import is the only thing that puts it in scope"
)]
use trading_data_derivatives::__td_node_Zipf;
use trading_data_derivatives::{Zipf, ZipfSpec};

struct Xs;
impl Cell for Xs {
	type Out<'t> = &'t [Reading];
}
slice_nudge!(Xs, Reading);

/// `s = 2`, because `ζ(2) = π²/6` is the one normaliser with a closed form to check against.
struct Basel;
impl ZipfSpec for Basel {
	const NAME: &'static str = "2";

	fn exponent() -> f64 {
		2.0
	}
}

const HEAD: usize = 4;

graph! {
	struct G;
	batches Batches;
	roots { xs: Xs[Reading] };
	out GOut;
	outputs { z: Zipf<Xs, Basel, 4> }
}

/// One sample per tick, `None` standing for an absent one.
fn drive(xs: &[Option<f64>]) -> Vec<Option<f64>> {
	let mut g = G::default();
	let mut out = Vec::new();
	for x in xs {
		let x = x.map_or(Reading::ABSENT, Reading::from);
		let o = g.tick(0, Batches { xs: core::slice::from_ref(&x) });
		out.extend(o.z.iter().map(|v| v.present()));
	}
	out
}

fn close(got: f64, want: f64, tol: f64) {
	assert!((got - want).abs() < tol, "{got} vs {want}");
}

#[test]
fn while_history_fits_the_head_it_is_the_exact_zipf_mean() {
	let xs = [3.0, -1.0, 7.5, 2.0];
	let out = drive(&xs.map(Some));
	for t in 0..HEAD {
		let w = |k: usize| ((k + 1) as f64).powi(-2);
		let want = (0..=t).map(|k| w(k) * xs[t - k]).sum::<f64>() / (0..=t).map(w).sum::<f64>();
		close(out[t].expect("a sample was seen"), want, 1e-12);
	}
}

#[test]
fn a_constant_comes_back_unchanged_however_deep_the_tail() {
	for v in drive(&[Some(3.7); 500]) {
		close(v.expect("every sample present"), 3.7, 1e-12);
	}
}

/// After a long silence of zeros, one sample's share is its own weight over everything seen —
/// which, as history grows, is `1/ζ(2)`.
#[test]
fn the_newest_sample_weighs_one_over_zeta() {
	let mut xs = vec![Some(0.0); 2000];
	xs.push(Some(1.0));
	close(drive(&xs).last().copied().flatten().expect("present"), 6.0 / core::f64::consts::PI.powi(2), 1e-9);
}

#[test]
fn an_absent_sample_neither_enters_nor_ages_the_average() {
	let with = drive(&[Some(1.0), None, Some(2.0), Some(5.0), Some(-3.0), Some(4.0), Some(0.5)]);
	let without = drive(&[Some(1.0), Some(2.0), Some(5.0), Some(-3.0), Some(4.0), Some(0.5)]);
	assert_eq!(with[1], None);
	assert_eq!(with.last(), without.last());
}
