use labyrinth_engine::{
    blueprints,
    catalog::load_yaml,
    intent::{PartialIntent, Readiness, resolve},
    model::*,
    pipeline::select_devices,
    selection::SelectionError,
};
use serde_json::{Value, json};
fn input() -> Value {
    serde_json::from_str(include_str!("../../tests/fixtures/intents/enterprise.json")).unwrap()
}
fn resolved(v: Value) -> labyrinth_engine::intent::ResolvedIntent {
    let Readiness::Ready(r) = resolve(serde_json::from_value::<PartialIntent>(v).unwrap()).unwrap()
    else {
        panic!("fixture not ready")
    };
    *r
}
fn catalog() -> labyrinth_engine::catalog::CatalogSnapshot {
    load_yaml(include_str!("../../tests/fixtures/catalog/blueprints.yaml")).unwrap()
}
#[test]
fn hub_spoke_maps_roles_and_growth_without_generation_claims() {
    let p = blueprints::plan(&resolved(input()), &catalog(), 1).unwrap();
    assert_eq!(
        p.blueprint,
        labyrinth_engine::intent::BlueprintChoice::HubSpoke
    );
    assert!(
        p.requirements
            .devices
            .iter()
            .any(|d| d.role == DeviceRole::CoreSwitch)
    );
    assert!(
        p.requirements
            .devices
            .iter()
            .any(|d| d.role == DeviceRole::Firewall)
    );
    assert_eq!(
        p.access_populations
            .iter()
            .find(|a| a.zone.as_str() == "staff")
            .unwrap()
            .endpoints,
        120
    );
    assert!(
        p.requirements
            .devices
            .iter()
            .filter(|d| d.role == DeviceRole::Firewall)
            .all(|d| d.peak_mbps == 240)
    );
    assert!(!p.decisions.is_empty());
}
#[test]
fn explicit_unsupported_blueprints_and_missing_hub_are_not_silently_replaced() {
    let mut v = input();
    v["blueprint"] = json!("spine_leaf");
    assert!(matches!(
        blueprints::plan(&resolved(v), &catalog(), 1),
        Err(SelectionError::UnsupportedBlueprint(_))
    ));
    let mut v = input();
    v["sites"][0]["role"] = json!("other");
    assert!(matches!(
        blueprints::plan(&resolved(v), &catalog(), 1),
        Err(SelectionError::ClarificationRequired(_))
    ));
}
#[test]
fn automatic_density_rule_is_explicit_and_overridable() {
    let mut v = input();
    v["sites"].as_array_mut().unwrap().truncate(1);
    v["wan_services"] = json!([]);
    v["demands"] = json!([]);
    v["policies"] = json!([]);
    v["sites"][0]["zones"][0]["endpoint_count"] = json!(600);
    assert!(matches!(
        blueprints::plan(&resolved(v.clone()), &catalog(), 1),
        Err(SelectionError::UnsupportedBlueprint(_))
    ));
    v["blueprint"] = json!("hub_spoke");
    assert!(blueprints::plan(&resolved(v), &catalog(), 3).is_ok());
}
#[test]
fn bounded_horizontal_repair_selects_without_claiming_infeasibility() {
    let result = select_devices(&resolved(input()), &catalog(), 10000).unwrap();
    assert_eq!(result.attempts.len(), 3);
    assert_eq!(result.blueprint.pass, 3);
    assert!(
        result
            .assignment
            .devices
            .iter()
            .any(|d| d.role == DeviceRole::AccessSwitch)
    );
    let c = load_yaml(
        &include_str!("../../tests/fixtures/catalog/blueprints.yaml")
            .replace("count: 48", "count: 1"),
    )
    .unwrap();
    assert!(matches!(
        select_devices(&resolved(input()), &c, 10000),
        Err(SelectionError::RepairExhausted { passes: 3, .. })
    ));
}
#[test]
fn failure_tiers_change_required_redundant_roles() {
    let mut v = input();
    v["redundancy"] = json!("tier0_none");
    let a = blueprints::plan(&resolved(v), &catalog(), 1).unwrap();
    let b = blueprints::plan(&resolved(input()), &catalog(), 1).unwrap();
    assert_eq!(
        a.requirements
            .devices
            .iter()
            .filter(|d| d.role == DeviceRole::CoreSwitch)
            .count(),
        2
    );
    assert_eq!(
        b.requirements
            .devices
            .iter()
            .filter(|d| d.role == DeviceRole::CoreSwitch)
            .count(),
        4
    );
    assert_eq!(
        b.requirements
            .devices
            .iter()
            .filter(|d| d.role == DeviceRole::Firewall)
            .count(),
        4
    );
}
#[test]
fn catalog_and_intent_order_do_not_decide_role_ids_or_winners() {
    let first = select_devices(&resolved(input()), &catalog(), 10000).unwrap();
    let mut v = input();
    v["sites"].as_array_mut().unwrap().reverse();
    let second = select_devices(&resolved(v), &catalog(), 10000).unwrap();
    assert_eq!(first, second);
}
#[test]
fn pinned_updates_need_prior_context_not_silent_redesign() {
    let mut v = input();
    v["update"] = json!({"mode":"incremental_update","base_revision":"revision-a","ipam_pins":[{"zone":"staff","subnet":"10.0.0.0/24"}],"core_pins":[{"device":"core-a","site":"hub","archetype":"core-switch"}]});
    assert!(matches!(
        blueprints::plan(&resolved(v), &catalog(), 1),
        Err(SelectionError::PinConflict(_))
    ));
}
#[test]
fn global_search_limit_is_not_hidden_by_repair() {
    assert!(matches!(
        select_devices(&resolved(input()), &catalog(), 1),
        Err(SelectionError::SearchExhausted { .. })
    ));
}

