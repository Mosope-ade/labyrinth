//! Fixed identity vectors: run unchanged with serde_json/preserve_order as well.
//! Expected values were independently calculated with Python hashlib/json using
//! eight-byte big-endian length framing for review v2 and the committed pre-move
//! admission_v1 sorted-object codec. Input whitespace/array order are intentional.
use labyrinth_api::{
    ai::{ProviderMetadata, ReviewContext, ReviewedProposal},
    jobs::{AdmissionContext, PreparedAdmission},
};
use labyrinth_engine::{artifacts::GenerationOptions, catalog};
use uuid::Uuid;

const CATALOG: &str = include_str!("../../tests/fixtures/catalog/blueprints.yaml");
const INCOMPLETE: &str = include_str!("../../tests/fixtures/ai/incomplete-proposal.json");
const COMPLETE: &str = include_str!("../../tests/fixtures/intents/enterprise.json");
const PROMPT: &str = "Review this incomplete network request.";
const CATALOG_HASH: &str = "f337bdf91144260729922d1b62f3c88e38d417ce1397b0da8861e3158e34213b";

fn admission() -> AdmissionContext {
    AdmissionContext {
        actor_id: Uuid::from_u128(1),
        tenant_id: Uuid::from_u128(2),
        project_id: Uuid::from_u128(3),
        expected_head: None,
    }
}

#[test]
fn fixed_review_v2_vector_is_independent_of_serde_object_storage_order() {
    let catalog = catalog::load_yaml(CATALOG).unwrap();
    assert_eq!(catalog.version().sha256, CATALOG_HASH);
    let context = ReviewContext::new(admission(), &catalog, &GenerationOptions::default()).unwrap();
    let proposal = ReviewedProposal::parse(
        INCOMPLETE,
        PROMPT,
        context,
        ProviderMetadata::new("fixture-only", "mock-parser-v1").unwrap(),
    )
    .unwrap();
    assert_eq!(
        proposal.digest(),
        "b92dbc7d1ce88c8523330cd8f8c9b21db86a389aa38174aa23a9b0dce5b14eae"
    );
}

#[test]
fn fixed_admission_v1_vector_preserves_the_pre_refactor_identity_codec() {
    let catalog = catalog::load_yaml(CATALOG).unwrap();
    assert_eq!(catalog.version().sha256, CATALOG_HASH);
    let request = PreparedAdmission::new(
        admission(),
        "identity-regression",
        COMPLETE,
        &GenerationOptions::default(),
        &catalog,
    )
    .unwrap();
    assert_eq!(
        request.request_sha256(),
        "650051eac198c532dec88c3250b6f489c9fb4a2c5a2263fe1e87d5b0aa158688"
    );
}
