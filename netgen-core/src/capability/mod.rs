use uuid::Uuid;

use crate::graph::{NetworkNode, NodeRole, TopologyGraph};
use crate::schema::{NetworkIntent, RoutingProtocol, Vendor};

// ─── Capability taxonomy ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityCategory {
    Routing,
    Security,
    Segmentation,
    Wan,
    Services,
    HighAvailability,
}

/// Capability support level — ordered from weakest to strongest.
/// A vendor that meets `Advanced` also satisfies any requirement at `Standard` or `Basic`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CapabilityLevel {
    Basic,
    Standard,
    Advanced,
    Enterprise,
}

/// A single vendor capability: what it supports and at what depth.
#[derive(Debug, Clone)]
pub struct Capability {
    /// Canonical feature identifier, e.g. `"ospf"`, `"nat"`, `"vrf"`.
    pub id: &'static str,
    pub category: CapabilityCategory,
    pub level: CapabilityLevel,
    /// Known constraints or caveats for this capability on this platform.
    pub limitations: Vec<&'static str>,
}

impl Capability {
    fn new(
        id: &'static str,
        category: CapabilityCategory,
        level: CapabilityLevel,
    ) -> Self {
        Self { id, category, level, limitations: vec![] }
    }

    fn with_limitations(mut self, lims: &[&'static str]) -> Self {
        self.limitations = lims.to_vec();
        self
    }
}

// ─── Vendor profile ───────────────────────────────────────────────────────────

/// Complete capability map for a specific vendor platform and version.
#[derive(Debug, Clone)]
pub struct VendorProfile {
    pub vendor: Vendor,
    /// Platform family, e.g. `"ios_xe"`, `"junos"`.
    pub family: &'static str,
    /// Software version line, e.g. `"17.x"`.
    pub version: &'static str,
    pub capabilities: Vec<Capability>,
}

impl VendorProfile {
    /// Returns the support level for the given capability id, or `None` if unsupported.
    pub fn capability_level(&self, id: &str) -> Option<CapabilityLevel> {
        self.capabilities.iter().find(|c| c.id == id).map(|c| c.level)
    }

    /// Returns `true` if the profile supports `id` at least at `min_level`.
    pub fn supports(&self, id: &str, min_level: CapabilityLevel) -> bool {
        self.capability_level(id).is_some_and(|l| l >= min_level)
    }
}

// ─── Capability contract ──────────────────────────────────────────────────────

/// A capability requirement produced by the design engine.
/// Satisfied contracts are returned from `resolve()` on success.
#[derive(Debug, Clone)]
pub struct CapabilityContract {
    /// Feature identifier, e.g. `"ospf"`.
    pub feature: String,
    /// Whether this gap blocks generation (`true`) or is advisory (`false`).
    pub required: bool,
    /// Minimum acceptable support level.
    pub min_level: CapabilityLevel,
    /// Human-readable explanation of why this feature is needed.
    pub reason: String,
    /// Actual level found in the vendor profile (populated on success).
    pub satisfied_by: CapabilityLevel,
}

// ─── Errors ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum CapabilityError {
    /// A required feature is absent or below the required level on a specific node.
    CapabilityGap {
        node_id: Uuid,
        node_label: String,
        vendor: Vendor,
        missing: String,
        required_level: CapabilityLevel,
        /// `None` means the feature is entirely absent from the vendor profile.
        found_level: Option<CapabilityLevel>,
        reason: String,
    },
    /// No registered profile found for a vendor used in the graph.
    UnknownVendor {
        node_id: Uuid,
        node_label: String,
        vendor: Vendor,
    },
}

impl std::fmt::Display for CapabilityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CapabilityError::CapabilityGap {
                node_label,
                vendor,
                missing,
                required_level,
                found_level,
                ..
            } => {
                let found = found_level
                    .map(|l| format!("{l:?}"))
                    .unwrap_or_else(|| "absent".into());
                write!(
                    f,
                    "Node '{node_label}' ({vendor:?}): capability '{missing}' requires \
                     {required_level:?} but found {found}"
                )
            }
            CapabilityError::UnknownVendor { node_label, vendor, .. } => {
                write!(f, "Node '{node_label}': no profile registered for vendor {vendor:?}")
            }
        }
    }
}

