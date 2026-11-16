//! Machine-readable admission contracts. Other endpoints remain documented in docs/contracts.
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
/// Export the supported admission subset; semantic engine checks still run after schema validation.
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
    for (path, id, request, success) in [
        (
            "/v1/onboarding/personal-workspace",
            "onboardPersonalWorkspace",
            "OnboardingRequest",
            "201",
        ),
        (
            "/v1/tenants/{tenant}/projects",
            "createProject",
            "CreateProjectRequest",
            "201",
        ),
        (
            "/v1/tenants/{tenant}/projects/{project}/jobs",
            "submitGeneration",
            "SubmitJobRequest",
            "202",
        ),
    ] {
        let mut parameters = Vec::new();
        for name in ["tenant", "project"] {
            if path.contains(&format!("{{{name}}}")) {
                parameters.push(json!({"name":name,"in":"path","required":true,"schema":uuid}));
            }
        }
        if request != "OnboardingRequest" {
            parameters.push(json!({"name":"Idempotency-Key","in":"header","required":true,"schema":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9._:-]+$"}}));
        }
        let mut responses = json!({"200":{"description":"Idempotent replay"},"default":{"description":"Structured error; see endpoint Markdown contract for status semantics","content":{"application/json":{"schema":reference("Error")}}}});
        responses[success] = json!({"description":"Created or admitted"});
        paths[path] = json!({"post":{"operationId":id,"parameters":parameters,"requestBody":{"required":true,"content":{"application/json":{"schema":reference(request)}}},"responses":responses}});
    }
    for path in ["/health/live", "/health/ready"] {
        paths[path] = json!({"get":{"operationId":if path.ends_with("live") {"liveness"} else {"readiness"},"security":[],"responses":{"204":{"description":"Healthy"},"503":{"description":"Not ready (readiness only)"}}}});
    }
    paths["/health/live"]["get"]["responses"]
        .as_object_mut()
        .expect("responses")
        .remove("503");
    json!({"openapi":"3.1.0","info":{"title":"Labyrinth admission API","version":env!("CARGO_PKG_VERSION"),"description":"Admission request subset. Response payloads, discovery, downloads and admin operations are specified in docs/contracts; this document does not claim complete API coverage."},"servers":[{"url":"/"}],"security":[{"bearerAuth":[]}],"paths":paths,"components":{"securitySchemes":{"bearerAuth":{"type":"http","scheme":"bearer","bearerFormat":"JWT"}},"schemas":schemas}})
}
