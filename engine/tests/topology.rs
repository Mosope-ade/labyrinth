use labyrinth_engine::{
    catalog::load_yaml,
    intent::{PartialIntent, Readiness, resolve},
    ipam::AddressError,
    model::*,
    pipeline::{PlanningError, allocate_topology},
    topology::{AddressingPolicy, PortPurpose},
};
use serde_json::{Value, json};
fn input() -> Value {
    serde_json::from_str(include_str!("../../tests/fixtures/intents/enterprise.json")).unwrap()
}
fn run(
    v: Value,
    policy: &AddressingPolicy,
) -> Result<labyrinth_engine::topology::AllocatedTopology, PlanningError> {
    let Readiness::Ready(intent) =
        resolve(serde_json::from_value::<PartialIntent>(v).unwrap()).unwrap()
    else {
        panic!("fixture unresolved")
    };
    let c = load_yaml(include_str!("../../tests/fixtures/catalog/blueprints.yaml")).unwrap();
    allocate_topology(&intent, &c, policy, 10000)
}
#[test]
fn selection_to_concrete_ports_and_addresses_needs_no_ai_or_services() {
    let t = run(input(), &AddressingPolicy::default()).unwrap();
    assert_eq!(t.zones().len(), 2);
    assert_eq!(t.zones()[0].vlan, 10);
    assert_eq!(t.zones()[1].vlan, 10);
    assert!(t.zones().iter().all(|z| z.gateway_reservations.count == 3));
    assert_eq!(
        t.ports()
            .iter()
            .filter(|p| matches!(p.purpose, PortPurpose::Endpoint { .. }))
            .count(),
        180
    );
    assert_eq!(t.loopbacks().len(), t.selection().assignment.devices.len());
    assert_eq!(
        t.ports()
            .iter()
            .filter(|p| matches!(p.purpose, PortPurpose::Internal { .. }))
            .count(),
        t.links().len() * 2
    );
    let mut ports = std::collections::BTreeSet::new();
    let mut ids = std::collections::BTreeSet::new();
    for p in t.ports() {
        assert!(ports.insert((&p.device, &p.port.group, p.port.index)));
        assert!(ids.insert(&p.interface));
    }
    for l in t.links() {
        assert!(ids.contains(&l.from.interface) && ids.contains(&l.to.interface));
    }
    // Blueprint-declared L2 and HA links never receive fabricated routed /31s.
    assert!(t.transits().len() < t.links().len());
    assert!(
        t.transits()
            .iter()
            .all(|x| x.from.as_str().ends_with("/31") && x.to.as_str().ends_with("/31"))
    );
    let staff = t
        .addresses()
        .assignments()
        .iter()
        .find(|a| {
            a.allocation.zone.as_str() == "staff"
                && matches!(a.allocation.purpose, AllocationPurpose::Endpoints)
        })
        .unwrap();
    assert_eq!(staff.allocation.required_hosts, 120);
    assert_eq!(staff.allocation.reserved_hosts, 3);
}
#[test]
fn address_policy_is_configurable_and_persisted_with_the_plan() {
    let mut p = AddressingPolicy {
        transit_prefix_len: 30,
        single_gateway_reservations: 2,
        redundant_gateway_reservations: 5,
        allocate_loopbacks: false,
        ..Default::default()
    };
    p.vlans.first = 200;
    p.vlans.reuse_across_sites = false;
    let t = run(input(), &p).unwrap();
    assert_eq!(
        t.zones().iter().map(|z| z.vlan).collect::<Vec<_>>(),
        [200, 201]
    );
    assert!(t.zones().iter().all(|z| z.gateway_reservations.count == 5));
    assert!(t.loopbacks().is_empty());
    assert!(
        t.transits()
            .iter()
            .all(|x| x.from.as_str().ends_with("/30"))
    );
    let mut v = input();
    v["redundancy"] = json!("tier0_none");
    assert!(
        run(v, &p)
            .unwrap()
            .zones()
            .iter()
            .all(|z| z.gateway_reservations.count == 2)
    );
    p.transit_prefix_len = 32;
    assert!(matches!(
        run(input(), &p),
        Err(PlanningError::Addressing(
            AddressError::InvalidRequest { .. }
        ))
    ));
}
#[test]
fn site_order_and_zone_order_do_not_change_allocated_output() {
    let first = run(input(), &AddressingPolicy::default()).unwrap();
    let mut v = input();
    v["sites"].as_array_mut().unwrap().reverse();
    let second = run(v, &AddressingPolicy::default()).unwrap();
    assert_eq!(
        serde_json::to_value(first).unwrap(),
        serde_json::to_value(second).unwrap()
    );
}
#[test]
fn exhausted_pool_returns_structured_error_and_no_topology() {
    let mut v = input();
    v["ipv4_pool"] = json!("10.0.0.0/28");
    assert!(matches!(
        run(v, &AddressingPolicy::default()),
        Err(PlanningError::Addressing(AddressError::Exhausted { .. }))
    ));
}

