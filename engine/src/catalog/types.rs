use crate::model::{ArchetypeId, DeviceRole, Feature, Media, ModeId, PortGroupId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CatalogBundle {
    pub schema_version: String,
    pub version: String,
    #[schemars(length(min = 1, max = 256))]
    pub archetypes: Vec<Archetype>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Archetype {
    pub id: ArchetypeId,
    #[schemars(length(min = 1, max = 9))]
    pub roles: Vec<DeviceRole>,
    #[schemars(range(min = 1, max = 255))]
    pub relative_cost_tier: u8,
    #[schemars(length(max = 32))]
    pub features: Vec<Feature>,
    pub poe_budget_watts: Option<u32>,
    #[schemars(length(min = 1, max = 32))]
    pub port_groups: Vec<PortGroup>,
    #[schemars(length(min = 1, max = 32))]
    pub operating_modes: Vec<OperatingMode>,
    #[schemars(length(max = 32))]
    pub reference_hints: Vec<ReferenceHint>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PortGroup {
    pub id: PortGroupId,
    #[schemars(range(min = 1))]
    pub count: u16,
    #[schemars(length(min = 1, max = 16))]
    pub speeds_mbps: Vec<u64>,
    pub media: Media,
    pub poe_watts_per_port: Option<u16>,
    pub shared_capacity_mbps: Option<u64>,
    pub breakout: Option<Breakout>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Breakout {
    pub children_per_port: u16,
    pub child_speed_mbps: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OperatingMode {
    pub id: ModeId,
    #[schemars(length(max = 32))]
    pub features: Vec<Feature>,
    pub throughput_mbps: Option<u64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReferenceHint {
    #[schemars(length(min = 1, max = 256))]
    pub description: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CatalogVersion {
    pub semantic_version: String,
    pub sha256: String,
}
