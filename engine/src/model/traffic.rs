use super::{FlowId, PolicyId, ZoneId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TrafficDemand {
    pub id: FlowId,
    pub from_zone: ZoneId,
    pub to_zone: ZoneId,
    pub peak_mbps: u64,
    pub distribution: Option<TrafficDistribution>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FlowPolicy {
    pub id: PolicyId,
    pub from_zone: ZoneId,
    pub to_zone: ZoneId,
    pub action: PolicyAction,
    pub service: ServiceMatch,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PolicyAction {
    Allow,
    Deny,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ServiceMatch {
    pub protocol: Protocol,
    pub destination_ports: Vec<PortRange>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    Any,
    Tcp,
    Udp,
    Icmp,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PortRange {
    pub start: u16,
    pub end: u16,
}

/// Session cohorts are simultaneous upper-bound reservations, not average rates.
/// Sessions remain indivisible even when a demand may use multiple paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum TrafficDistribution {
    SinglePath,
    Sessions {
        #[schemars(length(min = 1, max = 64))]
        cohorts: Vec<SessionCohort>,
        growth: Option<SessionGrowth>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SessionCohort {
    #[schemars(range(min = 1))]
    pub session_peak_mbps: u64,
    #[schemars(range(min = 1))]
    pub simultaneous_sessions: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SessionGrowth {
    Rates,
    Concurrency,
}