#[test]
fn l2_and_ha_links_are_disjoint_from_routed_transits_and_exclusions_are_honored() {
    let mut v = input();
    v["excluded_ipv4"] = json!(["10.0.0.0/16"]);
    let t = run(v, &AddressingPolicy::default()).unwrap();
    let bp = &t.selection().blueprint;
    for link in bp
        .layer2_domains
        .iter()
        .flat_map(|d| &d.links)
        .chain(bp.firewall_ha.iter().flat_map(|h| &h.sync_links))
    {
        assert!(t.links().iter().any(|l| &l.id == link));
        assert!(t.transits().iter().all(|x| &x.link != link));
    }
    let exclusion = Ipv4Prefix::new("10.0.0.0/16").unwrap();
    for a in t.addresses().assignments() {
        assert!(!exclusion.overlaps(&a.allocation.subnet));
    }
    assert_eq!(t.addresses().request().exclusions, [exclusion]);
    assert_eq!(t.addresses().rules_version(), "addressing_rules_v1");
    let p = AddressingPolicy {
        redundant_gateway_reservations: 2,
        ..Default::default()
    };
    assert!(matches!(
        run(input(), &p),
        Err(PlanningError::Addressing(
            AddressError::InvalidRequest { .. }
        ))
    ));
}

#[test]
fn l2_tree_blocks_cycles_preserves_parallel_members_and_reconverges() {
    use labyrinth_engine::topology::{Failures, l2_forwarding};
    let t = run(input(), &AddressingPolicy::default()).unwrap();
    let baseline = l2_forwarding(&t, &Failures::default(), &Default::default()).unwrap();
    assert!(
        baseline
            .iter()
            .all(|s| s.unreachable.is_empty() && !s.blocked.is_empty())
    );
    for s in &baseline {
        let req = t
            .selection()
            .blueprint
            .layer2_domains
            .iter()
            .find(|d| {
                t.zones()
                    .iter()
                    .any(|z| z.domain == s.domain && z.zone == d.zone)
            })
            .unwrap();
        assert_eq!(s.forwarding.len() + s.blocked.len(), req.links.len());
    }
    let failed = baseline[0].root.clone().unwrap();
    let f = Failures {
        devices: [failed.clone()].into(),
        ..Default::default()
    };
    let next = l2_forwarding(&t, &f, &Default::default()).unwrap();
    assert_ne!(next[0].root.as_ref(), Some(&failed));
    // The firewall physically attached to the failed distribution is unreachable;
    // the other firewall and all dual-homed access switches remain reachable.
    assert_eq!(next[0].unreachable.len(), 1);
    assert_eq!(
        t.selection()
            .assignment
            .devices
            .iter()
            .find(|d| d.requirement == next[0].unreachable[0])
            .unwrap()
            .role,
        DeviceRole::Firewall
    );
    let mut v = input();
    v["redundancy"] = json!("tier1_link");
    let t = run(v, &AddressingPolicy::default()).unwrap();
    let s = l2_forwarding(&t, &Failures::default(), &Default::default()).unwrap();
    assert!(s.iter().all(|x| x.blocked.is_empty()));
    let f = Failures {
        links: [s[0].forwarding[0].clone()].into(),
        ..Default::default()
    };
    assert!(
        l2_forwarding(&t, &f, &Default::default())
            .unwrap()
            .iter()
            .all(|s| s.unreachable.is_empty())
    );
}

