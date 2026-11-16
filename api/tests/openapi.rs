use serde_json::{Value, json};
#[test]
fn exported_openapi_matches_the_checked_in_contract() {
    let document = labyrinth_api::openapi::document();
    let stored: Value = serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../schemas/http-v1.openapi.json"
        ))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(document, stored);
    let paths = document["paths"].as_object().unwrap();
    assert!(paths.contains_key("/v1/onboarding/personal-workspace"));
    assert!(paths.contains_key("/v1/tenants/{tenant}/projects/{project}/jobs"));
    assert!(paths.contains_key("/health/ready"));
    let mut ids = std::collections::BTreeSet::new();
    for path in paths.values() {
        for (method, operation) in path.as_object().unwrap() {
            if ["get", "post"].contains(&method.as_str()) {
                assert!(ids.insert(operation["operationId"].as_str().unwrap()));
            }
        }
    }
}
#[test]
fn authority_fields_and_missing_head_preconditions_are_rejected_by_wire_schemas() {
    let document = labyrinth_api::openapi::document();
    fn remap(value: &mut Value) {
        match value {
            Value::Object(map) => {
                if let Some(Value::String(reference)) = map.get_mut("$ref") {
                    *reference = reference.replace("#/components/schemas/", "#/$defs/");
                }
                for child in map.values_mut() {
                    remap(child)
                }
            }
            Value::Array(items) => {
                for child in items {
                    remap(child)
                }
            }
            _ => {}
        }
    }
    let valid = |name: &str, input: Value| {
        let mut schema =
            json!({"$defs":document["components"]["schemas"],"$ref":format!("#/$defs/{name}")});
        remap(&mut schema);
        jsonschema::validator_for(&schema).unwrap().is_valid(&input)
    };
    assert!(valid("OnboardingRequest", json!({})));
    assert!(!valid("OnboardingRequest", json!({"role":"admin"})));
    assert!(valid("CreateProjectRequest", json!({"name":"Office"})));
    assert!(!valid(
        "CreateProjectRequest",
        json!({"name":"Office","actor_id":"forged"})
    ));
    let intent: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/intents/enterprise.json")).unwrap();
    let mut request =
        json!({"intent":intent,"catalog_sha256":"a".repeat(64),"expected_head_revision_id":null});
    assert!(valid("SubmitJobRequest", request.clone()));
    request
        .as_object_mut()
        .unwrap()
        .remove("expected_head_revision_id");
    assert!(!valid("SubmitJobRequest", request));
}

#[test]
fn every_exported_schema_resolves_without_remote_references() {
    let document = labyrinth_api::openapi::document();
    for name in document["components"]["schemas"]
        .as_object()
        .unwrap()
        .keys()
    {
        let mut schema = document.clone();
        schema["$ref"] = json!(format!("#/components/schemas/{name}"));
        jsonschema::validator_for(&schema).unwrap();
    }
}
