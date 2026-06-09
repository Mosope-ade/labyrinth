use std::collections::HashMap;

use ipnetwork::Ipv4Network;

use crate::capability::{CapabilityError, CapabilityResolutionEngine};
use crate::graph::{NodeRole, TopologyGraph};
use crate::schema::NetworkIntent;

// ─── Result types ─────────────────────────────────────────────────────────────

/// A single failed verification check with enough context to fix it.
#[derive(Debug, Clone)]
pub struct VerificationFailure {
    /// Which check produced this failure (matches the five canonical check names).
    pub check: &'static str,
    /// The specific graph component involved (node label, zone pair, etc.).
    pub component: Option<String>,
    /// Human-readable description of the problem.
    pub message: String,
    /// Ordered list of suggested fixes.
    pub resolution: Vec<String>,
}

impl std::fmt::Display for VerificationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(ref c) = self.component {
            write!(f, "[{}] {}: {}", self.check, c, self.message)
        } else {
            write!(f, "[{}] {}", self.check, self.message)
        }
    }
}

/// Returned when all blocking checks pass.
#[derive(Debug)]
pub struct VerificationReport {
    /// Names of checks that passed without any issues.
    pub passed: Vec<&'static str>,
}

impl VerificationReport {
    pub fn all_passed(&self) -> bool {
        self.passed.len() == CHECK_NAMES.len()
    }
}

const CHECK_NAMES: [&str; 5] = [
    "interface_consistency",
    "routing_reachability",
    "zone_policy_completeness",
    "addressing_conflicts",
    "capability_contracts",
];

// ─── VerificationEngine ───────────────────────────────────────────────────────

/// Post-generation structural gate.
///
/// Runs after config generation. If any check fails the pipeline is blocked —
/// no partial configs are exported. All failures are collected and returned at
/// once so the user can fix everything in one round.
pub struct VerificationEngine;