#[test]
fn conservative_ha_tracks_owner_failure_control_loss_and_no_preemption() {
    use labyrinth_engine::topology::{Failures, LogicalPolicy, OwnershipStatus, ha_ownership};
    let t = run(input(), &AddressingPolicy::default()).unwrap();
    let policy = LogicalPolicy::default();
    let initial = ha_ownership(&t, &Failures::default(), &policy, &Default::default()).unwrap();
    assert!(initial.iter().all(|s| s.status == OwnershipStatus::Active));
    let previous: std::collections::BTreeMap<_, _> = initial
        .iter()
        .map(|s| (s.zone.clone(), s.owner.clone().unwrap()))
        .collect();
    let pair = &t.selection().blueprint.firewall_ha[0];
    assert_eq!(pair.sync_links.len(), 2);
    let one = Failures {
        links: [pair.sync_links[0].clone()].into(),
        ..Default::default()
    };
    assert!(
        ha_ownership(&t, &one, &policy, &previous)
            .unwrap()
            .iter()
            .all(|s| s.status == OwnershipStatus::Active)
    );
    let both = Failures {
        links: pair.sync_links.iter().cloned().collect(),
        ..Default::default()
    };
    assert!(
        ha_ownership(&t, &both, &policy, &previous)
            .unwrap()
            .iter()
            .any(|s| s.status == OwnershipStatus::SyncLost && s.owner.is_none())
    );
    let old = previous[&pair.zone].clone();
    let failed = Failures {
        devices: [old.clone()].into(),
        ..Default::default()
    };
    let next = ha_ownership(&t, &failed, &policy, &previous).unwrap();
    let owner = next
        .iter()
        .find(|s| s.zone == pair.zone)
        .unwrap()
        .owner
        .clone()
        .unwrap();
    assert_ne!(owner, old);
    let previous = next
        .into_iter()
        .map(|s| (s.zone, s.owner.unwrap()))
        .collect();
    assert_eq!(
        ha_ownership(&t, &Failures::default(), &policy, &previous)
            .unwrap()
            .iter()
            .find(|s| s.zone == pair.zone)
            .unwrap()
            .owner
            .as_ref(),
        Some(&owner)
    );
}
#[test]
fn canonical_assembly_validates_all_tiers_and_rejects_forged_state() {
    use labyrinth_engine::topology::{LogicalPolicy, assemble};
    let c = load_yaml(include_str!("../../tests/fixtures/catalog/blueprints.yaml")).unwrap();
    for tier in ["tier0_none", "tier1_link", "tier2_device"] {
        let mut v = input();
        v["redundancy"] = json!(tier);
        let t = run(v, &AddressingPolicy::default()).unwrap();
        let n = assemble(&t, &c, &LogicalPolicy::default()).unwrap();
        let n = n.network();
        assert_eq!(n.devices.len(), t.selection().assignment.devices.len());
        assert_eq!(n.links.len(), t.links().len());
        assert!(n.routes.is_empty() && n.policies.is_empty());
        assert_eq!(n.gateways.len(), 2);
        assert!(
            n.devices
                .iter()
                .flat_map(|d| &d.interfaces)
                .any(|i| matches!(i.kind, InterfaceKind::Provider { .. }))
        );
        let encoded = serde_json::to_string(n).unwrap();
        let round: NetworkDraft = serde_json::from_str(&encoded).unwrap();
        round.validate(&c).unwrap();
        let mut bad = n.clone();
        bad.gateways[0].owner = Some(DeviceId::new("unknown").unwrap());
        assert!(bad.validate(&c).is_err());
        let mut bad = n.clone();
        bad.layer2_forwarding[0].forwarding.clear();
        assert!(bad.validate(&c).is_err());
    }
}

#[test]
fn canonical_validation_rejects_l2_cycles_and_gateway_addresses_outside_reservations() {
    use labyrinth_engine::topology::{LogicalPolicy, assemble};
    let c = load_yaml(include_str!("../../tests/fixtures/catalog/blueprints.yaml")).unwrap();
    let t = run(input(), &AddressingPolicy::default()).unwrap();
    let n = assemble(&t, &c, &LogicalPolicy::default()).unwrap();
    let mut cycle = n.network().clone();
    let blocked = std::mem::take(&mut cycle.layer2_forwarding[0].blocked);
    cycle.layer2_forwarding[0].forwarding.extend(blocked);
    assert!(cycle.validate(&c).is_err());
    let mut bad = n.network().clone();
    let member = bad.gateways[0].members[0].clone();
    let iface = bad
        .devices
        .iter_mut()
        .find(|d| d.id == member.device)
        .unwrap()
        .interfaces
        .iter_mut()
        .find(|i| i.id == member.interface)
        .unwrap();
    if let InterfaceKind::Gateway { address, .. } = &mut iface.kind {
        let (ip, prefix) = address.as_str().split_once('/').unwrap();
        let ip: u32 = ip.parse::<std::net::Ipv4Addr>().unwrap().into();
        *address =
            Ipv4InterfaceAddress::new(format!("{}/{prefix}", std::net::Ipv4Addr::from(ip + 10)))
                .unwrap();
    }
    assert!(
        bad.validate(&c).is_err(),
        "gateway cannot consume an endpoint host outside reserved space"
    );
}

