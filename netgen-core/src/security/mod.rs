use thiserror::Error;
use uuid::Uuid;

use crate::graph::{DesignDecision, PolicyAction, SecurityZone, TopologyGraph, TriggerSource, ZonePolicy};
use crate::schema::{Environment, NetworkIntent, SecurityLevel, ZoneRole};

const MAX_SECURITY_ITERATIONS: usize = 10;

// ─── Public types ─────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum SecurityError {
    #[error("Security rule blocked export — rule: {rule_id}, reason: {message}")]
    Blocked { rule_id: String, message: String },
}

/// Severity of a security policy violation.
/// Critical and High are downgraded to Medium when `lab_mode = true`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Export refused until resolved.
    Critical,
    /// Engine corrects the graph automatically, emits a SecurityWarning.
    High,
    /// Flag in report, non-blocking, no graph change.
    Medium,
    /// Recommendation only.
    Low,
}

/// A security policy violation that was detected (and possibly auto-fixed).
#[derive(Debug, Clone)]
pub struct SecurityWarning {
    pub rule_id: String,
    pub severity: Severity,
    pub message: String,
}

// ─── Policy and remediation types ────────────────────────────────────────────

/// A security policy: describes what must be true about the graph.
/// Separate from Remediation: the same policy may require different fixes per vendor.
pub struct SecurityPolicy {
    pub id: &'static str,
    pub name: &'static str,
    pub severity: Severity,
    pub scope_envs: Option<&'static [Environment]>,
    pub scope_levels: Option<&'static [SecurityLevel]>,
    /// Returns Some(violation_message) if the policy is violated.
    pub check: fn(&TopologyGraph, &NetworkIntent) -> Option<String>,
    /// ID of the remediation to apply when this policy fires and severity is High.
    pub remediation_id: Option<&'static str>,
}

/// A remediation: knows how to fix a specific policy violation.
/// Produces a new graph — never mutates.
pub struct Remediation {
    pub id: &'static str,
    /// Which policy this remediates.
    pub policy_ref: &'static str,
    /// Immutable transform: produces a fixed graph from the current one.
    pub fix: fn(&TopologyGraph, &NetworkIntent) -> TopologyGraph,
}

/// A named group of related policies and their remediations.
pub struct SecurityPack {
    pub name: &'static str,
    policies: Vec<SecurityPolicy>,
    remediations: Vec<Remediation>,
}

impl SecurityPack {
    fn new(
        name: &'static str,
        policies: Vec<SecurityPolicy>,
        remediations: Vec<Remediation>,
    ) -> Self {
        Self { name, policies, remediations }
    }
}

// ─── SecurityEngine ───────────────────────────────────────────────────────────

/// Hard gate between the rule engine and config generation.
///
/// Evaluates policies from all loaded packs, applying remediations for High-severity
/// violations, then re-evaluates until stable or max iterations reached.
/// Critical violations block export immediately (downgraded to Medium in lab_mode).
pub struct SecurityEngine {
    policies: Vec<SecurityPolicy>,
    remediations: Vec<Remediation>,
}

impl Default for SecurityEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl SecurityEngine {
    /// Load Base Pack + Enterprise Pack.
    pub fn new() -> Self {
        let mut policies = Vec::new();
        let mut remediations = Vec::new();

        for pack in [base_pack(), enterprise_pack()] {
            policies.extend(pack.policies);
            remediations.extend(pack.remediations);
        }

        Self { policies, remediations }
    }

