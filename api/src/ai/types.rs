use labyrinth_engine::{
    diagnostics::ValidationErrors,
    intent::{MissingField, PartialIntent},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use uuid::Uuid;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct ProposalOutput {
    #[schemars(extend("const" = "ai_proposal_v1"))]
    pub schema_version: String,
    #[schemars(with = "PartialIntent")]
    pub intent: Box<RawValue>,
    #[schemars(length(max = 64))]
    pub assumptions: Vec<Assumption>,
}
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Assumption {
    #[schemars(length(min = 1, max = 256))]
    pub path: String,
    #[schemars(length(min = 1, max = 1024))]
    pub explanation: String,
}
#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct ReviewContext {
    pub(super) actor_id: Uuid,
    pub(super) tenant_id: Uuid,
    pub(super) project_id: Uuid,
    pub(super) expected_head: Option<Uuid>,
    pub(super) catalog_sha256: String,
    pub(super) generation_options: serde_json::Value,
}
#[derive(Debug, Serialize)]
pub struct ProviderMetadata {
    pub(super) provider: String,
    pub(super) model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) profile: Option<String>,
}
/// Acknowledgement of an exact reviewed proposal, never an auth capability.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Confirmation {
    #[schemars(extend("const" = "ai_confirmation_v1"))]
    pub schema_version: String,
    #[schemars(regex(pattern = "^[0-9a-f]{64}$"))]
    pub proposal_sha256: String,
}
/// Review projection only: deliberately never exposes an engine ResolvedIntent.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(tag = "status", content = "missing_fields", rename_all = "snake_case")]
pub enum ProposalReadiness {
    NeedsClarification(Vec<MissingField>),
    ReadyForReview,
}
/// Constructed by deterministic review only; cannot deserialize model assertions.
pub struct ReviewedProposal {
    pub(super) output: ProposalOutput,
    pub(super) context: ReviewContext,
    pub(super) provider: ProviderMetadata,
    pub(super) prompt_sha256: String,
    pub(super) digest: String,
    pub(super) readiness: ProposalReadiness,
    pub(super) defaults: std::collections::BTreeMap<String, serde_json::Value>,
}
#[derive(Serialize)]
pub struct ProposalView<'a> {
    pub schema_version: &'static str,
    pub review_state: &'static str,
    pub proposal_sha256: &'a str,
    pub context: &'a ReviewContext,
    pub provider: &'a ProviderMetadata,
    pub prompt_sha256: &'a str,
    pub intent: &'a RawValue,
    pub assumptions: &'a [Assumption],
    pub engine_readiness: &'a ProposalReadiness,
    pub engine_defaults: &'a std::collections::BTreeMap<String, serde_json::Value>,
}
/// A reviewed input only: not an admitted job or generated/verified network.
/// ```compile_fail
/// let _: labyrinth_api::ai::ConfirmedProposal = serde_json::from_str("{}").unwrap();
/// ```
pub struct ConfirmedProposal {
    pub(super) reviewed: ReviewedProposal,
}
#[derive(Debug, thiserror::Error)]
pub enum ProposalError {
    #[error("invalid provider proposal")]
    InvalidOutput,
    #[error("provider proposal exceeds its byte limit")]
    TooLarge,
    #[error("invalid or oversized prompt")]
    InvalidPrompt,
    #[error("invalid review context")]
    InvalidContext,
    #[error("invalid configured provider identity")]
    InvalidProvider,
    #[error("proposed requirements are invalid")]
    InvalidIntent(ValidationErrors),
    #[error("critical requirements need clarification")]
    NeedsClarification,
    #[error("proposal confirmation or current context does not match")]
    ReviewConflict,
}
