use crate::{catalog::CatalogVersion, model::*};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PortDemand {
    pub id: PortGroupId,
    pub count: u16,
    pub speed_mbps: u64,
    pub media: Media,
    pub min_poe_watts: Option<u16>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeviceRequirement {
    pub id: DeviceId,
    pub role: DeviceRole,
    pub required_features: Vec<Feature>,
    pub peak_mbps: u64,
    pub poe_watts: Option<u64>,
    pub pinned_archetype: Option<ArchetypeId>,
    pub ports: Vec<PortDemand>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct PortAssignment {
    pub demand: PortGroupId,
    pub group: PortGroupId,
    pub first_index: u16,
    pub count: u16,
    pub speed_mbps: u64,
    pub media: Media,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Candidate {
    pub archetype: ArchetypeId,
    pub mode: ModeId,
    pub cost_tier: u8,
    pub port_assignments: Vec<PortAssignment>,
    pub compatible_port_count: u32,
    pub used_port_count: u32,
    pub spare_shortfall_ppm: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RejectReason {
    Role,
    ModeFeatures,
    UnknownThroughput,
    Throughput,
    PortCompatibility,
    PoeBudget,
    PinConflict,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CandidateRejection {
    pub archetype: ArchetypeId,
    pub mode: ModeId,
    pub reasons: Vec<RejectReason>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CandidateSet {
    pub candidates: Vec<Candidate>,
    pub rejections: Vec<CandidateRejection>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum SelectionError {
    InvalidRequirement(String),
    ArithmeticOverflow,
    UnsupportedBlueprint(String),
    ClarificationRequired(String),
    PinConflict(String),
    Infeasible(String),
    NoCandidate {
        device: DeviceId,
        required: Box<DeviceRequirement>,
        rejections: Vec<CandidateRejection>,
    },
    SearchExhausted {
        visited: u64,
        limit: u64,
    },
    RepairExhausted {
        passes: u8,
        bottlenecks: Vec<Bottleneck>,
    },
}
impl std::fmt::Display for SelectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for SelectionError {}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, JsonSchema)]
pub struct AssignmentScore {
    pub standardization_penalty: u32,
    pub relative_cost_sum: u64,
    pub spare_shortfall_ppm: u64,
    pub device_count: u32,
    pub stable_tiebreak: Vec<(DeviceId, ArchetypeId, ModeId)>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct SelectedDevice {
    pub requirement: DeviceId,
    pub role: DeviceRole,
    pub candidate: Candidate,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Assignment {
    pub catalog_version: CatalogVersion,
    pub devices: Vec<SelectedDevice>,
    pub score: AssignmentScore,
    pub visited_states: u64,
    pub candidate_rejections: std::collections::BTreeMap<DeviceId, Vec<CandidateRejection>>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DemandEndpoint {
    pub device: DeviceId,
    pub demand: PortGroupId,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConnectionRequirement {
    pub id: LinkId,
    pub from: DemandEndpoint,
    pub to: DemandEndpoint,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SelectionRequest {
    pub devices: Vec<DeviceRequirement>,
    pub connections: Vec<ConnectionRequirement>,
    pub spare_port_target_percent: u8,
    pub max_states: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Bottleneck {
    pub pass: u8,
    pub device: Option<DeviceId>,
    pub message: String,
    pub rejections: Vec<CandidateRejection>,
}
