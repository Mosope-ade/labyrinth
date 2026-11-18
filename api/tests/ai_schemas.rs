use serde_json::Value;

#[test]
fn checked_in_ai_schemas_match_exports_and_accept_the_partial_fixture() {
    for (exported, stored) in [
        (
            labyrinth_api::ai::proposal_schema(),
            include_str!("../../schemas/ai-proposal-v1.schema.json"),
        ),
        (
            labyrinth_api::ai::confirmation_schema(),
            include_str!("../../schemas/ai-confirmation-v1.schema.json"),
        ),
    ] {
        let stored: Value = serde_json::from_str(stored).unwrap();
        assert_eq!(exported.as_value(), &stored);
        jsonschema::validator_for(&stored).unwrap();
    }
    let partial: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/ai/incomplete-proposal.json"
    ))
    .unwrap();
    assert!(
        jsonschema::validator_for(labyrinth_api::ai::proposal_schema().as_value())
            .unwrap()
            .is_valid(&partial)
    );
}