impl VerificationEngine {
    /// Run all five post-generation checks against the final graph.
    ///
    /// Returns `Ok(VerificationReport)` only when every check passes.
    /// Returns `Err(failures)` — every violation found — if any check fails.
    pub fn verify(
        &self,
        graph: &TopologyGraph,
        intent: &NetworkIntent,
    ) -> Result<VerificationReport, Vec<VerificationFailure>> {
        let check_results: [(&'static str, Vec<VerificationFailure>); 5] = [
            ("interface_consistency",     check_interface_consistency(graph)),
            ("routing_reachability",      check_routing_reachability(graph)),
            ("zone_policy_completeness",  check_zone_policy_completeness(graph)),
            ("addressing_conflicts",      check_addressing_conflicts(graph)),
            ("capability_contracts",      check_capability_contracts(graph, intent)),
        ];

        let mut failures: Vec<VerificationFailure> = Vec::new();
        let mut passed: Vec<&'static str> = Vec::new();

        for (name, check_failures) in check_results {
            if check_failures.is_empty() {
                passed.push(name);
            } else {
                failures.extend(check_failures);
            }
        }

        if failures.is_empty() {
            Ok(VerificationReport { passed })
        } else {
            Err(failures)
        }
    }
}

// ─── Check 1: Interface Consistency ──────────────────────────────────────────

fn check_interface_consistency(graph: &TopologyGraph) -> Vec<VerificationFailure> {
    let mut failures = Vec::new();

    for node in &graph.nodes {
        // Hosts and cloud nodes are terminal — no interface requirement
        if matches!(node.role, NodeRole::Host | NodeRole::Cloud) {
            continue;
        }
        if node.interfaces.is_empty() {
            failures.push(VerificationFailure {
                check: "interface_consistency",
                component: Some(node.label.clone()),
                message: format!("Node '{}' (role: {:?}) has no interfaces", node.label, node.role),
                resolution: vec![
                    "Run the link injection rule to populate interfaces".into(),
                    "Verify the rule engine completed successfully".into(),
                ],
            });
        }
    }

    // Every edge endpoint must reference an interface that exists on that node
    for edge in &graph.edges {
        let from_ok = graph.nodes.iter()
            .find(|n| n.id == edge.from_node)
            .is_some_and(|n| n.interfaces.iter().any(|i| i.id == edge.from_iface));
        let to_ok = graph.nodes.iter()
            .find(|n| n.id == edge.to_node)
            .is_some_and(|n| n.interfaces.iter().any(|i| i.id == edge.to_iface));

        if !from_ok || !to_ok {
            failures.push(VerificationFailure {
                check: "interface_consistency",
                component: Some(format!("edge {}", edge.id)),
                message: format!(
                    "Edge {} references a non-existent interface (from_ok={}, to_ok={})",
                    edge.id, from_ok, to_ok
                ),
                resolution: vec!["Regenerate links — stale edge referencing deleted interface".into()],
            });
        }
    }

    failures
}

// ─── Check 2: Routing Reachability ───────────────────────────────────────────

fn check_routing_reachability(graph: &TopologyGraph) -> Vec<VerificationFailure> {
    // Hosts and cloud nodes are terminal — they do not participate in routing.
    let active: Vec<&crate::graph::NetworkNode> = graph.nodes.iter()
        .filter(|n| !matches!(n.role, NodeRole::Host | NodeRole::Cloud))
        .collect();

    if active.len() <= 1 {
        return vec![];
    }

    let start = active[0].id;
    let reachable: std::collections::HashSet<uuid::Uuid> =
        graph.reachable_from(start).into_iter().collect();

    active.iter()
        .filter(|n| !reachable.contains(&n.id))
        .map(|n| VerificationFailure {
            check: "routing_reachability",
            component: Some(n.label.clone()),
            message: format!(
                "Node '{}' is isolated — not reachable from '{}'",
                n.label, active[0].label
            ),
            resolution: vec![
                "Add a link connecting this node to the rest of the topology".into(),
                "Verify the link injection rule fired for this node's zone".into(),
            ],
        })
        .collect()
}

// ─── Check 3: Zone Policy Completeness ───────────────────────────────────────

fn check_zone_policy_completeness(graph: &TopologyGraph) -> Vec<VerificationFailure> {
    let mut failures = Vec::new();
    let n = graph.zones.len();

    for i in 0..n {
        for j in 0..n {
            if i == j {
                continue;
            }
            let from = &graph.zones[i];
            let to = &graph.zones[j];
            if graph.policy_between(from.id, to.id).is_none() {
                failures.push(VerificationFailure {
                    check: "zone_policy_completeness",
                    component: Some(format!("{} \u{2192} {}", from.name, to.name)),
                    message: format!(
                        "No zone policy from '{}' to '{}' — every zone pair must have an explicit policy",
                        from.name, to.name
                    ),
                    resolution: vec![
                        "Run the zone policy derivation rule".into(),
                        "The rule engine must complete before verification".into(),
                    ],
                });
            }
        }
    }

    failures
}

// ─── Check 4: Addressing Conflicts ───────────────────────────────────────────

fn check_addressing_conflicts(graph: &TopologyGraph) -> Vec<VerificationFailure> {
    let mut failures = Vec::new();
    let mut seen_v4: HashMap<String, String> = HashMap::new();
    let mut seen_v6: HashMap<String, String> = HashMap::new();

    for node in &graph.nodes {
        for iface in &node.interfaces {
            if let Some(ref cidr) = iface.ipv4_addr {
                let ip = cidr.split('/').next().unwrap_or(cidr).to_string();
                if let Some(existing) = seen_v4.get(&ip) {
                    failures.push(VerificationFailure {
                        check: "addressing_conflicts",
                        component: Some(format!("{}:{}", node.label, iface.name)),
                        message: format!(
                            "IPv4 address {} is already assigned to '{}'",
                            ip, existing
                        ),
                        resolution: vec![
                            "Widen the base IP range to give each zone a non-overlapping block".into(),
                        ],
                    });
                } else {
                    seen_v4.insert(ip, format!("{}:{}", node.label, iface.name));
                }
            }

            if let Some(ref cidr) = iface.ipv6_addr {
                let ip = cidr.split('/').next().unwrap_or(cidr).to_string();
                if let Some(existing) = seen_v6.get(&ip) {
                    failures.push(VerificationFailure {
                        check: "addressing_conflicts",
                        component: Some(format!("{}:{}", node.label, iface.name)),
                        message: format!(
                            "IPv6 address {} is already assigned to '{}'",
                            ip, existing
                        ),
                        resolution: vec!["Regenerate IPv6 addresses from a wider ULA block".into()],
                    });
                } else {
                    seen_v6.insert(ip, format!("{}:{}", node.label, iface.name));
                }
            }
        }
    }

    // Overlapping zone subnets
    let allocs = &graph.addressing.ipv4_allocations;
    for i in 0..allocs.len() {
        for j in (i + 1)..allocs.len() {
            if subnets_overlap(&allocs[i].subnet, &allocs[j].subnet) {
                let zi = zone_name(graph, allocs[i].zone_id);
                let zj = zone_name(graph, allocs[j].zone_id);
                failures.push(VerificationFailure {
                    check: "addressing_conflicts",
                    component: Some(format!("{}/{}", zi, zj)),
                    message: format!(
                        "Subnet {} (zone '{}') overlaps with {} (zone '{}')",
                        allocs[i].subnet, zi, allocs[j].subnet, zj
                    ),
                    resolution: vec![
                        "Expand the base IP range so each zone gets a distinct, non-overlapping block".into(),
                    ],
                });
            }
        }
    }

    failures
}

fn subnets_overlap(a: &str, b: &str) -> bool {
    let Ok(net_a) = a.parse::<Ipv4Network>() else { return false };
    let Ok(net_b) = b.parse::<Ipv4Network>() else { return false };
    net_a.contains(net_b.network()) || net_b.contains(net_a.network())
}

fn zone_name(graph: &TopologyGraph, id: uuid::Uuid) -> String {
    graph.zones.iter()
        .find(|z| z.id == id)
        .map(|z| z.name.clone())
        .unwrap_or_else(|| id.to_string())
}

// ─── Check 5: Capability Contract Satisfaction ───────────────────────────────

fn check_capability_contracts(
    graph: &TopologyGraph,
    intent: &NetworkIntent,
) -> Vec<VerificationFailure> {
    let engine = CapabilityResolutionEngine::new();
    match engine.resolve(graph, intent) {
        Ok(_) => vec![],
        Err(errors) => errors.into_iter().map(|e| {
            let (component, message) = match &e {
                CapabilityError::CapabilityGap { node_label, .. } => {
                    (Some(node_label.clone()), e.to_string())
                }
                CapabilityError::UnknownVendor { node_label, vendor, .. } => (
                    Some(node_label.clone()),
                    format!(
                        "No capability profile for vendor {:?} on node '{}'",
                        vendor, node_label
                    ),
                ),
            };
            VerificationFailure {
                check: "capability_contracts",
                component,
                message,
                resolution: vec![
                    "Switch to a supported vendor or add a capability profile for this vendor".into(),
                ],
            }
        }).collect(),
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{
        Interface, InterfaceRole, Ipv4Allocation, LinkType, NetworkLink, NetworkNode, NodeRole,
        PolicyAction, SecurityZone, TopologyGraph, ZonePolicy,
    };
    use crate::schema::{
        AddressingIntent, AddressingMode, AddressingScheme, ConstraintsIntent, ContextIntent,
        Environment, IntentMetadata, Ipv4Config, NetworkIntent, RedundancyLevel, RoutingIntent,
        RoutingProtocol, SecurityLevel, SegmentationIntent, ServicesIntent, TopologyIntent,
        TopologyType, Vendor, ZoneIntent, ZoneRole, CURRENT_SCHEMA_VERSION,
    };

    fn engine() -> VerificationEngine {
        VerificationEngine
    }

    fn base_intent() -> NetworkIntent {
        NetworkIntent {
            metadata: IntentMetadata {
                name: "verify-test".into(),
                environment: Environment::Lab,
                vendor_preference: vec![Vendor::Cisco],
                security_level: SecurityLevel::Basic,
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
                vlan_count: 1,
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
                internet_gateway: false,
                vpn: false,
                nat: false,
                dns: false,
                ntp: false,
            },
            constraints: ConstraintsIntent {
                no_single_point_of_failure: false,
                strict_security: false,
                lab_mode: true,
            },
            context: ContextIntent {
                industry: None,
                compliance_profiles: vec![],
                departments: vec![],
                custom_labels: vec![],
            },
        }
    }

    /// Minimal fully-valid graph: two zones, two routers, linked, full policy matrix,
    /// non-overlapping address allocations.
    fn valid_two_zone_graph() -> TopologyGraph {
        let mut g = TopologyGraph::new("valid");

        let lan = SecurityZone::new("LAN", ZoneRole::Lan, 80);
        let mgmt = SecurityZone::new("MGMT", ZoneRole::Mgmt, 90);
        let lan_id = lan.id;
        let mgmt_id = mgmt.id;
        g.add_zone(lan);
        g.add_zone(mgmt);

        g.add_policy(ZonePolicy::new(lan_id, mgmt_id, PolicyAction::Permit));
        g.add_policy(ZonePolicy::new(mgmt_id, lan_id, PolicyAction::Deny));

        let mut rtr_lan = NetworkNode::new("RTR-LAN-01", NodeRole::Router, Vendor::Cisco, lan_id);
        let mut iface_lan = Interface::new("GigabitEthernet0/0", InterfaceRole::Access);
        iface_lan.ipv4_addr = Some("10.10.10.1/24".into());
        rtr_lan.interfaces.push(iface_lan);

        let mut rtr_mgmt = NetworkNode::new("RTR-MGMT-01", NodeRole::Router, Vendor::Cisco, mgmt_id);
        let mut iface_mgmt = Interface::new("GigabitEthernet0/0", InterfaceRole::Access);
        iface_mgmt.ipv4_addr = Some("10.10.20.1/28".into());
        rtr_mgmt.interfaces.push(iface_mgmt);

        let rtr_lan_iface_id  = rtr_lan.interfaces[0].id;
        let rtr_mgmt_iface_id = rtr_mgmt.interfaces[0].id;
        let rtr_lan_id  = rtr_lan.id;
        let rtr_mgmt_id = rtr_mgmt.id;
        g.add_node(rtr_lan);
        g.add_node(rtr_mgmt);

        g.edges.push(NetworkLink::new(
            rtr_lan_id,  rtr_lan_iface_id,
            rtr_mgmt_id, rtr_mgmt_iface_id,
            LinkType::Ethernet,
        ));

        g.addressing.ipv4_allocations.push(Ipv4Allocation {
            zone_id: lan_id,
            subnet: "10.10.10.0/24".into(),
            purpose: "LAN /24".into(),
        });
        g.addressing.ipv4_allocations.push(Ipv4Allocation {
            zone_id: mgmt_id,
            subnet: "10.10.20.0/28".into(),
            purpose: "MGMT /28".into(),
        });

        g
    }

    // ── Full verify ───────────────────────────────────────────────────────────

    #[test]
    fn verify_passes_for_complete_graph() {
        let g = valid_two_zone_graph();
        let result = engine().verify(&g, &base_intent());
        assert!(result.is_ok(), "expected Ok: {:?}", result.err());
    }

    #[test]
    fn verify_report_lists_all_five_checks_as_passed() {
        let report = engine().verify(&valid_two_zone_graph(), &base_intent()).unwrap();
        assert_eq!(report.passed.len(), 5, "got: {:?}", report.passed);
        assert!(report.all_passed());
    }

    #[test]
    fn verify_empty_graph_passes() {
        let g = TopologyGraph::new("empty");
        assert!(engine().verify(&g, &base_intent()).is_ok());
    }

    // ── Interface consistency ─────────────────────────────────────────────────

    #[test]
    fn interface_consistency_fails_for_router_without_interfaces() {
        let mut g = valid_two_zone_graph();
        let zone_id = g.zones[0].id;
        g.add_node(NetworkNode::new("RTR-BARE", NodeRole::Router, Vendor::Cisco, zone_id));

        let failures = engine().verify(&g, &base_intent()).unwrap_err();
        assert!(
            failures.iter().any(|f| {
                f.check == "interface_consistency"
                    && f.component.as_deref() == Some("RTR-BARE")
            }),
            "got: {failures:?}"
        );
    }

    #[test]
    fn interface_consistency_host_without_interfaces_passes() {
        let mut g = valid_two_zone_graph();
        let zone_id = g.zones[0].id;
        g.add_node(NetworkNode::new("HOST-01", NodeRole::Host, Vendor::Cisco, zone_id));
        assert!(engine().verify(&g, &base_intent()).is_ok());
    }

    #[test]
    fn interface_consistency_cloud_without_interfaces_passes() {
        let mut g = valid_two_zone_graph();
        let zone_id = g.zones[0].id;
        g.add_node(NetworkNode::new("INTERNET", NodeRole::Cloud, Vendor::Cisco, zone_id));
        assert!(engine().verify(&g, &base_intent()).is_ok());
    }

    // ── Routing reachability ──────────────────────────────────────────────────

    #[test]
    fn routing_reachability_passes_for_connected_graph() {
        assert!(check_routing_reachability(&valid_two_zone_graph()).is_empty());
    }

    #[test]
    fn routing_reachability_passes_for_single_node() {
        let mut g = TopologyGraph::new("single");
        let zone = SecurityZone::new("LAN", ZoneRole::Lan, 80);
        let zone_id = zone.id;
        g.add_zone(zone);
        let mut rtr = NetworkNode::new("RTR-01", NodeRole::Router, Vendor::Cisco, zone_id);
        rtr.interfaces.push(Interface::new("GigabitEthernet0/0", InterfaceRole::Access));
        g.add_node(rtr);
        assert!(check_routing_reachability(&g).is_empty());
    }

    #[test]
    fn routing_reachability_fails_for_isolated_node() {
        let mut g = valid_two_zone_graph();
        let zone_id = g.zones[0].id;
        let mut isolated = NetworkNode::new("RTR-ISOLATED", NodeRole::Router, Vendor::Cisco, zone_id);
        isolated.interfaces.push(Interface::new("GigabitEthernet0/0", InterfaceRole::Access));
        g.add_node(isolated);

        let failures = check_routing_reachability(&g);
        assert!(
            failures.iter().any(|f| f.component.as_deref() == Some("RTR-ISOLATED")),
            "got: {failures:?}"
        );
    }

    // ── Zone policy completeness ──────────────────────────────────────────────

    #[test]
    fn zone_policy_completeness_passes_for_full_matrix() {
        assert!(check_zone_policy_completeness(&valid_two_zone_graph()).is_empty());
    }

    #[test]
    fn zone_policy_completeness_fails_for_missing_direction() {
        let mut g = TopologyGraph::new("incomplete");
        let lan = SecurityZone::new("LAN", ZoneRole::Lan, 80);
        let mgmt = SecurityZone::new("MGMT", ZoneRole::Mgmt, 90);
        let lan_id = lan.id;
        let mgmt_id = mgmt.id;
        g.add_zone(lan);
        g.add_zone(mgmt);
        // Only LAN→MGMT; MGMT→LAN is missing
        g.add_policy(ZonePolicy::new(lan_id, mgmt_id, PolicyAction::Permit));

        let failures = check_zone_policy_completeness(&g);
        assert!(
            failures.iter().any(|f| f.component.as_deref() == Some("MGMT \u{2192} LAN")),
            "got: {failures:?}"
        );
    }

    #[test]
    fn zone_policy_single_zone_needs_no_policy() {
        let mut g = TopologyGraph::new("single-zone");
        g.add_zone(SecurityZone::new("LAN", ZoneRole::Lan, 80));
        assert!(check_zone_policy_completeness(&g).is_empty());
    }

    // ── Addressing conflicts ──────────────────────────────────────────────────

    #[test]
    fn addressing_passes_for_unique_addresses() {
        assert!(check_addressing_conflicts(&valid_two_zone_graph()).is_empty());
    }

    #[test]
    fn addressing_fails_for_duplicate_ipv4() {
        let mut g = valid_two_zone_graph();
        let zone_id = g.zones[0].id;
        let mut rtr2 = NetworkNode::new("RTR-DUP", NodeRole::Router, Vendor::Cisco, zone_id);
        let mut iface = Interface::new("GigabitEthernet0/0", InterfaceRole::Access);
        iface.ipv4_addr = Some("10.10.10.1/24".into()); // already used
        rtr2.interfaces.push(iface);
        g.add_node(rtr2);

        let failures = check_addressing_conflicts(&g);
        assert!(
            failures.iter().any(|f| f.message.contains("10.10.10.1")),
            "got: {failures:?}"
        );
    }

    #[test]
    fn addressing_fails_for_overlapping_subnets() {
        let mut g = TopologyGraph::new("overlap");
        let z1 = SecurityZone::new("Z1", ZoneRole::Lan, 80);
        let z2 = SecurityZone::new("Z2", ZoneRole::Lan, 80);
        let z1_id = z1.id;
        let z2_id = z2.id;
        g.add_zone(z1);
        g.add_zone(z2);
        g.addressing.ipv4_allocations.push(Ipv4Allocation {
            zone_id: z1_id,
            subnet: "10.0.0.0/8".into(),
            purpose: "Z1".into(),
        });
        g.addressing.ipv4_allocations.push(Ipv4Allocation {
            zone_id: z2_id,
            subnet: "10.0.1.0/24".into(), // inside 10.0.0.0/8
            purpose: "Z2".into(),
        });

        let failures = check_addressing_conflicts(&g);
        assert!(
            failures.iter().any(|f| f.message.contains("overlap")),
            "got: {failures:?}"
        );
    }

    // ── Capability contracts ──────────────────────────────────────────────────

    #[test]
    fn capability_contracts_passes_for_cisco() {
        assert!(check_capability_contracts(&valid_two_zone_graph(), &base_intent()).is_empty());
    }

    #[test]
    fn capability_contracts_fails_for_unknown_vendor() {
        let mut g = valid_two_zone_graph();
        let zone_id = g.zones[0].id;
        let mut jnx = NetworkNode::new("RTR-JNX", NodeRole::Router, Vendor::Juniper, zone_id);
        jnx.interfaces.push(Interface::new("ge-0/0/0", InterfaceRole::Access));
        g.add_node(jnx);

        let failures = check_capability_contracts(&g, &base_intent());
        assert!(
            failures.iter().any(|f| f.check == "capability_contracts"),
            "got: {failures:?}"
        );
    }

    // ── All errors collected ──────────────────────────────────────────────────

    #[test]
    fn verify_collects_failures_from_multiple_checks() {
        let mut g = TopologyGraph::new("multi-fail");
        g.add_zone(SecurityZone::new("LAN", ZoneRole::Lan, 80));
        g.add_zone(SecurityZone::new("MGMT", ZoneRole::Mgmt, 90));
        // No policies, no nodes, no links — zone_policy_completeness fails

        let failures = engine().verify(&g, &base_intent()).unwrap_err();
        let checks: Vec<&str> = failures.iter().map(|f| f.check).collect();
        assert!(checks.contains(&"zone_policy_completeness"), "{checks:?}");
        assert!(failures.len() >= 2, "expected multiple failures: {failures:?}");
    }

    #[test]
    fn failure_display_is_readable() {
        let f = VerificationFailure {
            check: "interface_consistency",
            component: Some("RTR-CORE-01".into()),
            message: "Node has no interfaces".into(),
            resolution: vec![],
        };
        let s = f.to_string();
        assert!(s.contains("interface_consistency"));
        assert!(s.contains("RTR-CORE-01"));
        assert!(s.contains("no interfaces"));
    }
}
