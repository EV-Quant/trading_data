//! A clocked series is one element per period of its clock, whether or not anything traded in it —
//! the count of its elements is a span of time, and every `Elems` reach over it reads as one.

#[allow(
	unused_imports,
	reason = "rust#52234: `graph!` reaches a node's shim textually, so the import is the only thing that puts it in scope"
)]
use trading_data::{__td_node_Bars, __td_node_Ohlcs, __td_node_Volumes};
use trading_data::{Bar, Bars, Precision, PrecisionPriceQty, Side, TradeBuf, TradeCols, Trades, Ts, graph};
use v_utils::TF_1MIN;

const PREC: PrecisionPriceQty = PrecisionPriceQty {
	price: Precision(2),
	qty: Precision(4),
};
const MIN: i64 = 60_000_000_000;

graph! {
	struct G;
	batches Batches;
	roots { trades: Trades[TradeCols] };
	out GOut;
	outputs { bars: Bars<{ TF_1MIN }> }
}

/// One trade per tick, at `(minute, second)`, and every bar the graph closed.
fn bars(trades: &[(i64, i64, i32)]) -> Vec<Bar> {
	let mut g = G::default();
	let mut buf = TradeBuf::new(PREC);
	let mut out = Vec::new();
	for (seq, &(min, sec, price)) in trades.iter().enumerate() {
		buf.clear();
		buf.prec = PREC;
		buf.push(Ts::from_nanos(min * MIN + sec * 1_000_000_000), None, None, seq as u64, Side::Buy, price, 10_000);
		out.extend_from_slice(g.tick(0, Batches { trades: buf.cols(0..1) }).bars);
	}
	out
}

#[test]
fn a_minute_nothing_traded_in_is_still_a_bar() {
	let got = bars(&[(0, 10, 100_00), (0, 50, 101_00), (3, 5, 99_00), (4, 1, 98_00)]);
	let closes: Vec<i64> = got.iter().map(|b| b.ts_close.as_nanos() / MIN).collect();
	assert_eq!(closes, [1, 2, 3, 4], "the bars closing at 1..=4 all closed by the trade in minute 4");
	for quiet in &got[1..3] {
		assert_eq!(
			(quiet.open, quiet.high, quiet.low, quiet.close),
			(101.0, 101.0, 101.0, 101.0),
			"flat at the last close: {quiet:?}"
		);
		assert_eq!(quiet.vol_base, 0.0);
	}
	assert_eq!((got[3].open, got[3].close), (99.0, 99.0));
}

#[test]
fn bars_tile_time() {
	let got = bars(&[(0, 1, 100_00), (7, 1, 100_00), (8, 1, 100_00)]);
	assert!(got.windows(2).all(|w| w[1].ts_close.as_nanos() - w[0].ts_close.as_nanos() == MIN), "{got:?}");
	assert_eq!(got.len(), 8);
}
