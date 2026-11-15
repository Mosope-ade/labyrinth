use super::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Layer2Forwarding {
    pub domain: DomainId,
    pub root: Option<DeviceId>,
    pub forwarding: Vec<LinkId>,
    pub blocked: Vec<LinkId>,
    pub unreachable: Vec<DeviceId>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GatewayMember {
    pub device: DeviceId,
    pub interface: InterfaceId,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GatewayOwnership {
    pub domain: DomainId,
    pub members: Vec<GatewayMember>,
    pub virtual_address: Option<Ipv4InterfaceAddress>,
    pub owner: Option<DeviceId>,
    pub sync_links: Vec<LinkId>,
}

/// Confirmed failures only; absence is not a health-discovery mechanism.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Failures {
    pub devices: BTreeSet<DeviceId>,
    pub links: BTreeSet<LinkId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OwnershipStatus {
    Active,
    SyncLost,
    Unavailable,
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct OwnershipState {
    pub zone: ZoneId,
    pub owner: Option<DeviceId>,
    pub status: OwnershipStatus,
}
