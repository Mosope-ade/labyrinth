use super::{imported, object, reference};
use serde_json::{Value, json};
fn nullable(value: Value) -> Value {
    json!({"anyOf":[value,{"type":"null"}]})
}
fn array(value: Value) -> Value {
    json!({"type":"array","items":value})
}
fn record(fields: Value) -> Value {
    let required: Vec<&str> = fields
        .as_object()
        .expect("fields")
        .keys()
        .map(String::as_str)
        .collect();
    object(fields.clone(), &required)
}
fn page(item: &str, cursor: Value) -> Value {
    record(json!({"items":array(reference(item)),"next_after":nullable(cursor)}))
}
pub(super) fn extend(document: &mut Value) {
    let s = &mut document["components"]["schemas"];
    let uuid = json!({"type":"string","format":"uuid"});
    let text = json!({"type":"string"});
    let boolean = json!({"type":"boolean"});
    let count = json!({"type":"integer","minimum":0});
    let date = json!({"type":"string","format":"date-time"});
    let hash = json!({"type":"string","pattern":"^[0-9a-f]{64}$"});
    s["GenerationError"] = imported(
        serde_json::to_value(schemars::schema_for!(
            labyrinth_engine::artifacts::GenerationError
        ))
        .expect("generation error schema"),
        "GenerationError",
    );
    s["EngineEvidence"] = record(
        json!({"schema_version":{"const":"engine_failure_v1"},"outcome":{"type":"string","enum":["rejected","inconclusive"]},"truncated":boolean,"error":nullable(reference("GenerationError"))}),
    );
    s["AttemptEvidence"] =
        record(json!({"attempt_id":uuid,"evidence":nullable(reference("EngineEvidence"))}));
    s["JobState"] = json!({"type":"string","enum":["queued","running","retry_wait","cancel_requested","cancelled","failed","succeeded"]});
    s["Identity"] = record(
        json!({"user_id":uuid,"provisioned":boolean,"personal_tenant_id":nullable(uuid.clone())}),
    );
    s["Onboarding"] = record(
        json!({"tenant_id":uuid,"role":{"type":"string","enum":["owner","editor","viewer"]},"max_active_jobs":count,"max_projects":count,"replayed":boolean}),
    );
    let project = json!({"project_id":uuid,"name":text,"head_revision_id":nullable(uuid.clone()),"created_at":date});
    s["Project"] = record(project.clone());
    let mut created = project;
    created["replayed"] = boolean.clone();
    s["CreatedProject"] = record(created);
    s["ProjectPage"] = page("Project", uuid.clone());
    s["AdmittedJob"] =
        record(json!({"job_id":uuid,"state":reference("JobState"),"replayed":boolean}));
    s["CancelledJob"] = record(json!({"job_id":uuid,"state":reference("JobState")}));
    s["JobStatus"] = record(
        json!({"job_id":uuid,"state":reference("JobState"),"created_at":date,"revision":nullable(record(json!({"revision_id":uuid,"manifest_sha256":hash})))}),
    );
    s["JobSummary"] = record(
        json!({"job_id":uuid,"state":reference("JobState"),"created_at":date,"revision_id":nullable(uuid.clone())}),
    );
    s["JobPage"] = page("JobSummary", uuid.clone());
    let revision = json!({"revision_id":uuid,"job_id":uuid,"manifest_sha256":hash,"catalog_sha256":hash,"created_at":date});
    s["Revision"] = record(revision.clone());
    s["RevisionPage"] = page("Revision", uuid.clone());
    s["ArtifactName"] = json!({"type":"string","enum":["manifest.json","bom.json","catalog.json","decisions.json","firewall.json","ipam.json","options.json","provenance.json","requirements.json","routing.json","selection.json","topology.json","verification.json"]});
    let mut detail = revision;
    detail["artifacts"] = array(record(
        json!({"name":reference("ArtifactName"),"sha256":hash,"size_bytes":count}),
    ));
    s["RevisionDetail"] = record(detail);
    s["CatalogSummary"] = record(json!({"sha256":hash,"semantic_version":text,"created_at":date}));
    s["CatalogPage"] = page("CatalogSummary", hash.clone());
    s["CatalogVersion"] = record(json!({"semantic_version":text,"sha256":hash}));
    s["CatalogBundle"] = imported(
        serde_json::to_value(labyrinth_engine::catalog::catalog_schema()).expect("catalog schema"),
        "CatalogBundle",
    );
    s["CatalogDetail"] = record(
        json!({"catalog_version":reference("CatalogVersion"),"bundle":reference("CatalogBundle")}),
    );
    s["Requirements"] = imported(
        serde_json::to_value(schemars::schema_for!(labyrinth_engine::intent::IntentData))
            .expect("requirements schema"),
        "Requirements",
    );
    s["ValidateIntentRequest"] = object(
        json!({"intent":reference("Intent"),"options":reference("GenerationOptions")}),
        &["intent"],
    );
    s["ValidatedIntent"] = record(
        json!({"status":{"const":"ready"},"requirements":reference("Requirements"),"provenance":{"type":"object","additionalProperties":{"type":"string","enum":["provided","engine_default"]}},"assurance":{"const":"requirements_validated_not_generated"}}),
    );
    s["Diagnostic"] = record(
        json!({"code":text,"message":text,"outcome":{"type":"string","enum":["rejected","inconclusive","unavailable","conflict","cancelled","verified","unknown"]}}),
    );
    s["Attempt"] = record(
        json!({"attempt_id":uuid,"attempt_number":{"type":"integer","minimum":1,"maximum":3},"started_at":nullable(date.clone()),"finished_at":nullable(date.clone()),"retry_scheduled":boolean,"retry_at":nullable(date.clone()),"diagnostic":nullable(reference("Diagnostic"))}),
    );
    s["Attempts"] = record(json!({"job_id":uuid,"items":array(reference("Attempt"))}));
    s["AdminOverview"] = record(
        json!({"enabled_users":count,"tenants":count,"projects":count,"job_counts":{"type":"object","additionalProperties":count},"attempt_counts":{"type":"object","additionalProperties":count},"average_completed_attempt_seconds":nullable(json!({"type":"number","minimum":0})),"artifact_bytes":count,"ai_telemetry":record(json!({"available":{"const":false}}))}),
    );
    s["AdminUser"] = record(json!({"id":uuid,"suspended":boolean,"created_at":date}));
    s["AdminUserPage"] = page("AdminUser", uuid.clone());
    s["AdminTenant"] = record(
        json!({"id":uuid,"suspended":boolean,"max_active_jobs":count,"max_projects":nullable(count.clone()),"project_count":count,"artifact_bytes":count}),
    );
    s["AdminTenantPage"] = page("AdminTenant", uuid.clone());
    s["AuditRecord"] = record(
        json!({"id":{"type":"string","pattern":"^[1-9][0-9]*$"},"actor_id":nullable(uuid.clone()),"operator_principal":nullable(text.clone()),"action":text,"target_kind":text,"target_reference":nullable(text.clone()),"outcome":{"type":"string","enum":["allowed","denied","failed"]},"occurred_at":date}),
    );
    s["AuditPage"] = record(
        json!({"items":array(reference("AuditRecord")),"next_before":nullable(json!({"type":"string","pattern":"^[1-9][0-9]*$"}))}),
    );
    s["CatalogDraftRequest"] = record(json!({"yaml":text}));
    s["CatalogPromotionRequest"] =
        record(json!({"yaml":text,"expected_sha256":hash,"confirm_sha256":hash}));
    s["CatalogValidation"] =
        record(json!({"catalog_version":reference("CatalogVersion"),"archetypes":count}));
    s["CatalogPromotion"] = record(
        json!({"sha256":hash,"semantic_version":text,"admitted":boolean,"replayed":boolean}),
    );
    s["CatalogAdmissionRequest"] =
        record(json!({"expected_admitted":boolean,"admitted":boolean,"confirm_sha256":hash}));
    s["CatalogAdmission"] = record(json!({"sha256":hash,"admitted":boolean}));
    s["SuspensionRequest"] =
        record(json!({"expected_suspended":boolean,"suspended":boolean,"confirm_target":uuid}));
    s["Suspension"] = record(json!({"id":uuid,"suspended":boolean}));
    s["AdminCancelRequest"] =
        record(json!({"expected_state":reference("JobState"),"confirm_job":uuid}));
    s["AdminJob"] = record(
        json!({"tenant_id":uuid,"project_id":uuid,"job_id":uuid,"state":reference("JobState"),"created_at":date,"attempts":count}),
    );
    super::operations::extend(document);
}
