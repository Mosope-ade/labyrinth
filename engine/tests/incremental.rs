use labyrinth_engine::{
    artifacts::*,
    catalog,
    intent::{self, Readiness},
    model::*,
};
use serde_json::{Value, json};
fn input() -> Value {
    serde_json::from_str(include_str!("../../tests/fixtures/intents/enterprise.json")).unwrap()
}
fn resolve(v: Value) -> intent::ResolvedIntent {
    let Readiness::Ready(i) = intent::resolve(serde_json::from_value(v).unwrap()).unwrap() else {
        panic!("unresolved")
    };
    *i
}
fn catalog() -> catalog::CatalogSnapshot {
    catalog::load_yaml(include_str!("../../tests/fixtures/catalog/blueprints.yaml")).unwrap()
}
#[test]
fn incremental_endpoint_expansion_preserves_every_allocation_vlan_and_core_mode() {
    let c = catalog();
    let options = GenerationOptions::default();
    let old = generate(&resolve(input()), &c, &options).unwrap();
    let before = old.files().clone();
    let base =
        BaseRevision::from_verified(RevisionId::new("revision-one").unwrap(), &old, &c).unwrap();
    let mut v = input();
    v["update"] = serde_json::to_value(base.update_mode()).unwrap();
    v["sites"][0]["zones"][0]["endpoint_count"] = json!(101);
    let next = generate_incremental(&resolve(v), &c, &options, &base).unwrap();
    assert!(next.report().passed());
    for a in &old.network().allocations {
        let new = next
            .network()
            .allocations
            .iter()
            .find(|n| n.id == a.id)
            .unwrap();
        assert_eq!(a.subnet, new.subnet);
    }
    for d in old
        .network()
        .devices
        .iter()
        .filter(|d| d.role == DeviceRole::CoreSwitch)
    {
        let n = next
            .network()
            .devices
            .iter()
            .find(|n| n.id == d.id)
            .unwrap();
        assert_eq!(
            (&d.archetype, &d.operating_mode),
            (&n.archetype, &n.operating_mode)
        );
    }
    for d in &old.network().domains {
        assert_eq!(
            next.network()
                .domains
                .iter()
                .find(|n| n.id == d.id)
                .unwrap()
                .vlan,
            d.vlan
        );
    }
    assert_eq!(
        next.manifest()
            .base_revision
            .as_ref()
            .unwrap()
            .revision
            .as_str(),
        "revision-one"
    );
    assert!(old.files() == &before, "base revision was mutated");
}
#[test]
fn stale_or_missing_pins_catalog_changes_and_tampered_saved_payloads_reject() {
    let c = catalog();
    let options = GenerationOptions::default();
    let old = generate(&resolve(input()), &c, &options).unwrap();
    let base =
        BaseRevision::from_verified(RevisionId::new("revision-one").unwrap(), &old, &c).unwrap();
    let mut v = input();
    v["update"] = serde_json::to_value(base.update_mode()).unwrap();
    v["update"]["base_revision"] = json!("stale");
    assert!(matches!(
        generate_incremental(&resolve(v), &c, &options, &base),
        Err(GenerationError::RevisionConflict(_))
    ));
    let mut v = input();
    v["update"] = serde_json::to_value(base.update_mode()).unwrap();
    v["update"]["core_pins"].as_array_mut().unwrap().pop();
    assert!(matches!(
        generate_incremental(&resolve(v), &c, &options, &base),
        Err(GenerationError::RevisionConflict(_))
    ));
    let mut files = old.files().clone();
    files.get_mut("topology.json").unwrap().push(b' ');
    assert!(
        BaseRevision::load_saved(
            RevisionId::new("revision-one").unwrap(),
            old.manifest_sha256(),
            &files,
            &c
        )
        .is_err()
    );
    assert!(
        BaseRevision::load_saved(
            RevisionId::new("revision-one").unwrap(),
            &"0".repeat(64),
            old.files(),
            &c
        )
        .is_err()
    );
    let changed = catalog::load_yaml(
        &include_str!("../../tests/fixtures/catalog/blueprints.yaml")
            .replace("version: 1.2.0", "version: 1.2.1"),
    )
    .unwrap();
    assert!(
        BaseRevision::from_verified(RevisionId::new("revision-one").unwrap(), &old, &changed)
            .is_err()
    );
}
#[test]
fn insufficient_pinned_subnet_capacity_and_context_free_updates_fail_closed() {
    let c = catalog();
    let options = GenerationOptions::default();
    let old = generate(&resolve(input()), &c, &options).unwrap();
    let base =
        BaseRevision::from_verified(RevisionId::new("revision-one").unwrap(), &old, &c).unwrap();
    let mut v = input();
    v["update"] = serde_json::to_value(base.update_mode()).unwrap();
    v["sites"][0]["zones"][0]["endpoint_count"] = json!(200);
    let i = resolve(v);
    assert!(generate_incremental(&i, &c, &options, &base).is_err());
    assert!(generate(&i, &c, &options).is_err());
}

