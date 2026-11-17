use labyrinth_engine::{catalog::load_yaml, model::*, selection::*};
fn catalog() -> labyrinth_engine::catalog::CatalogSnapshot {
    load_yaml(include_str!("../../catalog/archetypes/reference.yaml")).unwrap()
}
fn need() -> DeviceRequirement {
    DeviceRequirement {
        id: DeviceId::new("access-a").unwrap(),
        role: DeviceRole::AccessSwitch,
        required_features: vec![Feature::Switching, Feature::Vlan],
        peak_mbps: 1000,
        poe_watts: None,
        pinned_archetype: None,
        pinned_mode: None,
        ports: vec![
            PortDemand {
                id: PortGroupId::new("endpoints").unwrap(),
                count: 24,
                speed_mbps: 1000,
                media: Media::Copper,
                min_poe_watts: None,
            },
            PortDemand {
                id: PortGroupId::new("uplink").unwrap(),
                count: 2,
                speed_mbps: 10000,
                media: Media::Fiber,
                min_poe_watts: None,
            },
        ],
    }
}
#[test]
fn filters_real_ports_modes_and_records_rejections() {
    let set = filter_candidates(&need(), &catalog(), 20).unwrap();
    assert_eq!(set.candidates.len(), 2);
    assert!(set.rejections.iter().any(
        |r| r.archetype.as_str() == "edge-firewall" && r.reasons.contains(&RejectReason::Role)
    ));
    for c in set.candidates {
        assert_eq!(
            c.port_assignments
                .iter()
                .map(|p| u32::from(p.count))
                .sum::<u32>(),
            26
        );
    }
}
#[test]
fn missing_throughput_is_not_treated_as_infinite() {
    let text = include_str!("../../catalog/archetypes/reference.yaml")
        .replace("throughput_mbps: 10000", "throughput_mbps: null")
        .replace("throughput_mbps: 4000", "throughput_mbps: null");
    let set = filter_candidates(&need(), &load_yaml(&text).unwrap(), 20).unwrap();
    assert!(set.candidates.is_empty());
    assert!(
        set.rejections
            .iter()
            .any(|r| r.reasons.contains(&RejectReason::UnknownThroughput))
    );
}
#[test]
fn poe_and_pins_are_hard_constraints() {
    let mut n = need();
    n.poe_watts = Some(741);
    n.required_features.push(Feature::Poe);
    n.ports[0].min_poe_watts = Some(30);
    assert!(
        filter_candidates(&n, &catalog(), 20)
            .unwrap()
            .candidates
            .is_empty()
    );
    let mut n = need();
    n.pinned_archetype = Some(ArchetypeId::new("edge-firewall").unwrap());
    assert!(
        filter_candidates(&n, &catalog(), 20)
            .unwrap()
            .candidates
            .is_empty()
    );
}
#[test]
fn incompatible_ports_and_over_capacity_modes_reject() {
    let mut n = need();
    n.ports[0].count = 49;
    assert!(
        filter_candidates(&n, &catalog(), 20)
            .unwrap()
            .candidates
            .is_empty()
    );
    let mut n = need();
    n.ports[0].media = Media::Fiber;
    assert!(
        filter_candidates(&n, &catalog(), 20)
            .unwrap()
            .candidates
            .is_empty()
    );
    let mut n = need();
    n.peak_mbps = 5000;
    assert_eq!(
        filter_candidates(&n, &catalog(), 20)
            .unwrap()
            .candidates
            .len(),
        1
    );
}
#[test]
fn port_matching_handles_flexible_groups_without_greedy_false_rejection() {
    let text = include_str!("../../catalog/archetypes/reference.yaml")
        .replace("speeds_mbps: [1000]", "speeds_mbps: [1000, 10000]")
        .replace("count: 48", "count: 1")
        .replace("count: 4", "count: 1")
        .replace("poe_budget_watts: 740", "poe_budget_watts: 30")
        .replace("media: fiber", "media: copper");
    let c = load_yaml(&text).unwrap();
    let mut n = need();
    n.ports[0].count = 1;
    n.ports[0].speed_mbps = 10000;
    n.ports[1].count = 1;
    n.ports[1].speed_mbps = 1000;
    n.ports[1].media = Media::Copper;
    assert!(!filter_candidates(&n, &c, 20).unwrap().candidates.is_empty());
}
#[test]
fn shared_capacity_and_per_port_poe_are_not_ignored() {
    let text = include_str!("../../catalog/archetypes/reference.yaml").replacen(
        "shared_capacity_mbps: null",
        "shared_capacity_mbps: 500",
        1,
    );
    assert!(
        filter_candidates(&need(), &load_yaml(&text).unwrap(), 20)
            .unwrap()
            .candidates
            .is_empty()
    );
    let mut n = need();
    n.required_features.push(Feature::Poe);
    n.poe_watts = Some(100);
    n.ports[0].min_poe_watts = Some(60);
    assert!(
        filter_candidates(&n, &catalog(), 20)
            .unwrap()
            .candidates
            .is_empty()
    );
}
#[test]
fn repeat_filtering_is_identical_and_has_no_catalog_order_tiebreak() {
    let a = filter_candidates(&need(), &catalog(), 20).unwrap();
    let b = filter_candidates(&need(), &catalog(), 20).unwrap();
    assert_eq!(a, b);
}

