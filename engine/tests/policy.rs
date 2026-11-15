use labyrinth_engine::{
    intent::{PartialIntent, Readiness, resolve},
    model::*,
    policy::*,
};
use serde_json::{Value, json};
fn input() -> Value {
    serde_json::from_str(include_str!("../../tests/fixtures/intents/enterprise.json")).unwrap()
}
fn intent(v: Value) -> labyrinth_engine::intent::ResolvedIntent {
    let Readiness::Ready(i) = resolve(serde_json::from_value::<PartialIntent>(v).unwrap()).unwrap()
    else {
        panic!()
    };
    *i
}
fn zones() -> (ZoneId, ZoneId) {
    (ZoneId::new("staff").unwrap(), ZoneId::new("guest").unwrap())
}
#[test]
fn policies_are_directional_default_deny_and_deny_overrides_independent_of_order() {
    let mut v = input();
    let allow = json!({"id":"wide","from_zone":"staff","to_zone":"guest","action":"allow","service":{"protocol":"tcp","destination_ports":[{"start":80,"end":100}]}});
    let deny = json!({"id":"narrow","from_zone":"staff","to_zone":"guest","action":"deny","service":{"protocol":"tcp","destination_ports":[{"start":90,"end":110}]}});
    v["policies"] = json!([allow, deny]);
    let a = intent(v.clone());
    v["policies"].as_array_mut().unwrap().reverse();
    let b = intent(v);
    let (from, to) = zones();
    for port in 1..=65535 {
        let s = ServiceClass {
            ip_protocol: 6,
            destination_port: Some(port),
        };
        let a = evaluate(&a, &from, &to, &s).unwrap();
        let b = evaluate(&b, &from, &to, &s).unwrap();
        assert_eq!(a.permitted, (80..90).contains(&port));
        assert_eq!(a.permitted, b.permitted);
    }
    assert!(
        !evaluate(
            &a,
            &to,
            &from,
            &ServiceClass {
                ip_protocol: 6,
                destination_port: Some(80)
            }
        )
        .unwrap()
        .permitted
    );
    assert!(validate_demands(&a).is_ok());
}
#[test]
fn demand_with_no_permitted_service_rejects_and_protocol_space_includes_other_ip_protocols() {
    let mut v = input();
    v["policies"] = json!([]);
    let i = intent(v);
    assert!(matches!(
        validate_demands(&i),
        Err(PolicyError::NoPermittedService(_))
    ));
    let i = intent(input());
    let (f, t) = zones();
    assert!(
        !evaluate(
            &i,
            &f,
            &t,
            &ServiceClass {
                ip_protocol: 47,
                destination_port: None
            }
        )
        .unwrap()
        .permitted
    );
    assert!(matches!(
        evaluate(
            &i,
            &f,
            &t,
            &ServiceClass {
                ip_protocol: 1,
                destination_port: Some(80)
            }
        ),
        Err(PolicyError::InvalidService)
    ));
    assert!(matches!(
        evaluate(
            &i,
            &f,
            &t,
            &ServiceClass {
                ip_protocol: 6,
                destination_port: None
            }
        ),
        Err(PolicyError::InvalidService)
    ));
    assert!(service_classes(&i).iter().any(|s| s.ip_protocol == 255));
}
