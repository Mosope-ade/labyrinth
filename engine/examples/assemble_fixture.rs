//! Canonical engine-stage demonstration; not a verified engineering package.
use labyrinth_engine::{
    catalog, intent,
    pipeline::assemble_network,
    topology::{AddressingPolicy, LogicalPolicy},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let intent::Readiness::Ready(intent) =
        intent::parse_json(include_str!("../../tests/fixtures/intents/enterprise.json"))?
    else {
        return Err("fixture needs clarification".into());
    };
    let c = catalog::load_yaml(include_str!("../../tests/fixtures/catalog/blueprints.yaml"))?;
    let n = assemble_network(
        &intent,
        &c,
        &AddressingPolicy::default(),
        &LogicalPolicy::default(),
        10000,
    )?;
    println!("{}", serde_json::to_string_pretty(n.network())?);
    Ok(())
}
