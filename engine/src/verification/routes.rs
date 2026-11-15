use super::types::*;
use crate::{
    model::*,
    routing::{self, FailureScenario, RoutingSnapshot},
    topology::AllocatedTopology,
};
use std::collections::{BTreeMap, BTreeSet};
pub(super) fn check(
    t: &AllocatedTopology,
    wan: &[WanConnection],
    snapshot: &RoutingSnapshot,
    scenario: &FailureScenario,
) -> Vec<Counterexample> {
    let f = routing::internal_failures(t, scenario);
    let mut errors = Vec::new();
    let table = match routing::RouteTable::new(&snapshot.routes) {
        Ok(table) => table,
        Err(e) => return vec![failure("invalid_route_table", "routing", e.to_string())],
    };
    let mut keys = BTreeSet::new();
    for r in &snapshot.routes {
        if !keys.insert((&r.device, &r.destination)) {
            errors.push(failure(
                "duplicate_route",
                r.device.to_string(),
                "ambiguous destination next hop",
            ));
        }
        let endpoint = |id: &LinkId| t.links().iter().find(|l| &l.id == id);
        let good = match &r.via {
            ForwardingAdjacency::LocalGateway(_) => r.next_hop.is_none(),
            ForwardingAdjacency::Internal(id) => endpoint(id).is_some_and(|l| {
                !f.links.contains(id)
                    && !scenario.interfaces.contains(&l.from.interface)
                    && !scenario.interfaces.contains(&l.to.interface)
                    && !f.devices.contains(&l.from.device)
                    && !f.devices.contains(&l.to.device)
                    && ((l.from.device == r.device && r.next_hop.as_ref() == Some(&l.to.device))
                        || (l.to.device == r.device && r.next_hop.as_ref() == Some(&l.from.device)))
            }),
            ForwardingAdjacency::Wan(id) => wan.iter().find(|w| &w.id == id).is_some_and(|w| {
                !scenario.wan_links.contains(id)
                    && !scenario.providers.contains(&w.provider_failure_domain)
                    && !scenario.interfaces.contains(&w.from.interface)
                    && !scenario.interfaces.contains(&w.to.interface)
                    && !f.devices.contains(&w.from.device)
                    && !f.devices.contains(&w.to.device)
                    && ((w.from.device == r.device && r.next_hop.as_ref() == Some(&w.to.device))
                        || (w.to.device == r.device && r.next_hop.as_ref() == Some(&w.from.device)))
            }),
        };
        if !good {
            errors.push(failure(
                "invalid_next_hop",
                r.device.to_string(),
                "next hop is not a live adjacent forwarding peer",
            ));
        }
    }
    let owners: BTreeMap<_, _> = snapshot
        .ownership
        .iter()
        .filter_map(|o| o.owner.as_ref().map(|d| (&o.zone, d)))
        .collect();
    for target in t
        .zones()
        .iter()
        .filter(|z| !scenario.sites.contains(&z.site))
    {
        let Some(root) = owners.get(&target.zone) else {
            errors.push(failure(
                "missing_gateway",
                target.zone.to_string(),
                "surviving zone has no active gateway",
            ));
            continue;
        };
        let subnet = &t
            .addresses()
            .assignments()
            .iter()
            .find(|a| a.allocation.id == target.allocation)
            .expect("allocation")
            .allocation
            .subnet;
        for source in t
            .zones()
            .iter()
            .filter(|z| !scenario.sites.contains(&z.site))
        {
            let Some(from) = owners.get(&source.zone) else {
                continue;
            };
            if let Err(e) = table.trace(from, root, subnet) {
                errors.push(failure(
                    "unreachable_or_loop",
                    format!("{}->{}", source.zone, target.zone),
                    e.to_string(),
                ));
            }
        }
    }
    errors.truncate(128);
    errors
}