    /// Evaluate security policies against the graph.
    ///
    /// Returns the (possibly auto-fixed) graph and all warnings produced.
    /// - High policies: apply remediation + emit warning. Re-evaluate until stable.
    /// - Critical policies: return Err immediately (downgraded to Medium in lab_mode).
    /// - Medium / Low: emit warning only.
    pub fn validate(
        &self,
        graph: &TopologyGraph,
        intent: &NetworkIntent,
    ) -> Result<(TopologyGraph, Vec<SecurityWarning>), SecurityError> {
        let lab_mode = intent.constraints.lab_mode;
        let mut current = graph.clone();
        let mut all_warnings: Vec<SecurityWarning> = Vec::new();

        for _ in 0..MAX_SECURITY_ITERATIONS {
            let mut any_fixed = false;

            for policy in &self.policies {
                if !self.policy_in_scope(policy, intent) {
                    continue;
                }

                let Some(violation) = (policy.check)(&current, intent) else {
                    continue;
                };

                match effective_severity(policy.severity, lab_mode) {
                    Severity::Critical => {
                        return Err(SecurityError::Blocked {
                            rule_id: policy.id.to_string(),
                            message: violation,
                        });
                    }
                    Severity::High => {
                        if let Some(rem_id) = policy.remediation_id {
                            if let Some(rem) =
                                self.remediations.iter().find(|r| r.id == rem_id)
                            {
                                let mut fixed = (rem.fix)(&current, intent);
                                fixed.log_decision(
                                    DesignDecision::new(
                                        policy.id,
                                        &violation,
                                        TriggerSource::SecurityPolicy,
                                        false,
                                    )
                                    .with_rule(policy.id),
                                );
                                current = fixed;
                                any_fixed = true;
                            }
                        }
                        all_warnings.push(SecurityWarning {
                            rule_id: policy.id.to_string(),
                            severity: Severity::High,
                            message: format!("[AUTO-FIXED] {violation}"),
                        });
                    }
                    Severity::Medium | Severity::Low => {
                        all_warnings.push(SecurityWarning {
                            rule_id: policy.id.to_string(),
                            severity: effective_severity(policy.severity, lab_mode),
                            message: violation,
                        });
                    }
                }
            }

            if !any_fixed {
                break;
            }
        }

        Ok((current, all_warnings))
    }

    fn policy_in_scope(&self, policy: &SecurityPolicy, intent: &NetworkIntent) -> bool {
        if let Some(envs) = policy.scope_envs {
            if !envs.contains(&intent.metadata.environment) {
                return false;
            }
        }
        if let Some(levels) = policy.scope_levels {
            if !levels.contains(&intent.metadata.security_level) {
                return false;
            }
        }
        true
    }
}

fn effective_severity(base: Severity, lab_mode: bool) -> Severity {
    match base {
        Severity::Critical | Severity::High if lab_mode => Severity::Medium,
        other => other,
    }
}

// ─── Base Pack ────────────────────────────────────────────────────────────────
// Always loaded. Core correctness properties that apply to all environments.

fn base_pack() -> SecurityPack {
    SecurityPack::new(
        "Base",
        vec![
            SecurityPolicy {
                id: "NO_FLAT_NETWORK",
                name: "Flat networks are prohibited",
                severity: Severity::Critical,
                scope_envs: None,
                scope_levels: None,
                check: check_no_flat_network,
                remediation_id: None,
            },
            SecurityPolicy {
                id: "GUEST_ADJACENT_TO_DMZ",
                name: "Guest zone should not be directly adjacent to DMZ",
                severity: Severity::Medium,
                scope_envs: None,
                scope_levels: None,
                check: check_guest_adjacent_to_dmz,
                remediation_id: None,
            },
            SecurityPolicy {
                id: "NO_TELNET",
                name: "Telnet must not be used for management",
                severity: Severity::Critical,
                scope_envs: None,
                scope_levels: None,
                // Config-level check deferred to VerificationEngine (Phase 7).
                check: |_g, _i| None,
                remediation_id: None,
            },
            SecurityPolicy {
                id: "SNMPV3_ONLY",
                name: "Only SNMPv3 is permitted",
                severity: Severity::Critical,
                scope_envs: None,
                scope_levels: Some(&[SecurityLevel::Standard, SecurityLevel::Hardened]),
                // Config-level check deferred to VerificationEngine (Phase 7).
                check: |_g, _i| None,
                remediation_id: None,
            },
        ],
        vec![],
    )
}

// ─── Enterprise Pack ──────────────────────────────────────────────────────────
// Loaded always; individual policies filter by scope.
// Contains production-grade posture rules with auto-fix remediations.

