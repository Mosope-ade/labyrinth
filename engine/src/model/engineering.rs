use super::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
/// Logical transport over explicitly declared, reciprocal WAN services. Not a cable.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WanConnection {
    pub id: LinkId,
    pub from: LinkEndpoint,
    pub to: LinkEndpoint,
    pub provider_failure_domain: FailureDomainId,
    pub bandwidth_mbps: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "type",
    content = "id",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ForwardingAdjacency {
    Internal(LinkId),
    Wan(LinkId),
    LocalGateway(InterfaceId),
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ForwardingEntry {
    pub device: DeviceId,
    pub destination: Ipv4Prefix,
    pub next_hop: Option<DeviceId>,
    pub via: ForwardingAdjacency,
}

use std::collections::BTreeSet;
#[derive(Debug, Clone, Default, Serialize, JsonSchema)]
pub struct FailureScenario {
    pub internal: Failures,
    pub wan_links: BTreeSet<LinkId>,
    pub interfaces: BTreeSet<InterfaceId>,
    pub providers: BTreeSet<FailureDomainId>,
    pub sites: BTreeSet<SiteId>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FlowPath {
    pub flow: FlowId,
    pub from_zone: ZoneId,
    pub to_zone: ZoneId,
    pub cohort: u16,
    pub sessions: u64,
    pub session_peak_mbps: u64,
    pub steered: bool,
    pub devices: Vec<DeviceId>,
    pub adjacencies: Vec<ForwardingAdjacency>,
    pub peak_mbps: u64,
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct RoutingSnapshot {
    pub routes: Vec<ForwardingEntry>,
    pub paths: Vec<FlowPath>,
    pub ownership: Vec<OwnershipState>,
    pub layer2: Vec<Layer2Forwarding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RoutingLimits {
    pub max_search_states: u64,
    pub max_paths_per_demand: usize,
    pub timeout_ms: u64,
}
impl Default for RoutingLimits {
    fn default() -> Self {
        Self {
            max_search_states: 100_000,
            max_paths_per_demand: 64,
            timeout_ms: 15_000,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CoreAssignment {
    pub site: SiteId,
    pub archetype: ArchetypeId,
    pub mode: ModeId,
}
#[derive(Debug, Clone, Default)]
pub(crate) struct PinnedConstraints {
    pub allocations: std::collections::BTreeMap<AllocationId, AddressAllocation>,
    pub vlans: std::collections::BTreeMap<(SiteId, ZoneId), u16>,
    pub cores: std::collections::BTreeMap<DeviceId, CoreAssignment>,
    pub access_counts: std::collections::BTreeMap<ZoneId, u64>,
}