#[test]
fn zone_distribution_and_control_links_are_explicit_requirements() {
    let p = blueprints::plan(&resolved(input()), &catalog(), 3).unwrap();
    assert_eq!(p.rules_version, "blueprint_rules_v3");
    assert_eq!(
        p.requirements
            .devices
            .iter()
            .filter(|d| d.role == DeviceRole::DistributionSwitch)
            .count(),
        4
    );
    assert_eq!(p.layer2_domains.len(), 2);
    assert_eq!(p.firewall_ha.len(), 2);
    assert!(
        p.layer2_domains
            .iter()
            .all(|d| d.loop_prevention == Some(Feature::RapidSpanningTree))
    );
    for d in &p.requirements.devices {
        if matches!(
            d.role,
            DeviceRole::AccessSwitch | DeviceRole::DistributionSwitch
        ) {
            assert!(d.required_features.contains(&Feature::RapidSpanningTree));
        }
    }
    let roles: std::collections::BTreeMap<_, _> = p
        .requirements
        .devices
        .iter()
        .map(|d| (&d.id, d.role))
        .collect();
    assert!(!p.requirements.connections.iter().any(|c| matches!(
        (roles[&c.from.device], roles[&c.to.device]),
        (DeviceRole::AccessSwitch, DeviceRole::Firewall)
            | (DeviceRole::Firewall, DeviceRole::AccessSwitch)
    )));
    for h in &p.firewall_ha {
        assert_eq!(h.devices.len(), 2);
        assert!(
            p.requirements
                .connections
                .iter()
                .any(|l| h.sync_links.contains(&l.id))
        );
        assert!(
            p.layer2_domains
                .iter()
                .all(|d| h.sync_links.iter().all(|id| !d.links.contains(id)))
        );
    }
}
#[test]
fn missing_l2_redundancy_capabilities_cannot_be_assumed_from_ha_or_ecmp() {
    let yaml = include_str!("../../tests/fixtures/catalog/blueprints.yaml");
    let no_rstp = load_yaml(&yaml.replace(", rapid_spanning_tree", "")).unwrap();
    assert!(matches!(
        select_devices(&resolved(input()), &no_rstp, 10000),
        Err(SelectionError::RepairExhausted { .. })
    ));
    let mut v = input();
    v["redundancy"] = json!("tier1_link");
    let no_lacp = load_yaml(&yaml.replace(", lacp", "")).unwrap();
    assert!(matches!(
        select_devices(&resolved(v), &no_lacp, 10000),
        Err(SelectionError::RepairExhausted { .. })
    ));
}