#[test]
fn explicit_root_owner_preferences_and_unknown_failure_ids_are_validated() {
    use labyrinth_engine::topology::{
        Failures, LogicalPolicy, assemble, ha_ownership, l2_forwarding,
    };
    let t = run(input(), &AddressingPolicy::default()).unwrap();
    let mut p = LogicalPolicy::default();
    let z = &t.zones()[0];
    let dist =
        t.selection()
            .blueprint
            .placements
            .iter()
            .rev()
            .find(|x| {
                x.zone.as_ref() == Some(&z.zone)
                    && t.selection().assignment.devices.iter().any(|d| {
                        d.requirement == x.device && d.role == DeviceRole::DistributionSwitch
                    })
            })
            .unwrap();
    let fw = t
        .selection()
        .blueprint
        .firewall_ha
        .iter()
        .find(|h| h.zone == z.zone)
        .unwrap()
        .devices
        .last()
        .unwrap();
    p.preferred_roots
        .insert(z.zone.clone(), dist.device.clone());
    p.preferred_owners.insert(z.zone.clone(), fw.clone());
    assert_eq!(
        l2_forwarding(&t, &Failures::default(), &p.preferred_roots).unwrap()[0]
            .root
            .as_ref(),
        Some(&dist.device)
    );
    assert_eq!(
        ha_ownership(&t, &Failures::default(), &p, &Default::default()).unwrap()[0]
            .owner
            .as_ref(),
        Some(fw)
    );
    let c = load_yaml(include_str!("../../tests/fixtures/catalog/blueprints.yaml")).unwrap();
    assemble(&t, &c, &p).unwrap();
    p.preferred_owners
        .insert(z.zone.clone(), dist.device.clone());
    assert!(ha_ownership(&t, &Failures::default(), &p, &Default::default()).is_err());
    let f = Failures {
        devices: [DeviceId::new("unknown").unwrap()].into(),
        ..Default::default()
    };
    assert!(l2_forwarding(&t, &f, &Default::default()).is_err());
}
#[test]
fn canonical_assembly_is_order_invariant_and_supports_tier3_role_shape() {
    use labyrinth_engine::topology::{LogicalPolicy, assemble};
    let c = load_yaml(include_str!("../../tests/fixtures/catalog/blueprints.yaml")).unwrap();
    let first = run(input(), &AddressingPolicy::default()).unwrap();
    let a = assemble(&first, &c, &LogicalPolicy::default()).unwrap();
    let mut v = input();
    v["sites"].as_array_mut().unwrap().reverse();
    let second = run(v, &AddressingPolicy::default()).unwrap();
    let b = assemble(&second, &c, &LogicalPolicy::default()).unwrap();
    assert_eq!(
        serde_json::to_value(a.network()).unwrap(),
        serde_json::to_value(b.network()).unwrap()
    );
    let mut v = input();
    let mut second_hub = v["sites"][0].clone();
    second_hub["id"] = json!("hub-two");
    second_hub["location"] = json!("abuja");
    second_hub["zones"][0]["id"] = json!("backup");
    v["sites"].as_array_mut().unwrap().push(second_hub);
    for provider in ["isp-a", "isp-b"] {
        v["wan_services"].as_array_mut().unwrap().push(json!({"site":"hub-two","provider_failure_domain":provider,"committed_bandwidth_mbps":1000,"connected_sites":["hub","branch"]}));
        for attachment in v["wan_services"].as_array_mut().unwrap() {
            if attachment["site"] != "hub-two" && attachment["provider_failure_domain"] == provider
            {
                attachment["connected_sites"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!("hub-two"));
            }
        }
    }
    v["redundancy"] = json!("tier3_site");
    let t = run(v, &AddressingPolicy::default()).unwrap();
    let n = assemble(&t, &c, &LogicalPolicy::default()).unwrap();
    assert_eq!(n.network().sites.len(), 3);
}

