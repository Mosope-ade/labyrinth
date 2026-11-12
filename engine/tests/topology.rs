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
        .chain(bp.firewall_ha.iter().map(|h| &h.sync_link))
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
