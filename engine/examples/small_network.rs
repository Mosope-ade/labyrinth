//! AI-free complete package demonstration. Prints the verified package manifest.
use labyrinth_engine::{artifacts, catalog, intent};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/fixtures/intents/enterprise.json"))?;
    input["name"] = serde_json::json!("My first small network");
    input["sites"][0]["zones"][0]["endpoint_count"] = serde_json::json!(10);
    input["sites"][1]["zones"][0]["endpoint_count"] = serde_json::json!(10);
    let intent::Readiness::Ready(intent) = intent::resolve(serde_json::from_value(input)?)? else {
        return Err("requirements need clarification".into());
    };

    let catalog = catalog::load_yaml(include_str!("../../tests/fixtures/catalog/blueprints.yaml"))?;
    let package = artifacts::generate(&intent, &catalog, &Default::default())?;
    let output = std::path::Path::new(".local/my-first-network");
    std::fs::create_dir_all(output)?;

    for (name, bytes) in package.files() {
        std::fs::write(output.join(name), bytes)?;
    }
    eprintln!("Saved verified package to {}", output.display());
    println!("{}", serde_json::to_string_pretty(package.manifest())?);
    Ok(())
}
