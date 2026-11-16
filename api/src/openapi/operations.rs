use super::reference;
use serde_json::{Value, json};
fn response(schema: &str) -> Value {
    json!({"description":"Successful response","content":{"application/json":{"schema":reference(schema)}}})
}
pub(super) fn extend(doc: &mut Value) {
    // Request, response and pagination bindings are explicit; authority is checked server-side.
    for (path, method, id, request, result, page) in [
        ("/v1/me", "get", "inspectIdentity", "", "Identity", ""),
        (
            "/v1/onboarding/personal-workspace",
            "post",
            "onboardPersonalWorkspace",
            "OnboardingRequest",
            "Onboarding",
            "",
        ),
        (
            "/v1/tenants/{tenant}/projects",
            "post",
            "createProject",
            "CreateProjectRequest",
            "CreatedProject",
            "",
        ),
        (
            "/v1/tenants/{tenant}/projects",
            "get",
            "listProjects",
            "",
            "ProjectPage",
            "uuid",
        ),
        (
            "/v1/tenants/{tenant}/projects/{project}",
            "get",
            "getProject",
            "",
            "Project",
            "",
        ),
        (
            "/v1/tenants/{tenant}/projects/{project}/jobs",
            "post",
            "submitGeneration",
            "SubmitJobRequest",
            "AdmittedJob",
            "",
        ),
        (
            "/v1/tenants/{tenant}/projects/{project}/jobs",
            "get",
            "listJobs",
            "",
            "JobPage",
            "uuid",
        ),
        (
            "/v1/tenants/{tenant}/projects/{project}/jobs/{job}",
            "get",
            "getJob",
            "",
            "JobStatus",
            "",
        ),
        (
            "/v1/tenants/{tenant}/projects/{project}/jobs/{job}/cancel",
            "post",
            "cancelJob",
            "",
            "CancelledJob",
            "",
        ),
        (
            "/v1/tenants/{tenant}/projects/{project}/jobs/{job}/attempts",
            "get",
            "listAttempts",
            "",
            "Attempts",
            "",
        ),
        (
            "/v1/tenants/{tenant}/projects/{project}/revisions",
            "get",
            "listRevisions",
            "",
            "RevisionPage",
            "uuid",
        ),
        (
            "/v1/tenants/{tenant}/projects/{project}/revisions/{revision}",
            "get",
            "getRevision",
            "",
            "RevisionDetail",
            "",
        ),
        (
            "/v1/tenants/{tenant}/projects/{project}/revisions/{revision}/artifacts/{name}",
            "get",
            "downloadArtifact",
            "",
            "",
            "",
        ),
        (
            "/v1/tenants/{tenant}/catalogs",
            "get",
            "listCatalogs",
            "",
            "CatalogPage",
            "hash",
        ),
        (
            "/v1/tenants/{tenant}/catalogs/{hash}",
            "get",
            "getCatalog",
            "",
            "CatalogDetail",
            "",
        ),
        (
            "/v1/tenants/{tenant}/intent/validate",
            "post",
            "validateIntent",
            "ValidateIntentRequest",
            "ValidatedIntent",
            "",
        ),
        (
            "/v1/admin/overview",
            "get",
            "adminOverview",
            "",
            "AdminOverview",
            "",
        ),
        (
            "/v1/admin/users",
            "get",
            "adminUsers",
            "",
            "AdminUserPage",
            "uuid",
        ),
        (
            "/v1/admin/tenants",
            "get",
            "adminTenants",
            "",
            "AdminTenantPage",
            "uuid",
        ),
        (
            "/v1/admin/audit",
            "get",
            "adminAudit",
            "",
            "AuditPage",
            "audit",
        ),
        (
            "/v1/admin/catalogs/validate",
            "post",
            "adminValidateCatalog",
            "CatalogDraftRequest",
            "CatalogValidation",
            "",
        ),
        (
            "/v1/admin/catalogs",
            "post",
            "adminPromoteCatalog",
            "CatalogPromotionRequest",
            "CatalogPromotion",
            "",
        ),
        (
            "/v1/admin/catalogs/{hash}/admission",
            "post",
            "adminCatalogAdmission",
            "CatalogAdmissionRequest",
            "CatalogAdmission",
            "",
        ),
        (
            "/v1/admin/users/{id}/suspension",
            "post",
            "adminSuspendUser",
            "SuspensionRequest",
            "Suspension",
            "",
        ),
        (
            "/v1/admin/tenants/{id}/suspension",
            "post",
            "adminSuspendTenant",
            "SuspensionRequest",
            "Suspension",
            "",
        ),
        (
            "/v1/admin/tenants/{tenant}/projects/{project}/jobs/{job}",
            "get",
            "adminGetJob",
            "",
            "AdminJob",
            "",
        ),
        (
            "/v1/admin/tenants/{tenant}/projects/{project}/jobs/{job}/cancel",
            "post",
            "adminCancelJob",
            "AdminCancelRequest",
            "CancelledJob",
            "",
        ),
    ] {
        let mut parameters = Vec::new();
        for part in path.split('/') {
            if let Some(name) = part.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
                let schema = match name {
                    "hash" => json!({"type":"string","pattern":"^[0-9a-f]{64}$"}),
                    "name" => reference("ArtifactName"),
                    _ => json!({"type":"string","format":"uuid"}),
                };
                parameters.push(json!({"name":name,"in":"path","required":true,"schema":schema}));
            }
        }
        if !page.is_empty() {
            parameters.push(json!({"name":"limit","in":"query","schema":{"type":"integer","minimum":1,"maximum":100,"default":25}}));
            let (name, schema) = match page {
                "audit" => (
                    "before",
                    json!({"type":"integer","format":"int64","minimum":1}),
                ),
                "hash" => ("after", json!({"type":"string","pattern":"^[0-9a-f]{64}$"})),
                _ => ("after", json!({"type":"string","format":"uuid"})),
            };
            parameters.push(json!({"name":name,"in":"query","schema":schema}));
        }
        if ["createProject", "submitGeneration"].contains(&id) {
            parameters.push(json!({"name":"Idempotency-Key","in":"header","required":true,"schema":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9._:-]+$"}}));
        }
        let mut op = json!({"operationId":id,"parameters":parameters,"responses":{"default":{"description":"Structured error; status semantics and engine detail payloads are documented in docs/contracts","content":{"application/json":{"schema":reference("Error")}}}}});
        if !request.is_empty() {
            op["requestBody"] = json!({"required":true,"content":{"application/json":{"schema":reference(request)}}});
        }
        if result.is_empty() {
            op["responses"]["200"] = json!({"description":"Verified artifact JSON file, streamed as an attachment. Payload type depends on the allowlisted file name.","headers":{"ETag":{"schema":{"type":"string"}},"Content-Disposition":{"schema":{"type":"string"}},"Cache-Control":{"schema":{"const":"no-store"}}},"content":{"application/json":{"schema":{}}}});
        } else {
            op["responses"]["200"] = response(result);
        }
        if ["createProject", "onboardPersonalWorkspace"].contains(&id) {
            op["responses"]["201"] = response(result);
        }
        if id == "submitGeneration" {
            op["responses"]["202"] = response(result);
            op["responses"]["202"]["headers"] = json!({"Location":{"schema":{"type":"string"},"description":"Scoped job status URL"}});
        }
        if path.starts_with("/v1/admin/") {
            op["x-required-role"] = json!("admin");
            op["description"] = json!(
                "Requires current operator-granted admin role; grants no automatic access to private topology artifacts. Confirmations and expected state are checked by the server."
            );
        }
        if doc["paths"].get(path).is_none() {
            doc["paths"][path] = json!({});
        }
        doc["paths"][path][method] = op;
    }
}
