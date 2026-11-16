//! Bounded private engine diagnostics. Never include worker exceptions or credentials.
use serde::{Deserialize, Serialize};
use serde_json::Value;
pub const MAX_WIRE: usize = 32 * 1024;
const MAX_REPORT: usize = 16 * 1024;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Rejected,
    Inconclusive,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub schema_version: String,
    pub outcome: Outcome,
    pub truncated: bool,
    pub error: Option<Value>,
}
impl Evidence {
    pub(crate) fn from_engine(
        error: &labyrinth_engine::artifacts::GenerationError,
        resource: bool,
    ) -> Self {
        // Serialization/configuration strings are implementation diagnostics, not counterexamples.
        let value = match error {
            labyrinth_engine::artifacts::GenerationError::Serialization(_)
            | labyrinth_engine::artifacts::GenerationError::Configuration(_) => None,
            _ => serde_json::to_value(error).ok(),
        };
        let truncated = value
            .as_ref()
            .is_some_and(|v| serde_json::to_vec(v).map_or(true, |b| b.len() > MAX_REPORT));
        Self {
            schema_version: "engine_failure_v1".into(),
            outcome: if resource {
                Outcome::Inconclusive
            } else {
                Outcome::Rejected
            },
            truncated,
            error: if truncated { None } else { value },
        }
    }
    pub(crate) fn decode(bytes: &[u8], resource: bool) -> Option<Self> {
        if bytes.len() > MAX_WIRE {
            return None;
        }
        let evidence: Self = serde_json::from_slice(bytes).ok()?;
        if evidence.schema_version != "engine_failure_v1"
            || (evidence.outcome == Outcome::Inconclusive) != resource
            || (evidence.truncated && evidence.error.is_some())
        {
            return None;
        }
        if let Some(value) = &evidence.error
            && (serde_json::to_vec(value).ok()?.len() > MAX_REPORT
                || ![
                    "planning",
                    "routing",
                    "verification_failed",
                    "package_limit",
                    "revision_conflict",
                    "capacity_rejected",
                    "repair_exhausted",
                ]
                .contains(&value.get("code")?.as_str()?))
        {
            return None;
        }
        Some(evidence)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn oversized_reports_are_explicit_and_invalid_envelopes_reject() {
        let error =
            labyrinth_engine::artifacts::GenerationError::RevisionConflict("x".repeat(MAX_REPORT));
        let evidence = Evidence::from_engine(&error, false);
        assert!(evidence.truncated);
        assert!(evidence.error.is_none());
        let bytes = serde_json::to_vec(&evidence).unwrap();
        assert!(Evidence::decode(&bytes, false).is_some());
        assert!(Evidence::decode(&bytes, true).is_none());
        assert!(Evidence::decode(b"private exception text", false).is_none());
        let mut forged = serde_json::to_value(evidence).unwrap();
        forged["secret"] = serde_json::json!("secret");
        assert!(Evidence::decode(&serde_json::to_vec(&forged).unwrap(), false).is_none());
    }
}
