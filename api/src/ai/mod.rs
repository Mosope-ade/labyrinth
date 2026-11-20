//! Provider-independent proposal review. Review itself performs no I/O or admission.
//! The opt-in `gemini` submodule supplies a separate transport; no hosted AI route exists.
//! Engine readiness never implies user approval, feasibility or verification.
mod types;
use labyrinth_engine::{
    artifacts::GenerationOptions, catalog::CatalogSnapshot, diagnostics::JsonInputError, intent,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
pub use types::*;

pub const MAX_PROPOSAL_BYTES: usize = intent::MAX_JSON_BYTES;
pub const MAX_PROMPT_BYTES: usize = 16 * 1024;
pub const MAX_ASSUMPTIONS: usize = 64;
pub const MAX_REVIEW_OPTIONS_BYTES: usize = 65_536;

impl ReviewContext {
    /// Server-owned context only. This does not authorize access; admission must
    /// recheck current membership, catalog eligibility and the project head.
    pub fn new(
        admission: crate::jobs::AdmissionContext,
        catalog: &CatalogSnapshot,
        options: &GenerationOptions,
    ) -> Result<Self, ProposalError> {
        if [
            admission.actor_id,
            admission.tenant_id,
            admission.project_id,
        ]
        .iter()
        .any(uuid::Uuid::is_nil)
            || admission.expected_head.is_some_and(|id| id.is_nil())
        {
            return Err(ProposalError::InvalidContext);
        }
        let generation_options = crate::canonical::value(
            serde_json::to_value(options).map_err(|_| ProposalError::InvalidContext)?,
        );
        if serde_json::to_vec_pretty(&generation_options)
            .map_err(|_| ProposalError::InvalidContext)?
            .len()
            > MAX_REVIEW_OPTIONS_BYTES
        {
            return Err(ProposalError::InvalidContext);
        }
        Ok(Self {
            actor_id: admission.actor_id,
            tenant_id: admission.tenant_id,
            project_id: admission.project_id,
            expected_head: admission.expected_head,
            catalog_sha256: catalog.version().sha256.clone(),
            generation_options,
        })
    }
}
impl ProviderMetadata {
    /// Identifiers come from trusted adapter configuration, never model output.
    pub fn new(provider: &str, model: &str) -> Result<Self, ProposalError> {
        if [provider, model]
            .iter()
            .any(|v| v.len() > 100 || v.trim().is_empty() || v.chars().any(char::is_control))
        {
            return Err(ProposalError::InvalidProvider);
        }
        Ok(Self {
            provider: provider.into(),
            model: model.into(),
            profile: None,
        })
    }
}
impl ProviderMetadata {
    pub fn with_profile(mut self, profile: &str) -> Result<Self, ProposalError> {
        if profile.len() > 100 || profile.trim().is_empty() || profile.chars().any(char::is_control)
        {
            return Err(ProposalError::InvalidProvider);
        }
        self.profile = Some(profile.into());
        Ok(self)
    }
}
impl ReviewedProposal {
    /// Parse the exact untrusted response before constructing any generic JSON map.
    /// Even a complete intent remains an unconfirmed proposal.
    pub fn parse(
        raw_output: &str,
        prompt: &str,
        context: ReviewContext,
        provider: ProviderMetadata,
    ) -> Result<Self, ProposalError> {
        if raw_output.len() > MAX_PROPOSAL_BYTES {
            return Err(ProposalError::TooLarge);
        }
        if prompt.len() > MAX_PROMPT_BYTES || prompt.trim().is_empty() {
            return Err(ProposalError::InvalidPrompt);
        }
        let output: ProposalOutput =
            serde_json::from_str(raw_output).map_err(|_| ProposalError::InvalidOutput)?;
        if output.schema_version != "ai_proposal_v1" || output.assumptions.len() > MAX_ASSUMPTIONS {
            return Err(ProposalError::InvalidOutput);
        }
        let readiness = intent::parse_json(output.intent.get()).map_err(|error| match error {
            JsonInputError::Invalid(errors) => ProposalError::InvalidIntent(errors),
            _ => ProposalError::InvalidOutput,
        })?;
        let mut defaults = std::collections::BTreeMap::new();
        let readiness = match readiness {
            intent::Readiness::NeedsClarification(fields) => {
                ProposalReadiness::NeedsClarification(fields)
            }
            intent::Readiness::Ready(resolved) => {
                let data = serde_json::to_value(resolved.data())
                    .map_err(|_| ProposalError::InvalidOutput)?;
                for (path, source) in resolved.provenance() {
                    if *source == intent::ValueSource::EngineDefault {
                        let value = data.pointer(path).ok_or(ProposalError::InvalidOutput)?;
                        defaults.insert(path.clone(), value.clone());
                    }
                }
                ProposalReadiness::ReadyForReview
            }
        };
        // Typed engine parsing above rejects duplicate fields before map creation.
        let value: serde_json::Value =
            serde_json::from_str(output.intent.get()).map_err(|_| ProposalError::InvalidOutput)?;
        let mut paths = BTreeSet::new();
        for assumption in &output.assumptions {
            if assumption.path.is_empty()
                || assumption.path.len() > 256
                || !assumption.path.starts_with('/')
                || assumption.explanation.len() > 1024
                || assumption.explanation.trim().is_empty()
                || !paths.insert(&assumption.path)
                || value
                    .pointer(&assumption.path)
                    .is_none_or(serde_json::Value::is_null)
            {
                return Err(ProposalError::InvalidOutput);
            }
        }
        let prompt_sha256 = hash(prompt.as_bytes());
        let digest = review_digest(raw_output, &prompt_sha256, &context, &provider)?;
        Ok(Self {
            output,
            context,
            provider,
            prompt_sha256,
            digest,
            readiness,
            defaults,
        })
    }
    pub fn digest(&self) -> &str {
        &self.digest
    }
    pub fn readiness(&self) -> &ProposalReadiness {
        &self.readiness
    }
    pub fn defaults(&self) -> &std::collections::BTreeMap<String, serde_json::Value> {
        &self.defaults
    }
    /// Contains sensitive design data and untrusted model text; render as text,
    /// never log it wholesale. It contains the prompt hash, not the raw prompt.
    pub fn view(&self) -> ProposalView<'_> {
        ProposalView {
            schema_version: "ai_review_v1",
            review_state: "awaiting_confirmation",
            proposal_sha256: &self.digest,
            context: &self.context,
            provider: &self.provider,
            prompt_sha256: &self.prompt_sha256,
            intent: &self.output.intent,
            assumptions: &self.output.assumptions,
            engine_readiness: &self.readiness,
            engine_defaults: &self.defaults,
        }
    }
    /// An acknowledgement is not an authorization token or proof of human review.
    /// This consumes the in-memory draft, preventing accidental repeated use here.
    pub fn confirm(
        self,
        current_context: &ReviewContext,
        confirmation: Confirmation,
    ) -> Result<ConfirmedProposal, ProposalError> {
        if current_context != &self.context
            || confirmation.schema_version != "ai_confirmation_v1"
            || confirmation.proposal_sha256 != self.digest
        {
            return Err(ProposalError::ReviewConflict);
        }
        if !matches!(self.readiness, ProposalReadiness::ReadyForReview) {
            return Err(ProposalError::NeedsClarification);
        }
        Ok(ConfirmedProposal { reviewed: self })
    }
}
impl ConfirmedProposal {
    /// Submit through existing authenticated preparation/admission, not directly
    /// to the engine or queue. The normal path revalidates and authorizes it.
    pub fn raw_intent(&self) -> &str {
        self.reviewed.output.intent.get()
    }
    pub fn review_digest(&self) -> &str {
        &self.reviewed.digest
    }
    pub fn context(&self) -> &ReviewContext {
        &self.reviewed.context
    }
    pub fn provider(&self) -> &ProviderMetadata {
        &self.reviewed.provider
    }
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn review_digest(
    raw_output: &str,
    prompt_sha256: &str,
    context: &ReviewContext,
    provider: &ProviderMetadata,
) -> Result<String, ProposalError> {
    let context = crate::canonical::value(
        serde_json::to_value(context).map_err(|_| ProposalError::InvalidContext)?,
    );
    let context = serde_json::to_vec(&context).map_err(|_| ProposalError::InvalidContext)?;
    let provider = serde_json::to_vec(provider).map_err(|_| ProposalError::InvalidProvider)?;
    let mut hash = Sha256::new();
    for part in [
        b"ai_review_digest_v2".as_slice(),
        raw_output.as_bytes(),
        prompt_sha256.as_bytes(),
        &context,
        &provider,
    ] {
        hash.update((part.len() as u64).to_be_bytes());
        hash.update(part);
    }
    Ok(format!("{:x}", hash.finalize()))
}

pub fn proposal_schema() -> schemars::Schema {
    schemars::schema_for!(ProposalOutput)
}
pub fn confirmation_schema() -> schemars::Schema {
    schemars::schema_for!(Confirmation)
}

impl std::fmt::Debug for ReviewedProposal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReviewedProposal")
            .field("digest", &self.digest)
            .finish_non_exhaustive()
    }
}
impl std::fmt::Debug for ConfirmedProposal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfirmedProposal").finish_non_exhaustive()
    }
}

impl std::fmt::Debug for ReviewContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReviewContext")
            .field("actor_id", &self.actor_id)
            .field("tenant_id", &self.tenant_id)
            .field("project_id", &self.project_id)
            .field("expected_head", &self.expected_head)
            .field("catalog_sha256", &self.catalog_sha256)
            .finish_non_exhaustive()
    }
}

pub mod gemini;
