//! Developer demonstration, not the hosted application's CLI or a verified package.
use labyrinth_engine::{catalog, intent, pipeline::allocate_topology, topology::AddressingPolicy};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let intent::Readiness::Ready(intent) =
        intent::parse_json(include_str!("../../tests/fixtures/intents/enterprise.json"))?
    else {
        return Err("enterprise fixture needs clarification".into());
    };
    let catalog = catalog::load_yaml(include_str!("../../tests/fixtures/catalog/blueprints.yaml"))?;
    let topology = allocate_topology(&intent, &catalog, &AddressingPolicy::default(), 10000)?;
    println!("{}", serde_json::to_string_pretty(&topology)?);
    Ok(())
}
