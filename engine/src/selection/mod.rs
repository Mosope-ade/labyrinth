//! Deterministic capability filtering and assignment, without topology-verification claims.
mod filter;
mod ports;
mod types;
pub use filter::filter_candidates;
pub use types::*;

mod assignment;
pub use assignment::{assign, with_growth};
