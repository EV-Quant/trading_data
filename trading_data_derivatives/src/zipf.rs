use core::{array, marker::PhantomData};

use trading_data_dag::{
	Carried, Cell, DepReads, Env, Folding, Folds, Nudge, Present, Reading, Series, Slots, Tag, Unbounded, Vars, Witness, constant, lt, min, node, select, slice_nudge, sum,
};

/// The exponent of a [`Zipf`] — a number an app reading it from a config file has no const to give.
pub trait ZipfSpec: 'static {
	/// How [`Zipf`] spells this spec in its own [`Cell::NAME`].
	const NAME: &'static str;

	/// `s` in `(k+1)^-s`. The weights total `ζ(s)`, which is finite only past 1.
	fn exponent() -> f64;
}

/// Mean of `X` under weight `(k+1)^-s` on the sample `k` back, normalised by the weight the history
/// seen so far carries — which tends to `ζ(s)`.
///
/// The last `N` samples are weighted exactly. Older ones share one geometrically decaying sum whose
/// first weight is `(N+1)^-s` and whose total is the true tail's, `ζ(s) - H(N,s)`.
pub struct Zipf<X, S, const N: usize> {
	state: Carried,
	/// `(k+1)^-s` for `k < N`.
	head: [f64; N],
	/// `(N+1)^-s`: what a sample leaving the head enters the tail at.
	entry: f64,
	decay: f64,
	_wiring: PhantomData<(X, S)>,
}

/// Env layout: the sample, then the state — how many head slots hold a sample, the tail's weighted
/// sum and its weight, and the head itself, newest first.
const COUNT: usize = 1;
const RING: usize = 4;

impl<X, S, const N: usize> Clone for Zipf<X, S, N> {
	fn clone(&self) -> Self {
		Self {
			state: self.state,
			head: self.head,
			entry: self.entry,
			decay: self.decay,
			_wiring: PhantomData,
		}
	}
}
impl<X, S: ZipfSpec, const N: usize> Default for Zipf<X, S, N> {
	fn default() -> Self {
		const { assert!(N >= 1, "a Zipf with no exact head has no first weight to start its tail at") }
		let s = S::exponent();
		assert!(s > 1.0, "Zipf exponent {s}: ζ(s) diverges at s <= 1, so the tail would outweigh any head");
		let entry = ((N + 1) as f64).powf(-s);
		Self {
			state: Carried::default(),
			head: array::from_fn(|k| ((k + 1) as f64).powf(-s)),
			entry,
			decay: 1.0 - entry / zeta_tail(s, N),
			_wiring: PhantomData,
		}
	}
}
impl<X: Series<Item = Reading> + Nudge, S: ZipfSpec, const N: usize> Zipf<X, S, N> {
	const TAG: Tag = Tag::of("Zipf", &[X::NAME, S::NAME]).count(N);
}
impl<X: Series<Item = Reading> + Nudge, S: ZipfSpec, const N: usize> Cell for Zipf<X, S, N> {
	type Out<'t> = &'t [Reading];

	const NAME: &'static str = Self::TAG.as_str();
}
#[node]
impl<X: Series<Item = Reading> + Nudge, S: ZipfSpec, const N: usize> Folds for Zipf<X, S, N> {
	type Deps = (Folding<X, Unbounded>,);

	const STATE: usize = RING - COUNT + N;

	/// An absent sample is not a sample: it neither enters the head nor ages it.
	fn read<W: Witness>((xs,): DepReads<'_, Self>, i: usize, env: &mut Env<'_, W>) -> Option<i64> {
		let x = xs.at(i)?;
		x.present()?;
		env.put(x);
		Some(0)
	}

	fn step(&self, v: Vars) -> impl Slots {
		let (count, tail, mass) = (v.get::<1>(), v.get::<2>(), v.get::<3>());
		// counts are whole, so this is `count >= N`: only then is the head's last slot a sample.
		let full = lt(constant(N as f64 - 1.0), count);
		(
			min(count + constant(1.0), constant(N as f64)),
			constant(self.decay) * tail + constant(self.entry) * v.at(RING + N - 1),
			constant(self.decay) * mass + select(full, constant(self.entry), constant(0.0)),
			array::from_fn::<_, N, _>(|k| v.at(if k == 0 { 0 } else { RING + k - 1 })),
		)
	}

	fn value(&self, v: Vars) -> impl Slots {
		let count = v.get::<1>();
		let num = sum(array::from_fn::<_, N, _>(|k| constant(self.head[k]) * v.at(RING + k))) + v.get::<2>();
		let den = sum(array::from_fn::<_, N, _>(|k| select(lt(constant(k as f64), count), constant(self.head[k]), constant(0.0)))) + v.get::<3>();
		num / den
	}

	fn carried(&self) -> &Carried {
		&self.state
	}

	fn carried_mut(&mut self) -> &mut Carried {
		&mut self.state
	}
}
slice_nudge!([X: Series<Item = Reading> + Nudge, S: ZipfSpec, const N: usize] Zipf<X, S, N>, Reading);

/// `Σ_{j>n} j^-s`: summed out to `n + 32`, Euler–Maclaurin past it.
fn zeta_tail(s: f64, n: usize) -> f64 {
	let a = (n + 33) as f64;
	let near: f64 = (n + 1..n + 33).map(|j| (j as f64).powf(-s)).sum();
	near + a.powf(1.0 - s) / (s - 1.0) + a.powf(-s) / 2.0 + s * a.powf(-s - 1.0) / 12.0 - s * (s + 1.0) * (s + 2.0) * a.powf(-s - 3.0) / 720.0
}
