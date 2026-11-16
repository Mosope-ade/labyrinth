//! Public synthetic input for offline container smoke checks; no hosted credentials.
use labyrinth_engine::{artifacts, catalog, intent};
use labyrinth_worker::{Context, package::EngineInput};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let intent::Readiness::Ready(intent) =
        intent::parse_json(include_str!("../../tests/fixtures/intents/enterprise.json"))?
    else {
        return Err("fixture needs clarification".into());
    };
    let yaml = include_str!("../../tests/fixtures/catalog/blueprints.yaml");
    let input = EngineInput {
        context: Context {
            requirements: serde_json::from_slice(&artifacts::canonical_requirements(&intent)?)?,
            provenance: serde_json::from_slice(&artifacts::canonical_provenance(&intent)?)?,
            options: serde_json::to_value(artifacts::GenerationOptions::default())?,
            catalog_yaml: yaml.into(),
            catalog_sha256: catalog::load_yaml(yaml)?.version().sha256.clone(),
            base_revision_id: None,
            base_manifest_sha256: None,
            staged_prefix: "attempts/container/smoke/".into(),
            base_files: vec![],
        },
        base: None,
    };
    serde_json::to_writer(std::io::stdout().lock(), &input)?;
    Ok(())
}
