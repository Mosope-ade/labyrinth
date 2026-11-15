use crate::{
    catalog::CatalogVersion,
    model::*,
    topology::{AddressingPolicy, LogicalPolicy},
    verification::{VerificationLimits, VerificationReport},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GenerationOptions {
    pub addressing: AddressingPolicy,
    pub logical: LogicalPolicy,
    pub verification: VerificationLimits,
    pub selection_search_states: u64,
}
impl Default for GenerationOptions {
    fn default() -> Self {
        Self {
            addressing: Default::default(),
            logical: Default::default(),
            verification: Default::default(),
            selection_search_states: 100_000,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FileDigest {
    pub sha256: String,
    pub size_bytes: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackageManifest {
    pub schema_version: String,
    pub engine_version: String,
    pub rules: BTreeMap<String, String>,
    pub catalog_version: CatalogVersion,
    pub requirements_sha256: String,
    pub options_sha256: String,
    pub files: BTreeMap<String, FileDigest>,
    pub base_revision: Option<super::BaseReference>,
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct BomLine {
    pub role: DeviceRole,
    pub archetype: ArchetypeId,
    pub mode: ModeId,
    pub quantity: u32,
    pub relative_cost_tier: u8,
    pub reference_hints: Vec<crate::catalog::ReferenceHint>,
}
/// Constructed only after all five verification passes; immutable payload views.
///
/// ```compile_fail
/// use labyrinth_engine::artifacts::VerifiedPackage;
/// let _:VerifiedPackage=serde_json::from_str("{}").unwrap();
/// ```
#[derive(Debug, Clone)]
pub struct VerifiedPackage {
    pub(super) manifest: PackageManifest,
    pub(super) manifest_sha256: String,
    pub(super) files: BTreeMap<String, Vec<u8>>,
    pub(super) network: NetworkDraft,
    pub(super) report: VerificationReport,
    pub(super) options: GenerationOptions,
}
impl VerifiedPackage {
    pub fn manifest(&self) -> &PackageManifest {
        &self.manifest
    }
    pub fn manifest_sha256(&self) -> &str {
        &self.manifest_sha256
    }
    pub fn files(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.files
    }
    pub fn network(&self) -> &NetworkDraft {
        &self.network
    }
    pub fn report(&self) -> &VerificationReport {
        &self.report
    }
    pub fn options(&self) -> &GenerationOptions {
        &self.options
    }
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum GenerationError {
    Planning(crate::topology::PlanningError),
    Routing(crate::routing::RoutingError),
    VerificationFailed(Box<VerificationReport>),
    Serialization(String),
    Configuration(String),
    PackageLimit {
        file: String,
        bytes: u64,
        limit: u64,
    },
    RevisionConflict(String),
    CapacityRejected(Vec<crate::diagnostics::Counterexample>),
    RepairExhausted {
        passes: u8,
        bottlenecks: Vec<crate::selection::Bottleneck>,
        last: Box<GenerationError>,
    },
}
impl From<crate::topology::PlanningError> for GenerationError {
    fn from(e: crate::topology::PlanningError) -> Self {
        Self::Planning(e)
    }
}
impl From<crate::routing::RoutingError> for GenerationError {
    fn from(e: crate::routing::RoutingError) -> Self {
        Self::Routing(e)
    }
}
impl From<serde_json::Error> for GenerationError {
    fn from(e: serde_json::Error) -> Self {
        Self::Serialization(e.to_string())
    }
}
impl std::fmt::Display for GenerationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::VerificationFailed(r) => write!(
                f,
                "verification failed: {:?}",
                r.passes
                    .iter()
                    .filter(|p| !p.passed)
                    .map(|p| &p.kind)
                    .collect::<Vec<_>>()
            ),
            _ => write!(f, "{self:?}"),
        }
    }
}
impl std::error::Error for GenerationError {}
