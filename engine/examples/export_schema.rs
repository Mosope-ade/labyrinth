//! Developer tooling: print a generated contract schema or bundled catalog identity.
use labyrinth_engine::{catalog, intent, model};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let value = match std::env::args().nth(1).as_deref() {
        Some("catalog") => serde_json::to_value(catalog::catalog_schema())?,
        Some("intent") => serde_json::to_value(intent::intent_schema())?,
        Some("network") => serde_json::to_value(schemars::schema_for!(model::NetworkDraft))?,
        Some("addressing-policy") => serde_json::to_value(schemars::schema_for!(
            labyrinth_engine::topology::AddressingPolicy
        ))?,
        Some("catalog-version") => serde_json::to_value(
            catalog::load_yaml(include_str!("../../catalog/archetypes/reference.yaml"))?.version(),
        )?,
        _ => {
            return Err(
                "usage: export_schema <catalog|intent|network|addressing-policy|catalog-version>"
                    .into(),
            );
        }
    };
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}
