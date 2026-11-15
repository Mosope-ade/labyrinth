//! Destination-based, deterministic abstract forwarding over confirmed transport.
mod sessions;
mod transport;
pub use crate::model::{FailureScenario, FlowPath, RoutingSnapshot};
pub use crate::selection::{SessionBatch, batches, reserved_peak};
use crate::{
    intent::ResolvedIntent,
    model::*,
    topology::{self, AllocatedTopology, LogicalPolicy, PlanningError},
};
use schemars::JsonSchema;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
pub use transport::build_transport;
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum RoutingError {
    Planning(PlanningError),
    Policy(crate::policy::PolicyError),
    UnknownFailure,
    SearchExhausted {
        states: u64,
        detail: String,
    },
    Timeout,
    InvalidLimits,
    UnsatisfiedTraffic {
        flow: FlowId,
        bottlenecks: Vec<crate::diagnostics::Counterexample>,
    },
    Unreachable {
        from: ZoneId,
        to: ZoneId,
    },
    InvalidRoute(String),
}
impl From<PlanningError> for RoutingError {
    fn from(e: PlanningError) -> Self {
        Self::Planning(e)
    }
}
impl From<crate::policy::PolicyError> for RoutingError {
    fn from(e: crate::policy::PolicyError) -> Self {
        Self::Policy(e)
    }
}
impl std::fmt::Display for RoutingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for RoutingError {}
pub use crate::topology::effective_failures as internal_failures;
/// Recompute steady-state forwarding, preserving committed HA owners. A route
/// decreases destination-tree distance at every hop. Firewalls are terminal zone
/// boundaries, never shortcuts for unrelated zone transit.
pub fn synthesize(
    intent: &ResolvedIntent,
    t: &AllocatedTopology,
    wan: &[WanConnection],
    policy: &LogicalPolicy,
    s: &FailureScenario,
    previous: &BTreeMap<ZoneId, DeviceId>,
) -> Result<RoutingSnapshot, RoutingError> {
    synthesize_scope(intent, t, wan, policy, s, previous, false)
}
pub(crate) fn synthesize_capacity(
    intent: &ResolvedIntent,
    t: &AllocatedTopology,
    wan: &[WanConnection],
    policy: &LogicalPolicy,
    s: &FailureScenario,
    previous: &BTreeMap<ZoneId, DeviceId>,
) -> Result<RoutingSnapshot, RoutingError> {
    synthesize_scope(intent, t, wan, policy, s, previous, true)
}
fn synthesize_scope(
    intent: &ResolvedIntent,
    t: &AllocatedTopology,
    wan: &[WanConnection],
    policy: &LogicalPolicy,
    s: &FailureScenario,
    previous: &BTreeMap<ZoneId, DeviceId>,
    demand_destinations_only: bool,
) -> Result<RoutingSnapshot, RoutingError> {
    crate::policy::validate_demands(intent)?;
    if s.interfaces
        .iter()
        .any(|id| !t.ports().iter().any(|p| &p.interface == id))
        || s.wan_links
            .iter()
            .any(|id| !wan.iter().any(|w| &w.id == id))
        || s.sites
            .iter()
            .any(|id| !intent.data().sites.iter().any(|site| &site.id == id))
        || s.providers.iter().any(|id| {
            !intent
                .data()
                .wan_services
                .iter()
                .any(|w| &w.provider_failure_domain == id)
        })
    {
        return Err(RoutingError::UnknownFailure);
    }
    let f = internal_failures(t, s);
    let ownership = topology::ha_ownership(t, &f, policy, previous)?;
    let layer2 = topology::l2_forwarding(t, &f, &policy.preferred_roots)?;
    let mut adjacent: BTreeMap<DeviceId, Vec<(DeviceId, ForwardingAdjacency)>> = BTreeMap::new();
    let mut add = |a: &DeviceId, b: &DeviceId, via: ForwardingAdjacency| {
        if !f.devices.contains(a) && !f.devices.contains(b) {
            adjacent
                .entry(a.clone())
                .or_default()
                .push((b.clone(), via.clone()));
            adjacent
                .entry(b.clone())
                .or_default()
                .push((a.clone(), via));
        }
    };
    for tr in t.transits() {
        let l = t
            .links()
            .iter()
            .find(|l| l.id == tr.link)
            .expect("transit cable");
        if !f.links.contains(&l.id)
            && !s.interfaces.contains(&l.from.interface)
            && !s.interfaces.contains(&l.to.interface)
        {
            add(
                &l.from.device,
                &l.to.device,
                ForwardingAdjacency::Internal(l.id.clone()),
            );
        }
    }
    for w in wan {
        if !s.wan_links.contains(&w.id)
            && !s.providers.contains(&w.provider_failure_domain)
            && !s.interfaces.contains(&w.from.interface)
            && !s.interfaces.contains(&w.to.interface)
        {
            add(
                &w.from.device,
                &w.to.device,
                ForwardingAdjacency::Wan(w.id.clone()),
            );
        }
    }
    for peers in adjacent.values_mut() {
        peers.sort();
    }
    let firewalls: BTreeSet<_> = t
        .selection()
        .assignment
        .devices
        .iter()
        .filter(|d| d.role == DeviceRole::Firewall)
        .map(|d| &d.requirement)
        .collect();
    let mut routes = Vec::new();
    for zone in t.zones() {
        if demand_destinations_only && !intent.data().demands.iter().any(|d| d.to_zone == zone.zone)
        {
            continue;
        }
        if s.sites.contains(&zone.site) {
            continue;
        }
        let Some(root) = ownership
            .iter()
            .find(|o| o.zone == zone.zone)
            .and_then(|o| o.owner.as_ref())
        else {
            continue;
        };
        let allocation = t
            .addresses()
            .assignments()
            .iter()
            .find(|a| a.allocation.id == zone.allocation)
            .expect("zone allocation");
        let destination = allocation.allocation.subnet.clone();
        routes.push(ForwardingEntry {
            device: root.clone(),
            destination: destination.clone(),
            next_hop: None,
            via: ForwardingAdjacency::LocalGateway(
                InterfaceId::new(crate::blueprints::stable_id(
                    "gw",
                    &[root.as_str(), zone.domain.as_str()],
                    0,
                ))
                .expect("derived ID"),
            ),
        });
        let mut visited = BTreeSet::from([root.clone()]);
        let mut queue = VecDeque::from([root.clone()]);
        while let Some(current) = queue.pop_front() {
            if &current != root && firewalls.contains(&current) {
                continue;
            }
            for (peer, via) in adjacent.get(&current).into_iter().flatten() {
                if visited.insert(peer.clone()) {
                    routes.push(ForwardingEntry {
                        device: peer.clone(),
                        destination: destination.clone(),
                        next_hop: Some(current.clone()),
                        via: via.clone(),
                    });
                    queue.push_back(peer.clone());
                }
            }
        }
    }
    routes.sort_by(|a, b| (&a.device, &a.destination).cmp(&(&b.device, &b.destination)));
    let mut paths = Vec::new();
    for demand in &intent.data().demands {
        let from_site = &t
            .zones()
            .iter()
            .find(|z| z.zone == demand.from_zone)
            .expect("source zone")
            .site;
        let to_site = &t
            .zones()
            .iter()
            .find(|z| z.zone == demand.to_zone)
            .expect("destination zone")
            .site;
        if s.sites.contains(from_site) || s.sites.contains(to_site) {
            continue;
        }
        let owner = |zone: &ZoneId| {
            ownership
                .iter()
                .find(|o| &o.zone == zone)
                .and_then(|o| o.owner.clone())
                .ok_or_else(|| RoutingError::Unreachable {
                    from: demand.from_zone.clone(),
                    to: demand.to_zone.clone(),
                })
        };
        let from = owner(&demand.from_zone)?;
        let to = owner(&demand.to_zone)?;
        let destination = &t
            .addresses()
            .assignments()
            .iter()
            .find(|a| {
                a.allocation.id
                    == t.zones()
                        .iter()
                        .find(|z| z.zone == demand.to_zone)
                        .expect("zone")
                        .allocation
            })
            .expect("allocation")
            .allocation
            .subnet;
        let (devices, adjacencies) = trace(&routes, &from, &to, destination)?;
        paths.push(FlowPath {
            flow: demand.id.clone(),
            from_zone: demand.from_zone.clone(),
            to_zone: demand.to_zone.clone(),
            cohort: 0,
            sessions: 1,
            session_peak_mbps: crate::selection::with_growth(
                demand.peak_mbps,
                intent.data().growth_percent,
            )
            .map_err(PlanningError::from)?,
            steered: false,
            devices,
            adjacencies,
            peak_mbps: crate::selection::with_growth(
                demand.peak_mbps,
                intent.data().growth_percent,
            )
            .map_err(PlanningError::from)?,
        });
    }
    paths.sort_by(|a, b| a.flow.cmp(&b.flow));
    let snapshot = RoutingSnapshot {
        routes,
        paths,
        ownership,
        layer2,
    };
    if intent
        .data()
        .demands
        .iter()
        .any(|d| matches!(d.distribution, Some(TrafficDistribution::Sessions { .. })))
    {
        sessions::pack(intent, t, wan, policy, s, snapshot, &adjacent)
    } else {
        Ok(snapshot)
    }
}
/// Independent route-table walk: reject loops, missing next hops or wrong terminals.
pub struct RouteTable<'a> {
    rows: BTreeMap<&'a DeviceId, BTreeMap<&'a Ipv4Prefix, &'a ForwardingEntry>>,
}
impl<'a> RouteTable<'a> {
    pub fn new(routes: &'a [ForwardingEntry]) -> Result<Self, RoutingError> {
        if routes.len() > 65536 {
            return Err(RoutingError::InvalidRoute(
                "route table admission exceeded".into(),
            ));
        }
        let mut rows: BTreeMap<&DeviceId, BTreeMap<&Ipv4Prefix, &ForwardingEntry>> =
            BTreeMap::new();
        for r in routes {
            if rows
                .entry(&r.device)
                .or_default()
                .insert(&r.destination, r)
                .is_some()
            {
                return Err(RoutingError::InvalidRoute(
                    "duplicate device/destination route".into(),
                ));
            }
        }
        Ok(Self { rows })
    }
    pub fn trace(
        &self,
        from: &DeviceId,
        to: &DeviceId,
        destination: &Ipv4Prefix,
    ) -> Result<(Vec<DeviceId>, Vec<ForwardingAdjacency>), RoutingError> {
        let mut current = from.clone();
        let mut devices = Vec::new();
        let mut adjacencies = Vec::new();
        let mut seen = BTreeSet::new();
        loop {
            if !seen.insert(current.clone()) {
                return Err(RoutingError::InvalidRoute("forwarding loop".into()));
            }
            devices.push(current.clone());
            let route = self
                .rows
                .get(&current)
                .and_then(|r| r.get(destination))
                .ok_or_else(|| RoutingError::InvalidRoute(format!("missing route at {current}")))?;
            if &current == to {
                if route.next_hop.is_none()
                    && matches!(route.via, ForwardingAdjacency::LocalGateway(_))
                {
                    return Ok((devices, adjacencies));
                }
                return Err(RoutingError::InvalidRoute(
                    "gateway route has nonlocal terminal".into(),
                ));
            }
            current = route
                .next_hop
                .clone()
                .ok_or_else(|| RoutingError::InvalidRoute("unexpected local terminal".into()))?;
            adjacencies.push(route.via.clone());
        }
    }
}
pub fn trace(
    routes: &[ForwardingEntry],
    from: &DeviceId,
    to: &DeviceId,
    destination: &Ipv4Prefix,
) -> Result<(Vec<DeviceId>, Vec<ForwardingAdjacency>), RoutingError> {
    RouteTable::new(routes)?.trace(from, to, destination)
}

impl RoutingError {
    pub(crate) fn resource_limit_code(&self) -> Option<&'static str> {
        match self {
            Self::Timeout => Some("solver_timeout"),
            Self::SearchExhausted { .. } => Some("routing_search_exhausted"),
            Self::InvalidLimits => Some("invalid_routing_limits"),
            _ => None,
        }
    }
}
