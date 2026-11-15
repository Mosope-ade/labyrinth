use super::*;
use crate::{blueprints::stable_id, catalog::CatalogSnapshot, model::*};
use std::collections::{BTreeMap, BTreeSet};

/// Assemble baseline canonical entities. Empty routes/policies and unresolved
/// provider handoffs remain explicit; structural validation is not verification.
pub fn assemble(
    t: &AllocatedTopology,
    catalog: &CatalogSnapshot,
    policy: &LogicalPolicy,
) -> Result<ValidatedNetwork, PlanningError> {
    let forwarding = l2_forwarding(t, &Failures::default(), &policy.preferred_roots)?;
    if forwarding.iter().any(|s| !s.unreachable.is_empty()) {
        return Err(PlanningError::InvalidPolicy(
            "baseline L2 domain is disconnected".into(),
        ));
    }
    let owners = ha_ownership(t, &Failures::default(), policy, &BTreeMap::new())?;
    let mut kinds = BTreeMap::<InterfaceId, InterfaceKind>::new();
    let links: BTreeMap<_, _> = t.links.iter().map(|l| (&l.id, l)).collect();
    let roles: BTreeMap<_, _> = t
        .selection
        .assignment
        .devices
        .iter()
        .map(|d| (&d.requirement, d.role))
        .collect();
    for p in &t.ports {
        match &p.purpose {
            PortPurpose::Endpoint { zone } => {
                let z = t
                    .zones
                    .iter()
                    .find(|z| &z.zone == zone)
                    .expect("allocated zone");
                kinds.insert(
                    p.interface.clone(),
                    InterfaceKind::Access {
                        domain: z.domain.clone(),
                    },
                );
            }
            PortPurpose::Provider { failure_domain } => {
                kinds.insert(
                    p.interface.clone(),
                    InterfaceKind::Provider {
                        failure_domain: failure_domain.clone(),
                    },
                );
            }
            PortPurpose::Internal { .. } => (),
        }
    }
    let mut bundles = Vec::new();
    for z in &t.zones {
        let layer = t
            .selection
            .blueprint
            .layer2_domains
            .iter()
            .find(|d| d.zone == z.zone)
            .expect("zone requirement");
        let mut pairs: BTreeMap<(DeviceId, DeviceId), Vec<LinkId>> = BTreeMap::new();
        for id in &layer.links {
            let l = links[id];
            let mut pair = [l.from.device.clone(), l.to.device.clone()];
            pair.sort();
            pairs
                .entry((pair[0].clone(), pair[1].clone()))
                .or_default()
                .push(id.clone());
            for end in [&l.from, &l.to] {
                let kind = if roles[&end.device] == DeviceRole::Firewall {
                    InterfaceKind::Attachment {
                        domain: z.domain.clone(),
                    }
                } else {
                    InterfaceKind::Access {
                        domain: z.domain.clone(),
                    }
                };
                kinds.insert(end.interface.clone(), kind);
            }
        }
        for ((a, b), mut ids) in pairs {
            if ids.len() > 1 {
                ids.sort();
                bundles.push(LinkBundle {
                    id: BundleId::new(stable_id(
                        "lag",
                        &[z.domain.as_str(), a.as_str(), b.as_str()],
                        0,
                    ))
                    .expect("derived ID"),
                    kind: BundleKind::Lacp,
                    links: ids,
                });
            }
        }
    }
    for h in &t.selection.blueprint.firewall_ha {
        for id in &h.sync_links {
            let l = links[id];
            kinds.insert(
                l.from.interface.clone(),
                InterfaceKind::HaControl {
                    peer: l.to.device.clone(),
                },
            );
            kinds.insert(
                l.to.interface.clone(),
                InterfaceKind::HaControl {
                    peer: l.from.device.clone(),
                },
            );
        }
    }
    for tr in &t.transits {
        let l = links[&tr.link];
        let site = &t
            .selection
            .blueprint
            .placements
            .iter()
            .find(|p| p.device == l.from.device)
            .expect("placement")
            .site;
        let zone = t
            .infrastructure_zones
            .iter()
            .find(|z| &z.site == site)
            .expect("infra zone")
            .id
            .clone();
        kinds.insert(
            l.from.interface.clone(),
            InterfaceKind::Routed {
                zone: zone.clone(),
                address: tr.from.clone(),
            },
        );
        kinds.insert(
            l.to.interface.clone(),
            InterfaceKind::Routed {
                zone,
                address: tr.to.clone(),
            },
        );
    }
    let mut routed_pairs: BTreeMap<(DeviceId, DeviceId), Vec<LinkId>> = BTreeMap::new();
    for tr in &t.transits {
        let l = links[&tr.link];
        let mut pair = [l.from.device.clone(), l.to.device.clone()];
        pair.sort();
        routed_pairs
            .entry((pair[0].clone(), pair[1].clone()))
            .or_default()
            .push(l.id.clone());
    }
    for ((a, b), mut ids) in routed_pairs {
        if ids.len() > 1 {
            ids.sort();
            bundles.push(LinkBundle {
                id: BundleId::new(stable_id("ecmp", &[a.as_str(), b.as_str()], 0))
                    .expect("derived ID"),
                kind: BundleKind::Ecmp,
                links: ids,
            });
        }
    }
    let mut devices = Vec::new();
    for selected in &t.selection.assignment.devices {
        let placement = t
            .selection
            .blueprint
            .placements
            .iter()
            .find(|p| p.device == selected.requirement)
            .expect("placement");
        let mut interfaces = Vec::new();
        for p in t.ports.iter().filter(|p| p.device == selected.requirement) {
            interfaces.push(Interface {
                id: p.interface.clone(),
                port: Some(p.port.clone()),
                speed_mbps: Some(p.speed_mbps),
                media: p.media,
                kind: kinds.remove(&p.interface).expect("classified port"),
            });
        }
        for lb in t
            .loopbacks
            .iter()
            .filter(|l| l.device == selected.requirement)
        {
            interfaces.push(Interface {
                id: InterfaceId::new(stable_id("lo", &[lb.device.as_str()], 0))
                    .expect("derived ID"),
                port: None,
                speed_mbps: None,
                media: Media::Virtual,
                kind: InterfaceKind::Loopback {
                    zone: t
                        .infrastructure_zones
                        .iter()
                        .find(|z| z.site == placement.site)
                        .expect("infra zone")
                        .id
                        .clone(),
                    address: lb.address.clone(),
                },
            });
        }
        devices.push(Device {
            id: selected.requirement.clone(),
            site: placement.site.clone(),
            role: selected.role,
            archetype: selected.candidate.archetype.clone(),
            operating_mode: selected.candidate.mode.clone(),
            interfaces,
            failure_domains: vec![site_failure(&placement.site)],
        });
    }
    let mut gateways = Vec::new();
    for z in &t.zones {
        let mut members: Vec<_> = devices
            .iter_mut()
            .filter(|d| {
                d.role == DeviceRole::Firewall
                    && d.site == z.site
                    && t.selection
                        .blueprint
                        .placements
                        .iter()
                        .any(|p| p.device == d.id && p.zone.as_ref() == Some(&z.zone))
            })
            .collect();
        members.sort_by(|a, b| a.id.cmp(&b.id));
        let allocation = t
            .addresses
            .assignments()
            .iter()
            .find(|a| a.allocation.id == z.allocation)
            .expect("allocation");
        let prefix = allocation.allocation.subnet.net().prefix_len();
        let base = u32::from(z.gateway_reservations.first);
        let ha = members.len() == 2;
        let mut refs = Vec::new();
        for (index, d) in members.into_iter().enumerate() {
            let id = InterfaceId::new(stable_id("gw", &[d.id.as_str(), z.domain.as_str()], 0))
                .expect("derived ID");
            let address = Ipv4InterfaceAddress::new(format!(
                "{}/{prefix}",
                std::net::Ipv4Addr::from(base + u32::try_from(index).expect("two members"))
            ))
            .expect("reserved gateway");
            refs.push(GatewayMember {
                device: d.id.clone(),
                interface: id.clone(),
            });
            d.interfaces.push(Interface {
                id,
                port: None,
                speed_mbps: None,
                media: Media::Virtual,
                kind: InterfaceKind::Gateway {
                    domain: z.domain.clone(),
                    zone: z.zone.clone(),
                    address,
                },
            });
        }
        gateways.push(GatewayOwnership {
            domain: z.domain.clone(),
            members: refs,
            virtual_address: ha.then(|| {
                Ipv4InterfaceAddress::new(format!(
                    "{}/{prefix}",
                    std::net::Ipv4Addr::from(base + 2)
                ))
                .expect("reserved VIP")
            }),
            owner: owners
                .iter()
                .find(|o| o.zone == z.zone)
                .expect("owner state")
                .owner
                .clone(),
            sync_links: t
                .selection
                .blueprint
                .firewall_ha
                .iter()
                .find(|h| h.zone == z.zone)
                .map_or_else(Vec::new, |h| h.sync_links.clone()),
        });
    }
    let sites: Vec<_> = t
        .zones
        .iter()
        .map(|z| z.site.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|id| NetworkSite {
            failure_domains: vec![site_failure(&id)],
            id,
        })
        .collect();
    let mut zones: Vec<_> = t
        .zones
        .iter()
        .map(|z| SecurityZone {
            id: z.zone.clone(),
            site: z.site.clone(),
        })
        .chain(t.infrastructure_zones.iter().cloned())
        .collect();
    zones.sort_by(|a, b| a.id.cmp(&b.id));
    for d in &mut devices {
        d.interfaces.sort_by(|a, b| a.id.cmp(&b.id));
    }
    let mut network_links = t.links.clone();
    for l in &mut network_links {
        let site = &devices
            .iter()
            .find(|d| d.id == l.from.device)
            .expect("link device")
            .site;
        l.failure_domains = vec![site_failure(site)];
    }
    bundles.sort_by(|a, b| a.id.cmp(&b.id));
    let decisions=t.zones.iter().map(|z|DesignDecision{id:DecisionId::new(stable_id("decision",&[z.domain.as_str()],0)).expect("derived ID"),component_ids:vec![ComponentRef::Domain(z.domain.clone())],reason:"Abstract L2 steady-state tree; active/standby gateway ownership with no automatic preemption. Routes, policies and carrier reachability are not synthesized.".into(),rule_id:Some("logical_rules_v1".into()),triggered_by:TriggerSource::Blueprint,overridable:true}).collect();
    let n = NetworkDraft {
        schema_version: "network_v1".into(),
        catalog_version: t.selection.assignment.catalog_version.clone(),
        sites,
        zones,
        devices,
        links: network_links,
        link_bundles: bundles,
        domains: t
            .zones
            .iter()
            .map(|z| Layer2Domain {
                id: z.domain.clone(),
                site: z.site.clone(),
                zone: z.zone.clone(),
                vlan: z.vlan,
            })
            .collect(),
        allocations: t
            .addresses
            .assignments()
            .iter()
            .map(|a| a.allocation.clone())
            .collect(),
        routes: vec![],
        policies: vec![],
        decisions,
        layer2_forwarding: forwarding,
        gateways,
        wan_connections: vec![],
        forwarding_routes: vec![],
        traffic_paths: vec![],
    };
    n.validate(catalog).map_err(PlanningError::Canonical)
}
fn site_failure(id: &SiteId) -> FailureDomainId {
    FailureDomainId::new(stable_id("site", &[id.as_str()], 0)).expect("derived ID")
}
