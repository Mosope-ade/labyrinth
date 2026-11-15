use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VerificationLimits {
    pub max_scenarios: usize,
    pub max_service_classes: usize,
}
impl Default for VerificationLimits {
    fn default() -> Self {
        Self {
            max_scenarios: 4096,
            max_service_classes: 1_000_000,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PassKind {
    Structural,
    Addressing,
    Routing,
    Security,
    Resilience,
}
pub use crate::diagnostics::Counterexample;
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PassEvidence {
    pub kind: PassKind,
    pub passed: bool,
    pub checked: u64,
    pub counterexamples: Vec<Counterexample>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScenarioEvidence {
    pub name: String,
    pub passed: bool,
    pub protected_demands: usize,
    pub excluded_demands: usize,
    pub counterexamples: Vec<Counterexample>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VerificationReport {
    pub rules_version: String,
    pub passes: Vec<PassEvidence>,
    pub scenarios: Vec<ScenarioEvidence>,
    pub protected_set: Vec<String>,
    pub exclusions: Vec<String>,
    pub assumptions: Vec<String>,
}
impl VerificationReport {
    pub fn passed(&self) -> bool {
        self.passes.len() == 5 && self.passes.iter().all(|p| p.passed)
    }
}
pub(super) fn failure(
    code: &str,
    component: impl Into<String>,
    detail: impl Into<String>,
) -> Counterexample {
    Counterexample {
        code: code.into(),
        component: component.into(),
        detail: detail.into(),
        required: None,
        available: None,
    }
}
pub(super) fn pass(kind: PassKind, checked: u64, errors: Vec<Counterexample>) -> PassEvidence {
    PassEvidence {
        kind,
        passed: errors.is_empty(),
        checked,
        counterexamples: errors,
    }
}
