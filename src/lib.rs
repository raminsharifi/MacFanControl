//! macOS fan control and SMC temperature access.
//!
//! This library requires macOS. Disable default features when using it without
//! the `macfan` terminal application.

pub mod control;
pub mod curve;
pub mod fan;
pub mod smc;
pub mod temps;
