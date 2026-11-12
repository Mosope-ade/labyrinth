//! Partial/untrusted requirements and validated readiness; neither is a generated design.
mod resolve;
mod types;
mod validate;
pub use resolve::resolve;
pub use types::*;

pub fn intent_schema() -> schemars::Schema {
    schemars::schema_for!(PartialIntent)
}

pub use crate::model::{
    FlowPolicy, PolicyAction, PortRange, Protocol, ServiceMatch, TrafficDemand,
};

pub use crate::diagnostics::{JsonInputError, MAX_JSON_BYTES};
/// Bounded external JSON entrypoint. Ready means structurally resolved, not verified.
pub fn parse_json(input: &str) -> Result<Readiness, JsonInputError> {
    resolve(crate::diagnostics::decode_json(input)?).map_err(JsonInputError::Invalid)
}
