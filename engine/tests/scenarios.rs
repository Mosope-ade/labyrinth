use labyrinth_engine::{
    artifacts::*,
    catalog,
    intent::{self, Readiness},
    verification::PassKind,
};
use serde_json::{Value, json};
fn design(hubs: usize, branches: usize, tier: &str) -> Value {
    let mut v: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/intents/enterprise.json")).unwrap();
    let original = v["sites"][0].clone();
    let mut sites = Vec::new();
    for i in 0..hubs + branches {
        let mut s = original.clone();
        let id = format!("site-{i:02}");
        s["id"] = json!(id);
        s["role"] = json!(if i < hubs { "hub" } else { "branch" });
        s["location"] = json!(format!("location-{i}"));
        s["zones"][0]["id"] = json!(format!("zone-{i:02}"));
        s["zones"][0]["endpoint_count"] = json!(8);
        sites.push(s);
    }
    let mut services = Vec::new();
    for i in 0..sites.len() {
        for provider in ["isp-a", "isp-b"] {
            let peers: Vec<_> = (0..sites.len())
                .filter(|j| *j != i && (i < hubs || *j < hubs))
                .map(|j| format!("site-{j:02}"))
                .collect();
            services.push(json!({"site":format!("site-{i:02}"),"provider_failure_domain":provider,"committed_bandwidth_mbps":1000,"connected_sites":peers}));
        }
    }
    v["sites"] = json!(sites);
    v["wan_services"] = json!(services);
    v["redundancy"] = json!(tier);
    v["demands"] = json!([]);
    v["policies"] = json!([]);
    v
}
fn traffic(v: &mut Value, from: usize, to: usize, peak: u64) {
    let id = format!("flow-{from}-{to}");
    v["demands"].as_array_mut().unwrap().push(json!({"id":id,"from_zone":format!("zone-{from:02}"),"to_zone":format!("zone-{to:02}"),"peak_mbps":peak,"distribution":{"mode":"single_path"}}));
    v["policies"].as_array_mut().unwrap().push(json!({"id":format!("allow-{from}-{to}"),"from_zone":format!("zone-{from:02}"),"to_zone":format!("zone-{to:02}"),"action":"allow","service":{"protocol":"tcp","destination_ports":[{"start":443,"end":443}]}}));
}
fn build(v: Value) -> VerifiedPackage {
    let Readiness::Ready(i) = intent::resolve(serde_json::from_value(v).unwrap()).unwrap() else {
        panic!("scenario unresolved")
    };
    let c =
        catalog::load_yaml(include_str!("../../tests/fixtures/catalog/blueprints.yaml")).unwrap();
    generate(&i, &c, &Default::default()).unwrap()
}
#[test]
fn multi_branch_scenario_completes_tier0_tier1_and_tier2_with_precise_protection() {
    for tier in ["tier0_none", "tier1_link", "tier2_device"] {
        let mut v = design(1, 3, tier);
        traffic(&mut v, 1, 2, 100);
        traffic(&mut v, 2, 1, 80);
        traffic(&mut v, 3, 0, 30);
        let p = build(v);
        assert!(p.report().passed());
        if tier == "tier0_none" {
            assert!(p.report().scenarios.is_empty());
            assert!(p.report().protected_set.is_empty());
        }
        if tier == "tier1_link" {
            assert!(
                p.report()
                    .scenarios
                    .iter()
                    .any(|s| s.name.starts_with("wan_handoff:"))
            );
            assert!(
                !p.report()
                    .protected_set
                    .iter()
                    .any(|s| s.starts_with("backbone_device:"))
            );
        }
        if tier == "tier2_device" {
            assert!(
                p.report()
                    .scenarios
                    .iter()
                    .any(|s| s.name.starts_with("device:"))
            );
        }
    }
}
#[test]
fn tier3_survives_every_declared_site_and_provider_failure_with_surviving_branch_traffic() {
    let mut v = design(2, 2, "tier3_site");
    traffic(&mut v, 2, 3, 100);
    traffic(&mut v, 3, 2, 80);
    traffic(&mut v, 2, 0, 20);
    traffic(&mut v, 1, 3, 30);
    let p = build(v);
    assert!(p.report().passed());
    let sites: Vec<_> = p
        .report()
        .scenarios
        .iter()
        .filter(|s| s.name.starts_with("site:"))
        .collect();
    assert_eq!(sites.len(), 4);
    assert!(sites.iter().all(|s| s.passed));
    assert!(
        sites
            .iter()
            .filter(|s| s.name == "site:site-00" || s.name == "site:site-01")
            .all(|s| s.protected_demands >= 2)
    );
    assert_eq!(
        p.report()
            .scenarios
            .iter()
            .filter(|s| s.name.starts_with("provider:"))
            .count(),
        2
    );
    assert!(
        p.report()
            .passes
            .iter()
            .find(|p| p.kind == PassKind::Resilience)
            .unwrap()
            .checked
            > 0
    );
}
#[test]
fn near_device_admission_limit_is_exercised_end_to_end() {
    let mut v = design(1, 13, "tier2_device");
    traffic(&mut v, 1, 2, 20);
    let start = std::time::Instant::now();
    let p = build(v);
    assert_eq!(p.network().devices.len(), 126);
    assert!(p.report().passed());
    eprintln!(
        "scale acceptance: {} devices, {} failure scenarios, {:?}",
        p.network().devices.len(),
        p.report().scenarios.len(),
        start.elapsed()
    );
}
