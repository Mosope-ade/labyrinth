//! Export library-only contracts; no AI HTTP operation is advertised.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let schema = match std::env::args().nth(1).as_deref() {
        Some("proposal") => labyrinth_api::ai::proposal_schema(),
        Some("confirmation") => labyrinth_api::ai::confirmation_schema(),
        _ => return Err("expected proposal or confirmation".into()),
    };
    println!("{}", serde_json::to_string_pretty(&schema)?);
    Ok(())
}
