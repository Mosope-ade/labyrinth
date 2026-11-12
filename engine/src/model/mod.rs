//! Canonical vendor-neutral network contracts, not a forwarding verifier.
mod address;
mod ids;
pub use address::{Ipv4InterfaceAddress, Ipv4Prefix};
mod traffic;
pub use traffic::*;
mod logical;
mod network;
mod validate;
pub use ids::*;
pub use network::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DeviceRole {
    Router,
    CoreSwitch,
    DistributionSwitch,
    AccessSwitch,
    Firewall,
    WanEdge,
    Host,
    AccessPoint,
    ExternalNetwork,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Feature {
    Routing,
    Switching,
    Vlan,
    Ospf,
    Bgp,
    StaticRouting,
    Firewall,
    Ips,
    Vpn,
    Nat,
    Lacp,
    Ecmp,
    Poe,
    HighAvailability,
    RapidSpanningTree,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Media {
    Copper,
    Fiber,
    Virtual,
}

/// Bounded external draft JSON entrypoint; full five-pass verification is separate.
pub fn parse_json(
    input: &str,
    catalog: &crate::catalog::CatalogSnapshot,
) -> Result<ValidatedNetwork, crate::diagnostics::JsonInputError> {
    crate::diagnostics::decode_json::<NetworkDraft>(input)?
        .validate(catalog)
        .map_err(crate::diagnostics::JsonInputError::Invalid)
}