#[test]
fn growth_rounds_up_once_and_overflow_is_a_structured_error() {
    assert_eq!(with_growth(101, 20).unwrap(), 122);
    assert_eq!(with_growth(1000, 20).unwrap(), 1200);
    assert_eq!(with_growth(7, 0).unwrap(), 7);
    assert_eq!(
        with_growth(u64::MAX, 100),
        Err(SelectionError::ArithmeticOverflow)
    );
}

#[test]
fn spare_headroom_is_a_soft_fraction_of_compatible_installed_ports() {
    let mut n = need();
    n.ports[0].count = 48;
    n.ports[1].count = 4;
    let set = filter_candidates(&n, &catalog(), 20).unwrap();
    assert!(
        !set.candidates.is_empty(),
        "soft headroom must not turn feasible hardware into hard rejection"
    );
    assert_eq!(set.candidates[0].spare_shortfall_ppm, 200_000);
}

#[test]
fn complete_assignment_prefers_peer_standardization_before_cost() {
    let text = include_str!("../../tests/fixtures/catalog/selection.yaml");
    let c = load_yaml(text).unwrap();
    let mut a = need();
    a.ports.truncate(1);
    a.ports[0].count = 24;
    a.peak_mbps = 100;
    let mut b = a.clone();
    b.id = DeviceId::new("access-b").unwrap();
    b.ports[0].count = 48;
    let request = SelectionRequest {
        devices: vec![a, b],
        connections: vec![],
        spare_port_target_percent: 20,
        max_states: 10000,
    };
    let result = assign(&request, &c).unwrap();
    assert!(
        result
            .devices
            .iter()
            .all(|d| d.candidate.archetype.as_str() == "large-access")
    );
    assert_eq!(result.score.standardization_penalty, 0);
    assert_eq!(result.score.relative_cost_sum, 6);
}

#[test]
fn assignment_is_input_order_invariant_and_search_budget_is_not_infeasibility() {
    let mut a = need();
    a.ports[0].count = 12;
    let mut b = a.clone();
    b.id = DeviceId::new("access-b").unwrap();
    let mut request = SelectionRequest {
        devices: vec![a, b],
        connections: vec![],
        spare_port_target_percent: 20,
        max_states: 10000,
    };
    let first = assign(&request, &catalog()).unwrap();
    request.devices.reverse();
    assert_eq!(first, assign(&request, &catalog()).unwrap());
    request.max_states = 1;
    assert!(matches!(
        assign(&request, &catalog()),
        Err(SelectionError::SearchExhausted { .. })
    ));
}

#[test]
fn link_requirements_cannot_connect_incompatible_or_overused_ports() {
    let a = need();
    let mut b = need();
    b.id = DeviceId::new("access-b").unwrap();
    b.ports[1].speed_mbps = 1000;
    let connection = ConnectionRequirement {
        id: LinkId::new("uplink").unwrap(),
        from: DemandEndpoint {
            device: a.id.clone(),
            demand: a.ports[1].id.clone(),
        },
        to: DemandEndpoint {
            device: b.id.clone(),
            demand: b.ports[1].id.clone(),
        },
    };
    let request = SelectionRequest {
        devices: vec![a, b],
        connections: vec![connection],
        spare_port_target_percent: 20,
        max_states: 10000,
    };
    assert!(matches!(
        assign(&request, &catalog()),
        Err(SelectionError::InvalidRequirement(_))
    ));
}

