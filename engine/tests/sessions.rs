use labyrinth_engine::{
    catalog,
    intent::{self, Readiness},
    pipeline::allocate_topology,
    routing::{self, FailureScenario, RoutingError},
    topology::{AddressingPolicy, LogicalPolicy},
    verification,
};
use serde_json::{Value, json};
fn input() -> Value {
    let mut v: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/intents/enterprise.json")).unwrap();
    v["redundancy"] = json!("tier0_none");
    v["demands"][0]["distribution"] = json!({"mode":"sessions","growth":"rates","cohorts":[{"session_peak_mbps":10,"simultaneous_sessions":10}]});
    for w in v["wan_services"].as_array_mut().unwrap() {
        w["committed_bandwidth_mbps"] = json!(60);
    }
    v
}
fn setup(
    v: Value,
    yaml: &str,
) -> (
    intent::ResolvedIntent,
    catalog::CatalogSnapshot,
    labyrinth_engine::topology::AllocatedTopology,
) {
    let Readiness::Ready(i) = intent::resolve(serde_json::from_value(v).unwrap()).unwrap() else {
        panic!("unresolved")
    };
    let c = catalog::load_yaml(yaml).unwrap();
    let t = allocate_topology(&i, &c, &AddressingPolicy::default(), 10000).unwrap();
    (*i, c, t)
}
#[test]
fn sessions_split_across_explicit_services_without_striping_individual_sessions() {
    let (i, c, t) = setup(
        input(),
        include_str!("../../tests/fixtures/catalog/blueprints.yaml"),
    );
    let p = LogicalPolicy::default();
    let (n, snapshot) = verification::prepare(&i, &c, &t, &p).unwrap();
    assert!(snapshot.paths.len() >= 2);
    assert_eq!(snapshot.paths.iter().map(|p| p.sessions).sum::<u64>(), 10);
    assert!(
        snapshot
            .paths
            .iter()
            .all(|p| p.session_peak_mbps == 12 && p.peak_mbps <= 60 && p.steered)
    );
    assert_eq!(snapshot.paths.iter().map(|p| p.peak_mbps).sum::<u64>(), 120);
    let r = verification::verify(&i, &c, &t, &p, &n, &Default::default());
    assert!(r.passed(), "{}", serde_json::to_string_pretty(&r).unwrap());
}
#[test]
fn session_rate_cannot_be_justified_by_summed_ecmp_capacity_and_steering_is_required() {
    let yaml = include_str!("../../tests/fixtures/catalog/blueprints.yaml");
    let mut v = input();
    v["demands"][0]["peak_mbps"] = json!(160);
    v["demands"][0]["distribution"] = json!({"mode":"sessions","growth":"rates","cohorts":[{"session_peak_mbps":80,"simultaneous_sessions":2}]});
    let (i, _c, t) = setup(v, yaml);
    let wan = routing::build_transport(&i, &t);
    assert!(matches!(
        routing::synthesize(
            &i,
            &t,
            &wan,
            &LogicalPolicy::default(),
            &FailureScenario::default(),
            &Default::default()
        ),
        Err(RoutingError::UnsatisfiedTraffic { .. })
    ));
    let Readiness::Ready(i) = intent::resolve(serde_json::from_value(input()).unwrap()).unwrap()
    else {
        panic!()
    };
    let c = catalog::load_yaml(&yaml.replace(", session_steering", "")).unwrap();
    assert!(allocate_topology(&i, &c, &AddressingPolicy::default(), 10000).is_err());
}
#[test]
fn session_growth_is_explicit_and_granularity_is_not_hidden_double_growth() {
    let mut v = input();
    v["growth_percent"] = json!(20);
    v["demands"][0]["peak_mbps"] = json!(3);
    v["demands"][0]["distribution"] = json!({"mode":"sessions","growth":"rates","cohorts":[{"session_peak_mbps":1,"simultaneous_sessions":3}]});
    let Readiness::Ready(i) = intent::resolve(serde_json::from_value(v.clone()).unwrap()).unwrap()
    else {
        panic!()
    };
    let b = routing::batches(&i.data().demands[0], 20).unwrap();
    assert_eq!((b[0].session_peak_mbps, b[0].count), (2, 3));
    assert_eq!(routing::reserved_peak(&i.data().demands[0], 20).unwrap(), 6);
    assert_eq!(labyrinth_engine::selection::with_growth(3, 20).unwrap(), 4);
    v["demands"][0]["distribution"]["growth"] = json!("concurrency");
    let Readiness::Ready(i) = intent::resolve(serde_json::from_value(v).unwrap()).unwrap() else {
        panic!()
    };
    let b = routing::batches(&i.data().demands[0], 20).unwrap();
    assert_eq!((b[0].session_peak_mbps, b[0].count), (1, 4));
}
#[test]
fn session_search_budget_exhaustion_returns_no_partial_allocation() {
    let (i, _c, t) = setup(
        input(),
        include_str!("../../tests/fixtures/catalog/blueprints.yaml"),
    );
    let mut p = LogicalPolicy::default();
    p.routing_limits.max_search_states = 1;
    assert!(matches!(
        routing::synthesize(
            &i,
            &t,
            &routing::build_transport(&i, &t),
            &p,
            &Default::default(),
            &Default::default()
        ),
        Err(RoutingError::SearchExhausted { .. })
    ));
}

#[test]
fn compressed_session_packing_matches_an_exhaustive_two_service_reference() {
    let yaml = include_str!("../../tests/fixtures/catalog/blueprints.yaml");
    for rate in [2u64, 3] {
        for count in [2u32, 3] {
            for a in 1..=5u64 {
                for b in 1..=5u64 {
                    let mut v = input();
                    v["growth_percent"] = json!(0);
                    v["demands"][0]["peak_mbps"] = json!(rate * u64::from(count));
                    v["demands"][0]["distribution"] = json!({"mode":"sessions","growth":"rates","cohorts":[{"session_peak_mbps":rate,"simultaneous_sessions":count}]});
                    for w in v["wan_services"].as_array_mut().unwrap() {
                        w["committed_bandwidth_mbps"] =
                            json!(if w["provider_failure_domain"] == "isp-a" {
                                a
                            } else {
                                b
                            });
                    }
                    let (i, _c, t) = setup(v, yaml);
                    let wan = routing::build_transport(&i, &t);
                    let result = routing::synthesize(
                        &i,
                        &t,
                        &wan,
                        &LogicalPolicy::default(),
                        &Default::default(),
                        &Default::default(),
                    );
                    let expected = (0..=count).any(|left| {
                        u64::from(left) * rate <= a && u64::from(count - left) * rate <= b
                    });
                    assert_eq!(
                        result.is_ok(),
                        expected,
                        "rate={rate}, count={count}, capacities={a}/{b}, result={result:?}"
                    );
                }
            }
        }
    }
}
