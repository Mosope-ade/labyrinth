use super::*;
use crate::{
    blueprints::stable_id,
    intent::{RedundancyTier, ResolvedIntent},
    ipam::*,
    model::*,
    selection::DemandEndpoint,
};
use std::collections::BTreeMap;

pub(crate) fn build(
    intent: &ResolvedIntent,
    selection: SelectedPlan,
    policy: &AddressingPolicy,
    catalog: &crate::catalog::CatalogSnapshot,
    pins: Option<&PinnedConstraints>,
) -> Result<AllocatedTopology, PlanningError> {
    let input = intent.data();
    let bp = &selection.blueprint;
    let placements: BTreeMap<_, _> = bp.placements.iter().map(|p| (&p.device, p)).collect();
    let mut purposes = BTreeMap::new();
    for p in &bp.access_populations {
        purposes.insert(
            DemandEndpoint {
                device: p.device.clone(),
                demand: PortGroupId::new("endpoints").expect("literal ID"),
            },
            PortPurpose::Endpoint {
                zone: p.zone.clone(),
            },
        );
    }
    for c in &bp.requirements.connections {
        for end in [&c.from, &c.to] {
            purposes.insert(end.clone(), PortPurpose::Internal { link: c.id.clone() });
        }
    }
    for e in &bp.external_attachments {
        purposes.insert(
            DemandEndpoint {
                device: e.device.clone(),
                demand: e.demand.clone(),
            },
            PortPurpose::Provider {
                failure_domain: e.provider_domain.clone(),
            },
        );
    }
    let mut ports = Vec::new();
    let mut ends = BTreeMap::new();
    for d in &selection.assignment.devices {
        for p in &d.candidate.port_assignments {
            let demand = DemandEndpoint {
                device: d.requirement.clone(),
                demand: p.demand.clone(),
            };
            for offset in 0..p.count {
                let index = p.first_index + offset;
                let interface = InterfaceId::new(stable_id(
                    "if",
                    &[d.requirement.as_str(), p.group.as_str()],
                    u64::from(index),
                ))
                .expect("derived ID");
                ends.entry(demand.clone())
                    .or_insert_with(Vec::new)
                    .push(LinkEndpoint {
                        device: d.requirement.clone(),
                        interface: interface.clone(),
                    });
                ports.push(BoundPort {
                    device: d.requirement.clone(),
                    interface,
                    port: PortBinding {
                        group: p.group.clone(),
                        index,
                    },
                    speed_mbps: p.speed_mbps,
                    media: p.media,
                    purpose: purposes[&demand].clone(),
                });
            }
        }
    }
    ports.sort_by(|a, b| a.interface.cmp(&b.interface));
    let by_interface: BTreeMap<_, _> = ports.iter().map(|p| (&p.interface, p)).collect();
    let links: Vec<_> = bp
        .requirements
        .connections
        .iter()
        .map(|c| PhysicalLink {
            id: c.id.clone(),
            from: ends[&c.from][0].clone(),
            to: ends[&c.to][0].clone(),
            bandwidth_mbps: by_interface[&ends[&c.from][0].interface].speed_mbps,
            failure_domains: vec![],
        })
        .collect();
    let infrastructure_zones: Vec<_> = {
        let mut sites: Vec<_> = input.sites.iter().collect();
        sites.sort_by_key(|s| &s.id);
        sites
            .into_iter()
            .map(|s| SecurityZone {
                site: s.id.clone(),
                id: ZoneId::new(stable_id("infra", &[s.id.as_str()], 0)).expect("derived ID"),
            })
            .collect()
    };
    if infrastructure_zones.iter().any(|z| {
        input
            .sites
            .iter()
            .flat_map(|s| &s.zones)
            .any(|u| u.id == z.id)
    }) {
        return Err(PlanningError::InvalidPolicy(
            "derived infrastructure zone ID collides with user zone".into(),
        ));
    }
    let infra: BTreeMap<_, _> = infrastructure_zones
        .iter()
        .map(|z| (&z.site, &z.id))
        .collect();
    let mut requests = Vec::new();
    let mut vlan_requests = Vec::new();
    for site in &input.sites {
        for zone in &site.zones {
            let count = bp
                .access_populations
                .iter()
                .filter(|p| p.zone == zone.id)
                .map(|p| p.endpoints)
                .sum::<u64>();
            let count = u32::try_from(count).map_err(|_| {
                PlanningError::InvalidPolicy(
                    "grown endpoint count exceeds IPv4 request range".into(),
                )
            })?;
            requests.push(SubnetRequest {
                id: endpoint_allocation(&zone.id),
                zone: zone.id.clone(),
                purpose: AllocationPurpose::Endpoints,
                required_hosts: count,
                reserved_hosts: u32::from(gateway_count(input.redundancy, policy)),
                prefix_len: None,
                pinned: None,
            });
            vlan_requests.push(VlanRequest {
                site: site.id.clone(),
                zone: zone.id.clone(),
                pinned: None,
            });
        }
    }
    let mut routed_links = Vec::new();
    for l in &links {
        // L2 and HA-control cables are explicitly declared by the blueprint.
        // Neither is an ordinary routed data link requiring a fabricated /31.
        if bp.layer2_domains.iter().any(|d| d.links.contains(&l.id))
            || bp.firewall_ha.iter().any(|h| h.sync_links.contains(&l.id))
        {
            continue;
        }
        let site = &placements[&l.from.device].site;
        let id = transit_allocation(&l.id);
        requests.push(SubnetRequest {
            id: id.clone(),
            zone: (*infra[site]).clone(),
            purpose: AllocationPurpose::Transit,
            required_hosts: 2,
            reserved_hosts: 0,
            prefix_len: Some(policy.transit_prefix_len),
            pinned: None,
        });
        routed_links.push((l, id));
    }
    if policy.allocate_loopbacks {
        for d in &selection.assignment.devices {
            requests.push(SubnetRequest {
                id: loopback_allocation(&d.requirement),
                zone: (*infra[&placements[&d.requirement].site]).clone(),
                purpose: AllocationPurpose::Loopback,
                required_hosts: 1,
                reserved_hosts: 0,
                prefix_len: Some(32),
                pinned: None,
            });
        }
    }
    if let Some(pins) = pins {
        for (id, old) in &pins.allocations {
            let Some(request) = requests.iter_mut().find(|r| &r.id == id) else {
                return Err(PlanningError::Selection(
                    crate::selection::SelectionError::PinConflict(format!(
                        "existing allocation {id} would be removed"
                    )),
                ));
            };
            if request.zone != old.zone {
                return Err(PlanningError::Selection(
                    crate::selection::SelectionError::PinConflict(format!(
                        "existing allocation {id} changes zone"
                    )),
                ));
            }
            request.pinned = Some(old.subnet.clone());
        }
        for request in &mut vlan_requests {
            request.pinned = pins
                .vlans
                .get(&(request.site.clone(), request.zone.clone()))
                .copied();
        }
    }
    let vlans = allocate_vlans(&vlan_requests, &policy.vlans)?;
    let addresses = allocate(&AddressRequest {
        pool: input.ipv4_pool.clone(),
        exclusions: input.excluded_ipv4.clone(),
        subnets: requests,
    })?;
    let allocations: BTreeMap<_, _> = addresses
        .assignments()
        .iter()
        .map(|a| (&a.allocation.id, a))
        .collect();
    let zones = vlans
        .into_iter()
        .map(|v| {
            let allocation = endpoint_allocation(&v.zone);
            let gateway_reservations = allocations[&allocation]
                .reserved
                .clone()
                .expect("positive reservation policy");
            ZoneAddressing {
                domain: DomainId::new(stable_id("domain", &[v.site.as_str(), v.zone.as_str()], 0))
                    .expect("derived ID"),
                site: v.site,
                zone: v.zone,
                vlan: v.vlan,
                allocation,
                gateway_reservations,
            }
        })
        .collect();
    let transits = routed_links
        .into_iter()
        .map(|(link, id)| {
            let a = allocations[&id];
            let hosts = a.hosts.as_ref().expect("two transit hosts");
            let len = a.allocation.subnet.net().prefix_len();
            TransitAddressing {
                link: link.id.clone(),
                allocation: id,
                from: Ipv4InterfaceAddress::new(format!("{}/{len}", hosts.first))
                    .expect("usable allocated host"),
                to: Ipv4InterfaceAddress::new(format!("{}/{len}", hosts.last))
                    .expect("usable allocated host"),
            }
        })
        .collect();
    let loopbacks = if policy.allocate_loopbacks {
        selection
            .assignment
            .devices
            .iter()
            .map(|d| {
                let id = loopback_allocation(&d.requirement);
                let host = allocations[&id]
                    .hosts
                    .as_ref()
                    .expect("loopback host")
                    .first;
                LoopbackAddressing {
                    device: d.requirement.clone(),
                    allocation: id,
                    address: Ipv4InterfaceAddress::new(format!("{host}/32"))
                        .expect("allocated /32"),
                }
            })
            .collect()
    } else {
        vec![]
    };
    Ok(AllocatedTopology {
        catalog: catalog.clone(),
        selection,
        policy: policy.clone(),
        ports,
        links,
        addresses,
        zones,
        infrastructure_zones,
        transits,
        loopbacks,
    })
}
fn endpoint_allocation(zone: &ZoneId) -> AllocationId {
    AllocationId::new(stable_id("subnet", &[zone.as_str()], 0)).expect("derived ID")
}
fn transit_allocation(link: &LinkId) -> AllocationId {
    AllocationId::new(stable_id("transit", &[link.as_str()], 0)).expect("derived ID")
}
fn loopback_allocation(device: &DeviceId) -> AllocationId {
    AllocationId::new(stable_id("loopback", &[device.as_str()], 0)).expect("derived ID")
}
fn gateway_count(tier: RedundancyTier, p: &AddressingPolicy) -> u16 {
    if matches!(
        tier,
        RedundancyTier::Tier2Device | RedundancyTier::Tier3Site
    ) {
        p.redundant_gateway_reservations
    } else {
        p.single_gateway_reservations
    }
}
