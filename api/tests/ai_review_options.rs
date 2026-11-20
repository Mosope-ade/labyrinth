use labyrinth_api::{
    ai::{Confirmation, ProposalError, ProviderMetadata, ReviewContext, ReviewedProposal},
    jobs::{AdmissionContext, PreparedAdmission},
};
use labyrinth_engine::{
    artifacts::GenerationOptions,
    catalog,
    model::{DeviceId, ZoneId},
};
use serde_json::json;
use uuid::Uuid;

fn admission() -> AdmissionContext {
    AdmissionContext {
        actor_id: Uuid::from_u128(1),
        tenant_id: Uuid::from_u128(2),
        project_id: Uuid::from_u128(3),
        expected_head: None,
    }
}
fn context(options: &GenerationOptions) -> Result<ReviewContext, ProposalError> {
    let catalog =
        catalog::load_yaml(include_str!("../../tests/fixtures/catalog/blueprints.yaml")).unwrap();
    ReviewContext::new(admission(), &catalog, options)
}
fn proposal(options: &GenerationOptions) -> ReviewedProposal {
    let intent: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/fixtures/intents/enterprise.json")).unwrap();
    let raw =
        json!({"schema_version":"ai_proposal_v1", "intent":intent,"assumptions":[]}).to_string();
    ReviewedProposal::parse(
        &raw,
        "Review this network",
        context(options).unwrap(),
        ProviderMetadata::new("fixture-only", "mock-parser-v1").unwrap(),
    )
    .unwrap()
}
#[test]
fn displayed_options_are_bound_to_confirmation_for_each_option_family() {
    let original_options = GenerationOptions::default();
    let original = proposal(&original_options);
    let displayed = serde_json::to_value(original.view()).unwrap();
    assert_eq!(
        displayed["context"]["generation_options"],
        serde_json::to_value(&original_options).unwrap()
    );
    let digest = original.digest().to_owned();
    let mut variants = Vec::new();
    for family in 0..4 {
        let mut changed = original_options.clone();
        match family {
            0 => changed.addressing.transit_prefix_len = 30,
            1 => changed.logical.routing_limits.max_paths_per_demand = 32,
            2 => changed.verification.max_scenarios = 2048,
            _ => changed.selection_search_states = 50_000,
        }
        variants.push(changed);
    }
    for changed in variants {
        let catalog =
            catalog::load_yaml(include_str!("../../tests/fixtures/catalog/blueprints.yaml"))
                .unwrap();
        assert!(
            PreparedAdmission::new(
                admission(),
                "changed-options",
                include_str!("../../tests/fixtures/intents/enterprise.json"),
                &changed,
                &catalog
            )
            .is_ok()
        );
        let changed_proposal = proposal(&changed);
        assert_eq!(
            serde_json::to_value(changed_proposal.view()).unwrap()["context"]["generation_options"],
            serde_json::to_value(&changed).unwrap()
        );
        assert_ne!(changed_proposal.digest(), digest);
        assert!(matches!(
            proposal(&original_options).confirm(
                &context(&changed).unwrap(),
                Confirmation {
                    schema_version: "ai_confirmation_v1".into(),
                    proposal_sha256: digest.clone()
                }
            ),
            Err(ProposalError::ReviewConflict)
        ));
    }
}
#[test]
fn oversized_review_options_are_rejected_before_a_proposal_can_be_constructed() {
    fn options_with_entries(count: usize) -> GenerationOptions {
        let mut options = GenerationOptions::default();
        for i in 0..count {
            options.logical.preferred_roots.insert(
                ZoneId::new(format!("zone-{i}")).unwrap(),
                DeviceId::new(format!("device-{i}")).unwrap(),
            );
        }
        options
    }
    let mut accepted = 0;
    let mut rejected = 4096;
    assert!(
        serde_json::to_vec_pretty(&options_with_entries(rejected))
            .unwrap()
            .len()
            > 65_536
    );
    while accepted + 1 < rejected {
        let count = (accepted + rejected) / 2;
        if serde_json::to_vec_pretty(&options_with_entries(count))
            .unwrap()
            .len()
            <= 65_536
        {
            accepted = count;
        } else {
            rejected = count;
        }
    }
    assert!(context(&options_with_entries(accepted)).is_ok());
    assert!(matches!(
        context(&options_with_entries(rejected)),
        Err(ProposalError::InvalidContext)
    ));
}

#[test]
fn review_context_debug_does_not_dump_private_generation_options() {
    let mut options = GenerationOptions::default();
    options.logical.preferred_roots.insert(
        ZoneId::new("zone-private-review").unwrap(),
        DeviceId::new("device-private-review").unwrap(),
    );
    let context = context(&options).unwrap();
    let debug = format!("{context:?}");
    assert!(!debug.contains("device-private-review"));
    assert!(!debug.contains("preferred_roots"));
    assert_eq!(
        serde_json::to_value(&context).unwrap()["generation_options"],
        serde_json::to_value(&options).unwrap()
    );
}
