use labyrinth_engine::intent::{PartialIntent, Readiness, ValueSource, resolve};
use serde_json::{Value, json};
fn input() -> Value {
    serde_json::from_str(include_str!("../../tests/fixtures/intents/enterprise.json")).unwrap()
}
fn resolve_value(
    value: Value,
) -> Result<Readiness, labyrinth_engine::diagnostics::ValidationErrors> {
    resolve(serde_json::from_value::<PartialIntent>(value).unwrap())
}

#[test]
fn missing_critical_values_are_clarifications_not_zeros() {
    let input: PartialIntent =
        serde_json::from_str(include_str!("../../tests/fixtures/intents/incomplete.json")).unwrap();
    let Readiness::NeedsClarification(fields) = resolve(input).unwrap() else {
        panic!("missing input must not be ready")
    };
    assert!(fields.iter().any(|f| f.path == "/sites"));
    assert!(fields.iter().any(|f| f.path == "/policies"));
    assert!(fields.iter().any(|f| f.path == "/redundancy"));
}

#[test]
fn ready_input_records_only_secondary_defaults() {
    let Readiness::Ready(intent) = resolve_value(input()).unwrap() else {
        panic!("fixture must resolve")
    };
    assert_eq!(intent.data().growth_percent, 20);
    assert_eq!(intent.data().spare_port_target_percent, 20);
    assert_eq!(intent.data().ipv4_pool.as_str(), "10.0.0.0/8");
    assert_eq!(
        intent.provenance()["/growth_percent"],
        ValueSource::EngineDefault
    );
    assert_eq!(
        intent.provenance()["/sites/0/zones/0/endpoint_count"],
        ValueSource::Provided
    );
    assert_eq!(intent.data().sites[0].zones[0].endpoint_count, 100);
}

#[test]
fn explicit_zero_growth_is_preserved_but_zero_endpoints_is_invalid() {
    let mut v = input();
    v["growth_percent"] = json!(0);
    let Readiness::Ready(intent) = resolve_value(v).unwrap() else {
        panic!()
    };
    assert_eq!(intent.data().growth_percent, 0);
    assert_eq!(
        intent.provenance()["/growth_percent"],
        ValueSource::Provided
    );
    let mut v = input();
    v["sites"][0]["zones"][0]["endpoint_count"] = json!(0);
    assert!(resolve_value(v).is_err());
}

#[test]
fn missing_nested_capacity_clarifies_and_invalid_reference_rejects() {
    let mut v = input();
    v["sites"][0]["zones"][0]["endpoint_count"] = Value::Null;
    let Readiness::NeedsClarification(fields) = resolve_value(v).unwrap() else {
        panic!()
    };
    assert!(fields.iter().any(|f| f.path.ends_with("endpoint_count")));
    let mut v = input();
    v["demands"][0]["to_zone"] = json!("unknown");
    assert!(resolve_value(v).is_err());
}

#[test]
fn policy_and_demand_semantics_are_explicit_and_stateless() {
    let mut v = input();
    v["policies"][0]["service"]["protocol"] = json!("icmp");
    assert!(
        resolve_value(v).is_err(),
        "ICMP cannot have TCP/UDP port ranges"
    );
    let mut v = input();
    v["policies"][0]["service"]["destination_ports"][0] = json!({"start":450,"end":443});
    assert!(resolve_value(v).is_err());
    let mut v = input();
    v["demands"][0]["peak_mbps"] = json!(0);
    assert!(resolve_value(v).is_err());
    let Readiness::Ready(intent) = resolve_value(input()).unwrap() else {
        panic!()
    };
    assert_eq!(
        intent.data().policies.len(),
        2,
        "resolution must not invent return permits"
    );
}

#[test]
fn tier3_requires_two_distinct_hub_locations_and_provider_information() {
    let mut v = input();
    v["redundancy"] = json!("tier3_site");
    let Readiness::NeedsClarification(fields) = resolve_value(v.clone()).unwrap() else {
        panic!()
    };
    assert!(fields.iter().any(|f| f.path == "/sites"));
    v["sites"][1]["role"] = json!("hub");
    assert!(matches!(resolve_value(v.clone()), Ok(Readiness::Ready(_))));
    v["sites"][1]["location"] = json!("lagos");
    assert!(resolve_value(v).is_err());
}

#[test]
fn incremental_mode_needs_pins_and_cannot_silently_redesign() {
    let mut v = input();
    v["update"] = json!({"mode":"incremental_update","base_revision":"revision-a","ipam_pins":[],"core_pins":[]});
    assert!(resolve_value(v).is_err());
}

#[test]
fn duplicates_bad_prefix_unknown_fields_and_version_are_rejected() {
    let mut v = input();
    v["sites"][1]["zones"][0]["id"] = json!("staff");
    assert!(resolve_value(v).is_err());
    let mut v = input();
    v["schema_version"] = json!("intent_v9");
    assert!(resolve_value(v).is_err());
    let mut v = input();
    v["ipv4_pool"] = json!("10.0.0.1/24");
    assert!(serde_json::from_value::<PartialIntent>(v).is_err());
    let mut v = input();
    v["sites"][0]["vendor"] = json!("vendor");
    assert!(serde_json::from_value::<PartialIntent>(v).is_err());
}