impl std::error::Error for CapabilityError {}

// ─── Resolution engine ────────────────────────────────────────────────────────

/// Pipeline stage that runs after the security engine and before config generation.
///
/// For every node in the graph, derives the capabilities required by the network design
/// and checks them against the node's vendor profile. Returns all gaps at once so the
/// user can fix everything in one round.
pub struct CapabilityResolutionEngine {
    profiles: Vec<VendorProfile>,
}

impl Default for CapabilityResolutionEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl CapabilityResolutionEngine {
    /// Creates an engine with all built-in vendor profiles loaded.
    pub fn new() -> Self {
        Self {
            profiles: vec![cisco_ios_xe()],
        }
    }

    /// Resolve capability requirements for the fully-designed graph.
    ///
    /// Returns `Ok(contracts)` — all satisfied requirements — if no gaps exist.
    /// Returns `Err(errors)` — every gap found — so the user can fix all at once.
    pub fn resolve(
        &self,
        graph: &TopologyGraph,
        intent: &NetworkIntent,
    ) -> Result<Vec<CapabilityContract>, Vec<CapabilityError>> {
        let mut errors: Vec<CapabilityError> = Vec::new();
        let mut satisfied: Vec<CapabilityContract> = Vec::new();

        for node in &graph.nodes {
            let profile = match self.profile_for_vendor(&node.vendor) {
                Some(p) => p,
                None => {
                    errors.push(CapabilityError::UnknownVendor {
                        node_id: node.id,
                        node_label: node.label.clone(),
                        vendor: node.vendor.clone(),
                    });
                    continue;
                }
            };

            for contract in required_contracts(node, intent) {
                match profile.capability_level(&contract.feature) {
                    Some(found) if found >= contract.min_level => {
                        satisfied.push(CapabilityContract {
                            satisfied_by: found,
                            ..contract
                        });
                    }
                    found => {
                        if contract.required {
                            errors.push(CapabilityError::CapabilityGap {
                                node_id: node.id,
                                node_label: node.label.clone(),
                                vendor: node.vendor.clone(),
                                missing: contract.feature.clone(),
                                required_level: contract.min_level,
                                found_level: found,
                                reason: contract.reason.clone(),
                            });
                        }
                    }
                }
            }
        }

        if errors.is_empty() {
            Ok(satisfied)
        } else {
            Err(errors)
        }
    }

    fn profile_for_vendor(&self, vendor: &Vendor) -> Option<&VendorProfile> {
        self.profiles.iter().find(|p| &p.vendor == vendor)
    }
}

// ─── Contract derivation ──────────────────────────────────────────────────────

