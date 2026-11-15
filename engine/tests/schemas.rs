use labyrinth_engine::{catalog, intent, model};
use serde_json::{Value, json};

#[test]
fn generated_contract_schemas_are_current() {
    for (generated, stored) in [
        (
            serde_json::to_value(catalog::catalog_schema()).unwrap(),
            include_str!("../../catalog/schemas/catalog-v1.schema.json"),
        ),
        (
            serde_json::to_value(intent::intent_schema()).unwrap(),
            include_str!("../../schemas/intent-v1.schema.json"),
        ),
        (
            serde_json::to_value(schemars::schema_for!(model::NetworkDraft)).unwrap(),
            include_str!("../../schemas/network-v1.schema.json"),
        ),
        (
            serde_json::to_value(schemars::schema_for!(
                labyrinth_engine::topology::AddressingPolicy
            ))
            .unwrap(),
            include_str!("../../schemas/addressing-policy-v1.schema.json"),
        ),
        (
            serde_json::to_value(schemars::schema_for!(
                labyrinth_engine::topology::LogicalPolicy
            ))
            .unwrap(),
            include_str!("../../schemas/logical-policy-v1.schema.json"),
        ),
        (
            serde_json::to_value(schemars::schema_for!(
                labyrinth_engine::artifacts::GenerationOptions
            ))
            .unwrap(),
            include_str!("../../schemas/generation-options-v1.schema.json"),
        ),
        (
            serde_json::to_value(schemars::schema_for!(
                labyrinth_engine::verification::VerificationReport
            ))
            .unwrap(),
            include_str!("../../schemas/verification-report-v1.schema.json"),
        ),
        (
            serde_json::to_value(schemars::schema_for!(
                labyrinth_engine::artifacts::PackageManifest
            ))
            .unwrap(),
            include_str!("../../schemas/package-manifest-v1.schema.json"),
        ),
        (
            serde_json::to_value(schemars::schema_for!(
                labyrinth_engine::artifacts::GenerationError
            ))
            .unwrap(),
            include_str!("../../schemas/generation-error-v1.schema.json"),
        ),
    ] {
        assert_eq!(
            generated,
            serde_json::from_str::<Value>(stored).unwrap(),
            "regenerate stale contract schema"
        );
    }
}
#[test]
fn intent_and_model_fixtures_match_schemas_and_ids_are_constrained() {
    for (schema, fixture) in [
        (
            serde_json::to_value(intent::intent_schema()).unwrap(),
            include_str!("../../tests/fixtures/intents/enterprise.json"),
        ),
        (
            serde_json::to_value(schemars::schema_for!(model::NetworkDraft)).unwrap(),
            include_str!("../../tests/fixtures/scenarios/parallel-links.json"),
        ),
    ] {
        let validator = jsonschema::validator_for(&schema).unwrap();
        let mut value: Value = serde_json::from_str(fixture).unwrap();
        assert!(validator.is_valid(&value), "fixture fails generated schema");
        value["sites"][0]["id"] = json!("INVALID ID");
        assert!(!validator.is_valid(&value));
    }
}
#[test]
fn retained_catalog_bytes_match_the_pinned_manifest() {
    let live = catalog::load_yaml(include_str!("../../catalog/archetypes/reference.yaml")).unwrap();
    let manifest: catalog::CatalogVersion =
        serde_json::from_str(include_str!("../../catalog/bundles/reference.json")).unwrap();
    assert_eq!(live.version(), &manifest);
    let path = format!(
        "{}/../catalog/bundles/{}-{}.yaml",
        env!("CARGO_MANIFEST_DIR"),
        manifest.semantic_version,
        manifest.sha256
    );
    let bytes = std::fs::read_to_string(path).unwrap();
    assert_eq!(catalog::load_yaml(&bytes).unwrap().version(), &manifest);
}

#[test]
fn configurable_addressing_policy_has_a_strict_public_schema() {
    use labyrinth_engine::topology::AddressingPolicy;
    let schema = serde_json::to_value(schemars::schema_for!(AddressingPolicy)).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let mut value = serde_json::to_value(AddressingPolicy::default()).unwrap();
    assert!(validator.is_valid(&value));
    value["transit_prefix_len"] = json!(30);
    value["vlans"]["first"] = json!(200);
    value["vlans"]["reuse_across_sites"] = json!(false);
    assert!(validator.is_valid(&value));
    let policy: AddressingPolicy = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(policy.transit_prefix_len, 30);
    assert_eq!(policy.vlans.first, 200);
    value["unknown"] = json!(true);
    assert!(!validator.is_valid(&value));
    assert!(serde_json::from_value::<AddressingPolicy>(value.clone()).is_err());
    value.as_object_mut().unwrap().remove("unknown");
    value["transit_prefix_len"] = json!(32);
    assert!(!validator.is_valid(&value));
    value["transit_prefix_len"] = json!(31);
    value["vlans"]["first"] = json!(0);
    assert!(!validator.is_valid(&value));
}