fn enterprise_pack() -> SecurityPack {
    SecurityPack::new(
        "Enterprise",
        vec![
            SecurityPolicy {
                id: "DMZ_REQUIRED_FOR_INTERNET",
                name: "Internet-facing networks require a DMZ zone",
                severity: Severity::High,
                scope_envs: Some(&[Environment::Staging, Environment::Production]),
                scope_levels: None,
                check: check_dmz_required,
                remediation_id: Some("REM_DMZ_REQUIRED"),
            },
            SecurityPolicy {
                id: "MGMT_ISOLATION",
                name: "Management plane must not be directly reachable from user zones",
                severity: Severity::High,
                scope_envs: None,
                scope_levels: Some(&[SecurityLevel::Standard, SecurityLevel::Hardened]),
                check: check_mgmt_isolation,
                remediation_id: Some("REM_MGMT_ISOLATION"),
            },
            SecurityPolicy {
                id: "ACL_DENY_BY_DEFAULT",
                name: "All zone boundaries must have explicit deny policies",
                severity: Severity::High,
                scope_envs: Some(&[Environment::Production]),
                scope_levels: None,
                check: check_acl_deny_by_default,
                remediation_id: Some("REM_ACL_DENY"),
            },
        ],
        vec![
            Remediation {
                id: "REM_DMZ_REQUIRED",
                policy_ref: "DMZ_REQUIRED_FOR_INTERNET",
                fix: fix_dmz_required,
            },
            Remediation {
                id: "REM_MGMT_ISOLATION",
                policy_ref: "MGMT_ISOLATION",
                fix: fix_mgmt_isolation,
            },
            Remediation {
                id: "REM_ACL_DENY",
                policy_ref: "ACL_DENY_BY_DEFAULT",
                fix: fix_acl_deny_by_default,
            },
        ],
    )
}

// ─── Check functions ──────────────────────────────────────────────────────────

fn check_no_flat_network(graph: &TopologyGraph, _: &NetworkIntent) -> Option<String> {
    if graph.zones.len() < 2 {
        Some(format!(
            "Network has {} zone(s) — flat networks are prohibited; \
             at minimum a dedicated MGMT zone is required",
            graph.zones.len()
        ))
    } else {
        None
    }
}

fn check_dmz_required(graph: &TopologyGraph, intent: &NetworkIntent) -> Option<String> {
    if intent.services.internet_gateway && graph.zone_by_role(&ZoneRole::Dmz).is_none() {
        Some(
            "Internet gateway present but no DMZ zone defined — \
              internet-facing services must be placed in a DMZ"
                .into(),
        )
    } else {
        None
    }
}

fn check_mgmt_isolation(graph: &TopologyGraph, _: &NetworkIntent) -> Option<String> {
    let mgmt_id = graph.zones.iter().find(|z| z.role == ZoneRole::Mgmt).map(|z| z.id)?;

    let user_zone_ids: Vec<Uuid> = graph
        .zones
        .iter()
        .filter(|z| matches!(z.role, ZoneRole::Lan | ZoneRole::Guest | ZoneRole::Dmz))
        .map(|z| z.id)
        .collect();

    let violation = graph.policies.iter().any(|p| {
        p.to_zone == mgmt_id
            && p.action == PolicyAction::Permit
            && user_zone_ids.contains(&p.from_zone)
    });

    if violation {
        Some(
            "Management plane is directly accessible from user zones (Permit policy present) — \
             MGMT must only be reachable via OOB; converting to Deny"
                .into(),
        )
    } else {
        None
    }
}

fn check_acl_deny_by_default(graph: &TopologyGraph, _: &NetworkIntent) -> Option<String> {
    let zone_ids: Vec<Uuid> = graph.zones.iter().map(|z| z.id).collect();
    let mut missing = Vec::new();

    for &from in &zone_ids {
        for &to in &zone_ids {
            if from == to {
                continue;
            }
            if graph.policy_between(from, to).is_none() {
                let from_name = graph
                    .zones
                    .iter()
                    .find(|z| z.id == from)
                    .map(|z| z.name.as_str())
                    .unwrap_or("?");
                let to_name = graph
                    .zones
                    .iter()
                    .find(|z| z.id == to)
                    .map(|z| z.name.as_str())
                    .unwrap_or("?");
                missing.push(format!("{from_name}→{to_name}"));
            }
        }
    }

    if missing.is_empty() {
        None
    } else {
        Some(format!(
            "Zone boundaries missing explicit policy (implicit Deny will be enforced): {}",
            missing.join(", ")
        ))
    }
}

