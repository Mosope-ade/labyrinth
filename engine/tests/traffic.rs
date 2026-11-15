use labyrinth_engine::intent::{PartialIntent, Readiness, resolve};
use serde_json::{Value, json};
fn input() -> Value {
    serde_json::from_str(include_str!("../../tests/fixtures/intents/enterprise.json")).unwrap()
}
fn check(v: Value) -> Result<Readiness, labyrinth_engine::diagnostics::ValidationErrors> {
    resolve(serde_json::from_value::<PartialIntent>(v).unwrap())
}
#[test]
fn session_cohorts_are_explicit_and_their_simultaneous_upper_bounds_match_aggregate_peak() {
    let mut v = input();
    v["demands"][0]["distribution"] = json!({"mode":"sessions","growth":"rates","cohorts":[{"session_peak_mbps":10,"simultaneous_sessions":10}]});
    assert!(matches!(check(v.clone()).unwrap(), Readiness::Ready(_)));
    v["demands"][0]["peak_mbps"] = json!(99);
    assert!(check(v).is_err());
    let mut v = input();
    v["demands"][0]
        .as_object_mut()
        .unwrap()
        .remove("distribution");
    let Readiness::NeedsClarification(fields) = check(v).unwrap() else {
        panic!("traffic distribution was inferred")
    };
    assert!(fields.iter().any(|f| f.path == "/demands/0/distribution"));
}
#[test]
fn zero_or_unrepresentable_session_envelopes_are_rejected_without_integer_wrap() {
    for (rate, count) in [(0, 10), (10, 0), (u64::MAX, u32::MAX)] {
        let mut v = input();
        v["demands"][0]["distribution"] = json!({"mode":"sessions","growth":"rates","cohorts":[{"session_peak_mbps":rate,"simultaneous_sessions":count}]});
        assert!(check(v).is_err());
    }
}
