use labyrinth_engine::{
    artifacts::*,
    catalog,
    intent::{self, Readiness},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
fn input() -> Value {
    serde_json::from_str(include_str!("../../tests/fixtures/intents/enterprise.json")).unwrap()
}
fn build(v: Value) -> Result<VerifiedPackage, GenerationError> {
    let Readiness::Ready(i) = intent::resolve(serde_json::from_value(v).unwrap()).unwrap() else {
        panic!("unresolved")
    };
    let c =
        catalog::load_yaml(include_str!("../../tests/fixtures/catalog/blueprints.yaml")).unwrap();
    generate(&i, &c, &Default::default())
}
#[test]
fn complete_neutral_package_is_verified_reproducible_and_byte_hashed() {
    let a = build(input()).unwrap();
    let b = build(input()).unwrap();
    for (name, bytes) in a.files() {
        assert!(
            b.files().get(name) == Some(bytes),
            "package file differs: {name}"
        );
    }
    assert!(a.report().passed());
    assert_eq!(a.files().len(), 13);
    for (name, digest) in &a.manifest().files {
        let bytes = &a.files()[name];
        assert_eq!(digest.sha256, format!("{:x}", Sha256::digest(bytes)));
        assert_eq!(digest.size_bytes, bytes.len() as u64);
        assert!(!name.contains('/') && !name.contains(".."));
    }
    assert_eq!(
        a.manifest_sha256(),
        format!("{:x}", Sha256::digest(&a.files()["manifest.json"]))
    );
    let bom: Value = serde_json::from_slice(&a.files()["bom.json"]).unwrap();
    assert_eq!(
        bom["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["quantity"].as_u64().unwrap())
            .sum::<u64>(),
        a.network().devices.len() as u64
    );
    assert!(
        !a.files()
            .keys()
            .any(|n| n.ends_with(".cfg") || n.ends_with(".lab"))
    );
}
#[test]
fn source_collection_order_does_not_change_package_and_infeasible_designs_have_no_package() {
    let first = build(input()).unwrap();
    let mut v = input();
    v["sites"].as_array_mut().unwrap().reverse();
    v["wan_services"].as_array_mut().unwrap().reverse();
    v["policies"].as_array_mut().unwrap().reverse();
    let second = build(v).unwrap();
    for (name, bytes) in first.files() {
        assert!(
            second.files().get(name) == Some(bytes),
            "package file differs after input permutation: {name}"
        );
    }
    let mut v = input();
    for w in v["wan_services"].as_array_mut().unwrap() {
        w["committed_bandwidth_mbps"] = json!(10);
    }
    assert!(matches!(
        build(v),
        Err(GenerationError::RepairExhausted { .. })
    ));
}

#[test]
fn hard_failure_state_capacity_is_checked_before_cheap_assignment_can_win() {
    let mut bundle: catalog::CatalogBundle =
        serde_yaml_ng::from_str(include_str!("../../tests/fixtures/catalog/blueprints.yaml"))
            .unwrap();
    let mut cheap = bundle
        .archetypes
        .iter()
        .find(|a| {
            a.roles
                .contains(&labyrinth_engine::model::DeviceRole::Firewall)
        })
        .unwrap()
        .clone();
    cheap.id = labyrinth_engine::model::ArchetypeId::new("cheap-fw").unwrap();
    cheap.relative_cost_tier = 1;
    for group in &mut cheap.port_groups {
        group.shared_capacity_mbps = Some(240);
    }
    bundle.archetypes.push(cheap);
    let c = catalog::load_yaml(&serde_yaml_ng::to_string(&bundle).unwrap()).unwrap();
    let Readiness::Ready(i) = intent::resolve(serde_json::from_value(input()).unwrap()).unwrap()
    else {
        panic!()
    };
    let p = generate(&i, &c, &Default::default()).unwrap();
    assert!(p.report().passed());
    assert!(
        p.network()
            .devices
            .iter()
            .filter(|d| d.role == labyrinth_engine::model::DeviceRole::Firewall)
            .all(|d| d.archetype.as_str() != "cheap-fw")
    );
}

#[test]
fn invalid_generation_options_return_errors_before_building_or_publishing() {
    let Readiness::Ready(i) = intent::resolve(serde_json::from_value(input()).unwrap()).unwrap()
    else {
        panic!()
    };
    let c =
        catalog::load_yaml(include_str!("../../tests/fixtures/catalog/blueprints.yaml")).unwrap();
    let mut options = GenerationOptions::default();
    options.addressing.redundant_gateway_reservations = 0;
    assert!(generate(&i, &c, &options).is_err());
    let mut options = GenerationOptions::default();
    options.logical.routing_limits.timeout_ms = 0;
    assert!(generate(&i, &c, &options).is_err());
    let mut options = GenerationOptions::default();
    options.addressing.vlans.first = 0;
    assert!(generate(&i, &c, &options).is_err());
}
