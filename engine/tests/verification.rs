use labyrinth_engine::{
    catalog,
    intent::{self, Readiness},
    pipeline::allocate_topology,
    topology::{AddressingPolicy, LogicalPolicy},
    verification::*,
};
use serde_json::{Value, json};
fn input() -> Value {
    serde_json::from_str(include_str!("../../tests/fixtures/intents/enterprise.json")).unwrap()
}
fn setup(
    v: Value,
) -> (
    intent::ResolvedIntent,
    catalog::CatalogSnapshot,
    labyrinth_engine::topology::AllocatedTopology,
) {
    let Readiness::Ready(i) = intent::resolve(serde_json::from_value(v).unwrap()).unwrap() else {
        panic!("fixture unresolved")
    };
    let c =
        catalog::load_yaml(include_str!("../../tests/fixtures/catalog/blueprints.yaml")).unwrap();
    let t = allocate_topology(&i, &c, &AddressingPolicy::default(), 10000).unwrap();
    (*i, c, t)
}
#[test]
fn all_five_passes_include_failure_evidence_and_no_partial_budget_success() {
    let (i, c, t) = setup(input());
    let p = LogicalPolicy::default();
    let (n, _) = prepare(&i, &c, &t, &p).unwrap();
    let r = verify(&i, &c, &t, &p, &n, &VerificationLimits::default());
    assert!(r.passed(), "{}", serde_json::to_string_pretty(&r).unwrap());
    assert_eq!(r.passes.len(), 5);
    assert!(!r.scenarios.is_empty());
    assert!(r.scenarios.iter().all(|s| s.passed));
    assert!(!r.exclusions.is_empty());
    let budget = VerificationLimits {
        max_scenarios: 0,
        ..Default::default()
    };
    let r = verify(&i, &c, &t, &p, &n, &budget);
    assert!(!r.passed());
    assert!(
        !r.passes
            .iter()
            .find(|p| p.kind == PassKind::Resilience)
            .unwrap()
            .passed
    );
}
#[test]
fn tampered_routes_policies_and_addressing_cannot_receive_verification() {
    let (i, c, t) = setup(input());
    let p = LogicalPolicy::default();
    let (n, _) = prepare(&i, &c, &t, &p).unwrap();
    let mut changed = n.clone();
    changed.policies.clear();
    assert!(!verify(&i, &c, &t, &p, &changed, &Default::default()).passed());
    let mut changed = n.clone();
    changed.forwarding_routes.clear();
    assert!(!verify(&i, &c, &t, &p, &changed, &Default::default()).passed());
    let mut changed = n.clone();
    changed.allocations[0].subnet =
        labyrinth_engine::model::Ipv4Prefix::new("192.168.0.0/24").unwrap();
    assert!(!verify(&i, &c, &t, &p, &changed, &Default::default()).passed());
}
#[test]
fn duplicate_handoffs_never_multiply_committed_wan_service_bandwidth() {
    let mut v = input();
    for w in v["wan_services"].as_array_mut().unwrap() {
        w["committed_bandwidth_mbps"] = json!(60);
    }
    let (i, c, t) = setup(v);
    let p = LogicalPolicy::default();
    let (n, _) = prepare(&i, &c, &t, &p).unwrap();
    let r = verify(&i, &c, &t, &p, &n, &Default::default());
    assert!(!r.passed());
    assert!(r.passes.iter().flat_map(|p| &p.counterexamples).any(|e| {
        e.component.starts_with("wan_service:")
            && e.required == Some(120)
            && e.available == Some(60)
    }));
}
