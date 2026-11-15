use labyrinth_engine::intent::{PartialIntent, Readiness, resolve};
use serde_json::{Value, json};
fn input() -> Value {
    serde_json::from_str(include_str!("../../tests/fixtures/intents/enterprise.json")).unwrap()
}
fn check(v: Value) -> Result<Readiness, labyrinth_engine::diagnostics::ValidationErrors> {
    resolve(serde_json::from_value::<PartialIntent>(v).unwrap())
}
#[test]
fn matching_isp_names_never_substitute_for_explicit_confirmed_wan_services() {
    let mut v = input();
    v.as_object_mut().unwrap().remove("wan_services");
    let Readiness::NeedsClarification(fields) = check(v).unwrap() else {
        panic!("WAN connectivity inferred")
    };
    assert!(fields.iter().any(|f| f.path == "/wan_services"));
    assert!(matches!(check(input()).unwrap(), Readiness::Ready(_)));
}
#[test]
fn wan_capacity_references_reciprocity_and_duplicates_are_hard_constraints() {
    let mut v = input();
    v["wan_services"][0]["committed_bandwidth_mbps"] = json!(0);
    assert!(check(v).is_err());
    let mut v = input();
    v["wan_services"][0]["connected_sites"] = json!(["unknown"]);
    assert!(check(v).is_err());
    let mut v = input();
    v["wan_services"][0]["connected_sites"] = json!([]);
    assert!(check(v).is_err());
    let mut v = input();
    let dupe = v["wan_services"][0].clone();
    v["wan_services"].as_array_mut().unwrap().push(dupe);
    assert!(check(v).is_err());
}

#[test]
fn tier3_requires_confirmed_services_not_only_two_isp_names() {
    let mut v = input();
    v["redundancy"] = json!("tier3_site");
    v["wan_services"]
        .as_array_mut()
        .unwrap()
        .retain(|s| s["provider_failure_domain"] == "isp-a");
    let Readiness::NeedsClarification(fields) = check(v).unwrap() else {
        panic!("missing provider attachments were accepted")
    };
    assert!(fields.iter().any(|f| f.path == "/wan_services"));
}
