use labyrinth_engine::{
    catalog,
    intent::{self, Readiness},
    model::*,
    pipeline::allocate_topology,
    routing::*,
    topology::{self, AddressingPolicy, LogicalPolicy},
};
fn setup() -> (
    intent::ResolvedIntent,
    topology::AllocatedTopology,
    catalog::CatalogSnapshot,
) {
    let Readiness::Ready(i) =
        intent::parse_json(include_str!("../../tests/fixtures/intents/enterprise.json")).unwrap()
    else {
        panic!()
    };
    let c =
        catalog::load_yaml(include_str!("../../tests/fixtures/catalog/blueprints.yaml")).unwrap();
    let t = allocate_topology(&i, &c, &AddressingPolicy::default(), 10000).unwrap();
    (*i, t, c)
}
#[test]
fn routing_uses_explicit_transport_and_destination_trees_are_walkable() {
    let (i, t, c) = setup();
    let wan = build_transport(&i, &t);
    assert!(!wan.is_empty());
    let r = synthesize(
        &i,
        &t,
        &wan,
        &LogicalPolicy::default(),
        &Default::default(),
        &Default::default(),
    )
    .unwrap();
    assert_eq!(r.paths.len(), 1);
    assert!(
        r.paths[0]
            .adjacencies
            .iter()
            .any(|a| matches!(a, ForwardingAdjacency::Wan(_)))
    );
    assert!(
        synthesize(
            &i,
            &t,
            &[],
            &LogicalPolicy::default(),
            &Default::default(),
            &Default::default()
        )
        .is_err()
    );
    let n = topology::assemble(&t, &c, &LogicalPolicy::default()).unwrap();
    let b = labyrinth_engine::policy::bindings(&i, n.network());
    assert!(b.len() > i.data().policies.len());
    let mut n = n.network().clone();
    n.policies = b;
    n.wan_connections = wan;
    n.forwarding_routes = r.routes;
    n.validate(&c).unwrap();
}
#[test]
fn routing_recomputes_after_provider_or_edge_failure_without_inventing_return_traffic() {
    let (i, t, _) = setup();
    let wan = build_transport(&i, &t);
    let p = LogicalPolicy::default();
    let baseline = synthesize(&i, &t, &wan, &p, &Default::default(), &Default::default()).unwrap();
    let previous = baseline
        .ownership
        .iter()
        .filter_map(|s| s.owner.as_ref().map(|o| (s.zone.clone(), o.clone())))
        .collect();
    for provider in ["isp-a", "isp-b"] {
        let f = FailureScenario {
            providers: [FailureDomainId::new(provider).unwrap()].into(),
            ..Default::default()
        };
        let r = synthesize(&i, &t, &wan, &p, &f, &previous).unwrap();
        assert!(
            r.paths[0]
                .adjacencies
                .iter()
                .filter_map(|a| if let ForwardingAdjacency::Wan(id) = a {
                    wan.iter().find(|w| &w.id == id)
                } else {
                    None
                })
                .all(|w| w.provider_failure_domain.as_str() != provider)
        );
    }
    for d in t
        .selection()
        .assignment
        .devices
        .iter()
        .filter(|d| d.role == DeviceRole::WanEdge)
    {
        let mut f = FailureScenario::default();
        f.internal.devices.insert(d.requirement.clone());
        assert!(synthesize(&i, &t, &wan, &p, &f, &previous).is_ok());
    }
}
