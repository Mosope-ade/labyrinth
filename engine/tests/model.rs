use labyrinth_engine::{catalog::load_yaml, model::NetworkDraft};
use serde_json::{Value, json};
fn catalog() -> labyrinth_engine::catalog::CatalogSnapshot {
    load_yaml(include_str!("../../catalog/archetypes/reference.yaml")).unwrap()
}
fn draft() -> Value {
    serde_json::from_str(include_str!(
        "../../tests/fixtures/scenarios/parallel-links.json"
    ))
    .unwrap()
}
fn valid(v: Value) -> bool {
    serde_json::from_value::<NetworkDraft>(v)
        .unwrap()
        .validate(&catalog())
        .is_ok()
}
#[test]
fn parallel_links_are_valid_without_claiming_forwarding_or_resilience() {
    let d: NetworkDraft = serde_json::from_value(draft()).unwrap();
    let result = d.validate(&catalog()).unwrap();
    assert_eq!(result.network().links.len(), 2);
    assert_eq!(result.network().devices.len(), 2);
}
#[test]
fn dangling_endpoints_duplicate_ids_and_reused_ports_are_rejected() {
    let mut v = draft();
    v["links"][0]["to"]["interface"] = json!("missing");
    assert!(!valid(v));
    let mut v = draft();
    v["devices"][1]["id"] = json!("switch-a");
    assert!(!valid(v));
    let mut v = draft();
    v["devices"][0]["interfaces"][1]["port"]["index"] = json!(0);
    assert!(!valid(v));
    let mut v = draft();
    v["links"][1]["from"] = v["links"][0]["from"].clone();
    assert!(!valid(v));
}
#[test]
fn catalog_modes_port_capacity_and_hash_are_enforced() {
    let mut v = draft();
    v["devices"][0]["operating_mode"] = json!("invented");
    assert!(!valid(v));
    let mut v = draft();
    v["devices"][0]["interfaces"][0]["port"]["index"] = json!(48);
    assert!(!valid(v));
    let mut v = draft();
    v["catalog_version"]["sha256"] = json!("0".repeat(64));
    assert!(!valid(v));
    let mut v = draft();
    v["devices"][0]["archetype"] = json!("unknown");
    assert!(!valid(v));
}
#[test]
fn speed_media_and_subnet_mismatches_are_rejected() {
    let mut v = draft();
    v["devices"][0]["interfaces"][0]["speed_mbps"] = json!(999);
    assert!(!valid(v));
    let mut v = draft();
    v["devices"][0]["interfaces"][0]["media"] = json!("fiber");
    assert!(!valid(v));
    let mut v = draft();
    v["devices"][1]["interfaces"][0]["kind"]["address"] = json!("10.0.2.2/30");
    assert!(!valid(v));
}
#[test]
fn device_and_interface_zone_references_are_not_conflated() {
    let mut v = draft();
    v["devices"][0]["site"] = json!("unknown");
    assert!(!valid(v));
    let mut v = draft();
    v["devices"][0]["interfaces"][0]["kind"]["zone"] = json!("unknown");
    assert!(!valid(v));
}
#[test]
fn invalid_addresses_and_unknown_fields_fail_deserialization() {
    let mut v = draft();
    v["devices"][0]["interfaces"][0]["kind"]["address"] = json!("10.0.0.0/30");
    assert!(serde_json::from_value::<NetworkDraft>(v).is_err());
    let mut v = draft();
    v["devices"][0]["vendor"] = json!("vendor");
    assert!(serde_json::from_value::<NetworkDraft>(v).is_err());
}
#[test]
fn duplicate_address_and_cross_site_domain_membership_are_rejected() {
    let mut v = draft();
    v["devices"][1]["interfaces"][0]["kind"]["address"] =
        v["devices"][0]["interfaces"][0]["kind"]["address"].clone();
    assert!(!valid(v));
    let mut v = draft();
    v["devices"][0]["interfaces"][0]["kind"] = json!({"type":"access","domain":"missing"});
    assert!(!valid(v));
}
#[test]
fn link_bundle_membership_is_explicit_and_cannot_be_forged() {
    let mut v = draft();
    v["domains"] = json!([{"id":"staff-l2","site":"site-a","zone":"staff","vlan":10}]);
    for d in v["devices"].as_array_mut().unwrap() {
        for f in d["interfaces"].as_array_mut().unwrap() {
            f["kind"] = json!({"type":"trunk","domains":["staff-l2"]});
        }
    }
    v["link_bundles"] = json!([{"id":"uplinks","kind":"lacp","links":["uplink-a","uplink-b"]}]);
    assert!(valid(v));
    let mut v = draft();
    v["link_bundles"] = json!([{"id":"uplinks","kind":"lacp","links":["uplink-a","missing"]}]);
    assert!(!valid(v));
}

#[test]
fn decision_component_references_must_resolve() {
    let mut v = draft();
    v["decisions"] = json!([{"id":"choice-a","component_ids":[{"type":"device","id":"switch-a"}],"reason":"Selected to satisfy declared capacity","rule_id":"role-mapping","triggered_by":"selection","overridable":true}]);
    assert!(valid(v.clone()));
    v["decisions"][0]["component_ids"][0]["id"] = json!("missing");
    assert!(!valid(v));
}
