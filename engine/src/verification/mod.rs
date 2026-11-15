//! Evidence-backed structural, addressing, routing, security and resilience passes.
use crate::capacity;
mod routes;
mod scenarios;
mod security;
mod sessions;
mod types;
use crate::{
    catalog::CatalogSnapshot,
    intent::ResolvedIntent,
    model::*,
    routing::{self, RoutingSnapshot},
    topology::{self, AllocatedTopology, LogicalPolicy},
};
use std::collections::BTreeMap;
pub use types::*;

fn same<T: serde::Serialize>(a: &T, b: &T) -> bool {
    serde_json::to_value(a).expect("typed encoding")
        == serde_json::to_value(b).expect("typed encoding")
}
fn skeleton() -> VerificationReport {
    VerificationReport{rules_version:"verification_v1".into(),passes:vec![],scenarios:vec![],protected_set:vec![],exclusions:vec!["single-homed endpoint ports and access-switch device loss are outside the backbone device protection contract".into()],assumptions:vec!["WAN reachability/capacity is conditional on the user's explicit confirmed abstract service declarations; carrier internals are not verified".into(),"stateless deny-overrides, default deny; no implicit return traffic, NAT translation, dynamic protocol convergence or actual fencing".into(),"simultaneous zone peak demands grow once; destination routes for single-path traffic, explicit controlled session paths otherwise; LACP per-port worst-case bounds, full-duplex WAN attachment budgets".into(),"HA synchronization reserves one active control channel at a full-zone traffic-copy bound; selected-mode budget includes that copy".into()]}
}
/// Verifies public canonical content against immutable allocation, confirmed intent,
/// catalog and policies. Budget exhaustion is failure, never partial success.
pub fn verify(
    intent: &ResolvedIntent,
    catalog: &CatalogSnapshot,
    t: &AllocatedTopology,
    logical: &LogicalPolicy,
    network: &NetworkDraft,
    limits: &VerificationLimits,
) -> VerificationReport {
    let mut report = skeleton();
    let mut structural = Vec::new();
    if limits.max_scenarios > 4096
        || limits.max_service_classes > 1_000_000
        || limits.max_service_classes == 0
    {
        structural.push(types::failure(
            "invalid_verification_limits",
            "limits",
            "bounds exceed documented engine admission limits",
        ));
    }
    if let Err(e) = network.clone().validate(catalog) {
        structural.push(types::failure(
            "invalid_canonical",
            "network",
            e.to_string(),
        ));
    }
    let wan = routing::build_transport(intent, t);
    let baseline = routing::synthesize(
        intent,
        t,
        &wan,
        logical,
        &Default::default(),
        &Default::default(),
    );
    match topology::assemble(t, catalog, logical) {
        Ok(n) => {
            let mut expected = n.network().clone();
            expected.wan_connections = wan.clone();
            expected.policies = crate::policy::bindings(intent, &expected);
            if let Ok(snapshot) = &baseline {
                expected.forwarding_routes = snapshot.routes.clone();
                expected.traffic_paths = snapshot.paths.clone();
            }
            if !same(&expected, network) {
                structural.push(types::failure("canonical_provenance_mismatch","network","canonical entities, service capacities, routes or enforcement differ from the declared generation context"));
            }
        }
        Err(e) => structural.push(types::failure(
            "invalid_generation_context",
            "allocation",
            e.to_string(),
        )),
    }
    if let Ok(snapshot) = &baseline {
        structural.extend(capacity::check(
            intent,
            catalog,
            t,
            &wan,
            snapshot,
            &Default::default(),
        ));
    }
    if let Some(first) = network.devices.first() {
        let mut reached = std::collections::BTreeSet::from([first.id.clone()]);
        loop {
            let old = reached.len();
            for (a, b) in network
                .links
                .iter()
                .map(|l| (&l.from.device, &l.to.device))
                .chain(
                    network
                        .wan_connections
                        .iter()
                        .map(|w| (&w.from.device, &w.to.device)),
                )
            {
                if reached.contains(a) {
                    reached.insert(b.clone());
                }
                if reached.contains(b) {
                    reached.insert(a.clone());
                }
            }
            if reached.len() == old {
                break;
            }
        }
        if network.devices.iter().any(|d| !reached.contains(&d.id)) {
            structural.push(types::failure(
                "disconnected_graph",
                "network",
                "physical and declared WAN graph does not connect all design devices",
            ));
        }
    }
    report.passes.push(types::pass(
        PassKind::Structural,
        network.devices.len() as u64 + network.links.len() as u64,
        structural,
    ));
    let mut addressing = Vec::new();
    for a in &network.allocations {
        if !intent.data().ipv4_pool.contains(&a.subnet)
            || intent
                .data()
                .excluded_ipv4
                .iter()
                .any(|e| e.overlaps(&a.subnet))
        {
            addressing.push(types::failure(
                "allocation_outside_allowed_space",
                a.id.to_string(),
                "allocation must fit pool and avoid exclusions",
            ));
        }
    }
    let allocated: Vec<_> = t
        .addresses()
        .assignments()
        .iter()
        .map(|a| a.allocation.clone())
        .collect();
    if !same(&allocated, &network.allocations) {
        addressing.push(types::failure(
            "allocation_context_mismatch",
            "allocations",
            "allocated identities/capacities/pins changed",
        ));
    }
    report.passes.push(types::pass(
        PassKind::Addressing,
        network.allocations.len() as u64,
        addressing,
    ));
    if !report.passes[0].passed {
        for kind in [PassKind::Routing, PassKind::Security, PassKind::Resilience] {
            report.passes.push(types::pass(
                kind,
                0,
                vec![types::failure(
                    "invalid_structural_precondition",
                    "network",
                    "cannot grant semantic assurance to an invalid canonical/context snapshot",
                )],
            ));
        }
        return report;
    }
    let baseline = match baseline {
        Ok(mut s) => {
            s.routes = network.forwarding_routes.clone();
            s.paths = network.traffic_paths.clone();
            s
        }
        Err(e) => {
            for kind in [PassKind::Routing, PassKind::Security, PassKind::Resilience] {
                report.passes.push(types::pass(
                    kind,
                    0,
                    vec![types::failure(
                        "baseline_generation_failure",
                        "routing",
                        e.to_string(),
                    )],
                ));
            }
            return report;
        }
    };
    let mut route_errors = routes::check(t, &wan, &baseline, &Default::default());
    route_errors.extend(sessions::check(
        intent,
        t,
        &wan,
        &baseline,
        &Default::default(),
    ));
    route_errors.extend(capacity::check(
        intent,
        catalog,
        t,
        &wan,
        &baseline,
        &Default::default(),
    ));
    report.passes.push(types::pass(
        PassKind::Routing,
        baseline.routes.len() as u64,
        route_errors,
    ));
    let (checked, errors) = security::check(
        intent,
        t,
        network,
        &baseline,
        &Default::default(),
        limits.max_service_classes,
    );
    report
        .passes
        .push(types::pass(PassKind::Security, checked, errors));
    let scope = scenarios::cases(intent, t, &wan);
    report.protected_set = scope.protected_set;
    report.exclusions.extend(scope.exclusions);
    let cases = scope.cases;
    let mut errors = Vec::new();
    if cases.len() > limits.max_scenarios {
        errors.push(types::failure(
            "scenario_budget_exhausted",
            "resilience",
            format!(
                "{} required scenarios exceed limit {}",
                cases.len(),
                limits.max_scenarios
            ),
        ));
    } else {
        let previous: BTreeMap<_, _> = baseline
            .ownership
            .iter()
            .filter_map(|o| o.owner.as_ref().map(|d| (o.zone.clone(), d.clone())))
            .collect();
        for (name, scenario) in cases {
            let (count, mut failures) =
                match routing::synthesize(intent, t, &wan, logical, &scenario, &previous) {
                    Ok(snapshot) => {
                        let mut errors = routes::check(t, &wan, &snapshot, &scenario);
                        errors.extend(sessions::check(intent, t, &wan, &snapshot, &scenario));
                        errors.extend(capacity::check(
                            intent, catalog, t, &wan, &snapshot, &scenario,
                        ));
                        let (_, security) = security::check(
                            intent,
                            t,
                            network,
                            &snapshot,
                            &scenario,
                            limits.max_service_classes,
                        );
                        errors.extend(security);
                        (
                            snapshot
                                .paths
                                .iter()
                                .map(|p| &p.flow)
                                .collect::<std::collections::BTreeSet<_>>()
                                .len(),
                            errors,
                        )
                    }
                    Err(e) => (
                        0,
                        vec![types::failure(
                            e.resource_limit_code()
                                .unwrap_or("failure_state_unreachable"),
                            name.clone(),
                            e.to_string(),
                        )],
                    ),
                };
            failures.truncate(128);
            let passed = failures.is_empty();
            if !passed && errors.len() < 128 {
                errors.push(types::failure(
                    "failure_scenario_failed",
                    name.clone(),
                    failures
                        .first()
                        .map_or("failure state rejected".into(), |f| f.detail.clone()),
                ));
            }
            report.scenarios.push(ScenarioEvidence {
                name,
                passed,
                protected_demands: count,
                excluded_demands: intent.data().demands.len() - count,
                counterexamples: failures,
            });
        }
    }
    report.passes.push(types::pass(
        PassKind::Resilience,
        report.scenarios.len() as u64,
        errors,
    ));
    report
}
/// Build a synthesized baseline for verification without an external service.
pub fn prepare(
    intent: &ResolvedIntent,
    catalog: &CatalogSnapshot,
    t: &AllocatedTopology,
    logical: &LogicalPolicy,
) -> Result<(NetworkDraft, RoutingSnapshot), routing::RoutingError> {
    let mut n = topology::assemble(t, catalog, logical)?.network().clone();
    n.wan_connections = routing::build_transport(intent, t);
    let snapshot = routing::synthesize(
        intent,
        t,
        &n.wan_connections,
        logical,
        &Default::default(),
        &Default::default(),
    )?;
    n.forwarding_routes = snapshot.routes.clone();
    n.traffic_paths = snapshot.paths.clone();
    n.policies = crate::policy::bindings(intent, &n);
    Ok((n, snapshot))
}

