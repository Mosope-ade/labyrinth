use super::AdmissionError;
use labyrinth_engine::{
    artifacts::{self, GenerationOptions},
    catalog::CatalogSnapshot,
    intent::{self, BlueprintChoice, Readiness, UpdateMode},
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Server context: actor_id must come from verified authentication, never a body
/// field. PostgreSQL rechecks current membership/suspension inside admission.
#[derive(Debug, Clone)]
pub struct AdmissionContext {
    pub actor_id: Uuid,
    pub tenant_id: Uuid,
    pub project_id: Uuid,
    /// Explicit optimistic concurrency precondition; None means an empty project.
    pub expected_head: Option<Uuid>,
}
/// Only validated preparation constructs a job payload. Readiness is not proof of
/// feasibility; workers still verify pins and run the complete core pipeline.
#[derive(Debug)]
pub struct PreparedAdmission {
    pub(super) context: AdmissionContext,
    pub(super) key: String,
    pub(super) digest: String,
    pub(super) requirements: Value,
    pub(super) provenance: Value,
    pub(super) options: Value,
    pub(super) catalog: String,
    pub(super) base: Option<Uuid>,
}
impl PreparedAdmission {
    /// Prepare bounded input using a server-loaded validated catalog snapshot.
    /// No computation, storage mutation, or authorization is performed here.
    pub fn new(
        context: AdmissionContext,
        key: &str,
        raw_intent: &str,
        options: &GenerationOptions,
        catalog: &CatalogSnapshot,
    ) -> Result<Self, AdmissionError> {
        if key.is_empty()
            || key.len() > 128
            || !key
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._:-".contains(&c))
        {
            return Err(AdmissionError::InvalidInput(
                "invalid idempotency key".into(),
            ));
        }
        let ready = intent::parse_json(raw_intent)
            .map_err(|e| AdmissionError::InvalidInput(e.to_string()))?;
        let intent = match ready {
            Readiness::Ready(intent) => intent,
            Readiness::NeedsClarification(fields) => {
                return Err(AdmissionError::NeedsClarification(fields));
            }
        };
        if !matches!(
            intent.data().blueprint,
            BlueprintChoice::Automatic | BlueprintChoice::HubSpoke
        ) {
            return Err(AdmissionError::InvalidInput("unsupported blueprint".into()));
        }
        artifacts::validate_options(&intent, options)
            .map_err(|e| AdmissionError::InvalidInput(e.to_string()))?;
        let base = match &intent.data().update {
            UpdateMode::StatelessRedesign => None,
            UpdateMode::IncrementalUpdate { base_revision, .. } => {
                let id = base_revision
                    .as_str()
                    .strip_prefix("rev-")
                    .and_then(|s| Uuid::parse_str(s).ok())
                    .ok_or_else(|| {
                        AdmissionError::InvalidInput(
                            "hosted base revision must be rev-<canonical UUID>".into(),
                        )
                    })?;
                if engine_revision_id(id).as_str() != base_revision.as_str() {
                    return Err(AdmissionError::InvalidInput(
                        "noncanonical hosted revision ID".into(),
                    ));
                }
                Some(id)
            }
        };
        if base.is_some() && base != context.expected_head {
            return Err(AdmissionError::StaleHead);
        }
        let requirement_bytes = artifacts::canonical_requirements(&intent)
            .map_err(|e| AdmissionError::InvalidInput(e.to_string()))?;
        let provenance_bytes = artifacts::canonical_provenance(&intent)
            .map_err(|e| AdmissionError::InvalidInput(e.to_string()))?;
        if requirement_bytes.len() > intent::MAX_JSON_BYTES
            || provenance_bytes.len() > intent::MAX_JSON_BYTES
        {
            return Err(AdmissionError::InvalidInput(
                "normalized input exceeds 1 MiB".into(),
            ));
        }
        let requirements = serde_json::from_slice(&requirement_bytes)?;
        let provenance = serde_json::from_slice(&provenance_bytes)?;
        let options = serde_json::to_value(options)?;
        if serde_json::to_vec_pretty(&options)?.len() > 65536 {
            return Err(AdmissionError::InvalidInput(
                "generation options exceed 64 KiB".into(),
            ));
        }
        let catalog = catalog.version().sha256.clone();
        let identity = serde_json::json!({"contract":"admission_v1","tenant":context.tenant_id,"actor":context.actor_id,"project":context.project_id,"expected_head":context.expected_head,"requirements":requirements,"provenance":provenance,"options":options,"catalog":catalog});
        let digest = format!("{:x}", Sha256::digest(serde_json::to_vec(&identity)?));
        Ok(Self {
            context,
            key: key.into(),
            digest,
            requirements,
            provenance,
            options,
            catalog,
            base,
        })
    }
    pub fn request_sha256(&self) -> &str {
        &self.digest
    }
}

/// Namespaces database UUIDs to satisfy the core's letter-prefixed stable-ID contract.
pub fn engine_revision_id(id: Uuid) -> labyrinth_engine::model::RevisionId {
    labyrinth_engine::model::RevisionId::new(format!("rev-{id}"))
        .expect("canonical UUID with fixed alphabetic prefix")
}