#[test]
fn every_single_l2_cable_failure_preserves_dual_homed_access_in_the_abstract_tree() {
    use labyrinth_engine::topology::{Failures, l2_forwarding};
    let t = run(input(), &AddressingPolicy::default()).unwrap();
    for id in t
        .selection()
        .blueprint
        .layer2_domains
        .iter()
        .flat_map(|d| &d.links)
    {
        let failed = Failures {
            links: [id.clone()].into(),
            ..Default::default()
        };
        let states = l2_forwarding(&t, &failed, &Default::default()).unwrap();
        for state in states {
            assert!(!state.forwarding.contains(id) && !state.blocked.contains(id));
            assert!(state.unreachable.iter().all(|d| {
                !t.selection()
                    .assignment
                    .devices
                    .iter()
                    .any(|x| &x.requirement == d && x.role == DeviceRole::AccessSwitch)
            }));
        }
    }
}

#[test]
fn coordinated_path_health_failover_requires_sync_and_does_not_preempt_on_recovery() {
    use labyrinth_engine::topology::{Failures, LogicalPolicy, OwnershipStatus, ha_ownership};
    let t = run(input(), &AddressingPolicy::default()).unwrap();
    let policy = LogicalPolicy::default();
    let initial = ha_ownership(&t, &Failures::default(), &policy, &Default::default()).unwrap();
    let previous: std::collections::BTreeMap<_, _> = initial
        .iter()
        .map(|s| (s.zone.clone(), s.owner.clone().unwrap()))
        .collect();
    let pair = &t.selection().blueprint.firewall_ha[0];
    let old = &previous[&pair.zone];
    let layer = t
        .selection()
        .blueprint
        .layer2_domains
        .iter()
        .find(|d| d.zone == pair.zone)
        .unwrap();
    let attachment = t
        .links()
        .iter()
        .find(|l| layer.links.contains(&l.id) && (&l.from.device == old || &l.to.device == old))
        .unwrap();
    let distribution = if &attachment.from.device == old {
        attachment.to.device.clone()
    } else {
        attachment.from.device.clone()
    };
    let core_links: std::collections::BTreeSet<_> = t
        .transits()
        .iter()
        .filter_map(|tr| {
            t.links()
                .iter()
                .find(|l| l.id == tr.link && (&l.from.device == old || &l.to.device == old))
        })
        .map(|l| l.id.clone())
        .collect();
    assert!(!core_links.is_empty());
    for failed in [
        Failures {
            devices: [distribution].into(),
            ..Default::default()
        },
        Failures {
            links: [attachment.id.clone()].into(),
            ..Default::default()
        },
        Failures {
            links: core_links,
            ..Default::default()
        },
    ] {
        assert!(!failed.devices.contains(old));
        let result = ha_ownership(&t, &failed, &policy, &previous).unwrap();
        let s = result.iter().find(|s| s.zone == pair.zone).unwrap();
        assert_eq!(s.status, OwnershipStatus::Active);
        let owner = s.owner.as_ref().unwrap();
        assert_ne!(owner, old);
        let committed = result
            .iter()
            .filter_map(|s| s.owner.as_ref().map(|o| (s.zone.clone(), o.clone())))
            .collect();
        assert_eq!(
            ha_ownership(&t, &Failures::default(), &policy, &committed)
                .unwrap()
                .iter()
                .find(|s| s.zone == pair.zone)
                .unwrap()
                .owner
                .as_ref(),
            Some(owner)
        );
        let mut lost_sync = failed.clone();
        lost_sync.links.extend(pair.sync_links.iter().cloned());
        let states = ha_ownership(&t, &lost_sync, &policy, &previous).unwrap();
        let s = states.iter().find(|s| s.zone == pair.zone).unwrap();
        assert_eq!(s.status, OwnershipStatus::SyncLost);
        assert!(s.owner.is_none());
    }
    let all_data: std::collections::BTreeSet<_> = t
        .transits()
        .iter()
        .filter_map(|tr| {
            t.links().iter().find(|l| {
                l.id == tr.link
                    && (pair.devices.contains(&l.from.device)
                        || pair.devices.contains(&l.to.device))
            })
        })
        .map(|l| l.id.clone())
        .collect();
    let states = ha_ownership(
        &t,
        &Failures {
            links: all_data,
            ..Default::default()
        },
        &policy,
        &previous,
    )
    .unwrap();
    assert_eq!(
        states.iter().find(|s| s.zone == pair.zone).unwrap().status,
        OwnershipStatus::Unavailable
    );
}
