use labyrinth_engine::catalog::{catalog_schema, load_yaml};

const CATALOG: &str = include_str!("../../catalog/archetypes/reference.yaml");

#[test]
fn bundled_catalog_is_valid_and_schema_agrees() {
    let snapshot = load_yaml(CATALOG).unwrap();
    assert_eq!(snapshot.archetypes().len(), 2);
    assert_eq!(snapshot.version().semantic_version, "1.0.0");
    assert_eq!(snapshot.version().sha256.len(), 64);
    let yaml: serde_json::Value = serde_saphyr::from_str(CATALOG).unwrap();
    let validator = jsonschema::validator_for(catalog_schema().as_value()).unwrap();
    assert!(validator.is_valid(&yaml));
}

#[test]
fn unknown_fields_and_vendor_selection_are_rejected() {
    for field in ["vendor: cisco", "unexpected_capacity: 100"] {
        let bad = CATALOG.replacen(
            "    relative_cost_tier: 1",
            &format!("    relative_cost_tier: 1\n    {field}"),
            1,
        );
        assert!(load_yaml(&bad).is_err());
    }
}

#[test]
fn unknown_mode_throughput_is_preserved_not_guessed() {
    let missing = CATALOG.replacen("throughput_mbps: 10000", "throughput_mbps: null", 1);
    let snapshot = load_yaml(&missing).unwrap();
    assert_eq!(
        snapshot.archetypes()[0].operating_modes[0].throughput_mbps,
        None
    );
}

#[test]
fn invalid_port_and_mode_capacities_are_rejected() {
    for bad in [
        CATALOG.replacen("count: 48", "count: 0", 1),
        CATALOG.replacen("throughput_mbps: 10000", "throughput_mbps: 0", 1),
        CATALOG.replacen("throughput_mbps: 4000", "throughput_mbps: 20000", 1),
        CATALOG.replacen("speeds_mbps: [1000]", "speeds_mbps: []", 1),
        CATALOG.replacen("version: 1.0.0", "version: banana", 1),
    ] {
        assert!(load_yaml(&bad).is_err(), "accepted invalid catalog: {bad}");
    }
}

#[test]
fn duplicates_and_unavailable_mode_features_are_rejected() {
    let duplicate = CATALOG.replace("id: edge-firewall", "id: access-switch");
    assert!(load_yaml(&duplicate).is_err());
    let unsupported = CATALOG.replacen(
        "features: [routing, firewall, ips, vpn]",
        "features: [routing, firewall]",
        1,
    );
    assert!(load_yaml(&unsupported).is_err());
}

#[test]
fn digest_ignores_yaml_formatting_but_pins_all_catalog_content() {
    let original = load_yaml(CATALOG).unwrap();
    let commented = load_yaml(&format!("# formatting only\n{CATALOG}")).unwrap();
    assert_eq!(original.version(), commented.version());
    let changed =
        load_yaml(&CATALOG.replace("relative_cost_tier: 1", "relative_cost_tier: 2")).unwrap();
    assert_ne!(original.version().sha256, changed.version().sha256);
}

#[test]
fn schema_rejects_extra_fields_and_yaml_duplicate_keys_reject() {
    let mut value: serde_json::Value = serde_saphyr::from_str(CATALOG).unwrap();
    value["archetypes"][0]["vendor"] = "anything".into();
    let validator = jsonschema::validator_for(catalog_schema().as_value()).unwrap();
    assert!(!validator.is_valid(&value));
    assert!(
        load_yaml(&CATALOG.replacen("version: 1.0.0", "version: 1.0.0\nversion: 1.1.0", 1))
            .is_err()
    );
}

#[test]
fn poe_and_breakout_constraints_reject_inconsistent_hardware() {
    for bad in [
        CATALOG.replacen("poe_budget_watts: 740", "poe_budget_watts: 2000", 1),
        CATALOG.replacen("media: copper", "media: fiber", 1),
        CATALOG.replacen("poe_budget_watts: 740", "poe_budget_watts: null", 1),
        CATALOG.replacen(
            "breakout: null",
            "breakout: {children_per_port: 4, child_speed_mbps: 1000}",
            1,
        ),
    ] {
        assert!(load_yaml(&bad).is_err());
    }
    let breakout=CATALOG.replacen("speeds_mbps: [10000]", "speeds_mbps: [40000]",1)
        .replacen("        media: fiber\n        poe_watts_per_port: null\n        shared_capacity_mbps: null\n        breakout: null",
            "        media: fiber\n        poe_watts_per_port: null\n        shared_capacity_mbps: null\n        breakout: {children_per_port: 4, child_speed_mbps: 10000}",1);
    assert!(load_yaml(&breakout).is_ok());
}

#[test]
fn input_size_empty_catalog_and_multidocument_yaml_are_rejected() {
    assert!(load_yaml(&" ".repeat(labyrinth_engine::catalog::MAX_YAML_BYTES + 1)).is_err());
    assert!(load_yaml("schema_version: catalog_v1\nversion: 1.0.0\narchetypes: []").is_err());
    assert!(load_yaml(&format!("{CATALOG}\n---\n{CATALOG}")).is_err());
}

#[test]
fn invalid_fixture_returns_all_independent_capacity_diagnostics() {
    let err = load_yaml(include_str!(
        "../../tests/fixtures/catalog/invalid-capacity.yaml"
    ))
    .unwrap_err();
    let labyrinth_engine::catalog::CatalogError::Invalid(errors) = err else {
        panic!("expected semantic errors")
    };
    assert!(errors.issues.iter().any(|i| i.path.ends_with("/count")));
    assert!(
        errors
            .issues
            .iter()
            .any(|i| i.path.ends_with("/throughput_mbps"))
    );
}

#[test]
fn catalog_rejects_unknown_tags_and_deep_input_without_echoing_source() {
    let tagged = CATALOG.replacen("version: 1.0.0", "version: !untrusted private-marker", 1);
    let error = load_yaml(&tagged).unwrap_err().to_string();
    assert!(!error.contains("private-marker"));
    assert!(load_yaml(&format!("{}0{}", "[".repeat(80), "]".repeat(80))).is_err());
}

#[test]
fn parser_upgrade_preserves_the_saved_blueprint_catalog_identity() {
    let snapshot = load_yaml(include_str!("../../tests/fixtures/catalog/blueprints.yaml")).unwrap();
    assert_eq!(
        snapshot.version().sha256,
        "f337bdf91144260729922d1b62f3c88e38d417ce1397b0da8861e3158e34213b"
    );
}