#[test]
fn provided_defaults_and_source_input_remain_unchanged() {
    let mut v = input();
    v["ipv4_pool"] = json!("172.16.0.0/16");
    v["growth_percent"] = json!(30);
    let parsed: PartialIntent = serde_json::from_value(v).unwrap();
    let before = serde_json::to_value(&parsed).unwrap();
    let a = resolve(parsed.clone()).unwrap();
    let b = resolve(parsed.clone()).unwrap();
    assert_eq!(a, b);
    assert_eq!(before, serde_json::to_value(&parsed).unwrap());
}

#[test]
fn excluded_pool_and_poe_unknowns_do_not_disappear() {
    let mut v = input();
    v["excluded_ipv4"] = json!(["10.0.0.0/8"]);
    assert!(resolve_value(v).is_err());
    let mut v = input();
    v["sites"][0]["zones"][0]["required_features"] = json!(["poe"]);
    let Readiness::NeedsClarification(fields) = resolve_value(v).unwrap() else {
        panic!()
    };
    assert!(fields.iter().any(|f| f.path.ends_with("poe_load_watts")));
}

#[test]
fn pins_preserve_explicit_assignments_and_reject_overlapping_space() {
    let mut v = input();
    v["update"] = json!({"mode":"incremental_update","base_revision":"revision-a","ipam_pins":[{"zone":"staff","subnet":"10.0.0.0/24"},{"zone":"guest","subnet":"10.0.1.0/24"}],"core_pins":[{"device":"core-a","site":"hub","archetype":"access-switch"}]});
    assert!(matches!(resolve_value(v.clone()), Ok(Readiness::Ready(_))));
    v["update"]["ipam_pins"][1]["subnet"] = json!("10.0.0.0/25");
    assert!(resolve_value(v).is_err());
}

#[test]
fn invalid_supplied_values_are_not_disguised_as_missing_fields() {
    let mut v = input();
    v["name"] = Value::Null;
    v["sites"][0]["zones"][0]["endpoint_count"] = json!(0);
    let errors = resolve_value(v).unwrap_err();
    assert!(
        errors
            .issues
            .iter()
            .any(|e| e.path.ends_with("endpoint_count"))
    );
    let mut v = input();
    v["sites"][0]["zones"] = Value::Null;
    assert!(matches!(
        resolve_value(v),
        Ok(Readiness::NeedsClarification(_))
    ));
}

#[test]
fn numeric_and_collection_admission_limits_are_enforced() {
    let mut v = input();
    v["growth_percent"] = json!(101);
    assert!(resolve_value(v).is_err());
    let mut v = input();
    v["spare_port_target_percent"] = json!(100);
    assert!(resolve_value(v).is_err());
    let mut v = input();
    v["sites"] = json!(vec![v["sites"][0].clone(); 65]);
    assert!(resolve_value(v).is_err());
    let mut v = input();
    v["demands"][0]["peak_mbps"] = json!(1.5);
    assert!(serde_json::from_value::<PartialIntent>(v).is_err());
}

#[test]
fn omitted_exclusions_are_recorded_as_default_not_user_provided() {
    let mut v = input();
    v.as_object_mut().unwrap().remove("excluded_ipv4");
    let Readiness::Ready(intent) = resolve_value(v).unwrap() else {
        panic!()
    };
    assert_eq!(
        intent.provenance()["/excluded_ipv4"],
        ValueSource::EngineDefault
    );
}

#[test]
fn bounded_json_entrypoint_rejects_oversize_and_duplicate_keys() {
    use labyrinth_engine::intent::{MAX_JSON_BYTES, parse_json};
    assert!(parse_json(&" ".repeat(MAX_JSON_BYTES + 1)).is_err());
    assert!(
        parse_json("{\"schema_version\":\"intent_v1\",\"schema_version\":\"intent_v2\"}").is_err()
    );
    assert!(matches!(
        parse_json(include_str!("../../tests/fixtures/intents/enterprise.json")),
        Ok(Readiness::Ready(_))
    ));
}

#[test]
fn endpoint_attachment_default_is_visible_and_can_be_overridden() {
    use labyrinth_engine::model::Media;
    let Readiness::Ready(r) = resolve_value(input()).unwrap() else {
        panic!()
    };
    assert_eq!(
        r.data().sites[0].zones[0].endpoint_attachment.speed_mbps,
        1000
    );
    assert_eq!(
        r.data().sites[0].zones[0].endpoint_attachment.media,
        Media::Copper
    );
    assert_eq!(
        r.provenance()["/sites/0/zones/0/endpoint_attachment"],
        ValueSource::EngineDefault
    );
    let mut v = input();
    v["sites"][0]["zones"][0]["endpoint_attachment"] =
        json!({"speed_mbps":10000,"media":"fiber","poe_watts_per_endpoint":null});
    let Readiness::Ready(r) = resolve_value(v).unwrap() else {
        panic!()
    };
    assert_eq!(
        r.data().sites[0].zones[0].endpoint_attachment.speed_mbps,
        10000
    );
    assert_eq!(
        r.provenance()["/sites/0/zones/0/endpoint_attachment"],
        ValueSource::Provided
    );
}

#[test]
fn poe_load_cannot_exceed_declared_endpoint_maximum() {
    let mut v = input();
    v["sites"][0]["zones"][0]["required_features"] = json!(["poe"]);
    v["sites"][0]["zones"][0]["poe_load_watts"] = json!(4000);
    v["sites"][0]["zones"][0]["endpoint_attachment"] =
        json!({"speed_mbps":1000,"media":"copper","poe_watts_per_endpoint":30});
    assert!(resolve_value(v).is_err());
}
