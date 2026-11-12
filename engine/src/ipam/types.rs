use crate::model::{AllocationId, AllocationPurpose, Ipv4Prefix, SiteId, ZoneId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::net::Ipv4Addr;

/// Counts are already sized by the caller. Allocation never reapplies growth.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubnetRequest {
    pub id: AllocationId,
    pub zone: ZoneId,
    pub purpose: AllocationPurpose,
    pub required_hosts: u32,
    pub reserved_hosts: u32,
    /// Exact prefix when a topology requires a particular link/host convention.
    pub prefix_len: Option<u8>,
    pub pinned: Option<Ipv4Prefix>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AddressRequest {
    pub pool: Ipv4Prefix,
    pub exclusions: Vec<Ipv4Prefix>,
    pub subnets: Vec<SubnetRequest>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct HostRange {
    pub first: Ipv4Addr,
    pub last: Ipv4Addr,
    pub count: u32,
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SubnetAssignment {
    pub allocation: crate::model::AddressAllocation,
    pub reserved: Option<HostRange>,
    pub hosts: Option<HostRange>,
    pub pinned: bool,
}
/// Constructed only by successful allocation; not routing/security verification.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct AddressPlan {
    pub(super) rules_version: String,
    pub(super) request: AddressRequest,
    pub(super) assignments: Vec<SubnetAssignment>,
}
impl AddressPlan {
    pub fn rules_version(&self) -> &str {
        &self.rules_version
    }
    pub fn request(&self) -> &AddressRequest {
        &self.request
    }
    pub fn assignments(&self) -> &[SubnetAssignment] {
        &self.assignments
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum AddressError {
    InvalidRequest {
        reason: String,
    },
    PinConflict {
        allocation: AllocationId,
        reason: String,
    },
    Exhausted {
        allocation: AllocationId,
        prefix_len: u8,
        pool: Ipv4Prefix,
    },
    VlanExhausted {
        site: SiteId,
    },
}
impl std::fmt::Display for AddressError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for AddressError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlanRequest {
    pub site: SiteId,
    pub zone: ZoneId,
    pub pinned: Option<u16>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct VlanAssignment {
    pub site: SiteId,
    pub zone: ZoneId,
    pub vlan: u16,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlanPolicy {
    #[schemars(range(min = 1, max = 4094))]
    pub first: u16,
    #[schemars(range(min = 1, max = 4094))]
    pub last: u16,
    pub excluded: Vec<u16>,
    pub reuse_across_sites: bool,
}
impl Default for VlanPolicy {
    fn default() -> Self {
        Self {
            first: 10,
            last: 4094,
            excluded: vec![],
            reuse_across_sites: true,
        }
    }
}
pub(super) fn invalid(reason: impl Into<String>) -> AddressError {
    AddressError::InvalidRequest {
        reason: reason.into(),
    }
}
