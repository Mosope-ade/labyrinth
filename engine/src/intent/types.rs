use crate::model::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const INTENT_SCHEMA_VERSION: &str = "intent_v1";

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PartialIntent {
    pub schema_version: String,
    pub name: Option<String>,
    pub sites: Option<Vec<SiteRequirement>>,
    pub demands: Option<Vec<TrafficDemand>>,
    pub policies: Option<Vec<FlowPolicy>>,
    pub redundancy: Option<RedundancyTier>,
    pub blueprint: Option<BlueprintChoice>,
    pub growth_percent: Option<u8>,
    pub spare_port_target_percent: Option<u8>,
    pub ipv4_pool: Option<Ipv4Prefix>,
    #[serde(default)]
    pub excluded_ipv4: Option<Vec<Ipv4Prefix>>,
    pub update: Option<UpdateMode>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SiteRequirement {
    pub id: SiteId,
    pub role: SiteRole,
    pub location: Option<String>,
    pub isp_failure_domains: Option<Vec<FailureDomainId>>,
    pub zones: Option<Vec<ZoneRequirement>>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SiteRole {
    Hub,
    Branch,
    Campus,
    DataCenter,
    Other,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ZoneRequirement {
    pub id: ZoneId,
    pub endpoint_count: Option<u32>,
    pub required_features: Option<Vec<Feature>>,
    pub poe_load_watts: Option<u32>,
    pub endpoint_attachment: Option<EndpointAttachment>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RedundancyTier {
    Tier0None,
    Tier1Link,
    Tier2Device,
    Tier3Site,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BlueprintChoice {
    Automatic,
    HubSpoke,
    ThreeTierCampus,
    SpineLeaf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum UpdateMode {
    StatelessRedesign,
    IncrementalUpdate {
        base_revision: RevisionId,
        ipam_pins: Vec<AllocationPin>,
        core_pins: Vec<CorePin>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AllocationPin {
    pub zone: ZoneId,
    pub subnet: Ipv4Prefix,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CorePin {
    pub device: DeviceId,
    pub site: SiteId,
    pub archetype: ArchetypeId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ResolvedSite {
    pub id: SiteId,
    pub role: SiteRole,
    pub location: Option<String>,
    pub isp_failure_domains: Option<Vec<FailureDomainId>>,
    pub zones: Vec<ResolvedZone>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ResolvedZone {
    pub id: ZoneId,
    pub endpoint_count: u32,
    pub required_features: Vec<Feature>,
    pub poe_load_watts: Option<u32>,
    pub endpoint_attachment: EndpointAttachment,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct IntentData {
    pub schema_version: String,
    pub name: String,
    pub sites: Vec<ResolvedSite>,
    pub demands: Vec<TrafficDemand>,
    pub policies: Vec<FlowPolicy>,
    pub redundancy: RedundancyTier,
    pub blueprint: BlueprintChoice,
    pub growth_percent: u8,
    pub spare_port_target_percent: u8,
    pub ipv4_pool: Ipv4Prefix,
    pub excluded_ipv4: Vec<Ipv4Prefix>,
    pub update: UpdateMode,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ValueSource {
    Provided,
    EngineDefault,
}

/// Readiness is not feasibility or verified generation. No public constructor/Deserialize.
///
/// ```compile_fail
/// use labyrinth_engine::intent::ResolvedIntent;
/// let _: ResolvedIntent = serde_json::from_str("{}").unwrap();
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ResolvedIntent {
    pub(super) data: IntentData,
    pub(super) provenance: BTreeMap<String, ValueSource>,
}
impl ResolvedIntent {
    pub fn data(&self) -> &IntentData {
        &self.data
    }
    pub fn provenance(&self) -> &BTreeMap<String, ValueSource> {
        &self.provenance
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct MissingField {
    pub path: String,
    pub reason: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "status", content = "data", rename_all = "snake_case")]
pub enum Readiness {
    NeedsClarification(Vec<MissingField>),
    Ready(Box<ResolvedIntent>),
}

/// Per-endpoint physical attachment. Missing attachment defaults to 1 Gbps copper.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EndpointAttachment {
    pub speed_mbps: u64,
    pub media: Media,
    pub poe_watts_per_endpoint: Option<u16>,
}
impl Default for EndpointAttachment {
    fn default() -> Self {
        Self {
            speed_mbps: 1000,
            media: Media::Copper,
            poe_watts_per_endpoint: None,
        }
    }
}
