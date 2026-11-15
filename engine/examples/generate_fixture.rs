//! AI-free complete package demonstration. Prints the verified package manifest.
use labyrinth_engine::{artifacts, catalog, intent};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let intent::Readiness::Ready(intent) =
        intent::parse_json(include_str!("../../tests/fixtures/intents/enterprise.json"))?
    else {
        return Err("fixture needs clarification".into());
    };
    let catalog = catalog::load_yaml(include_str!("../../tests/fixtures/catalog/blueprints.yaml"))?;
    let package = artifacts::generate(&intent, &catalog, &Default::default())?;
    println!("{}", serde_json::to_string_pretty(package.manifest())?);
    Ok(())
}
