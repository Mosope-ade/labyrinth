use crate::{Context, Error, Result};
use labyrinth_engine::{
    artifacts::{self, BaseRevision, GenerationOptions, PackageManifest},
    catalog, intent,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
pub const NAMES: [&str; 13] = [
    "bom.json",
    "catalog.json",
    "decisions.json",
    "firewall.json",
    "ipam.json",
    "manifest.json",
    "options.json",
    "provenance.json",
    "requirements.json",
    "routing.json",
    "selection.json",
    "topology.json",
    "verification.json",
];
pub type Files = BTreeMap<String, Vec<u8>>;
pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
#[derive(Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct Object {
    pub name: String,
    pub object_key: String,
    pub sha256: String,
    pub size_bytes: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineInput {
    pub context: Context,
    pub base: Option<Files>,
}
/// Child-only entry point. No credentials, networking or filesystem access is needed.
pub fn generate(input: EngineInput) -> Result<Files> {
    let c = input.context;
    let catalog = catalog::load_yaml(&c.catalog_yaml).map_err(|_| Error::Invalid)?;
    if catalog.version().sha256 != c.catalog_sha256 {
        return Err(Error::Invalid);
    }
    let intent =
        intent::restore_snapshot(c.requirements, c.provenance).map_err(|_| Error::Invalid)?;
    let options: GenerationOptions =
        serde_json::from_value(c.options).map_err(|_| Error::Invalid)?;
    let p = match (c.base_revision_id, c.base_manifest_sha256, input.base) {
        (None, None, None) => artifacts::generate(&intent, &catalog, &options),
        (Some(id), Some(digest), Some(files)) => {
            let revision = serde_json::from_value(serde_json::json!(format!("rev-{id}")))
                .map_err(|_| Error::Invalid)?;
            let base = BaseRevision::load_saved(revision, &digest, &files, &catalog)
                .map_err(|_| Error::Invalid)?;
            artifacts::generate_incremental(&intent, &catalog, &options, &base)
        }
        _ => return Err(Error::Invalid),
    }
    .map_err(|error| {
        let resource = matches!(classify_generation_error(&error), Error::ResourceLimit);
        Error::Engine(Box::new(crate::evidence::Evidence::from_engine(
            &error, resource,
        )))
    })?;
    Ok(p.files().clone())
}
pub fn validate(files: &Files, c: &Context, limit: usize) -> Result<Vec<Object>> {
    if files.len() != NAMES.len()
        || files.keys().any(|n| !NAMES.contains(&n.as_str()))
        || files.values().any(|b| b.is_empty())
        || files
            .values()
            .try_fold(0usize, |n, b| n.checked_add(b.len()))
            .is_none_or(|n| n > limit)
    {
        return Err(Error::Invalid);
    }
    let manifest: PackageManifest =
        serde_json::from_slice(&files["manifest.json"]).map_err(|_| Error::Invalid)?;
    if manifest.schema_version != "engineering_package_v1"
        || manifest.catalog_version.sha256 != c.catalog_sha256
        || manifest.files.len() != 12
    {
        return Err(Error::Invalid);
    }
    for (name, b) in files {
        if name != "manifest.json"
            && manifest
                .files
                .get(name)
                .is_none_or(|d| d.sha256 != hash(b) || d.size_bytes != b.len() as u64)
        {
            return Err(Error::Invalid);
        }
    }
    let intent = intent::restore_snapshot(c.requirements.clone(), c.provenance.clone())
        .map_err(|_| Error::Invalid)?;
    let options: GenerationOptions =
        serde_json::from_value(c.options.clone()).map_err(|_| Error::Invalid)?;
    if files["requirements.json"]
        != artifacts::canonical_requirements(&intent).map_err(|_| Error::Invalid)?
        || files["provenance.json"]
            != artifacts::canonical_provenance(&intent).map_err(|_| Error::Invalid)?
        || serde_json::from_slice::<serde_json::Value>(&files["options.json"])
            .map_err(|_| Error::Invalid)?
            != serde_json::to_value(options).map_err(|_| Error::Invalid)?
        || manifest.requirements_sha256 != hash(&files["requirements.json"])
        || manifest.options_sha256 != hash(&files["options.json"])
        || manifest
            .base_revision
            .as_ref()
            .map(|r| r.revision.to_string())
            != c.base_revision_id.map(|r| format!("rev-{r}"))
        || manifest
            .base_revision
            .as_ref()
            .map(|r| r.manifest_sha256.clone())
            != c.base_manifest_sha256
    {
        return Err(Error::Invalid);
    }
    let catalog = catalog::load_yaml(&c.catalog_yaml).map_err(|_| Error::Invalid)?;
    BaseRevision::load_saved(
        serde_json::from_value(serde_json::json!("rev-validation")).map_err(|_| Error::Invalid)?,
        &hash(&files["manifest.json"]),
        files,
        &catalog,
    )
    .map_err(|_| Error::Invalid)?;
    Ok(files
        .iter()
        .map(|(n, b)| Object {
            name: n.clone(),
            object_key: format!("{}{n}", c.staged_prefix),
            sha256: hash(b),
            size_bytes: b.len() as u64,
        })
        .collect())
}

fn resource_counterexample(c: &labyrinth_engine::diagnostics::Counterexample) -> bool {
    matches!(
        c.code.as_str(),
        "solver_timeout"
            | "routing_search_exhausted"
            | "scenario_budget_exhausted"
            | "service_class_budget"
    )
}
fn classify_generation_error(error: &artifacts::GenerationError) -> Error {
    use artifacts::GenerationError;
    use labyrinth_engine::{
        routing::RoutingError, selection::SelectionError, topology::PlanningError,
    };
    match error {
        GenerationError::Planning(PlanningError::Selection(
            SelectionError::Timeout | SelectionError::SearchExhausted { .. },
        ))
        | GenerationError::Routing(RoutingError::Timeout | RoutingError::SearchExhausted { .. })
        | GenerationError::PackageLimit { .. } => Error::ResourceLimit,
        GenerationError::RepairExhausted { last, .. } => classify_generation_error(last),
        GenerationError::VerificationFailed(report)
            if report
                .passes
                .iter()
                .flat_map(|p| &p.counterexamples)
                .chain(report.scenarios.iter().flat_map(|s| &s.counterexamples))
                .any(resource_counterexample) =>
        {
            Error::ResourceLimit
        }
        GenerationError::CapacityRejected(errors) if errors.iter().any(resource_counterexample) => {
            Error::ResourceLimit
        }
        _ => Error::Deterministic,
    }
}
