use crate::diagnostics::Counterexample;
fn failure(code: &str, component: impl Into<String>, detail: impl Into<String>) -> Counterexample {
    Counterexample {
        code: code.into(),
        component: component.into(),
        detail: detail.into(),
        required: None,
        available: None,
    }
}
use crate::{
    catalog::CatalogSnapshot, intent::ResolvedIntent, model::*, topology::AllocatedTopology,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
type PortKey = (DeviceId, InterfaceId, bool);
#[derive(Default)]
struct Loads {
    ports: BTreeMap<PortKey, u128>,
    groups: BTreeMap<(DeviceId, PortGroupId), u128>,
    modes: BTreeMap<DeviceId, u128>,
    services: BTreeMap<(SiteId, FailureDomainId, bool), u128>,
}
impl Loads {
    fn io(
        &mut self,
        t: &AllocatedTopology,
        end: &LinkEndpoint,
        tx: bool,
        amount: u128,
        group: bool,
    ) {
        *self
            .ports
            .entry((end.device.clone(), end.interface.clone(), tx))
            .or_default() += amount;
        if group {
            let port = t
                .ports()
                .iter()
                .find(|p| p.interface == end.interface)
                .expect("bound physical interface");
            *self
                .groups
                .entry((end.device.clone(), port.port.group.clone()))
                .or_default() += amount;
        }
    }
    fn cable(
        &mut self,
        t: &AllocatedTopology,
        from: &LinkEndpoint,
        to: &LinkEndpoint,
        amount: u128,
    ) {
        self.io(t, from, true, amount, true);
        self.io(t, to, false, amount, true);
    }
}
fn violation(errors: &mut Vec<Counterexample>, component: String, required: u128, available: u128) {
    if required > available && errors.len() < 128 {
        errors.push(Counterexample {
            code: "capacity_exceeded".into(),
            component,
            detail: format!(
                "simultaneous required load {required} Mbps exceeds available {available} Mbps"
            ),
            required: Some(u64::try_from(required).unwrap_or(u64::MAX)),
            available: Some(u64::try_from(available).unwrap_or(u64::MAX)),
        });
    }
}
pub(crate) fn check(
    intent: &ResolvedIntent,
    catalog: &CatalogSnapshot,
    t: &AllocatedTopology,
    wan: &[WanConnection],
    snapshot: &RoutingSnapshot,
    scenario: &FailureScenario,
) -> Vec<Counterexample> {
    let f = crate::topology::effective_failures(t, scenario);
    let mut loads = Loads::default();
    let mut errors = Vec::new();
    let placement = |id: &DeviceId| {
        t.selection()
            .blueprint
            .placements
            .iter()
            .find(|p| &p.device == id)
            .expect("device placement")
            .site
            .clone()
    };
    for path in &snapshot.paths {
        let peak = u128::from(path.peak_mbps);
        for d in &path.devices {
            *loads.modes.entry(d.clone()).or_default() += peak;
        }
        for (i, hop) in path.adjacencies.iter().enumerate() {
            let sender = &path.devices[i];
            match hop {
                ForwardingAdjacency::Internal(id) => {
                    let l = t
                        .links()
                        .iter()
                        .find(|l| &l.id == id)
                        .expect("routed cable");
                    let (a, b) = if &l.from.device == sender {
                        (&l.from, &l.to)
                    } else {
                        (&l.to, &l.from)
                    };
                    loads.cable(t, a, b, peak);
                }
                ForwardingAdjacency::Wan(id) => {
                    let w = wan.iter().find(|w| &w.id == id).expect("WAN transport");
                    let (a, b) = if &w.from.device == sender {
                        (&w.from, &w.to)
                    } else {
                        (&w.to, &w.from)
                    };
                    loads.cable(t, a, b, peak);
                    *loads
                        .services
                        .entry((
                            placement(&a.device),
                            w.provider_failure_domain.clone(),
                            true,
                        ))
                        .or_default() += peak;
                    *loads
                        .services
                        .entry((
                            placement(&b.device),
                            w.provider_failure_domain.clone(),
                            false,
                        ))
                        .or_default() += peak;
                    violation(
                        &mut errors,
                        format!("wan_connection:{id}"),
                        peak,
                        u128::from(w.bandwidth_mbps),
                    );
                }
                ForwardingAdjacency::LocalGateway(_) => errors.push(failure(
                    "invalid_path",
                    "capacity",
                    "a local terminal cannot be a transport hop",
                )),
            }
        }
    }
    // Robust zone envelopes: unknown endpoint placement can concentrate traffic in
    // any surviving access population, bounded by its aggregate endpoint line rate.
    for zone in t
        .zones()
        .iter()
        .filter(|z| !scenario.sites.contains(&z.site))
    {
        let outgoing: u128 = snapshot
            .paths
            .iter()
            .filter(|p| {
                intent
                    .data()
                    .demands
                    .iter()
                    .any(|d| d.id == p.flow && d.from_zone == zone.zone)
            })
            .map(|p| u128::from(p.peak_mbps))
            .sum();
        let incoming: u128 = snapshot
            .paths
            .iter()
            .filter(|p| {
                intent
                    .data()
                    .demands
                    .iter()
                    .any(|d| d.id == p.flow && d.to_zone == zone.zone)
            })
            .map(|p| u128::from(p.peak_mbps))
            .sum();
        if outgoing == 0 && incoming == 0 {
            continue;
        }
        let Some(owner) = snapshot
            .ownership
            .iter()
            .find(|o| o.zone == zone.zone)
            .and_then(|o| o.owner.as_ref())
        else {
            continue;
        };
        let Some(state) = snapshot.layer2.iter().find(|s| s.domain == zone.domain) else {
            continue;
        };
        let populations: Vec<_> = t
            .selection()
            .blueprint
            .access_populations
            .iter()
            .filter(|p| p.zone == zone.zone && !f.devices.contains(&p.device))
            .collect();
        let speed = intent
            .data()
            .sites
            .iter()
            .flat_map(|s| &s.zones)
            .find(|z| z.id == zone.zone)
            .expect("intent zone")
            .endpoint_attachment
            .speed_mbps;
        let population_capacity: u128 = populations
            .iter()
            .map(|p| u128::from(p.endpoints) * u128::from(speed))
            .sum();
        violation(
            &mut errors,
            format!("zone_endpoint_egress:{}", zone.zone),
            outgoing,
            population_capacity,
        );
        violation(
            &mut errors,
            format!("zone_endpoint_ingress:{}", zone.zone),
            incoming,
            population_capacity,
        );
        for p in &populations {
            let max = (outgoing + incoming).min(u128::from(p.endpoints) * u128::from(speed));
            let value = loads.modes.entry(p.device.clone()).or_default();
            *value = (*value).max(max);
        }
        let mut pairs: BTreeMap<(DeviceId, DeviceId), Vec<&PhysicalLink>> = BTreeMap::new();
        for id in &state.forwarding {
            let l = t
                .links()
                .iter()
                .find(|l| &l.id == id)
                .expect("forwarding cable");
            let mut pair = [l.from.device.clone(), l.to.device.clone()];
            pair.sort();
            pairs
                .entry((pair[0].clone(), pair[1].clone()))
                .or_default()
                .push(l);
        }
        for (pair, members) in &pairs {
            let mut reached = BTreeSet::from([owner.clone()]);
            let mut queue = VecDeque::from([owner.clone()]);
            while let Some(d) = queue.pop_front() {
                for (a, b) in pairs.keys().filter(|p| *p != pair) {
                    let peer = if a == &d {
                        b
                    } else if b == &d {
                        a
                    } else {
                        continue;
                    };
                    if reached.insert(peer.clone()) {
                        queue.push_back(peer.clone());
                    }
                }
            }
            let cap: u128 = populations
                .iter()
                .filter(|p| !reached.contains(&p.device))
                .map(|p| u128::from(p.endpoints) * u128::from(speed))
                .sum();
            let out = outgoing.min(cap);
            let input = incoming.min(cap);
            let mut group_envelopes = BTreeSet::new();
            for l in members {
                let (gateway_side, access_side) = if reached.contains(&l.from.device) {
                    (&l.from, &l.to)
                } else {
                    (&l.to, &l.from)
                };
                for (end, tx, amount) in [
                    (access_side, true, out),
                    (gateway_side, false, out),
                    (gateway_side, true, input),
                    (access_side, false, input),
                ] {
                    loads.io(t, end, tx, amount, false);
                    let port = t
                        .ports()
                        .iter()
                        .find(|p| p.interface == end.interface)
                        .expect("physical interface");
                    if group_envelopes.insert((end.device.clone(), port.port.group.clone(), tx)) {
                        *loads
                            .groups
                            .entry((end.device.clone(), port.port.group.clone()))
                            .or_default() += amount;
                    }
                }
            }
        }
        for placement in t
            .selection()
            .blueprint
            .placements
            .iter()
            .filter(|p| p.zone.as_ref() == Some(&zone.zone) && !f.devices.contains(&p.device))
        {
            if t.selection().assignment.devices.iter().any(|d| {
                d.requirement == placement.device && d.role == DeviceRole::DistributionSwitch
            }) {
                let v = loads.modes.entry(placement.device.clone()).or_default();
                *v = (*v).max(outgoing + incoming);
            }
        }
        if let Some(ha) = t
            .selection()
            .blueprint
            .firewall_ha
            .iter()
            .find(|h| h.zone == zone.zone)
        {
            let live: Vec<_> = ha
                .devices
                .iter()
                .filter(|d| !f.devices.contains(*d))
                .collect();
            if live.len() == 2
                && let Some(l) = ha
                    .sync_links
                    .iter()
                    .filter(|id| !f.links.contains(*id))
                    .filter_map(|id| t.links().iter().find(|l| &l.id == id))
                    .min_by_key(|l| &l.id)
            {
                let (a, b) = if &l.from.device == owner {
                    (&l.from, &l.to)
                } else {
                    (&l.to, &l.from)
                };
                let amount = outgoing + incoming;
                loads.cable(t, a, b, amount);
                *loads.modes.entry(a.device.clone()).or_default() += amount;
                *loads.modes.entry(b.device.clone()).or_default() += amount;
            }
        }
    }
    for ((device, interface, tx), load) in &loads.ports {
        let p = t
            .ports()
            .iter()
            .find(|p| &p.device == device && &p.interface == interface)
            .expect("bound port");
        violation(
            &mut errors,
            format!("port:{interface}:{}", if *tx { "tx" } else { "rx" }),
            *load,
            u128::from(p.speed_mbps),
        );
    }
    for ((device, group), load) in &loads.groups {
        let d = t
            .selection()
            .assignment
            .devices
            .iter()
            .find(|d| &d.requirement == device)
            .expect("selected device");
        let archetype = catalog
            .archetypes()
            .iter()
            .find(|a| a.id == d.candidate.archetype)
            .expect("catalog match");
        let g = archetype
            .port_groups
            .iter()
            .find(|g| &g.id == group)
            .expect("catalog port group");
        if let Some(cap) = g.shared_capacity_mbps {
            violation(
                &mut errors,
                format!("shared_port_group:{device}/{group}"),
                *load,
                u128::from(cap),
            );
        }
    }
    for (device, load) in &loads.modes {
        let d = t
            .selection()
            .assignment
            .devices
            .iter()
            .find(|d| &d.requirement == device)
            .expect("selected device");
        let mode = catalog
            .archetypes()
            .iter()
            .find(|a| a.id == d.candidate.archetype)
            .and_then(|a| a.operating_modes.iter().find(|m| m.id == d.candidate.mode))
            .expect("catalog mode");
        match mode.throughput_mbps {
            Some(cap) => violation(
                &mut errors,
                format!("mode:{device}"),
                *load,
                u128::from(cap),
            ),
            None => errors.push(failure(
                "unknown_mode_capacity",
                device.to_string(),
                "unknown throughput cannot be verified",
            )),
        }
    }
    for ((site, provider, tx), load) in &loads.services {
        let service = intent
            .data()
            .wan_services
            .iter()
            .find(|w| &w.site == site && &w.provider_failure_domain == provider)
            .expect("explicit service");
        violation(
            &mut errors,
            format!(
                "wan_service:{site}/{provider}:{}",
                if *tx { "out" } else { "in" }
            ),
            *load,
            u128::from(service.committed_bandwidth_mbps),
        );
    }
    errors.truncate(128);
    errors
}
