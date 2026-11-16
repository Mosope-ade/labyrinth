mod operations;
mod responses;
// Machine-readable hosted API contracts; engine semantic checks remain authoritative.
use serde_json::{Value, json};

fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
fn reference(name: &str) -> Value {
    json!({"$ref":format!("#/components/schemas/{name}")})
}
fn imported(mut schema: Value, name: &str) -> Value {
    fn rebase(value: &mut Value, name: &str) {
        match value {
            Value::Object(map) => {
                if let Some(Value::String(r)) = map.get_mut("$ref")
                    && let Some(suffix) = r.strip_prefix('#')
                {
                    *r = format!("#/components/schemas/{name}{suffix}");
                }
                for child in map.values_mut() {
                    rebase(child, name);
                }
            }
            Value::Array(items) => {
                for child in items {
                    rebase(child, name);
                }
            }
            _ => {}
        }
    }
    schema
        .as_object_mut()
        .expect("schema object")
        .remove("$schema");
    rebase(&mut schema, name);
    schema
}
/// Export the hosted API contract; semantic engine checks still run after schema validation.
pub fn document() -> Value {
    let uuid = json!({"type":"string","format":"uuid"});
    let hash = json!({"type":"string","pattern":"^[0-9a-f]{64}$"});
    let mut schemas = json!({
        "OnboardingRequest":object(json!({}), &[]),
        "CreateProjectRequest":object(json!({"name":{"type":"string","minLength":1,"maxLength":200}}), &["name"]),
        "SubmitJobRequest":object(json!({"intent":reference("Intent"),"options":reference("GenerationOptions"),"catalog_sha256":hash,"expected_head_revision_id":{"anyOf":[uuid,{"type":"null"}]}}), &["intent","catalog_sha256","expected_head_revision_id"]),
        "Error":object(json!({"error":object(json!({"code":{"type":"string"},"message":{"type":"string"},"details":{}}), &["code","message"])}), &["error"])
    });
    schemas["Intent"] = imported(
        serde_json::to_value(labyrinth_engine::intent::intent_schema()).expect("schema"),
        "Intent",
    );
    schemas["GenerationOptions"] = imported(
        serde_json::to_value(schemars::schema_for!(
            labyrinth_engine::artifacts::GenerationOptions
        ))
        .expect("schema"),
        "GenerationOptions",
    );
    let mut paths = json!({});
    for path in ["/health/live", "/health/ready"] {
        paths[path] = json!({"get":{"operationId":if path.ends_with("live") {"liveness"} else {"readiness"},"security":[],"responses":{"204":{"description":"Healthy"},"503":{"description":"Not ready (readiness only)"}}}});
    }
    paths["/health/live"]["get"]["responses"]
        .as_object_mut()
        .expect("responses")
        .remove("503");
    let mut document = json!({"openapi":"3.1.0","info":{"title":"Labyrinth hosted API","version":env!("CARGO_PKG_VERSION"),"description":"Hosted API requests, response envelopes, discovery and admin operations. Engine error details and artifact payload types are described in docs/contracts."},"servers":[{"url":"/"}],"security":[{"bearerAuth":[]}],"paths":paths,"components":{"securitySchemes":{"bearerAuth":{"type":"http","scheme":"bearer","bearerFormat":"JWT"}},"schemas":schemas}});
    responses::extend(&mut document);
    document
}

#[cfg(all(test, feature = "postgres-tests"))]
pub(crate) fn assert_response(method: &str, uri: &str, status: u16, body: &Value) {
    static DOCUMENT: std::sync::LazyLock<Value> = std::sync::LazyLock::new(document);
    let path = uri.split('?').next().unwrap();
    let segments: Vec<_> = path.split('/').collect();
    let (_, item) = DOCUMENT["paths"]
        .as_object()
        .unwrap()
        .iter()
        .find(|(pattern, _)| {
            let parts: Vec<_> = pattern.split('/').collect();
            parts.len() == segments.len()
                && parts
                    .iter()
                    .zip(&segments)
                    .all(|(a, b)| (a.starts_with('{') && a.ends_with('}')) || a == b)
        })
        .unwrap_or_else(|| panic!("undocumented route {method} {path}"));
    let operation = &item[method.to_ascii_lowercase()];
    assert!(operation.is_object(), "undocumented method {method} {path}");
    let responses = &operation["responses"];
    let response = responses
        .get(status.to_string())
        .unwrap_or(&responses["default"]);
    let expected = &response["content"]["application/json"]["schema"];
    assert!(
        expected.is_object(),
        "undocumented response {status} {path}"
    );
    let mut schema = DOCUMENT.clone();
    schema
        .as_object_mut()
        .unwrap()
        .extend(expected.as_object().unwrap().clone());
    let validator = jsonschema::options()
        .should_validate_formats(true)
        .build(&schema)
        .unwrap();
    let errors: Vec<_> = validator
        .iter_errors(body)
        .map(|e| format!("{} at {}", e, e.instance_path))
        .collect();
    assert!(
        errors.is_empty(),
        "response contract mismatch {method} {path} {status}: {errors:?}"
    );
}
