//! The indicator tier: smoothers over a series, and the nodes an indicator is wired from. A strategy
//! crate names these rather than re-deriving them; what it still owns is the wiring — which series,
//! which smoother, which lengths, which thresholds.
#![feature(adt_const_params)]

mod bar;
mod rsi_node;
mod smooth;
mod zipf;

pub use bar::{Bar, Bars, Ohlc, Ohlcs, Volume, Volumes, closed_by};
pub use rsi_node::{Gains, Losses, Rsi, RsiDelta};
pub use smooth::{Ema, Length, Wilder};
pub use zipf::{Zipf, ZipfSpec};