/// Assignment-ranking predicate: only hardware-dependent capacity is evaluated
/// here. Full structural/address/routing/security/failure proof still runs once
/// on the selected winner before constructing any VerifiedPackage.
pub(crate) fn assignment_capacity(
    intent: &ResolvedIntent,
    catalog: &CatalogSnapshot,
    t: &AllocatedTopology,
    logical: &LogicalPolicy,
    limits: &VerificationLimits,
) -> Result<Vec<Counterexample>, routing::RoutingError> {
    let wan = routing::build_transport(intent, t);
    let baseline = routing::synthesize_capacity(
        intent,
        t,
        &wan,
        logical,
        &Default::default(),
        &Default::default(),
    )?;
    let mut errors = capacity::check(intent, catalog, t, &wan, &baseline, &Default::default());
    if !errors.is_empty() {
        return Ok(errors);
    }
    let scope = scenarios::cases(intent, t, &wan);
    if scope.cases.len() > limits.max_scenarios {
        return Ok(vec![types::failure(
            "scenario_budget_exhausted",
            "resilience",
            "candidate capacity proof exceeds configured failure scenario budget",
        )]);
    }
    let previous = baseline
        .ownership
        .iter()
        .filter_map(|s| s.owner.as_ref().map(|o| (s.zone.clone(), o.clone())))
        .collect();
    for (name, scenario) in scope.cases {
        let snapshot =
            routing::synthesize_capacity(intent, t, &wan, logical, &scenario, &previous)?;
        errors = capacity::check(intent, catalog, t, &wan, &snapshot, &scenario);
        if !errors.is_empty() {
            for e in &mut errors {
                e.detail = format!("scenario {name}: {}", e.detail);
            }
            return Ok(errors);
        }
    }
    Ok(errors)
}