#[test]
fn cost_precedes_spare_headroom_and_headroom_precedes_id_ties() {
    let text = include_str!("../../tests/fixtures/catalog/selection.yaml");
    let mut n = need();
    n.ports.truncate(1);
    n.ports[0].count = 24;
    n.peak_mbps = 100;
    let request = SelectionRequest {
        devices: vec![n],
        connections: vec![],
        spare_port_target_percent: 20,
        max_states: 10000,
    };
    let cheap = assign(&request, &load_yaml(text).unwrap()).unwrap();
    assert_eq!(
        cheap.devices[0].candidate.archetype.as_str(),
        "small-access"
    );
    let equal_cost =
        load_yaml(&text.replace("relative_cost_tier: 3", "relative_cost_tier: 1")).unwrap();
    let spacious = assign(&request, &equal_cost).unwrap();
    assert_eq!(
        spacious.devices[0].candidate.archetype.as_str(),
        "large-access"
    );
}

#[test]
fn physical_port_matching_matches_an_exhaustive_small_reference() {
    use serde_json::json;
    for mask in 1u32..64 {
        let groups:Vec<_>=(0..3).filter_map(|i|{
            let bits=(mask>>(i*2))&3;if bits==0{return None;}
            let speeds:Vec<u64>=[(1,1000),(2,10000)].into_iter().filter(|(bit,_)|bits&bit!=0).map(|(_,s)|s).collect();
            Some(json!({"id":format!("group-{i}"),"count":1,"speeds_mbps":speeds,"media":"copper","poe_watts_per_port":null,"shared_capacity_mbps":null,"breakout":null}))
        }).collect();
        let expected = groups.iter().enumerate().any(|(i, a)| {
            groups.iter().enumerate().any(|(j, b)| {
                i != j
                    && a["speeds_mbps"].as_array().unwrap().contains(&json!(1000))
                    && b["speeds_mbps"].as_array().unwrap().contains(&json!(10000))
            })
        });
        let bundle = json!({"schema_version":"catalog_v1","version":"1.0.0","archetypes":[{"id":"ports","roles":["access_switch"],"relative_cost_tier":1,"features":["switching","vlan"],"poe_budget_watts":null,"port_groups":groups,"operating_modes":[{"id":"base","features":["switching","vlan"],"throughput_mbps":10000}],"reference_hints":[]}]});
        let c = load_yaml(&serde_saphyr::to_string(&bundle).unwrap()).unwrap();
        let mut n = need();
        n.ports[0].count = 1;
        n.ports[1].count = 1;
        n.ports[1].media = Media::Copper;
        assert_eq!(
            !filter_candidates(&n, &c, 20).unwrap().candidates.is_empty(),
            expected,
            "port capability mask {mask}"
        );
    }
}

#[test]
fn branch_and_bound_winner_matches_an_exhaustive_two_device_reference() {
    use std::collections::BTreeSet;
    let c = load_yaml(include_str!("../../tests/fixtures/catalog/selection.yaml")).unwrap();
    for count in [12, 24, 36, 48] {
        let mut a = need();
        a.ports.truncate(1);
        a.ports[0].count = 12;
        a.peak_mbps = 100;
        let mut b = a.clone();
        b.id = DeviceId::new("access-b").unwrap();
        b.ports[0].count = count;
        let ac = filter_candidates(&a, &c, 20).unwrap();
        let bc = filter_candidates(&b, &c, 20).unwrap();
        let expected = ac
            .candidates
            .iter()
            .flat_map(|x| bc.candidates.iter().map(move |y| (x, y)))
            .min_by_key(|(x, y)| {
                let diversity = BTreeSet::from([&x.archetype, &y.archetype]).len() - 1;
                (
                    diversity,
                    u64::from(x.cost_tier) + u64::from(y.cost_tier),
                    u64::from(x.spare_shortfall_ppm) + u64::from(y.spare_shortfall_ppm),
                    x.archetype.clone(),
                    x.mode.clone(),
                    y.archetype.clone(),
                    y.mode.clone(),
                )
            })
            .unwrap();
        let request = SelectionRequest {
            devices: vec![a, b],
            connections: vec![],
            spare_port_target_percent: 20,
            max_states: 10000,
        };
        let actual = assign(&request, &c).unwrap();
        assert_eq!(&actual.devices[0].candidate, expected.0);
        assert_eq!(&actual.devices[1].candidate, expected.1);
    }
}

#[test]
fn poe_feature_cannot_bypass_unknown_power_requirements() {
    let mut n = need();
    n.required_features.push(Feature::Poe);
    assert!(matches!(
        filter_candidates(&n, &catalog(), 20),
        Err(SelectionError::InvalidRequirement(_))
    ));
}
