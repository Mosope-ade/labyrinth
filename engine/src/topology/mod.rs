//! Physical binding and addressing stage. Logical forwarding is deliberately separate.
mod build;
mod types;
pub(crate) use build::build;
pub use types::*;