#[test]
fn adding_a_site_keeps_old_addresses_when_new_ids_sort_before_existing_sites() {
    let c = catalog();
    let options = GenerationOptions::default();
    let old = generate(&resolve(input()), &c, &options).unwrap();
    let base =
        BaseRevision::from_verified(RevisionId::new("revision-one").unwrap(), &old, &c).unwrap();
    let mut v = input();
    v["update"] = serde_json::to_value(base.update_mode()).unwrap();
    let mut new_site = v["sites"][1].clone();
    new_site["id"] = json!("aaa");
    new_site["location"] = json!("abuja");
    new_site["zones"][0]["id"] = json!("new-zone");
    new_site["zones"][0]["endpoint_count"] = json!(10);
    v["sites"].as_array_mut().unwrap().push(new_site);
    for provider in ["isp-a", "isp-b"] {
        for service in v["wan_services"].as_array_mut().unwrap() {
            if service["site"] == "hub" && service["provider_failure_domain"] == provider {
                service["connected_sites"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!("aaa"));
            }
        }
        v["wan_services"].as_array_mut().unwrap().push(json!({"site":"aaa","provider_failure_domain":provider,"committed_bandwidth_mbps":1000,"connected_sites":["hub"]}));
    }
    v["demands"].as_array_mut().unwrap().push(json!({"id":"to-new","from_zone":"staff","to_zone":"new-zone","peak_mbps":40,"distribution":{"mode":"single_path"}}));
    v["policies"].as_array_mut().unwrap().push(json!({"id":"allow-new","from_zone":"staff","to_zone":"new-zone","action":"allow","service":{"protocol":"tcp","destination_ports":[{"start":443,"end":443}]}}));
    let next = generate_incremental(&resolve(v), &c, &options, &base).unwrap();
    assert_eq!(next.network().sites.len(), 3);
    assert!(next.report().passed());
    for a in &old.network().allocations {
        assert_eq!(
            next.network()
                .allocations
                .iter()
                .find(|n| n.id == a.id)
                .unwrap()
                .subnet,
            a.subnet
        );
    }
    for d in &old.network().domains {
        assert_eq!(
            next.network()
                .domains
                .iter()
                .find(|n| n.id == d.id)
                .unwrap()
                .vlan,
            d.vlan
        );
    }
}
#[test]
fn core_mode_pins_cannot_be_relaxed_to_enable_new_session_requirements() {
    let mut bundle: catalog::CatalogBundle =
        serde_saphyr::from_str(include_str!("../../tests/fixtures/catalog/blueprints.yaml"))
            .unwrap();
    let core = bundle
        .archetypes
        .iter_mut()
        .find(|a| a.roles.contains(&DeviceRole::CoreSwitch))
        .unwrap();
    let mut steered = core.operating_modes[0].clone();
    steered.id = ModeId::new("z-steered").unwrap();
    core.operating_modes[0]
        .features
        .retain(|f| *f != Feature::SessionSteering);
    core.operating_modes.push(steered);
    let c = catalog::load_yaml(&serde_saphyr::to_string(&bundle).unwrap()).unwrap();
    let options = GenerationOptions::default();
    let old = generate(&resolve(input()), &c, &options).unwrap();
    assert!(
        old.network()
            .devices
            .iter()
            .filter(|d| d.role == DeviceRole::CoreSwitch)
            .all(|d| d.operating_mode.as_str() == "base")
    );
    let base =
        BaseRevision::from_verified(RevisionId::new("revision-one").unwrap(), &old, &c).unwrap();
    let mut v = input();
    v["update"] = serde_json::to_value(base.update_mode()).unwrap();
    v["demands"][0]["distribution"] = json!({"mode":"sessions","growth":"rates","cohorts":[{"session_peak_mbps":10,"simultaneous_sessions":10}]});
    assert!(generate_incremental(&resolve(v.clone()), &c, &options, &base).is_err());
    v["update"] = json!({"mode":"stateless_redesign"});
    let redesigned = generate(&resolve(v), &c, &options).unwrap();
    assert!(
        redesigned
            .network()
            .devices
            .iter()
            .filter(|d| d.role == DeviceRole::CoreSwitch)
            .all(|d| d.operating_mode.as_str() == "z-steered")
    );
}