fn required_contracts(node: &NetworkNode, intent: &NetworkIntent) -> Vec<CapabilityContract> {
    let mut contracts: Vec<CapabilityContract> = Vec::new();

    match node.role {
        NodeRole::Router => {
            // Routing protocol requirements
            for proto in &intent.routing.protocols {
                match proto {
                    RoutingProtocol::Ospf => contracts.push(CapabilityContract {
                        feature: "ospf".into(),
                        required: true,
                        min_level: CapabilityLevel::Standard,
                        reason: "OSPF routing protocol selected in intent".into(),
                        satisfied_by: CapabilityLevel::Basic, // placeholder; overwritten on success
                    }),
                    RoutingProtocol::Bgp => contracts.push(CapabilityContract {
                        feature: "bgp".into(),
                        required: true,
                        min_level: CapabilityLevel::Standard,
                        reason: "BGP routing protocol selected in intent".into(),
                        satisfied_by: CapabilityLevel::Basic,
                    }),
                    RoutingProtocol::Static => {}  // universal; no capability check needed
                    RoutingProtocol::IsIs => contracts.push(CapabilityContract {
                        feature: "isis".into(),
                        required: true,
                        min_level: CapabilityLevel::Standard,
                        reason: "IS-IS routing protocol selected in intent".into(),
                        satisfied_by: CapabilityLevel::Basic,
                    }),
                }
            }

            if intent.services.nat {
                contracts.push(CapabilityContract {
                    feature: "nat".into(),
                    required: true,
                    min_level: CapabilityLevel::Standard,
                    reason: "NAT service enabled in intent".into(),
                    satisfied_by: CapabilityLevel::Basic,
                });
            }

            if intent.services.vpn {
                contracts.push(CapabilityContract {
                    feature: "ipsec".into(),
                    required: true,
                    min_level: CapabilityLevel::Standard,
                    reason: "VPN service enabled in intent".into(),
                    satisfied_by: CapabilityLevel::Basic,
                });
            }
        }

        NodeRole::Firewall => {
            contracts.push(CapabilityContract {
                feature: "acl".into(),
                required: true,
                min_level: CapabilityLevel::Standard,
                reason: "Firewall node role requires ACL capability".into(),
                satisfied_by: CapabilityLevel::Basic,
            });
        }

        NodeRole::Switch => {
            contracts.push(CapabilityContract {
                feature: "vlan".into(),
                required: true,
                min_level: CapabilityLevel::Basic,
                reason: "Switch node role requires VLAN capability".into(),
                satisfied_by: CapabilityLevel::Basic,
            });
        }

        NodeRole::Host | NodeRole::Cloud | NodeRole::Ap => {}
    }

    contracts
}

// ─── Cisco IOS-XE profile ─────────────────────────────────────────────────────

