pub mod control;
pub mod curve;
pub mod fan;
pub mod smc;
pub mod temps;

#[cfg(feature = "tui")]
pub mod app;

#[cfg(feature = "tui")]
mod persist;

#[cfg(feature = "tui")]
mod plot;

#[cfg(feature = "tui")]
mod ui;
