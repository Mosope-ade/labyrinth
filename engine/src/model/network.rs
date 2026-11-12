use super::*;
use crate::{
    catalog::{CatalogSnapshot, CatalogVersion},
    diagnostics::ValidationErrors,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Untrusted complete draft. Structural validation does not grant verified status.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NetworkDraft {
    pub schema_version: String,
    pub catalog_version: CatalogVersion,
    pub sites: Vec<NetworkSite>,
    pub zones: Vec<SecurityZone>,
    pub devices: Vec<Device>,
    pub links: Vec<PhysicalLink>,
    pub link_bundles: Vec<LinkBundle>,
    pub domains: Vec<Layer2Domain>,
    pub allocations: Vec<AddressAllocation>,
    pub routes: Vec<AbstractRoute>,
    pub policies: Vec<PolicyBinding>,
    pub decisions: Vec<DesignDecision>,
}
impl NetworkDraft {
    pub fn validate(self, catalog: &CatalogSnapshot) -> Result<ValidatedNetwork, ValidationErrors> {
        super::validate::validate(&self, catalog)?;
        Ok(ValidatedNetwork(self))
    }
}
/// Structurally consistent snapshot, NOT a security/routing/resilience verified build.
///
/// ```compile_fail
/// use labyrinth_engine::model::ValidatedNetwork;
/// let _: ValidatedNetwork = serde_json::from_str("{}").unwrap();
/// ```
#[derive(Debug, Clone)]
pub struct ValidatedNetwork(NetworkDraft);
impl ValidatedNetwork {
    pub fn network(&self) -> &NetworkDraft {
        &self.0
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NetworkSite {
    pub id: SiteId,
    pub failure_domains: Vec<FailureDomainId>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SecurityZone {
    pub id: ZoneId,
    pub site: SiteId,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Device {
    pub id: DeviceId,
    pub site: SiteId,
    pub role: DeviceRole,
    pub archetype: ArchetypeId,
    pub operating_mode: ModeId,
    pub interfaces: Vec<Interface>,
    pub failure_domains: Vec<FailureDomainId>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Interface {
    pub id: InterfaceId,
    pub port: Option<PortBinding>,
    pub speed_mbps: Option<u64>,
    pub media: Media,
    pub kind: InterfaceKind,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PortBinding {
    pub group: PortGroupId,
    pub index: u16,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum InterfaceKind {
    Routed {
        zone: ZoneId,
        address: Ipv4InterfaceAddress,
    },
    Access {
        domain: DomainId,
    },
    Trunk {
        domains: Vec<DomainId>,
    },
    Loopback {
        zone: ZoneId,
        address: Ipv4InterfaceAddress,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LinkEndpoint {
    pub device: DeviceId,
    pub interface: InterfaceId,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PhysicalLink {
    pub id: LinkId,
    pub from: LinkEndpoint,
    pub to: LinkEndpoint,
    pub bandwidth_mbps: u64,
    pub failure_domains: Vec<FailureDomainId>,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BundleKind {
    Lacp,
    Ecmp,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LinkBundle {
    pub id: BundleId,
    pub kind: BundleKind,
    pub links: Vec<LinkId>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Layer2Domain {
    pub id: DomainId,
    pub site: SiteId,
    pub zone: ZoneId,
    pub vlan: u16,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AddressAllocation {
    pub id: AllocationId,
    pub zone: ZoneId,
    pub subnet: Ipv4Prefix,
    pub purpose: AllocationPurpose,
    pub required_hosts: u32,
    pub reserved_hosts: u32,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AllocationPurpose {
    Endpoints,
    Transit,
    Loopback,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AbstractRoute {
    pub device: DeviceId,
    pub destination: Ipv4Prefix,
    pub next_hop: String,
    pub egress: InterfaceId,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PolicyBinding {
    pub policy: FlowPolicy,
    pub device: DeviceId,
    pub ingress: InterfaceId,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DesignDecision {
    pub id: DecisionId,
    pub component_ids: Vec<ComponentRef>,
    pub reason: String,
    pub rule_id: Option<String>,
    pub triggered_by: TriggerSource,
    pub overridable: bool,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TriggerSource {
    UserIntent,
    EngineDefault,
    Blueprint,
    Selection,
    Policy,
    Addressing,
}

/// Typed references keep a human explanation attached to the correct entity kind.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "type",
    content = "id",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ComponentRef {
    Site(SiteId),
    Zone(ZoneId),
    Device(DeviceId),
    Interface(InterfaceId),
    Link(LinkId),
    Domain(DomainId),
    Allocation(AllocationId),
    Bundle(BundleId),
    Policy(PolicyId),
}
