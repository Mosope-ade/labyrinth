//! Strict loading of immutable, vendor-neutral hardware-archetype snapshots.
mod types;
mod validate;
use crate::diagnostics::ValidationErrors;
use sha2::{Digest, Sha256};
pub use types::*;

pub const MAX_YAML_BYTES: usize = 1_048_576;

#[derive(Debug)]
pub enum CatalogError {
    TooLarge,
    Parse(serde_saphyr::Error),
    Invalid(ValidationErrors),
    Serialization(serde_json::Error),
}
impl std::fmt::Display for CatalogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge => write!(f, "catalog exceeds {MAX_YAML_BYTES} bytes"),
            Self::Parse(e) => write!(f, "catalog parse: {e}"),
            Self::Invalid(e) => e.fmt(f),
            Self::Serialization(e) => write!(f, "catalog serialization: {e}"),
        }
    }
}
impl std::error::Error for CatalogError {}

/// Only the loader creates this snapshot; callers cannot mutate its validated bundle.
///
/// ```compile_fail
/// use labyrinth_engine::catalog::CatalogSnapshot;
/// let _: CatalogSnapshot = serde_json::from_str("{}").unwrap();
/// ```
#[derive(Debug, Clone)]
pub struct CatalogSnapshot {
    bundle: CatalogBundle,
    version: CatalogVersion,
}
impl CatalogSnapshot {
    pub fn archetypes(&self) -> &[Archetype] {
        &self.bundle.archetypes
    }
    pub fn version(&self) -> &CatalogVersion {
        &self.version
    }
    pub fn bundle(&self) -> &CatalogBundle {
        &self.bundle
    }
}

pub fn load_yaml(input: &str) -> Result<CatalogSnapshot, CatalogError> {
    if input.len() > MAX_YAML_BYTES {
        return Err(CatalogError::TooLarge);
    }
    // Catalogs are data only: no tag extensions, external includes or interpolation.
    // Explicit budgets bound parser work independently of the raw input byte cap.
    let options = serde_saphyr::options! {
        budget: serde_saphyr::budget! {
            max_depth: 64,
            max_documents: 1,
            max_events: 200_000,
            max_nodes: 100_000,
            max_aliases: 1_024,
            max_anchors: 1_024,
            max_recorded_anchor_events: 100_000,
            max_recorded_anchor_bytes: MAX_YAML_BYTES,
            max_total_scalar_bytes: MAX_YAML_BYTES,
        },
        reject_unsupported_tags: true,
        reject_tagged_field_names: true,
        with_snippet: false,
    };
    let bundle: CatalogBundle =
        serde_saphyr::from_str_with_options(input, options).map_err(CatalogError::Parse)?;
    validate::validate(&bundle).map_err(CatalogError::Invalid)?;
    let bytes = serde_json::to_vec(&bundle).map_err(CatalogError::Serialization)?;
    let version = CatalogVersion {
        semantic_version: bundle.version.clone(),
        sha256: format!("{:x}", Sha256::digest(bytes)),
    };
    Ok(CatalogSnapshot { bundle, version })
}

pub fn catalog_schema() -> schemars::Schema {
    schemars::schema_for!(CatalogBundle)
}