fn check_guest_adjacent_to_dmz(graph: &TopologyGraph, _: &NetworkIntent) -> Option<String> {
    let guest_id = graph.zones.iter().find(|z| z.role == ZoneRole::Guest).map(|z| z.id)?;
    let dmz_id = graph.zones.iter().find(|z| z.role == ZoneRole::Dmz).map(|z| z.id)?;

    if graph.adjacent_zones(guest_id).contains(&dmz_id) {
        Some(
            "Guest zone is directly adjacent to DMZ — \
             consider inserting a stateful firewall between them"
                .into(),
        )
    } else {
        None
    }
}

// ─── Fix (remediation) functions ──────────────────────────────────────────────

fn fix_dmz_required(graph: &TopologyGraph, _: &NetworkIntent) -> TopologyGraph {
    let mut g = graph.clone();
    g.add_zone(SecurityZone::new("DMZ", ZoneRole::Dmz, 40));
    g
}

fn fix_mgmt_isolation(graph: &TopologyGraph, _: &NetworkIntent) -> TopologyGraph {
    let mut g = graph.clone();

    let mgmt_id = match g.zones.iter().find(|z| z.role == ZoneRole::Mgmt).map(|z| z.id) {
        Some(id) => id,
        None => return g,
    };

    let user_zone_ids: Vec<Uuid> = g
        .zones
        .iter()
        .filter(|z| matches!(z.role, ZoneRole::Lan | ZoneRole::Guest | ZoneRole::Dmz))
        .map(|z| z.id)
        .collect();

    for policy in g.policies.iter_mut() {
        if policy.to_zone == mgmt_id
            && policy.action == PolicyAction::Permit
            && user_zone_ids.contains(&policy.from_zone)
        {
            policy.action = PolicyAction::Deny;
        }
    }

    g
}

