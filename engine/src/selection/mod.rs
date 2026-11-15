//! Deterministic capability filtering and assignment, without topology-verification claims.
mod filter;
mod ports;
mod types;
pub use filter::filter_candidates;
pub use types::*;

mod assignment;
pub use assignment::{assign, with_growth};

mod traffic;
pub use traffic::{SessionBatch, batches, reserved_peak};

pub(crate) use assignment::{assign_checked, score};
