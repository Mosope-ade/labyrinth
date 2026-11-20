//! Manual opt-in provider smoke. Sends one fixed public prompt; never project files.
use labyrinth_api::{
    ai::{
        ReviewContext,
        gemini::{GeminiAdapter, GeminiProfile, GeminiSettings, PromptDataClass},
    },
    jobs::AdmissionContext,
};
use serde_json::json;
use std::time::Duration;
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var("LABYRINTH_GEMINI_PUBLIC_TEST_ACK").as_deref() != Ok("public-fixture-only") {
        return Err("live calls disabled: read docs/runbooks/gemini-testing.md before setting the public-fixture acknowledgement".into());
    }
    let key = std::env::var("LABYRINTH_GEMINI_API_KEY")
        .map_err(|_| "set the Gemini key locally; never send it through chat")?;
    let primary_model = std::env::var("LABYRINTH_GEMINI_PRIMARY_MODEL")
        .map_err(|_| "set an explicitly selected primary model")?;
    let fallback_model = std::env::var("LABYRINTH_GEMINI_FALLBACK_MODEL").ok();
    let attempts = if fallback_model.is_some() { 2 } else { 1 };
    let adapter = GeminiAdapter::new(
        &key,
        GeminiSettings {
            primary_model,
            fallback_model,
            profile: GeminiProfile::FreeTesting,
            request_timeout: Duration::from_secs(30),
            max_output_tokens: 4096,
            attempt_budget: attempts,
        },
    )
    .map_err(|e| format!("invalid Gemini configuration: {e:?}"))?;
    let catalog = labyrinth_engine::catalog::load_yaml(include_str!(
        "../../tests/fixtures/catalog/blueprints.yaml"
    ))?;
    let context = ReviewContext::new(
        AdmissionContext {
            actor_id: Uuid::from_u128(1),
            tenant_id: Uuid::from_u128(2),
            project_id: Uuid::from_u128(3),
            expected_head: None,
        },
        &catalog,
        &Default::default(),
    )?;
    let result = adapter.propose(
        "Extract only these public fictional requirements: name Public fixture; a hub site named hub-one with a users zone containing 10 endpoints; tier0_none redundancy; stateless_redesign mode. Traffic demand, security policy and WAN details have not been supplied: leave them unknown so the deterministic engine asks for clarification. Do not invent them.",
        context, PromptDataClass::PublicSynthetic,
    ).await?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "review":result.proposal.view(),"model_switch":result.model_switch,"attempts":result.attempts,
            "assurance":"unconfirmed_public_fixture_not_generated"
        }))?
    );
    Ok(())
}