fn fix_acl_deny_by_default(graph: &TopologyGraph, _: &NetworkIntent) -> TopologyGraph {
    let mut g = graph.clone();
    let zone_ids: Vec<Uuid> = g.zones.iter().map(|z| z.id).collect();
    let mut to_add = Vec::new();

    for &from in &zone_ids {
        for &to in &zone_ids {
            if from == to {
                continue;
            }
            if g.policy_between(from, to).is_none() {
                to_add.push((from, to));
            }
        }
    }

    for (from, to) in to_add {
        g.add_policy(ZonePolicy::new(from, to, PolicyAction::Deny));
    }

    g
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{LinkType, NetworkLink, NetworkNode, NodeRole};
    use crate::schema::{
        AddressingIntent, AddressingMode, AddressingScheme, ConstraintsIntent, ContextIntent,
        IntentMetadata, Ipv4Config, NetworkIntent, RedundancyLevel, RoutingIntent, RoutingProtocol,
        SegmentationIntent, ServicesIntent, TopologyIntent, TopologyType, Vendor, ZoneIntent,
        CURRENT_SCHEMA_VERSION,
    };

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn make_intent(
        env: Environment,
        level: SecurityLevel,
        lab_mode: bool,
        internet: bool,
    ) -> NetworkIntent {
        NetworkIntent {
            metadata: IntentMetadata {
                name: "sec-test".into(),
                environment: env,
                vendor_preference: vec![Vendor::Cisco],
                security_level: level,
                schema_version: CURRENT_SCHEMA_VERSION.into(),
            },
            topology: TopologyIntent {
                sites: 1,
                topology_type: TopologyType::PointToPoint,
                redundancy_level: RedundancyLevel::None,
            },
            routing: RoutingIntent {
                protocols: vec![RoutingProtocol::Static],
                ospf: None,
                bgp: None,
            },
            segmentation: SegmentationIntent {
                dmz: false,
                vlan_count: 2,
                zones: vec![ZoneIntent {
                    name: "LAN".into(),
                    role: ZoneRole::Lan,
                    trust_level: 80,
                }],
            },
            addressing: AddressingIntent {
                mode: AddressingMode::Ipv4Only,
                ipv4: Some(Ipv4Config {
                    base_range: "10.0.0.0/8".into(),
                    scheme: AddressingScheme::CidrOptimized,
                }),
                ipv6: None,
            },
            services: ServicesIntent {
                internet_gateway: internet,
                vpn: false,
                nat: false,
                dns: false,
                ntp: false,
            },
            constraints: ConstraintsIntent {
                no_single_point_of_failure: false,
                strict_security: false,
                lab_mode,
            },
            context: ContextIntent {
                industry: None,
                compliance_profiles: vec![],
                departments: vec![],
                custom_labels: vec![],
            },
        }
    }

    fn two_zone_graph() -> (TopologyGraph, Uuid, Uuid) {
        let mut g = TopologyGraph::new("sec-test");
        let lan = SecurityZone::new("LAN", ZoneRole::Lan, 80);
        let mgmt = SecurityZone::new("MGMT", ZoneRole::Mgmt, 90);
        let (lan_id, mgmt_id) = (lan.id, mgmt.id);
        g.add_zone(lan);
        g.add_zone(mgmt);
        (g, lan_id, mgmt_id)
    }

    fn router_in(zone_id: Uuid) -> NetworkNode {
        NetworkNode::new("RTR-01", NodeRole::Router, Vendor::Cisco, zone_id)
    }

    fn eth_link(a: &NetworkNode, b: &NetworkNode) -> NetworkLink {
        NetworkLink::new(a.id, Uuid::new_v4(), b.id, Uuid::new_v4(), LinkType::Ethernet)
    }

    // ── NO_FLAT_NETWORK ───────────────────────────────────────────────────────

    #[test]
    fn no_flat_network_blocks_single_zone() {
        let engine = SecurityEngine::new();
        let mut g = TopologyGraph::new("test");
        g.add_zone(SecurityZone::new("LAN", ZoneRole::Lan, 80));
        let intent = make_intent(Environment::Production, SecurityLevel::Standard, false, false);
        assert!(matches!(
            engine.validate(&g, &intent),
            Err(SecurityError::Blocked { ref rule_id, .. }) if rule_id == "NO_FLAT_NETWORK"
        ));
    }

    #[test]
    fn no_flat_network_blocks_regardless_of_environment_when_not_lab_mode() {
        let engine = SecurityEngine::new();
        let mut g = TopologyGraph::new("test");
        g.add_zone(SecurityZone::new("LAN", ZoneRole::Lan, 80));
        let intent = make_intent(Environment::Lab, SecurityLevel::Basic, false, false);
        assert!(matches!(engine.validate(&g, &intent), Err(SecurityError::Blocked { .. })));
    }

    #[test]
    fn no_flat_network_downgrades_to_warn_in_lab_mode() {
        let engine = SecurityEngine::new();
        let mut g = TopologyGraph::new("test");
        g.add_zone(SecurityZone::new("LAN", ZoneRole::Lan, 80));
        let intent = make_intent(Environment::Lab, SecurityLevel::Basic, true, false);
        let (_, warnings) = engine.validate(&g, &intent).unwrap();
        assert!(warnings.iter().any(|w| w.rule_id == "NO_FLAT_NETWORK"));
    }

    #[test]
    fn no_flat_network_passes_with_two_zones() {
        let engine = SecurityEngine::new();
        let (g, _, _) = two_zone_graph();
        let intent = make_intent(Environment::Lab, SecurityLevel::Basic, false, false);
        assert!(engine.validate(&g, &intent).is_ok());
    }

    // ── DMZ_REQUIRED_FOR_INTERNET ─────────────────────────────────────────────

    #[test]
    fn dmz_required_auto_fixes_in_production() {
        let engine = SecurityEngine::new();
        let (g, _, _) = two_zone_graph();
        let intent = make_intent(Environment::Production, SecurityLevel::Basic, false, true);
        let (graph, _) = engine.validate(&g, &intent).unwrap();
        assert!(graph.zone_by_role(&ZoneRole::Dmz).is_some(), "DMZ zone should have been added");
    }

    #[test]
    fn dmz_required_auto_fixes_in_staging() {
        let engine = SecurityEngine::new();
        let (g, _, _) = two_zone_graph();
        let intent = make_intent(Environment::Staging, SecurityLevel::Basic, false, true);
        let (graph, _) = engine.validate(&g, &intent).unwrap();
        assert!(graph.zone_by_role(&ZoneRole::Dmz).is_some());
    }

    #[test]
    fn dmz_required_skipped_in_lab_environment() {
        let engine = SecurityEngine::new();
        let (g, _, _) = two_zone_graph();
        let intent = make_intent(Environment::Lab, SecurityLevel::Basic, false, true);
        let (graph, _) = engine.validate(&g, &intent).unwrap();
        assert!(
            graph.zone_by_role(&ZoneRole::Dmz).is_none(),
            "DMZ rule must not fire in Lab environment"
        );
    }

    #[test]
    fn dmz_required_passes_when_dmz_already_present() {
        let engine = SecurityEngine::new();
        let (mut g, _, _) = two_zone_graph();
        g.add_zone(SecurityZone::new("DMZ", ZoneRole::Dmz, 40));
        let intent = make_intent(Environment::Production, SecurityLevel::Basic, false, true);
        let (_, warnings) = engine.validate(&g, &intent).unwrap();
        assert!(!warnings.iter().any(|w| w.rule_id == "DMZ_REQUIRED_FOR_INTERNET"));
    }

    #[test]
    fn dmz_required_lab_mode_warns_without_fixing() {
        let engine = SecurityEngine::new();
        let (g, _, _) = two_zone_graph();
        let intent = make_intent(Environment::Production, SecurityLevel::Basic, true, true);
        let (graph, warnings) = engine.validate(&g, &intent).unwrap();
        assert!(
            warnings.iter().any(|w| w.rule_id == "DMZ_REQUIRED_FOR_INTERNET"),
            "should emit a warning"
        );
        assert!(graph.zone_by_role(&ZoneRole::Dmz).is_none(), "lab_mode must not apply the fix");
    }

    // ── MGMT_ISOLATION ────────────────────────────────────────────────────────

    #[test]
    fn mgmt_isolation_changes_permit_to_deny() {
        let engine = SecurityEngine::new();
        let (mut g, lan_id, mgmt_id) = two_zone_graph();
        g.add_policy(ZonePolicy::new(lan_id, mgmt_id, PolicyAction::Permit));
        let intent = make_intent(Environment::Staging, SecurityLevel::Standard, false, false);
        let (graph, _) = engine.validate(&g, &intent).unwrap();
        assert_eq!(
            graph.policy_between(lan_id, mgmt_id).map(|p| &p.action),
            Some(&PolicyAction::Deny)
        );
    }

    #[test]
    fn mgmt_isolation_skipped_for_basic_security_level() {
        let engine = SecurityEngine::new();
        let (mut g, lan_id, mgmt_id) = two_zone_graph();
        g.add_policy(ZonePolicy::new(lan_id, mgmt_id, PolicyAction::Permit));
        let intent = make_intent(Environment::Production, SecurityLevel::Basic, false, false);
        let (graph, _) = engine.validate(&g, &intent).unwrap();
        // Rule does not apply to Basic — Permit must remain unchanged
        assert_eq!(
            graph.policy_between(lan_id, mgmt_id).map(|p| &p.action),
            Some(&PolicyAction::Permit)
        );
    }

    #[test]
    fn mgmt_isolation_passes_when_deny_already_set() {
        let engine = SecurityEngine::new();
        let (mut g, lan_id, mgmt_id) = two_zone_graph();
        g.add_policy(ZonePolicy::new(lan_id, mgmt_id, PolicyAction::Deny));
        let intent = make_intent(Environment::Production, SecurityLevel::Hardened, false, false);
        let (_, warnings) = engine.validate(&g, &intent).unwrap();
        assert!(!warnings.iter().any(|w| w.rule_id == "MGMT_ISOLATION"));
    }

    #[test]
    fn mgmt_isolation_lab_mode_warns_without_fixing() {
        let engine = SecurityEngine::new();
        let (mut g, lan_id, mgmt_id) = two_zone_graph();
        g.add_policy(ZonePolicy::new(lan_id, mgmt_id, PolicyAction::Permit));
        let intent = make_intent(Environment::Production, SecurityLevel::Standard, true, false);
        let (graph, warnings) = engine.validate(&g, &intent).unwrap();
        assert!(warnings.iter().any(|w| w.rule_id == "MGMT_ISOLATION"));
        // lab_mode — policy must not be mutated
        assert_eq!(
            graph.policy_between(lan_id, mgmt_id).map(|p| &p.action),
            Some(&PolicyAction::Permit)
        );
    }

    // ── ACL_DENY_BY_DEFAULT ───────────────────────────────────────────────────

    #[test]
    fn acl_deny_enforced_in_production_fills_all_zone_pairs() {
        let engine = SecurityEngine::new();
        let (g, lan_id, mgmt_id) = two_zone_graph();
        let intent = make_intent(Environment::Production, SecurityLevel::Basic, false, false);
        let (graph, _) = engine.validate(&g, &intent).unwrap();
        assert!(graph.policy_between(lan_id, mgmt_id).is_some(), "LAN→MGMT must have a policy");
        assert!(graph.policy_between(mgmt_id, lan_id).is_some(), "MGMT→LAN must have a policy");
        assert_eq!(
            graph.policy_between(lan_id, mgmt_id).map(|p| &p.action),
            Some(&PolicyAction::Deny)
        );
        assert_eq!(
            graph.policy_between(mgmt_id, lan_id).map(|p| &p.action),
            Some(&PolicyAction::Deny)
        );
    }

    #[test]
    fn acl_deny_skipped_in_staging() {
        let engine = SecurityEngine::new();
        let (g, lan_id, mgmt_id) = two_zone_graph();
        let intent = make_intent(Environment::Staging, SecurityLevel::Basic, false, false);
        let (graph, _) = engine.validate(&g, &intent).unwrap();
        assert!(graph.policy_between(lan_id, mgmt_id).is_none());
        assert!(graph.policy_between(mgmt_id, lan_id).is_none());
    }

    #[test]
    fn acl_deny_skipped_in_lab_environment() {
        let engine = SecurityEngine::new();
        let (g, lan_id, mgmt_id) = two_zone_graph();
        let intent = make_intent(Environment::Lab, SecurityLevel::Basic, false, false);
        let (graph, _) = engine.validate(&g, &intent).unwrap();
        assert!(graph.policy_between(lan_id, mgmt_id).is_none());
        assert!(graph.policy_between(mgmt_id, lan_id).is_none());
    }

    #[test]
    fn acl_deny_does_not_overwrite_existing_policies() {
        let engine = SecurityEngine::new();
        let (mut g, lan_id, mgmt_id) = two_zone_graph();
        g.add_policy(ZonePolicy::new(lan_id, mgmt_id, PolicyAction::Inspect));
        let intent = make_intent(Environment::Production, SecurityLevel::Basic, false, false);
        let (graph, _) = engine.validate(&g, &intent).unwrap();
        // Existing Inspect policy must remain unchanged
        assert_eq!(
            graph.policy_between(lan_id, mgmt_id).map(|p| &p.action),
            Some(&PolicyAction::Inspect)
        );
    }

    // ── GUEST_ADJACENT_TO_DMZ ─────────────────────────────────────────────────

    #[test]
    fn guest_adjacent_to_dmz_emits_warning() {
        let engine = SecurityEngine::new();
        let mut g = TopologyGraph::new("test");
        let dmz = SecurityZone::new("DMZ", ZoneRole::Dmz, 40);
        let guest = SecurityZone::new("Guest", ZoneRole::Guest, 10);
        g.add_zone(SecurityZone::new("LAN", ZoneRole::Lan, 80));
        g.add_zone(dmz.clone());
        g.add_zone(guest.clone());

        let dmz_rtr = router_in(dmz.id);
        let guest_rtr = router_in(guest.id);
        let l = eth_link(&dmz_rtr, &guest_rtr);
        g.add_node(dmz_rtr);
        g.add_node(guest_rtr);
        g.add_link(l);

        let intent = make_intent(Environment::Lab, SecurityLevel::Basic, false, false);
        let (_, warnings) = engine.validate(&g, &intent).unwrap();
        assert!(warnings.iter().any(|w| w.rule_id == "GUEST_ADJACENT_TO_DMZ"));
    }

    #[test]
    fn guest_not_adjacent_to_dmz_passes() {
        let engine = SecurityEngine::new();
        let mut g = TopologyGraph::new("test");
        g.add_zone(SecurityZone::new("LAN", ZoneRole::Lan, 80));
        g.add_zone(SecurityZone::new("DMZ", ZoneRole::Dmz, 40));
        g.add_zone(SecurityZone::new("Guest", ZoneRole::Guest, 10));
        let intent = make_intent(Environment::Lab, SecurityLevel::Basic, false, false);
        let (_, warnings) = engine.validate(&g, &intent).unwrap();
        assert!(!warnings.iter().any(|w| w.rule_id == "GUEST_ADJACENT_TO_DMZ"));
    }

    // ── Decision log ──────────────────────────────────────────────────────────

    #[test]
    fn autofix_logs_decision() {
        let engine = SecurityEngine::new();
        let (g, _, _) = two_zone_graph();
        let intent = make_intent(Environment::Production, SecurityLevel::Basic, false, true);
        let (graph, _) = engine.validate(&g, &intent).unwrap();
        assert!(
            graph
                .decisions
                .iter()
                .any(|d| d.rule_id.as_deref() == Some("DMZ_REQUIRED_FOR_INTERNET")),
            "High severity must record a DesignDecision"
        );
    }

    #[test]
    fn enforce_logs_decision() {
        let engine = SecurityEngine::new();
        let (g, _, _) = two_zone_graph();
        let intent = make_intent(Environment::Production, SecurityLevel::Basic, false, false);
        let (graph, _) = engine.validate(&g, &intent).unwrap();
        assert!(
            graph
                .decisions
                .iter()
                .any(|d| d.rule_id.as_deref() == Some("ACL_DENY_BY_DEFAULT")),
            "High severity must record a DesignDecision"
        );
    }

    #[test]
    fn lab_mode_warn_does_not_log_decision_for_downgraded_block() {
        let engine = SecurityEngine::new();
        let mut g = TopologyGraph::new("test");
        g.add_zone(SecurityZone::new("LAN", ZoneRole::Lan, 80));
        let intent = make_intent(Environment::Lab, SecurityLevel::Basic, true, false);
        let (graph, _) = engine.validate(&g, &intent).unwrap();
        // Downgraded Critical → Medium: must not write to the decision log
        assert!(graph.decisions.iter().all(|d| d.rule_id.as_deref() != Some("NO_FLAT_NETWORK")));
    }

    // ── Clean graph ───────────────────────────────────────────────────────────

    #[test]
    fn clean_lab_graph_returns_empty_warnings() {
        let engine = SecurityEngine::new();
        let (g, _, _) = two_zone_graph();
        let intent = make_intent(Environment::Lab, SecurityLevel::Basic, false, false);
        let (_, warnings) = engine.validate(&g, &intent).unwrap();
        assert!(warnings.is_empty());
    }

    // ── Re-evaluation loop ────────────────────────────────────────────────────

    #[test]
    fn re_evaluation_fixes_cascading_violations() {
        // In Production with internet_gateway: DMZ is auto-added, triggering
        // ACL_DENY_BY_DEFAULT (now 3 zones with missing policies). Both must be fixed.
        let engine = SecurityEngine::new();
        let (g, lan_id, mgmt_id) = two_zone_graph();
        let intent = make_intent(Environment::Production, SecurityLevel::Basic, false, true);
        let (graph, _) = engine.validate(&g, &intent).unwrap();
        // DMZ was injected
        assert!(graph.zone_by_role(&ZoneRole::Dmz).is_some());
        // All zone pair policies exist (ACL_DENY fired on the expanded graph)
        assert!(graph.policy_between(lan_id, mgmt_id).is_some());
        assert!(graph.policy_between(mgmt_id, lan_id).is_some());
        let n = graph.zones.len();
        assert_eq!(graph.policies.len(), n * (n - 1));
    }

    #[test]
    fn severity_enum_coverage() {
        assert_eq!(effective_severity(Severity::Critical, false), Severity::Critical);
        assert_eq!(effective_severity(Severity::High, false), Severity::High);
        assert_eq!(effective_severity(Severity::Critical, true), Severity::Medium);
        assert_eq!(effective_severity(Severity::High, true), Severity::Medium);
        assert_eq!(effective_severity(Severity::Medium, true), Severity::Medium);
        assert_eq!(effective_severity(Severity::Low, true), Severity::Low);
    }
}
