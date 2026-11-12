use labyrinth_engine::{ipam::*, model::*};
fn prefix(s: &str) -> Ipv4Prefix {
    Ipv4Prefix::new(s).unwrap()
}
fn subnet(id: &str, hosts: u32, reserved: u32) -> SubnetRequest {
    SubnetRequest {
        id: AllocationId::new(id).unwrap(),
        zone: ZoneId::new("staff").unwrap(),
        purpose: AllocationPurpose::Endpoints,
        required_hosts: hosts,
        reserved_hosts: reserved,
        prefix_len: None,
        pinned: None,
    }
}
fn request(subnets: Vec<SubnetRequest>) -> AddressRequest {
    AddressRequest {
        pool: prefix("10.0.0.0/24"),
        exclusions: vec![],
        subnets,
    }
}
#[test]
fn largest_first_allocation_is_order_invariant_and_reserves_gateways() {
    let mut r = request(vec![subnet("small", 10, 1), subnet("large", 100, 3)]);
    let a = allocate(&r).unwrap();
    r.subnets.reverse();
    assert_eq!(
        serde_json::to_value(&a).unwrap(),
        serde_json::to_value(allocate(&r).unwrap()).unwrap()
    );
    let large = &a.assignments()[0];
    assert_eq!(large.allocation.id.as_str(), "large");
    assert_eq!(large.allocation.subnet.as_str(), "10.0.0.0/25");
    assert_eq!(
        large.reserved.as_ref().unwrap().last.to_string(),
        "10.0.0.3"
    );
    assert_eq!(large.hosts.as_ref().unwrap().first.to_string(), "10.0.0.4");
    assert_eq!(large.hosts.as_ref().unwrap().count, 100);
}
#[test]
fn exclusions_and_pins_are_reserved_before_free_allocations() {
    let mut r = request(vec![subnet("a", 30, 1), subnet("b", 20, 1)]);
    r.exclusions = vec![prefix("10.0.0.0/26"), prefix("10.0.0.32/27")];
    r.subnets[1].pinned = Some(prefix("10.0.0.64/27"));
    let p = allocate(&r).unwrap();
    assert_eq!(
        p.assignments()[0].allocation.subnet.as_str(),
        "10.0.0.128/26"
    );
    assert!(p.assignments()[1].pinned);
    r.subnets[1].pinned = Some(prefix("10.0.0.0/27"));
    assert!(matches!(
        allocate(&r),
        Err(AddressError::PinConflict { .. })
    ));
}
#[test]
fn point_to_point_and_loopback_semantics_and_top_of_ipv4_are_safe() {
    let mut link = subnet("link", 2, 0);
    link.purpose = AllocationPurpose::Transit;
    link.prefix_len = Some(31);
    let mut host = subnet("host", 1, 0);
    host.purpose = AllocationPurpose::Loopback;
    host.prefix_len = Some(32);
    let mut r = request(vec![link, host]);
    r.pool = prefix("255.255.255.252/30");
    let p = allocate(&r).unwrap();
    assert_eq!(
        p.assignments()[1].hosts.as_ref().unwrap().first.to_string(),
        "255.255.255.252"
    );
    assert_eq!(
        p.assignments()[0].allocation.subnet.as_str(),
        "255.255.255.254/32"
    );
}
#[test]
fn invalid_counts_prefixes_duplicates_and_pins_fail_without_partial_result() {
    assert!(allocate(&request(vec![subnet("a", 0, 0)])).is_err());
    let a = subnet("a", 10, 1);
    assert!(allocate(&request(vec![a.clone(), a])).is_err());
    let mut r = request(vec![subnet("a", 10, 1)]);
    r.subnets[0].prefix_len = Some(32);
    assert!(allocate(&r).is_err());
    r.subnets[0].prefix_len = None;
    r.subnets[0].pinned = Some(prefix("10.1.0.0/24"));
    assert!(matches!(
        allocate(&r),
        Err(AddressError::PinConflict { .. })
    ));
    r.subnets[0].pinned = Some(prefix("10.0.0.0/29"));
    assert!(matches!(
        allocate(&r),
        Err(AddressError::PinConflict { .. })
    ));
}
#[test]
fn full_pool_exclusion_and_fragmentation_report_exhaustion() {
    let mut r = request(vec![subnet("a", 100, 1)]);
    r.exclusions = vec![prefix("10.0.0.0/26"), prefix("10.0.0.128/26")];
    assert!(matches!(
        allocate(&r),
        Err(AddressError::Exhausted { prefix_len: 25, .. })
    ));
    r.exclusions = vec![prefix("0.0.0.0/0")];
    assert!(matches!(allocate(&r), Err(AddressError::Exhausted { .. })));
}
#[test]
fn vlan_scope_pins_and_configurable_ranges_are_enforced() {
    let make = |s, z, pinned| VlanRequest {
        site: SiteId::new(s).unwrap(),
        zone: ZoneId::new(z).unwrap(),
        pinned,
    };
    let mut r = vec![
        make("a", "one", None),
        make("a", "two", Some(10)),
        make("b", "three", None),
    ];
    let p = allocate_vlans(&r, &VlanPolicy::default()).unwrap();
    assert_eq!(p.iter().map(|v| v.vlan).collect::<Vec<_>>(), [11, 10, 10]);
    r.reverse();
    assert_eq!(p, allocate_vlans(&r, &VlanPolicy::default()).unwrap());
    let policy = VlanPolicy {
        first: 100,
        last: 102,
        excluded: vec![101],
        reuse_across_sites: false,
    };
    r[1].pinned = None;
    assert!(matches!(
        allocate_vlans(&r, &policy),
        Err(AddressError::VlanExhausted { .. })
    ));
}

