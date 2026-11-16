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

#[test]
fn every_router_operation_has_typed_success_and_admin_authority_metadata() {
    let document = labyrinth_api::openapi::document();
    let source = include_str!("../src/http/mod.rs");
    // Every literal route, including multiline registrations, must appear in the export.
    for literal in source
        .split('"')
        .filter(|s| s.starts_with("/v1/") || s.starts_with("/health/"))
    {
        assert!(
            document["paths"].get(literal).is_some(),
            "missing {literal}"
        );
    }
    for (path, item) in document["paths"].as_object().unwrap() {
        for (method, operation) in item.as_object().unwrap() {
            if !["get", "post"].contains(&method.as_str()) {
                continue;
            }
            if path.starts_with("/v1/admin/") {
                assert_eq!(operation["x-required-role"], "admin");
            }
            for (status, response) in operation["responses"].as_object().unwrap() {
                if status.starts_with('2') && status != "204" {
                    assert!(
                        response["content"]["application/json"]["schema"].is_object(),
                        "untyped {path} {method} {status}"
                    );
                }
            }
        }
    }
}

#[test]
fn admin_mutations_require_confirmations_and_reject_authority_injection() {
    let document = labyrinth_api::openapi::document();
    for (name, valid, confirmation) in [
        (
            "CatalogPromotionRequest",
            json!({"yaml":"example","expected_sha256":"a".repeat(64),"confirm_sha256":"a".repeat(64)}),
            "confirm_sha256",
        ),
        (
            "CatalogAdmissionRequest",
            json!({"expected_admitted":true,"admitted":false,"confirm_sha256":"a".repeat(64)}),
            "confirm_sha256",
        ),
        (
            "SuspensionRequest",
            json!({"expected_suspended":false,"suspended":true,"confirm_target":"00000000-0000-0000-0000-000000000001"}),
            "confirm_target",
        ),
        (
            "AdminCancelRequest",
            json!({"expected_state":"running","confirm_job":"00000000-0000-0000-0000-000000000001"}),
            "confirm_job",
        ),
    ] {
        let mut schema = document.clone();
        schema["$ref"] = json!(format!("#/components/schemas/{name}"));
        let validator = jsonschema::options()
            .should_validate_formats(true)
            .build(&schema)
            .unwrap();
        assert!(validator.is_valid(&valid), "{name}");
        let mut missing = valid.clone();
        missing.as_object_mut().unwrap().remove(confirmation);
        assert!(!validator.is_valid(&missing), "{name} missing confirmation");
        let mut forged = valid;
        forged["actor_id"] = json!("forged");
        assert!(!validator.is_valid(&forged), "{name} forged actor");
    }
}

#[test]
fn metadata_responses_reject_private_fields_and_malformed_identifiers() {
    let document = labyrinth_api::openapi::document();
    let mut schema = document.clone();
    schema["$ref"] = json!("#/components/schemas/Project");
    let validator = jsonschema::options()
        .should_validate_formats(true)
        .build(&schema)
        .unwrap();
    let mut project = json!({"project_id":"00000000-0000-0000-0000-000000000001","name":"Office","head_revision_id":null,"created_at":"2026-11-16T12:00:00Z"});
    assert!(validator.is_valid(&project));
    project["object_key"] = json!("private/storage/path");
    assert!(!validator.is_valid(&project));
    project.as_object_mut().unwrap().remove("object_key");
    project["project_id"] = json!("invalid");
    assert!(!validator.is_valid(&project));
    let audit = &document["paths"]["/v1/admin/audit"]["get"]["parameters"];
    assert!(
        audit
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["name"] == "before" && p["schema"]["format"] == "int64")
    );
    assert!(document["paths"]["/v1/admin/catalogs"]["post"]["requestBody"].is_object());
}
