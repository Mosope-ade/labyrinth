use labyrinth_api::jobs::{AdmissionContext, PreparedAdmission};
use labyrinth_engine::{artifacts::GenerationOptions, catalog};
use uuid::Uuid;
fn context() -> AdmissionContext {
    AdmissionContext {
        actor_id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        project_id: Uuid::new_v4(),
        expected_head: None,
    }
}
#[test]
fn preparation_rejects_missing_critical_inputs_invalid_keys_and_options() {
    let c =
        catalog::load_yaml(include_str!("../../tests/fixtures/catalog/blueprints.yaml")).unwrap();
    let raw = include_str!("../../tests/fixtures/intents/enterprise.json");
    assert!(PreparedAdmission::new(context(), "key", "{}", &Default::default(), &c).is_err());
    assert!(PreparedAdmission::new(context(), "bad key", raw, &Default::default(), &c).is_err());
    let options = GenerationOptions {
        selection_search_states: 0,
        ..Default::default()
    };
    assert!(PreparedAdmission::new(context(), "key", raw, &options, &c).is_err());
}
#[test]
fn normalized_identity_ignores_collection_order_but_binds_project_head_and_options() {
    let c =
        catalog::load_yaml(include_str!("../../tests/fixtures/catalog/blueprints.yaml")).unwrap();
    let raw = include_str!("../../tests/fixtures/intents/enterprise.json");
    let ctx = context();
    let a = PreparedAdmission::new(ctx.clone(), "one", raw, &Default::default(), &c).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(raw).unwrap();
    v["sites"].as_array_mut().unwrap().reverse();
    let b = PreparedAdmission::new(ctx.clone(), "two", &v.to_string(), &Default::default(), &c)
        .unwrap();
    assert_eq!(a.request_sha256(), b.request_sha256());
    let options = GenerationOptions {
        selection_search_states: 1234,
        ..Default::default()
    };
    let b = PreparedAdmission::new(ctx.clone(), "one", raw, &options, &c).unwrap();
    assert_ne!(a.request_sha256(), b.request_sha256());
    let mut changed = ctx;
    changed.expected_head = Some(Uuid::new_v4());
    let b = PreparedAdmission::new(changed, "one", raw, &Default::default(), &c).unwrap();
    assert_ne!(a.request_sha256(), b.request_sha256());
}

#[test]
fn clarification_and_unsupported_blueprints_cannot_be_prepared() {
    use labyrinth_api::jobs::AdmissionError;
    let c =
        catalog::load_yaml(include_str!("../../tests/fixtures/catalog/blueprints.yaml")).unwrap();
    let mut v: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/fixtures/intents/enterprise.json")).unwrap();
    v.as_object_mut().unwrap().remove("wan_services");
    assert!(matches!(
        PreparedAdmission::new(context(), "key", &v.to_string(), &Default::default(), &c),
        Err(AdmissionError::NeedsClarification(_))
    ));
    v = serde_json::from_str(include_str!("../../tests/fixtures/intents/enterprise.json")).unwrap();
    v["blueprint"] = serde_json::json!("spine_leaf");
    assert!(matches!(
        PreparedAdmission::new(context(), "key", &v.to_string(), &Default::default(), &c),
        Err(AdmissionError::InvalidInput(_))
    ));
}
#[test]
fn hosted_revision_mapping_handles_digit_prefixed_uuids() {
    use labyrinth_api::jobs::engine_revision_id;
    let id = Uuid::parse_str("01234567-89ab-4cde-8fab-0123456789ab").unwrap();
    assert_eq!(
        engine_revision_id(id).as_str(),
        "rev-01234567-89ab-4cde-8fab-0123456789ab"
    );
}
