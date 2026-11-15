use super::types::*;
use crate::{
    intent::ResolvedIntent,
    model::*,
    policy,
    routing::{self, FailureScenario, RoutingSnapshot},
    topology::AllocatedTopology,
};
/// Exhaust protocol/port equivalence classes, not arbitrary sampled ports. The
/// routing model has one next hop per destination, and all source/target firewall
/// ingresses carry default deny with deny-overrides. Missing bindings cannot pass.
pub(super) fn check(
    intent: &ResolvedIntent,
    t: &AllocatedTopology,
    n: &NetworkDraft,
    snapshot: &RoutingSnapshot,
    scenario: &FailureScenario,
    limit: usize,
) -> (u64, Vec<Counterexample>) {
    let mut errors = Vec::new();
    let mut checked = 0u64;
    let table = routing::RouteTable::new(&snapshot.routes).ok();
    let expected = policy::bindings(intent, n);
    let normalize = |rules: &[PolicyBinding]| {
        let mut v: Vec<_> = rules
            .iter()
            .map(|r| serde_json::to_string(r).expect("policy encoding"))
            .collect();
        v.sort();
        v
    };
    if normalize(&expected) != normalize(&n.policies) {
        errors.push(failure(
            "policy_binding_mismatch",
            "policies",
            "missing, added or altered enforcement rules",
        ));
    }
    for from in t
        .zones()
        .iter()
        .filter(|z| !scenario.sites.contains(&z.site))
    {
        for to in t
            .zones()
            .iter()
            .filter(|z| z.zone != from.zone && !scenario.sites.contains(&z.site))
        {
            let rules: Vec<_> = intent
                .data()
                .policies
                .iter()
                .filter(|p| p.from_zone == from.zone && p.to_zone == to.zone)
                .cloned()
                .collect();
            let source = snapshot
                .ownership
                .iter()
                .find(|o| o.zone == from.zone)
                .and_then(|o| o.owner.as_ref());
            let target = snapshot
                .ownership
                .iter()
                .find(|o| o.zone == to.zone)
                .and_then(|o| o.owner.as_ref());
            let source_rules:Vec<_>=n.policies.iter().filter(|b|b.policy.from_zone==from.zone&&b.policy.to_zone==to.zone&&Some(&b.device)==source&&n.devices.iter().find(|d|d.id==b.device).is_some_and(|d|d.interfaces.iter().any(|f|f.id==b.ingress&&matches!(&f.kind,InterfaceKind::Gateway{zone,..} if zone==&from.zone)))).map(|b|b.policy.clone()).collect();
            let target_rules: Vec<_> = n
                .policies
                .iter()
                .filter(|b| {
                    b.policy.from_zone == from.zone
                        && b.policy.to_zone == to.zone
                        && Some(&b.device) == target
                })
                .map(|b| b.policy.clone())
                .collect();
            if rules.is_empty() && source_rules.is_empty() && target_rules.is_empty() {
                // Default deny proves every protocol class (and the entire TCP/UDP
                // port domain) symbolically; no route or port sampling is needed.
                checked += 256;
                if checked > u64::try_from(limit).unwrap_or(u64::MAX) {
                    errors.push(failure(
                        "service_class_budget",
                        "security",
                        "exhaustive proof budget exhausted; no partial assurance",
                    ));
                    return (checked, errors);
                }
                continue;
            }
            let classes = policy::classes_for_rules(&rules);
            let subnet = &t
                .addresses()
                .assignments()
                .iter()
                .find(|a| a.allocation.id == to.allocation)
                .expect("allocation")
                .allocation
                .subnet;
            let reachable = source.zip(target).is_some_and(|(a, b)| {
                table
                    .as_ref()
                    .is_some_and(|table| table.trace(a, b, subnet).is_ok())
            });
            for service in classes {
                checked += 1;
                if checked > u64::try_from(limit).unwrap_or(u64::MAX) {
                    errors.push(failure(
                        "service_class_budget",
                        "security",
                        "exhaustive proof budget exhausted; no partial assurance",
                    ));
                    return (checked, errors);
                }
                let requested = policy::classify(&rules, &from.zone, &to.zone, &service).permitted;
                let actual = reachable
                    && policy::classify(&source_rules, &from.zone, &to.zone, &service).permitted
                    && policy::classify(&target_rules, &from.zone, &to.zone, &service).permitted;
                if requested != actual {
                    errors.push(failure(
                        if requested {
                            "allowed_flow_unreachable"
                        } else {
                            "isolation_breach"
                        },
                        format!(
                            "{}->{} protocol={} port={:?}",
                            from.zone, to.zone, service.ip_protocol, service.destination_port
                        ),
                        "modeled forwarding/enforcement differs from the requested service matrix",
                    ));
                    if errors.len() >= 128 {
                        return (checked, errors);
                    }
                }
            }
        }
    }
    (checked, errors)
}