/// Full Cisco IOS-XE 17.x capability profile.
pub fn cisco_ios_xe() -> VendorProfile {
    VendorProfile {
        vendor: Vendor::Cisco,
        family: "ios_xe",
        version: "17.x",
        capabilities: vec![
            // Routing
            Capability::new("ospf",               CapabilityCategory::Routing,         CapabilityLevel::Enterprise),
            Capability::new("bgp",                CapabilityCategory::Routing,         CapabilityLevel::Enterprise),
            Capability::new("static",             CapabilityCategory::Routing,         CapabilityLevel::Basic),
            Capability::new("isis",               CapabilityCategory::Routing,         CapabilityLevel::Advanced),
            Capability::new("route_summarization", CapabilityCategory::Routing,        CapabilityLevel::Advanced),
            Capability::new("ecmp",               CapabilityCategory::Routing,         CapabilityLevel::Advanced),
            // Security
            Capability::new("acl",                CapabilityCategory::Security,        CapabilityLevel::Advanced),
            Capability::new("ssh",                CapabilityCategory::Security,        CapabilityLevel::Standard),
            Capability::new("zone_firewall",      CapabilityCategory::Security,        CapabilityLevel::Basic)
                .with_limitations(&["limited stateful inspection compared to dedicated FW"]),
            // Segmentation
            Capability::new("vlan",               CapabilityCategory::Segmentation,    CapabilityLevel::Standard),
            Capability::new("vrf",                CapabilityCategory::Segmentation,    CapabilityLevel::Advanced),
            Capability::new("private_vlan",       CapabilityCategory::Segmentation,    CapabilityLevel::Standard),
            Capability::new("vxlan",              CapabilityCategory::Segmentation,    CapabilityLevel::Standard)
                .with_limitations(&["requires EVPN control plane on supported platforms"]),
            // WAN
            Capability::new("ipsec",              CapabilityCategory::Wan,             CapabilityLevel::Standard),
            Capability::new("gre",                CapabilityCategory::Wan,             CapabilityLevel::Standard),
            Capability::new("dmvpn",              CapabilityCategory::Wan,             CapabilityLevel::Advanced),
            // Services
            Capability::new("nat",                CapabilityCategory::Services,        CapabilityLevel::Advanced),
            Capability::new("dhcp",               CapabilityCategory::Services,        CapabilityLevel::Standard),
            Capability::new("dns",                CapabilityCategory::Services,        CapabilityLevel::Basic),
            // High Availability
            Capability::new("hsrp",               CapabilityCategory::HighAvailability, CapabilityLevel::Advanced),
            Capability::new("vrrp",               CapabilityCategory::HighAvailability, CapabilityLevel::Standard),
            Capability::new("glbp",               CapabilityCategory::HighAvailability, CapabilityLevel::Advanced),
            Capability::new("link_redundancy",    CapabilityCategory::HighAvailability, CapabilityLevel::Standard),
        ],
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{NetworkNode, SecurityZone, TopologyGraph};
    use crate::schema::{
        AddressingIntent, AddressingMode, AddressingScheme, ConstraintsIntent, ContextIntent,
        Environment, IntentMetadata, Ipv4Config, NetworkIntent, RedundancyLevel, RoutingIntent,
        SecurityLevel, SegmentationIntent, ServicesIntent, TopologyIntent, TopologyType, Vendor,
        ZoneIntent, ZoneRole, CURRENT_SCHEMA_VERSION,
    };
    use uuid::Uuid;

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn engine() -> CapabilityResolutionEngine {
        CapabilityResolutionEngine::new()
    }

    fn base_intent() -> NetworkIntent {
        NetworkIntent {
            metadata: IntentMetadata {
                name: "cap-test".into(),
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

    fn cisco_router(zone_id: Uuid) -> NetworkNode {
        NetworkNode::new("RTR-01", NodeRole::Router, Vendor::Cisco, zone_id)
    }

    fn cisco_switch(zone_id: Uuid) -> NetworkNode {
        NetworkNode::new("SW-01", NodeRole::Switch, Vendor::Cisco, zone_id)
    }

    fn cisco_firewall(zone_id: Uuid) -> NetworkNode {
        NetworkNode::new("FW-01", NodeRole::Firewall, Vendor::Cisco, zone_id)
    }

    fn minimal_graph_with_nodes(nodes: Vec<NetworkNode>) -> TopologyGraph {
        let mut g = TopologyGraph::new("cap-test");
        let z = SecurityZone::new("LAN", ZoneRole::Lan, 80);
        g.add_zone(z);
        for n in nodes {
            g.add_node(n);
        }
        g
    }

    // ── Profile ───────────────────────────────────────────────────────────────

    #[test]
    fn cisco_ios_xe_profile_has_expected_capabilities() {
        let profile = cisco_ios_xe();
        assert_eq!(profile.vendor, Vendor::Cisco);
        assert_eq!(profile.family, "ios_xe");
        assert_eq!(profile.version, "17.x");

        let cap_ids: Vec<&str> = profile.capabilities.iter().map(|c| c.id).collect();
        for expected in &["ospf", "bgp", "nat", "acl", "dmvpn", "vrf", "hsrp", "vlan", "ipsec"] {
            assert!(cap_ids.contains(expected), "missing capability: {expected}");
        }
    }

    #[test]
    fn cisco_ios_xe_ospf_is_enterprise() {
        let profile = cisco_ios_xe();
        assert_eq!(profile.capability_level("ospf"), Some(CapabilityLevel::Enterprise));
    }

    #[test]
    fn cisco_ios_xe_bgp_is_enterprise() {
        let profile = cisco_ios_xe();
        assert_eq!(profile.capability_level("bgp"), Some(CapabilityLevel::Enterprise));
    }

    #[test]
    fn cisco_ios_xe_nat_is_advanced() {
        let profile = cisco_ios_xe();
        assert_eq!(profile.capability_level("nat"), Some(CapabilityLevel::Advanced));
    }

    #[test]
    fn profile_supports_lower_level_requirements() {
        // Enterprise ≥ Standard, so a Standard requirement is satisfied by Enterprise
        let profile = cisco_ios_xe();
        assert!(profile.supports("ospf", CapabilityLevel::Standard));
        assert!(profile.supports("ospf", CapabilityLevel::Basic));
        assert!(profile.supports("ospf", CapabilityLevel::Enterprise));
    }

    #[test]
    fn profile_missing_capability_returns_none() {
        let profile = cisco_ios_xe();
        assert_eq!(profile.capability_level("nonexistent_feature"), None);
        assert!(!profile.supports("nonexistent_feature", CapabilityLevel::Basic));
    }

    // ── Capability level ordering ─────────────────────────────────────────────

    #[test]
    fn capability_level_ordering() {
        assert!(CapabilityLevel::Basic < CapabilityLevel::Standard);
        assert!(CapabilityLevel::Standard < CapabilityLevel::Advanced);
        assert!(CapabilityLevel::Advanced < CapabilityLevel::Enterprise);
        assert!(CapabilityLevel::Enterprise >= CapabilityLevel::Enterprise);
        assert_eq!(CapabilityLevel::Standard, CapabilityLevel::Standard);
    }

    // ── Resolution — success cases ────────────────────────────────────────────

    #[test]
    fn empty_graph_resolves_without_errors() {
        let g = TopologyGraph::new("empty");
        let intent = base_intent();
        let result = engine().resolve(&g, &intent);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn static_routing_on_cisco_router_succeeds() {
        let z_id = Uuid::new_v4();
        let g = minimal_graph_with_nodes(vec![cisco_router(z_id)]);
        // base_intent uses Static routing — no capability contract generated
        let result = engine().resolve(&g, &base_intent());
        assert!(result.is_ok());
    }

    #[test]
    fn ospf_on_cisco_router_succeeds() {
        let z_id = Uuid::new_v4();
        let g = minimal_graph_with_nodes(vec![cisco_router(z_id)]);
        let mut intent = base_intent();
        intent.routing.protocols = vec![RoutingProtocol::Ospf];
        let result = engine().resolve(&g, &intent);
        assert!(result.is_ok(), "OSPF on Cisco should resolve: {:?}", result);
    }

    #[test]
    fn bgp_on_cisco_router_succeeds() {
        let z_id = Uuid::new_v4();
        let g = minimal_graph_with_nodes(vec![cisco_router(z_id)]);
        let mut intent = base_intent();
        intent.routing.protocols = vec![RoutingProtocol::Bgp];
        let result = engine().resolve(&g, &intent);
        assert!(result.is_ok());
    }

    #[test]
    fn nat_on_cisco_router_succeeds() {
        let z_id = Uuid::new_v4();
        let g = minimal_graph_with_nodes(vec![cisco_router(z_id)]);
        let mut intent = base_intent();
        intent.services.nat = true;
        let result = engine().resolve(&g, &intent);
        assert!(result.is_ok());
    }

    #[test]
    fn vpn_on_cisco_router_succeeds() {
        let z_id = Uuid::new_v4();
        let g = minimal_graph_with_nodes(vec![cisco_router(z_id)]);
        let mut intent = base_intent();
        intent.services.vpn = true;
        let result = engine().resolve(&g, &intent);
        assert!(result.is_ok());
    }

    #[test]
    fn cisco_firewall_acl_capability_succeeds() {
        let z_id = Uuid::new_v4();
        let g = minimal_graph_with_nodes(vec![cisco_firewall(z_id)]);
        let result = engine().resolve(&g, &base_intent());
        assert!(result.is_ok());
    }

    #[test]
    fn cisco_switch_vlan_capability_succeeds() {
        let z_id = Uuid::new_v4();
        let g = minimal_graph_with_nodes(vec![cisco_switch(z_id)]);
        let result = engine().resolve(&g, &base_intent());
        assert!(result.is_ok());
    }

    #[test]
    fn satisfied_contracts_list_is_populated() {
        let z_id = Uuid::new_v4();
        let g = minimal_graph_with_nodes(vec![cisco_firewall(z_id)]);
        let contracts = engine().resolve(&g, &base_intent()).unwrap();
        assert!(
            contracts.iter().any(|c| c.feature == "acl"),
            "acl contract must appear in satisfied list"
        );
    }

    // ── Resolution — failure cases ────────────────────────────────────────────

    #[test]
    fn unknown_vendor_returns_error() {
        let z_id = Uuid::new_v4();
        // Juniper is not registered in the engine
        let node = NetworkNode::new("RTR-01", NodeRole::Router, Vendor::Juniper, z_id);
        let g = minimal_graph_with_nodes(vec![node]);
        let result = engine().resolve(&g, &base_intent());
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| matches!(e, CapabilityError::UnknownVendor { .. })));
    }

    #[test]
    fn missing_capability_returns_gap_error() {
        // Build a minimal profile that does NOT have ipsec
        let minimal_profile = VendorProfile {
            vendor: Vendor::Cisco,
            family: "test_platform",
            version: "1.0",
            capabilities: vec![
                Capability::new("static", CapabilityCategory::Routing, CapabilityLevel::Basic),
            ],
        };
        let engine = CapabilityResolutionEngine { profiles: vec![minimal_profile] };

        let z_id = Uuid::new_v4();
        let g = minimal_graph_with_nodes(vec![cisco_router(z_id)]);
        let mut intent = base_intent();
        intent.services.vpn = true; // requires ipsec

        let result = engine.resolve(&g, &intent);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| matches!(
            e,
            CapabilityError::CapabilityGap { missing, .. } if missing == "ipsec"
        )));
    }

    #[test]
    fn capability_below_required_level_returns_gap_error() {
        // Profile has ipsec at Basic, but Standard is required
        let profile = VendorProfile {
            vendor: Vendor::Cisco,
            family: "test_platform",
            version: "1.0",
            capabilities: vec![
                Capability::new("static", CapabilityCategory::Routing, CapabilityLevel::Basic),
                Capability::new("ipsec",  CapabilityCategory::Wan,     CapabilityLevel::Basic),
            ],
        };
        let engine = CapabilityResolutionEngine { profiles: vec![profile] };

        let z_id = Uuid::new_v4();
        let g = minimal_graph_with_nodes(vec![cisco_router(z_id)]);
        let mut intent = base_intent();
        intent.services.vpn = true;

        let result = engine.resolve(&g, &intent);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        let gap = errors.iter().find(|e| matches!(
            e,
            CapabilityError::CapabilityGap { missing, .. } if missing == "ipsec"
        ));
        assert!(gap.is_some(), "expected ipsec gap error");
        if let Some(CapabilityError::CapabilityGap { found_level, required_level, .. }) = gap {
            assert_eq!(*found_level, Some(CapabilityLevel::Basic));
            assert_eq!(*required_level, CapabilityLevel::Standard);
        }
    }

    #[test]
    fn all_errors_reported_at_once() {
        // Profile missing both ospf and bgp
        let profile = VendorProfile {
            vendor: Vendor::Cisco,
            family: "test_platform",
            version: "1.0",
            capabilities: vec![
                Capability::new("static", CapabilityCategory::Routing, CapabilityLevel::Basic),
            ],
        };
        let engine = CapabilityResolutionEngine { profiles: vec![profile] };

        let z_id = Uuid::new_v4();
        let g = minimal_graph_with_nodes(vec![cisco_router(z_id)]);
        let mut intent = base_intent();
        intent.routing.protocols = vec![RoutingProtocol::Ospf, RoutingProtocol::Bgp];

        let errors = engine.resolve(&g, &intent).unwrap_err();
        let features: Vec<_> = errors
            .iter()
            .filter_map(|e| match e {
                CapabilityError::CapabilityGap { missing, .. } => Some(missing.as_str()),
                _ => None,
            })
            .collect();
        assert!(features.contains(&"ospf"), "expected ospf error");
        assert!(features.contains(&"bgp"), "expected bgp error");
    }

    #[test]
    fn gap_error_display_is_readable() {
        let err = CapabilityError::CapabilityGap {
            node_id: Uuid::new_v4(),
            node_label: "RTR-CORE-01".into(),
            vendor: Vendor::Cisco,
            missing: "dmvpn".into(),
            required_level: CapabilityLevel::Advanced,
            found_level: None,
            reason: "VPN enabled".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("RTR-CORE-01"));
        assert!(msg.contains("dmvpn"));
        assert!(msg.contains("Advanced"));
    }
}