#[test]
fn interval_allocator_matches_a_small_exhaustive_first_fit_reference() {
    // Enumerate every combination of four excluded /30 blocks in a /28.
    // Reference scans only this tiny fixture; production must never scan host space.
    for mask in 0u8..16 {
        for hosts in [1, 2, 5, 10] {
            let mut r = request(vec![subnet("test", hosts, 0)]);
            r.pool = prefix("10.0.0.0/28");
            r.exclusions = (0..4)
                .filter(|i| mask & (1 << i) != 0)
                .map(|i| prefix(&format!("10.0.0.{}/30", i * 4)))
                .collect();
            let block: u8 = if hosts <= 2 {
                4
            } else if hosts <= 6 {
                8
            } else {
                16
            };
            let expected = (0u8..16)
                .step_by(usize::from(block))
                .find(|start| (*start..*start + block).all(|ip| mask & (1 << (ip / 4)) == 0));
            match (expected, allocate(&r)) {
                (Some(start), Ok(p)) => assert_eq!(
                    p.assignments()[0].allocation.subnet.as_str(),
                    format!("10.0.0.{start}/{}", 32 - block.ilog2())
                ),
                (None, Err(AddressError::Exhausted { .. })) => (),
                (expected, actual) => {
                    panic!("mask={mask}, hosts={hosts}, expected={expected:?}, actual={actual:?}")
                }
            }
        }
    }
}
#[test]
fn pins_cannot_overlap_or_change_exact_prefix_conventions() {
    let mut a = subnet("a", 1, 0);
    a.pinned = Some(prefix("10.0.0.0/27"));
    let mut b = subnet("b", 1, 0);
    b.pinned = Some(prefix("10.0.0.16/28"));
    assert!(matches!(
        allocate(&request(vec![a, b])),
        Err(AddressError::PinConflict { .. })
    ));
    let mut a = subnet("a", 2, 0);
    a.purpose = AllocationPurpose::Transit;
    a.prefix_len = Some(31);
    a.pinned = Some(prefix("10.0.0.0/30"));
    assert!(matches!(
        allocate(&request(vec![a])),
        Err(AddressError::PinConflict { .. })
    ));
}
#[test]
fn ipv4_zero_boundary_and_maximum_counts_do_not_wrap() {
    let mut r = request(vec![subnet("a", u32::MAX - 1, 0)]);
    r.pool = prefix("0.0.0.0/0");
    let p = allocate(&r).unwrap();
    assert_eq!(p.assignments()[0].allocation.subnet.as_str(), "0.0.0.0/0");
    assert_eq!(
        p.assignments()[0].hosts.as_ref().unwrap().last.to_string(),
        "255.255.255.254"
    );
    r.subnets[0].reserved_hosts = 1;
    assert!(matches!(
        allocate(&r),
        Err(AddressError::InvalidRequest { .. })
    ));
    let mut tiny = subnet("tiny", 1, 0);
    tiny.purpose = AllocationPurpose::Loopback;
    let r = AddressRequest {
        pool: prefix("255.255.255.255/32"),
        exclusions: vec![],
        subnets: vec![tiny],
    };
    assert_eq!(
        allocate(&r).unwrap().assignments()[0]
            .hosts
            .as_ref()
            .unwrap()
            .first
            .to_string(),
        "255.255.255.255"
    );
}
#[test]
fn vlan_validation_rejects_duplicate_scopes_reserved_ids_and_conflicting_pins() {
    let make = |site, zone, pinned| VlanRequest {
        site: SiteId::new(site).unwrap(),
        zone: ZoneId::new(zone).unwrap(),
        pinned,
    };
    assert!(
        allocate_vlans(
            &[make("s", "a", None), make("s", "a", None)],
            &VlanPolicy::default()
        )
        .is_err()
    );
    assert!(
        allocate_vlans(
            &[make("s", "a", Some(10)), make("s", "b", Some(10))],
            &VlanPolicy::default()
        )
        .is_err()
    );
    assert!(allocate_vlans(&[make("s", "a", Some(0))], &VlanPolicy::default()).is_err());
    let p = VlanPolicy {
        excluded: vec![10],
        ..Default::default()
    };
    assert!(allocate_vlans(&[make("s", "a", Some(10))], &p).is_err());
    let p = VlanPolicy {
        reuse_across_sites: false,
        ..Default::default()
    };
    assert!(allocate_vlans(&[make("s", "a", Some(10)), make("t", "b", Some(10))], &p).is_err());
}
