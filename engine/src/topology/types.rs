use crate::{
    blueprints::BlueprintPlan,
    ipam::*,
    model::*,
    selection::{Assignment, Bottleneck, SelectionError},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Defaults for the first blueprint, supplied explicitly to topology planning.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AddressingPolicy {
    #[schemars(range(min = 1))]
    pub single_gateway_reservations: u16,
    #[schemars(range(min = 1))]
    pub redundant_gateway_reservations: u16,
    #[schemars(range(max = 31))]
    pub transit_prefix_len: u8,
    pub allocate_loopbacks: bool,
    pub vlans: VlanPolicy,
}
impl Default for AddressingPolicy {
    fn default() -> Self {
        Self {
            single_gateway_reservations: 1,
            redundant_gateway_reservations: 3,
            transit_prefix_len: 31,
            allocate_loopbacks: true,
            vlans: VlanPolicy::default(),
        }
    }
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PortPurpose {
    Endpoint { zone: ZoneId },
    Internal { link: LinkId },
    Provider { failure_domain: FailureDomainId },
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct BoundPort {
    pub device: DeviceId,
    pub interface: InterfaceId,
    pub port: PortBinding,
    pub speed_mbps: u64,
    pub media: Media,
    pub purpose: PortPurpose,
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ZoneAddressing {
    pub site: SiteId,
    pub zone: ZoneId,
    pub domain: DomainId,
    pub vlan: u16,
    pub allocation: AllocationId,
    /// Reserved addresses only. HA ownership/forwarding is a later logical stage.
    pub gateway_reservations: HostRange,
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct TransitAddressing {
    pub link: LinkId,
    pub allocation: AllocationId,
    pub from: Ipv4InterfaceAddress,
    pub to: Ipv4InterfaceAddress,
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct LoopbackAddressing {
    pub device: DeviceId,
    pub allocation: AllocationId,
    pub address: Ipv4InterfaceAddress,
}
/// Physical connections and allocated addresses; not a complete canonical network.
/// No public constructor or Deserialize: pipeline construction owns consistency.
///
/// ```compile_fail
/// use labyrinth_engine::topology::AllocatedTopology;
/// let _: AllocatedTopology = serde_json::from_str("{}").unwrap();
/// ```
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct AllocatedTopology {
    pub(super) selection: SelectedPlan,
    pub(super) policy: AddressingPolicy,
    pub(super) ports: Vec<BoundPort>,
    pub(super) links: Vec<PhysicalLink>,
    pub(super) addresses: AddressPlan,
    pub(super) zones: Vec<ZoneAddressing>,
    pub(super) infrastructure_zones: Vec<SecurityZone>,
    pub(super) transits: Vec<TransitAddressing>,
    pub(super) loopbacks: Vec<LoopbackAddressing>,
}
impl AllocatedTopology {
    pub fn selection(&self) -> &SelectedPlan {
        &self.selection
    }
    pub fn policy(&self) -> &AddressingPolicy {
        &self.policy
    }
    pub fn ports(&self) -> &[BoundPort] {
        &self.ports
    }
    pub fn links(&self) -> &[PhysicalLink] {
        &self.links
    }
    pub fn addresses(&self) -> &AddressPlan {
        &self.addresses
    }
    pub fn zones(&self) -> &[ZoneAddressing] {
        &self.zones
    }
    pub fn infrastructure_zones(&self) -> &[SecurityZone] {
        &self.infrastructure_zones
    }
    pub fn transits(&self) -> &[TransitAddressing] {
        &self.transits
    }
    pub fn loopbacks(&self) -> &[LoopbackAddressing] {
        &self.loopbacks
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct RepairAttempt {
    pub pass: u8,
    pub outcome: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct SelectedPlan {
    pub blueprint: BlueprintPlan,
    pub assignment: Assignment,
    pub attempts: Vec<RepairAttempt>,
    pub repair_bottlenecks: Vec<Bottleneck>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum PlanningError {
    Selection(SelectionError),
    Addressing(crate::ipam::AddressError),
    InvalidPolicy(String),
}
impl From<SelectionError> for PlanningError {
    fn from(e: SelectionError) -> Self {
        Self::Selection(e)
    }
}
impl From<crate::ipam::AddressError> for PlanningError {
    fn from(e: crate::ipam::AddressError) -> Self {
        Self::Addressing(e)
    }
}
impl std::fmt::Display for PlanningError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for PlanningError {}
