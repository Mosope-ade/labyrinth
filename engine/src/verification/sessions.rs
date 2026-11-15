use super::types::*;
use crate::{
    intent::ResolvedIntent,
    model::*,
    routing::{self, FailureScenario, RoutingSnapshot},
    topology::AllocatedTopology,
};
use std::collections::{BTreeMap, BTreeSet};
pub(super) fn check(
    intent: &ResolvedIntent,
    t: &AllocatedTopology,
    wan: &[WanConnection],
    snapshot: &RoutingSnapshot,
    scenario: &FailureScenario,
) -> Vec<Counterexample> {
    let mut errors = Vec::new();
    let table = routing::RouteTable::new(&snapshot.routes).ok();
    let mut counts = BTreeMap::<(&FlowId, u16), u128>::new();
    let failed = routing::internal_failures(t, scenario);
    for p in &snapshot.paths {
        let Some(d) = intent.data().demands.iter().find(|d| d.id == p.flow) else {
            errors.push(failure(
                "unknown_traffic",
                p.flow.to_string(),
                "path has no declared demand",
            ));
            continue;
        };
        let expected =
            crate::selection::batches(d, intent.data().growth_percent).expect("resolved sizing");
        let batch = expected.iter().find(|b| b.cohort == p.cohort);
        if batch.is_none_or(|b| p.session_peak_mbps != b.session_peak_mbps)
            || p.sessions == 0
            || u128::from(p.peak_mbps) != u128::from(p.sessions) * u128::from(p.session_peak_mbps)
            || p.from_zone != d.from_zone
            || p.to_zone != d.to_zone
            || p.steered != matches!(d.distribution, Some(TrafficDistribution::Sessions { .. }))
        {
            errors.push(failure(
                "invalid_session_reservation",
                p.flow.to_string(),
                "session class, rate, count, distribution or aggregate reservation is inconsistent",
            ));
        }
        *counts.entry((&p.flow, p.cohort)).or_default() += u128::from(p.sessions);
        let owner = |zone: &ZoneId| {
            snapshot
                .ownership
                .iter()
                .find(|o| &o.zone == zone)
                .and_then(|o| o.owner.as_ref())
        };
        if p.devices.is_empty()
            || p.devices.first() != owner(&d.from_zone)
            || p.devices.last() != owner(&d.to_zone)
            || p.adjacencies.len() + 1 != p.devices.len()
            || p.devices.iter().collect::<BTreeSet<_>>().len() != p.devices.len()
        {
            errors.push(failure(
                "invalid_session_path",
                p.flow.to_string(),
                "path is not loop-free between the active zone gateways",
            ));
            continue;
        }
        for (i, via) in p.adjacencies.iter().enumerate() {
            let a = &p.devices[i];
            let b = &p.devices[i + 1];
            let connects = |from: &LinkEndpoint, to: &LinkEndpoint| {
                (&from.device == a && &to.device == b) || (&from.device == b && &to.device == a)
            };
            let valid = match via {
                ForwardingAdjacency::Internal(id) => {
                    t.transits().iter().any(|tr| &tr.link == id)
                        && t.links().iter().any(|l| {
                            &l.id == id && connects(&l.from, &l.to) && !failed.links.contains(id)
                        })
                }
                ForwardingAdjacency::Wan(id) => wan.iter().any(|w| {
                    &w.id == id
                        && connects(&w.from, &w.to)
                        && !scenario.wan_links.contains(id)
                        && !scenario.providers.contains(&w.provider_failure_domain)
                        && !scenario.interfaces.contains(&w.from.interface)
                        && !scenario.interfaces.contains(&w.to.interface)
                }),
                ForwardingAdjacency::LocalGateway(_) => false,
            };
            if !valid || failed.devices.contains(a) || failed.devices.contains(b) {
                errors.push(failure(
                    "invalid_session_hop",
                    p.flow.to_string(),
                    "path uses a failed or non-adjacent transport",
                ));
            }
        }
        for (i, device) in p.devices.iter().enumerate() {
            let selected = t
                .selection()
                .assignment
                .devices
                .iter()
                .find(|d| &d.requirement == device);
            if selected.is_none()
                || (i > 0
                    && i + 1 < p.devices.len()
                    && selected.is_some_and(|d| d.role == DeviceRole::Firewall))
            {
                errors.push(failure(
                    "invalid_transit_device",
                    device.to_string(),
                    "firewalls cannot transit unrelated zones",
                ));
            }
            if p.steered
                && !selected.is_some_and(|d| {
                    t.catalog()
                        .archetypes()
                        .iter()
                        .find(|a| a.id == d.candidate.archetype)
                        .and_then(|a| a.operating_modes.iter().find(|m| m.id == d.candidate.mode))
                        .is_some_and(|m| m.features.contains(&Feature::SessionSteering))
                })
            {
                errors.push(failure(
                    "missing_session_steering",
                    device.to_string(),
                    "selected mode lacks controlled session steering",
                ));
            }
        }
        if !p.steered {
            let dest = t
                .zones()
                .iter()
                .find(|z| z.zone == d.to_zone)
                .and_then(|z| {
                    t.addresses()
                        .assignments()
                        .iter()
                        .find(|a| a.allocation.id == z.allocation)
                })
                .expect("destination allocation");
            if !table.as_ref().is_some_and(|table| {
                table
                    .trace(
                        p.devices.first().expect("source"),
                        p.devices.last().expect("target"),
                        &dest.allocation.subnet,
                    )
                    .is_ok_and(|(devices, hops)| devices == p.devices && hops == p.adjacencies)
            }) {
                errors.push(failure(
                    "unsteered_path_mismatch",
                    p.flow.to_string(),
                    "single-path traffic differs from the destination table",
                ));
            }
        }
    }
    for d in &intent.data().demands {
        let excluded = t.zones().iter().any(|z| {
            (z.zone == d.from_zone || z.zone == d.to_zone) && scenario.sites.contains(&z.site)
        });
        for batch in
            crate::selection::batches(d, intent.data().growth_percent).expect("resolved sizing")
        {
            let count = counts.get(&(&d.id, batch.cohort)).copied().unwrap_or(0);
            let required = if excluded { 0 } else { u128::from(batch.count) };
            if count != required {
                errors.push(failure(
                    "session_conservation",
                    format!("{}/{}", d.id, batch.cohort),
                    format!("allocated {count} sessions; required {required}"),
                ));
            }
        }
    }
    errors.truncate(128);
    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (
        ResolvedIntent,
        AllocatedTopology,
        Vec<WanConnection>,
        RoutingSnapshot,
    ) {
        let crate::intent::Readiness::Ready(i) = crate::intent::parse_json(include_str!(
            "../../../tests/fixtures/intents/enterprise.json"
        ))
        .unwrap() else {
            panic!()
        };
        let c = crate::catalog::load_yaml(include_str!(
            "../../../tests/fixtures/catalog/blueprints.yaml"
        ))
        .unwrap();
        let p = crate::topology::LogicalPolicy::default();
        let t = crate::pipeline::allocate_topology(&i, &c, &Default::default(), 10000).unwrap();
        let wan = crate::routing::build_transport(&i, &t);
        let snapshot =
            crate::routing::synthesize(&i, &t, &wan, &p, &Default::default(), &Default::default())
                .unwrap();
        (*i, t, wan, snapshot)
    }
    #[test]
    fn independent_session_conservation_rejects_missing_or_altered_reservations() {
        let (i, t, wan, s) = setup();
        assert!(check(&i, &t, &wan, &s, &Default::default()).is_empty());
        let mut missing = s.clone();
        missing.paths.clear();
        assert!(
            check(&i, &t, &wan, &missing, &Default::default())
                .iter()
                .any(|e| e.code == "session_conservation")
        );
        let mut altered = s.clone();
        altered.paths[0].sessions += 1;
        assert!(
            check(&i, &t, &wan, &altered, &Default::default())
                .iter()
                .any(|e| e.code == "invalid_session_reservation")
        );
        let mut cycle = s;
        let first = cycle.paths[0].devices[0].clone();
        cycle.paths[0].devices.push(first);
        assert!(
            check(&i, &t, &wan, &cycle, &Default::default())
                .iter()
                .any(|e| e.code == "invalid_session_path")
        );
    }
}
