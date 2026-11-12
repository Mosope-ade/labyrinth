use crate::{intent::BlueprintChoice, model::*, selection::SelectionRequest};
use schemars::JsonSchema;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct AccessPopulation {
    pub device: DeviceId,
    pub site: SiteId,
    pub zone: ZoneId,
    pub endpoints: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct DevicePlacement {
    pub device: DeviceId,
    pub site: SiteId,
    pub zone: Option<ZoneId>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ExternalAttachment {
    pub device: DeviceId,
    pub provider_domain: FailureDomainId,
    pub demand: PortGroupId,
    pub circuit_count: u16,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ZoneLayer2Requirement {
    pub site: SiteId,
    pub zone: ZoneId,
    pub links: Vec<LinkId>,
    /// Required protocol capability; forwarding states are not synthesized yet.
    pub loop_prevention: Option<Feature>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct FirewallHaRequirement {
    pub zone: ZoneId,
    pub devices: Vec<DeviceId>,
    pub sync_link: LinkId,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct BlueprintPlan {
    pub rules_version: String,
    pub blueprint: BlueprintChoice,
    pub pass: u8,
    pub requirements: SelectionRequest,
    pub access_populations: Vec<AccessPopulation>,
    pub placements: Vec<DevicePlacement>,
    pub external_attachments: Vec<ExternalAttachment>,
    pub decisions: Vec<String>,
    pub layer2_domains: Vec<ZoneLayer2Requirement>,
    pub firewall_ha: Vec<FirewallHaRequirement>,
}
