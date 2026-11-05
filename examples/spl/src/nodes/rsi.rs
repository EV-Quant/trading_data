use trading_data::Length;

use crate::config::strategy;

/// The Wilder length, out of `config.nix`. Which series the chain runs on is not here: it is the
/// `Bars<..>` every naming site writes out, and `indies.rsi.timeframe` is checked against that in
/// [`crate::config::Config::load`] rather than dispatched on.
pub struct Knobs;
impl Length for Knobs {
	const NAME: &'static str = "Knobs";

	fn len() -> usize {
		strategy().indies.rsi.len
	}
}
