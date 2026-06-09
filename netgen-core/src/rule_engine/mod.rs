use uuid::Uuid;

use thiserror::Error;

use crate::graph::{
    DesignDecision, GraphChange, GraphVersionChain, Interface, InterfaceRole, LinkType,
    NetworkLink, NetworkNode, NodeRole, PolicyAction, SecurityZone, TopologyGraph, TriggerSource,
    ZonePolicy,
};
use crate::schema::{NetworkIntent, RoutingProtocol, Vendor, ZoneRole};

const MAX_ITERATIONS: usize = 50;

// ─── Phase and category ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RulePhase {
    Structural,
    Routing,
    Segmentation,
    Addressing,
    Service,
    Security,
    Optimization,
}

impl RulePhase {
    pub fn ordered() -> [RulePhase; 7] {
        [
            RulePhase::Structural,
            RulePhase::Routing,
            RulePhase::Segmentation,
            RulePhase::Addressing,
            RulePhase::Service,
            RulePhase::Security,
            RulePhase::Optimization,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleCategory {
    Mandatory,
    Constraint,
    Preference,
    Optimization,
}

// ─── Rule result ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleResult {
    Ok,
    Err(String),
}

// ─── EngineRule metadata ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct EngineRule {
    pub id: &'static str,
    pub name: &'static str,
    pub phase: RulePhase,
    pub priority: u16,
    pub category: RuleCategory,
}

// ─── Rule trait ───────────────────────────────────────────────────────────────

pub trait Rule: Send + Sync {
    fn metadata(&self) -> EngineRule;

    /// Is this rule applicable to the current intent + graph state?
    fn condition(&self, intent: &NetworkIntent, graph: &TopologyGraph) -> bool;

    /// Apply transformation — produces a new graph, never mutates the input.
    fn transform(&self, intent: &NetworkIntent, graph: &TopologyGraph) -> TopologyGraph;

    /// Prove the transformation produced a valid result.
    fn validate(&self, intent: &NetworkIntent, graph: &TopologyGraph) -> RuleResult;
}

// ─── Error ────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum RuleEngineError {
    #[error("Rule engine did not reach stable state after {MAX_ITERATIONS} iterations")]
    InfiniteLoop,
    #[error("Rule '{rule_id}' validation failed: {reason}")]
    RuleValidationFailed { rule_id: String, reason: String },
}

// ─── RuleEngine ───────────────────────────────────────────────────────────────

pub struct RuleEngine {
    rules: Vec<Box<dyn Rule + Send + Sync>>,
}

impl Default for RuleEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl RuleEngine {
    pub fn new() -> Self {
        Self {
            rules: vec![
                // Phase 1 — Structural (zones first, then devices, then links)
                Box::new(ZonesFromIntentRule),
                Box::new(MgmtZoneRequiredRule),
                Box::new(DmzRequiredRule),
                Box::new(DeviceInjectionRule),
                Box::new(LinkInjectionRule),
                // Phase 2 — Routing
                Box::new(RoutingProtocolSelectionRule),
                // Phase 3 — Segmentation
                Box::new(VlanAssignmentRule),
                Box::new(ZonePolicyDerivationRule),
                Box::new(GuestIsolationRule),
            ],
        }
    }

    /// 7-phase deterministic evaluation.
    /// Each phase runs its rules to stable state before the next phase begins.
    /// Returns the full version chain — every rule firing is a new graph version.
    pub fn evaluate(&self, intent: &NetworkIntent) -> Result<GraphVersionChain, RuleEngineError> {
        let initial = TopologyGraph::new(&intent.metadata.name);
        let mut chain = GraphVersionChain::new(initial, "engine:start");

        for phase in RulePhase::ordered() {
            let mut phase_rules: Vec<&dyn Rule> = self
                .rules
                .iter()
                .filter(|r| r.metadata().phase == phase)
                .map(|r| r.as_ref() as &dyn Rule)
                .collect();
            // Higher priority number fires first within a phase
            phase_rules.sort_by_key(|r| std::cmp::Reverse(r.metadata().priority));

            let mut iterations = 0usize;
            loop {
                let mut any_fired = false;

                for rule in &phase_rules {
                    let fires = {
                        let g = chain.latest_graph();
                        rule.condition(intent, g)
                    };

                    if fires {
                        let snapshot = chain.latest_graph().clone();
                        let new_graph = rule.transform(intent, &snapshot);
                        let result = rule.validate(intent, &new_graph);
                        match result {
                            RuleResult::Ok => {
                                let diff = compute_diff(&snapshot, &new_graph);
                                if !diff.is_empty() {
                                    chain.push(new_graph, rule.metadata().id, diff);
                                    any_fired = true;
                                }
                            }
                            RuleResult::Err(reason) => {
                                return Err(RuleEngineError::RuleValidationFailed {
                                    rule_id: rule.metadata().id.to_string(),
                                    reason,
                                });
                            }
                        }
                    }
                }

                if !any_fired {
                    break;
                }
                iterations += 1;
                if iterations >= MAX_ITERATIONS {
                    return Err(RuleEngineError::InfiniteLoop);
                }
            }
        }

        Ok(chain)
    }
}

// ─── Diff helper ──────────────────────────────────────────────────────────────

fn compute_diff(old: &TopologyGraph, new: &TopologyGraph) -> Vec<GraphChange> {
    let mut diff = Vec::new();

    for z in &new.zones {
        if old.zone(z.id).is_none() {
            diff.push(GraphChange::ZoneAdded(z.clone()));
        }
    }
    for z in &old.zones {
        if new.zone(z.id).is_none() {
            diff.push(GraphChange::ZoneRemoved { id: z.id });
        }
    }
    for n in &new.nodes {
        if old.node(n.id).is_none() {
            diff.push(GraphChange::NodeAdded(n.clone()));
        }
    }
    for n in &old.nodes {
        if new.node(n.id).is_none() {
            diff.push(GraphChange::NodeRemoved { id: n.id });
        }
    }
    for e in &new.edges {
        if old.link(e.id).is_none() {
            diff.push(GraphChange::EdgeAdded(e.clone()));
        }
    }
    for e in &old.edges {
        if new.link(e.id).is_none() {
            diff.push(GraphChange::EdgeRemoved { id: e.id });
        }
    }
    for p in &new.policies {
        if old.policy_between(p.from_zone, p.to_zone).is_none() {
            diff.push(GraphChange::PolicyAdded(p.clone()));
        }
    }
    for p in &old.policies {
        if new.policy_between(p.from_zone, p.to_zone).is_none() {
            diff.push(GraphChange::PolicyRemoved {
                from_zone: p.from_zone,
                to_zone: p.to_zone,
            });
        }
    }
    for d in &new.decisions {
        if old.decisions.iter().all(|od| od.id != d.id) {
            diff.push(GraphChange::DecisionLogged(d.clone()));
        }
    }

    diff
}

// ─── Shared helpers ───────────────────────────────────────────────────────────

pub fn has_fired(rule_id: &str, graph: &TopologyGraph) -> bool {
    graph
        .decisions
        .iter()
        .any(|d| d.rule_id.as_deref() == Some(rule_id))
}

fn preferred_vendor(intent: &NetworkIntent) -> Vendor {
    intent
        .metadata
        .vendor_preference
        .first()
        .cloned()
        .unwrap_or(Vendor::Cisco)
}

fn derive_action(from_trust: u8, to_trust: u8) -> PolicyAction {
    if from_trust < 30 {
        PolicyAction::Deny
    } else if (from_trust as u16 + to_trust as u16) >= 150 {
        PolicyAction::Permit
    } else {
        PolicyAction::Inspect
    }
}

// ─── Phase 1: Structural ──────────────────────────────────────────────────────
// Zone creation precedes device injection; link injection follows devices.
// All run in Phase 1 to preserve dependency ordering (devices need zones).

struct ZonesFromIntentRule;

impl Rule for ZonesFromIntentRule {
    fn metadata(&self) -> EngineRule {
        EngineRule {
            id: "ZONES_FROM_INTENT",
            name: "Create security zones from intent segmentation",
            phase: RulePhase::Structural,
            priority: 100,
            category: RuleCategory::Mandatory,
        }
    }

    fn condition(&self, intent: &NetworkIntent, graph: &TopologyGraph) -> bool {
        graph.zones.is_empty() && !intent.segmentation.zones.is_empty()
    }

    fn transform(&self, intent: &NetworkIntent, graph: &TopologyGraph) -> TopologyGraph {
        let mut g = graph.clone();
        for zi in &intent.segmentation.zones {
            let zone = SecurityZone::new(zi.name.clone(), zi.role.clone(), zi.trust_level);
            g.log_decision(
                DesignDecision::new(
                    format!("Zone '{}'", zi.name),
                    format!(
                        "Created from user intent: role={:?} trust_level={}",
                        zi.role, zi.trust_level
                    ),
                    TriggerSource::UserIntent,
                    true,
                )
                .with_rule("ZONES_FROM_INTENT"),
            );
            g.add_zone(zone);
        }
        g
    }

    fn validate(&self, _: &NetworkIntent, graph: &TopologyGraph) -> RuleResult {
        if graph.zones.is_empty() {
            RuleResult::Err("ZONES_FROM_INTENT must produce at least one zone".into())
        } else {
            RuleResult::Ok
        }
    }
}

struct MgmtZoneRequiredRule;

impl Rule for MgmtZoneRequiredRule {
    fn metadata(&self) -> EngineRule {
        EngineRule {
            id: "MGMT_ZONE_REQUIRED",
            name: "Inject MGMT zone if not declared in intent",
            phase: RulePhase::Structural,
            priority: 90,
            category: RuleCategory::Mandatory,
        }
    }

    fn condition(&self, _: &NetworkIntent, graph: &TopologyGraph) -> bool {
        !graph.zones.is_empty() && graph.zone_by_role(&ZoneRole::Mgmt).is_none()
    }

    fn transform(&self, _: &NetworkIntent, graph: &TopologyGraph) -> TopologyGraph {
        let mut g = graph.clone();
        let zone = SecurityZone::new("MGMT", ZoneRole::Mgmt, 100);
        g.log_decision(
            DesignDecision::new(
                "MGMT zone",
                "Management plane isolation requires a dedicated MGMT zone. Injected automatically.",
                TriggerSource::RuleEngine,
                false,
            )
            .with_rule("MGMT_ZONE_REQUIRED"),
        );
        g.add_zone(zone);
        g
    }

    fn validate(&self, _: &NetworkIntent, graph: &TopologyGraph) -> RuleResult {
        if graph.zone_by_role(&ZoneRole::Mgmt).is_some() {
            RuleResult::Ok
        } else {
            RuleResult::Err("MGMT zone must be present after MGMT_ZONE_REQUIRED".into())
        }
    }
}

struct DmzRequiredRule;

impl Rule for DmzRequiredRule {
    fn metadata(&self) -> EngineRule {
        EngineRule {
            id: "DMZ_REQUIRED_FOR_INTERNET",
            name: "Inject DMZ zone for internet-facing networks",
            phase: RulePhase::Structural,
            priority: 85,
            category: RuleCategory::Mandatory,
        }
    }

    fn condition(&self, intent: &NetworkIntent, graph: &TopologyGraph) -> bool {
        let needs_dmz = intent.services.internet_gateway || intent.segmentation.dmz;
        needs_dmz && !graph.zones.is_empty() && graph.zone_by_role(&ZoneRole::Dmz).is_none()
    }

    fn transform(&self, intent: &NetworkIntent, graph: &TopologyGraph) -> TopologyGraph {
        let mut g = graph.clone();
        let zone = SecurityZone::new("DMZ", ZoneRole::Dmz, 60);
        let reason = if intent.services.internet_gateway {
            "internet_gateway=true requires a DMZ for traffic isolation. Injected automatically."
        } else {
            "segmentation.dmz=true requested. DMZ zone injected."
        };
        g.log_decision(
            DesignDecision::new("DMZ zone", reason, TriggerSource::RuleEngine, false)
                .with_rule("DMZ_REQUIRED_FOR_INTERNET"),
        );
        g.add_zone(zone);
        g
    }

    fn validate(&self, _: &NetworkIntent, graph: &TopologyGraph) -> RuleResult {
        if graph.zone_by_role(&ZoneRole::Dmz).is_some() {
            RuleResult::Ok
        } else {
            RuleResult::Err("DMZ zone must be present after DMZ_REQUIRED_FOR_INTERNET".into())
        }
    }
}

struct DeviceInjectionRule;

impl Rule for DeviceInjectionRule {
    fn metadata(&self) -> EngineRule {
        EngineRule {
            id: "DEVICE_INJECTION",
            name: "Inject core router and per-zone access devices",
            phase: RulePhase::Structural,
            priority: 80,
            category: RuleCategory::Mandatory,
        }
    }

    fn condition(&self, _: &NetworkIntent, graph: &TopologyGraph) -> bool {
        !graph.zones.is_empty() && graph.nodes.is_empty()
    }

    fn transform(&self, intent: &NetworkIntent, graph: &TopologyGraph) -> TopologyGraph {
        let mut g = graph.clone();
        let vendor = preferred_vendor(intent);

        let core_zone_id = g
            .zone_by_role(&ZoneRole::Lan)
            .or_else(|| g.zones.first())
            .map(|z| z.id)
            .expect("zones must exist before device injection");

        let core =
            NetworkNode::new("RTR-CORE-01", NodeRole::Router, vendor.clone(), core_zone_id);
        g.log_decision(
            DesignDecision::new(
                "RTR-CORE-01",
                "Core router injected as hub for collapsed-core topology.",
                TriggerSource::RuleEngine,
                true,
            )
            .with_rule("DEVICE_INJECTION"),
        );
        g.add_node(core);

        let zone_snapshot: Vec<_> = g
            .zones
            .iter()
            .map(|z| (z.id, z.role.clone(), z.name.clone()))
            .collect();

        for (zone_id, role, name) in zone_snapshot {
            let name_up = name.to_uppercase();
            let (label, node_role) = match role {
                ZoneRole::Lan => (format!("SW-{name_up}-ACC-01"), NodeRole::Switch),
                ZoneRole::Dmz => (format!("FW-{name_up}-01"), NodeRole::Firewall),
                ZoneRole::Mgmt => ("SW-OOB-01".to_string(), NodeRole::Switch),
                ZoneRole::Guest => (format!("AP-{name_up}-01"), NodeRole::Ap),
                ZoneRole::Wan => ("RTR-WAN-01".to_string(), NodeRole::Router),
                ZoneRole::Transit | ZoneRole::Oob => {
                    (format!("RTR-{name_up}-01"), NodeRole::Router)
                }
            };

            let node = NetworkNode::new(&label, node_role, vendor.clone(), zone_id);
            g.log_decision(
                DesignDecision::new(
                    label.clone(),
                    format!("Access device injected for zone '{name}' (role={role:?})."),
                    TriggerSource::RuleEngine,
                    true,
                )
                .with_rule("DEVICE_INJECTION"),
            );
            g.add_node(node);
        }

        g
    }

    fn validate(&self, _: &NetworkIntent, graph: &TopologyGraph) -> RuleResult {
        if graph.nodes.iter().any(|n| n.label == "RTR-CORE-01") {
            RuleResult::Ok
        } else {
            RuleResult::Err("RTR-CORE-01 must be present after DEVICE_INJECTION".into())
        }
    }
}

struct LinkInjectionRule;

impl Rule for LinkInjectionRule {
    fn metadata(&self) -> EngineRule {
        EngineRule {
            id: "LINK_INJECTION",
            name: "Connect all devices to core router in star topology",
            phase: RulePhase::Structural,
            priority: 70,
            category: RuleCategory::Mandatory,
        }
    }

    fn condition(&self, _: &NetworkIntent, graph: &TopologyGraph) -> bool {
        !graph.nodes.is_empty() && graph.edges.is_empty()
    }

    fn transform(&self, _: &NetworkIntent, graph: &TopologyGraph) -> TopologyGraph {
        let mut g = graph.clone();

        let core_id = match g.nodes.iter().find(|n| n.label == "RTR-CORE-01") {
            Some(n) => n.id,
            None => return g,
        };

        let peers: Vec<(Uuid, String)> = g
            .nodes
            .iter()
            .filter(|n| n.id != core_id)
            .map(|n| (n.id, n.label.clone()))
            .collect();

        let mut core_ifaces: Vec<Interface> = Vec::new();
        let mut peer_ifaces: Vec<(Uuid, Interface)> = Vec::new();
        let mut links: Vec<(NetworkLink, String)> = Vec::new();

        for (idx, (peer_id, peer_label)) in peers.iter().enumerate() {
            let ci = Interface::new(format!("GigabitEthernet0/{idx}"), InterfaceRole::Uplink);
            let pi = Interface::new("GigabitEthernet0/0", InterfaceRole::Uplink);
            let link = NetworkLink::new(core_id, ci.id, *peer_id, pi.id, LinkType::Ethernet);

            core_ifaces.push(ci);
            peer_ifaces.push((*peer_id, pi));
            links.push((link, peer_label.clone()));
        }

        if let Some(core) = g.node_mut(core_id) {
            core.interfaces.extend(core_ifaces);
        }
        for (peer_id, iface) in peer_ifaces {
            if let Some(node) = g.node_mut(peer_id) {
                node.interfaces.push(iface);
            }
        }
        for (link, peer_label) in links {
            g.log_decision(
                DesignDecision::new(
                    format!("Link: RTR-CORE-01 ↔ {peer_label}"),
                    "Star topology: access device connected to core router.",
                    TriggerSource::RuleEngine,
                    true,
                )
                .with_rule("LINK_INJECTION"),
            );
            g.add_link(link);
        }

        g
    }

    fn validate(&self, _: &NetworkIntent, graph: &TopologyGraph) -> RuleResult {
        if graph.nodes.len() > 1 && graph.edges.is_empty() {
            RuleResult::Err(
                "no links produced by LINK_INJECTION despite multiple nodes".into(),
            )
        } else {
            RuleResult::Ok
        }
    }
}

// ─── Phase 2: Routing ─────────────────────────────────────────────────────────

struct RoutingProtocolSelectionRule;

impl Rule for RoutingProtocolSelectionRule {
    fn metadata(&self) -> EngineRule {
        EngineRule {
            id: "ROUTING_PROTOCOL_SELECTION",
            name: "Select and document routing protocol",
            phase: RulePhase::Routing,
            priority: 100,
            category: RuleCategory::Mandatory,
        }
    }

    fn condition(&self, _: &NetworkIntent, graph: &TopologyGraph) -> bool {
        !has_fired("ROUTING_PROTOCOL_SELECTION", graph)
    }

    fn transform(&self, intent: &NetworkIntent, graph: &TopologyGraph) -> TopologyGraph {
        let mut g = graph.clone();
        let protocols = &intent.routing.protocols;
        let sites = intent.topology.sites;

        let (selected, reason) = if protocols.contains(&RoutingProtocol::Ospf)
            && protocols.contains(&RoutingProtocol::Bgp)
        {
            ("OSPF + BGP", "Multi-domain: OSPF for internal routing, BGP for edge peering.")
        } else if protocols.contains(&RoutingProtocol::Bgp) {
            ("BGP", "BGP specified: inter-AS or ISP peering.")
        } else if protocols.contains(&RoutingProtocol::Ospf) {
            ("OSPF", "OSPF specified: single-AS interior routing.")
        } else if protocols.contains(&RoutingProtocol::Static) {
            ("Static", "Static routing specified in intent.")
        } else if sites <= 2 {
            ("Static", "Auto-selected: ≤2 sites, static routing is sufficient.")
        } else if sites <= 50 {
            ("OSPF", "Auto-selected: multi-site single-org, OSPF recommended.")
        } else {
            ("OSPF + BGP", "Auto-selected: large network (>50 sites).")
        };

        g.log_decision(
            DesignDecision::new(
                format!("Routing: {selected}"),
                reason,
                TriggerSource::RuleEngine,
                true,
            )
            .with_rule("ROUTING_PROTOCOL_SELECTION"),
        );

        g
    }

    fn validate(&self, _: &NetworkIntent, graph: &TopologyGraph) -> RuleResult {
        if has_fired("ROUTING_PROTOCOL_SELECTION", graph) {
            RuleResult::Ok
        } else {
            RuleResult::Err("routing protocol decision must be logged".into())
        }
    }
}

// ─── Phase 3: Segmentation ────────────────────────────────────────────────────

struct VlanAssignmentRule;

impl Rule for VlanAssignmentRule {
    fn metadata(&self) -> EngineRule {
        EngineRule {
            id: "VLAN_ASSIGNMENT",
            name: "Assign sequential VLANs grouped by zone role",
            phase: RulePhase::Segmentation,
            priority: 100,
            category: RuleCategory::Mandatory,
        }
    }

    fn condition(&self, _: &NetworkIntent, graph: &TopologyGraph) -> bool {
        !graph.zones.is_empty() && graph.zones.iter().any(|z| z.vlan.is_none())
    }

    fn transform(&self, _: &NetworkIntent, graph: &TopologyGraph) -> TopologyGraph {
        let mut g = graph.clone();
        let mut lan_next: u16 = 10;
        let mut dmz_next: u16 = 100;
        let mut mgmt_next: u16 = 200;
        let mut transit_next: u16 = 300;

        let zone_snapshot: Vec<_> = g
            .zones
            .iter()
            .map(|z| (z.id, z.role.clone(), z.name.clone(), z.vlan))
            .collect();

        for (id, role, name, existing_vlan) in zone_snapshot {
            if existing_vlan.is_some() {
                continue;
            }

            let (vlan, range_label) = match role {
                ZoneRole::Lan | ZoneRole::Guest => {
                    let v = lan_next;
                    lan_next += 1;
                    (v, "LAN/Guest 10–99")
                }
                ZoneRole::Dmz => {
                    let v = dmz_next;
                    dmz_next += 1;
                    (v, "DMZ 100–199")
                }
                ZoneRole::Mgmt | ZoneRole::Oob => {
                    let v = mgmt_next;
                    mgmt_next += 1;
                    (v, "MGMT 200–299")
                }
                ZoneRole::Transit | ZoneRole::Wan => {
                    let v = transit_next;
                    transit_next += 1;
                    (v, "Transit/WAN 300–399")
                }
            };

            if let Some(z) = g.zone_mut(id) {
                z.vlan = Some(vlan);
            }
            g.log_decision(
                DesignDecision::new(
                    format!("VLAN {vlan} → '{name}'"),
                    format!("Sequential VLAN from {range_label}."),
                    TriggerSource::RuleEngine,
                    true,
                )
                .with_rule("VLAN_ASSIGNMENT"),
            );
        }

        g
    }

    fn validate(&self, _: &NetworkIntent, graph: &TopologyGraph) -> RuleResult {
        if graph.zones.iter().all(|z| z.vlan.is_some()) {
            RuleResult::Ok
        } else {
            RuleResult::Err("all zones must have VLANs after VLAN_ASSIGNMENT".into())
        }
    }
}

struct ZonePolicyDerivationRule;

impl Rule for ZonePolicyDerivationRule {
    fn metadata(&self) -> EngineRule {
        EngineRule {
            id: "ZONE_POLICY_DERIVATION",
            name: "Derive directed zone policies from trust level deltas",
            phase: RulePhase::Segmentation,
            priority: 90,
            category: RuleCategory::Mandatory,
        }
    }

    fn condition(&self, _: &NetworkIntent, graph: &TopologyGraph) -> bool {
        !graph.zones.is_empty() && graph.policies.is_empty()
    }

    fn transform(&self, _: &NetworkIntent, graph: &TopologyGraph) -> TopologyGraph {
        let mut g = graph.clone();
        let zones: Vec<_> = g
            .zones
            .iter()
            .map(|z| (z.id, z.trust_level, z.name.clone()))
            .collect();

        let mut new_policies: Vec<(ZonePolicy, String, u8, u8, String)> = Vec::new();

        for (i, &(from_id, from_trust, ref from_name)) in zones.iter().enumerate() {
            for (j, &(to_id, to_trust, ref to_name)) in zones.iter().enumerate() {
                if i == j {
                    continue;
                }
                let action = derive_action(from_trust, to_trust);
                let policy = ZonePolicy::new(from_id, to_id, action);
                new_policies.push((
                    policy,
                    from_name.clone(),
                    from_trust,
                    to_trust,
                    to_name.clone(),
                ));
            }
        }

        for (policy, from_name, from_trust, to_trust, to_name) in new_policies {
            g.log_decision(
                DesignDecision::new(
                    format!("Policy: {from_name} → {to_name}"),
                    format!(
                        "Derived from trust levels: {from_trust} → {to_trust} = {:?}",
                        policy.action
                    ),
                    TriggerSource::RuleEngine,
                    true,
                )
                .with_rule("ZONE_POLICY_DERIVATION"),
            );
            g.add_policy(policy);
        }

        g
    }

    fn validate(&self, _: &NetworkIntent, graph: &TopologyGraph) -> RuleResult {
        let n = graph.zones.len();
        let expected = n * (n.saturating_sub(1));
        if graph.policies.len() >= expected {
            RuleResult::Ok
        } else {
            RuleResult::Err(format!(
                "expected at least {expected} policies, got {}",
                graph.policies.len()
            ))
        }
    }
}

struct GuestIsolationRule;

impl Rule for GuestIsolationRule {
    fn metadata(&self) -> EngineRule {
        EngineRule {
            id: "GUEST_ISOLATION",
            name: "Force all Guest outbound policies to Deny",
            phase: RulePhase::Segmentation,
            priority: 80,
            category: RuleCategory::Mandatory,
        }
    }

    fn condition(&self, _: &NetworkIntent, graph: &TopologyGraph) -> bool {
        let Some(guest) = graph.zone_by_role(&ZoneRole::Guest) else {
            return false;
        };
        let guest_id = guest.id;
        graph
            .policies
            .iter()
            .any(|p| p.from_zone == guest_id && p.action != PolicyAction::Deny)
    }

    fn transform(&self, _: &NetworkIntent, graph: &TopologyGraph) -> TopologyGraph {
        let mut g = graph.clone();

        let guest_id = match g.zone_by_role(&ZoneRole::Guest) {
            Some(z) => z.id,
            None => return g,
        };

        let others: Vec<(Uuid, String)> = g
            .zones
            .iter()
            .filter(|z| z.id != guest_id)
            .map(|z| (z.id, z.name.clone()))
            .collect();

        g.policies
            .retain(|p| !(p.from_zone == guest_id && p.action != PolicyAction::Deny));

        for (other_id, other_name) in others {
            if g.policy_between(guest_id, other_id).is_none() {
                g.log_decision(
                    DesignDecision::new(
                        format!("Policy: GUEST → {other_name} = DENY"),
                        "Guest isolation: all traffic from guest to internal zones is denied.",
                        TriggerSource::RuleEngine,
                        false,
                    )
                    .with_rule("GUEST_ISOLATION"),
                );
                g.add_policy(ZonePolicy::new(guest_id, other_id, PolicyAction::Deny));
            }
        }

        g
    }

    fn validate(&self, _: &NetworkIntent, graph: &TopologyGraph) -> RuleResult {
        let Some(guest) = graph.zone_by_role(&ZoneRole::Guest) else {
            return RuleResult::Ok;
        };
        let all_deny = graph
            .policies
            .iter()
            .filter(|p| p.from_zone == guest.id)
            .all(|p| p.action == PolicyAction::Deny);
        if all_deny {
            RuleResult::Ok
        } else {
            RuleResult::Err(
                "all guest outbound policies must be Deny after GUEST_ISOLATION".into(),
            )
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{
        AddressingIntent, AddressingMode, AddressingScheme, ConstraintsIntent, ContextIntent,
        Environment, IntentMetadata, Ipv4Config, RedundancyLevel, RoutingIntent, SecurityLevel,
        SegmentationIntent, ServicesIntent, TopologyIntent, TopologyType, Vendor, ZoneIntent,
        ZoneRole, CURRENT_SCHEMA_VERSION,
    };

    fn engine() -> RuleEngine {
        RuleEngine::new()
    }

    fn base_intent() -> NetworkIntent {
        NetworkIntent {
            metadata: IntentMetadata {
                name: "test-net".into(),
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

    // ── Zone construction ─────────────────────────────────────────────────────

    #[test]
    fn zones_created_from_intent() {
        let chain = engine().evaluate(&base_intent()).unwrap();
        let graph = chain.latest_graph();
        assert!(graph.zone_by_role(&ZoneRole::Lan).is_some());
        assert_eq!(graph.zone_by_role(&ZoneRole::Lan).unwrap().name, "LAN");
    }

    #[test]
    fn mgmt_zone_auto_injected() {
        let chain = engine().evaluate(&base_intent()).unwrap();
        let graph = chain.latest_graph();
        assert!(
            graph.zone_by_role(&ZoneRole::Mgmt).is_some(),
            "MGMT zone must be auto-injected"
        );
        assert!(graph
            .decisions
            .iter()
            .any(|d| d.rule_id.as_deref() == Some("MGMT_ZONE_REQUIRED")));
    }

    #[test]
    fn mgmt_zone_not_duplicated_when_declared() {
        let mut intent = base_intent();
        intent.segmentation.zones.push(ZoneIntent {
            name: "MGMT".into(),
            role: ZoneRole::Mgmt,
            trust_level: 100,
        });
        let chain = engine().evaluate(&intent).unwrap();
        let graph = chain.latest_graph();
        let mgmt_count = graph.zones.iter().filter(|z| z.role == ZoneRole::Mgmt).count();
        assert_eq!(mgmt_count, 1, "MGMT zone must not be duplicated");
    }

    #[test]
    fn dmz_auto_injected_for_internet_gateway() {
        let mut intent = base_intent();
        intent.services.internet_gateway = true;
        let chain = engine().evaluate(&intent).unwrap();
        let graph = chain.latest_graph();
        assert!(graph.zone_by_role(&ZoneRole::Dmz).is_some());
        assert!(graph
            .decisions
            .iter()
            .any(|d| d.rule_id.as_deref() == Some("DMZ_REQUIRED_FOR_INTERNET")));
    }

    #[test]
    fn dmz_not_injected_without_internet() {
        let chain = engine().evaluate(&base_intent()).unwrap();
        assert!(chain.latest_graph().zone_by_role(&ZoneRole::Dmz).is_none());
    }

    #[test]
    fn dmz_not_duplicated_when_already_present() {
        let mut intent = base_intent();
        intent.services.internet_gateway = true;
        intent.segmentation.zones.push(ZoneIntent {
            name: "DMZ".into(),
            role: ZoneRole::Dmz,
            trust_level: 60,
        });
        let chain = engine().evaluate(&intent).unwrap();
        let graph = chain.latest_graph();
        let dmz_count = graph.zones.iter().filter(|z| z.role == ZoneRole::Dmz).count();
        assert_eq!(dmz_count, 1);
    }

    // ── VLAN assignment ───────────────────────────────────────────────────────

    #[test]
    fn all_zones_get_vlans() {
        let chain = engine().evaluate(&base_intent()).unwrap();
        assert!(chain.latest_graph().zones.iter().all(|z| z.vlan.is_some()));
    }

    #[test]
    fn vlan_grouped_by_role() {
        let mut intent = base_intent();
        intent.segmentation.zones.push(ZoneIntent {
            name: "DMZ".into(),
            role: ZoneRole::Dmz,
            trust_level: 60,
        });
        let chain = engine().evaluate(&intent).unwrap();
        let graph = chain.latest_graph();

        let lan_vlan = graph.zone_by_role(&ZoneRole::Lan).unwrap().vlan.unwrap();
        let dmz_vlan = graph.zone_by_role(&ZoneRole::Dmz).unwrap().vlan.unwrap();
        let mgmt_vlan = graph.zone_by_role(&ZoneRole::Mgmt).unwrap().vlan.unwrap();

        assert!((10..100).contains(&lan_vlan), "LAN VLAN should be 10–99, got {lan_vlan}");
        assert!(
            (100..200).contains(&dmz_vlan),
            "DMZ VLAN should be 100–199, got {dmz_vlan}"
        );
        assert!(
            (200..300).contains(&mgmt_vlan),
            "MGMT VLAN should be 200–299, got {mgmt_vlan}"
        );
    }

    // ── Device injection ──────────────────────────────────────────────────────

    #[test]
    fn core_router_injected() {
        let chain = engine().evaluate(&base_intent()).unwrap();
        assert!(
            chain
                .latest_graph()
                .nodes
                .iter()
                .any(|n| n.label == "RTR-CORE-01"),
            "RTR-CORE-01 must always be present"
        );
    }

    #[test]
    fn each_zone_gets_a_device() {
        let chain = engine().evaluate(&base_intent()).unwrap();
        let graph = chain.latest_graph();
        for zone in &graph.zones {
            let devices_in_zone = graph.nodes_in_zone(zone.id);
            assert!(
                !devices_in_zone.is_empty(),
                "Zone '{}' has no devices",
                zone.name
            );
        }
    }

    #[test]
    fn firewall_injected_in_dmz() {
        let mut intent = base_intent();
        intent.segmentation.zones.push(ZoneIntent {
            name: "DMZ".into(),
            role: ZoneRole::Dmz,
            trust_level: 60,
        });
        let chain = engine().evaluate(&intent).unwrap();
        assert!(chain
            .latest_graph()
            .nodes
            .iter()
            .any(|n| matches!(n.role, NodeRole::Firewall)));
    }

    // ── Link injection ────────────────────────────────────────────────────────

    #[test]
    fn links_injected_star_topology() {
        let chain = engine().evaluate(&base_intent()).unwrap();
        let graph = chain.latest_graph();
        let non_core = graph.nodes.iter().filter(|n| n.label != "RTR-CORE-01").count();
        assert_eq!(
            graph.edges.len(),
            non_core,
            "star topology: one link per non-core device"
        );
    }

    #[test]
    fn every_device_reachable_from_core() {
        let chain = engine().evaluate(&base_intent()).unwrap();
        assert!(
            chain.latest_graph().is_fully_connected(),
            "all nodes must be reachable in star topology"
        );
    }

    #[test]
    fn devices_have_interfaces_after_link_injection() {
        let chain = engine().evaluate(&base_intent()).unwrap();
        let graph = chain.latest_graph();
        for node in &graph.nodes {
            assert!(
                !node.interfaces.is_empty(),
                "node '{}' has no interfaces",
                node.label
            );
        }
    }

    // ── Zone policy derivation ────────────────────────────────────────────────

    #[test]
    fn policies_cover_all_zone_pairs() {
        let chain = engine().evaluate(&base_intent()).unwrap();
        let graph = chain.latest_graph();
        let n = graph.zones.len();
        assert_eq!(graph.policies.len(), n * (n - 1));
    }

    #[test]
    fn high_trust_zones_get_permit() {
        // LAN(80) + MGMT(100) = 180 >= 150 → Permit
        let chain = engine().evaluate(&base_intent()).unwrap();
        let graph = chain.latest_graph();
        let lan_id = graph.zone_by_role(&ZoneRole::Lan).unwrap().id;
        let mgmt_id = graph.zone_by_role(&ZoneRole::Mgmt).unwrap().id;

        let policy = graph.policy_between(lan_id, mgmt_id).unwrap();
        assert_eq!(policy.action, PolicyAction::Permit);
    }

    // ── Guest isolation ───────────────────────────────────────────────────────

    #[test]
    fn guest_to_lan_is_deny() {
        let mut intent = base_intent();
        intent.segmentation.zones.push(ZoneIntent {
            name: "Guest".into(),
            role: ZoneRole::Guest,
            trust_level: 10,
        });
        let chain = engine().evaluate(&intent).unwrap();
        let graph = chain.latest_graph();

        let guest_id = graph.zone_by_role(&ZoneRole::Guest).unwrap().id;
        let lan_id = graph.zone_by_role(&ZoneRole::Lan).unwrap().id;

        let policy = graph.policy_between(guest_id, lan_id).unwrap();
        assert_eq!(policy.action, PolicyAction::Deny);
    }

    #[test]
    fn all_guest_outbound_are_deny() {
        let mut intent = base_intent();
        intent.segmentation.zones.push(ZoneIntent {
            name: "Guest".into(),
            role: ZoneRole::Guest,
            trust_level: 10,
        });
        let chain = engine().evaluate(&intent).unwrap();
        let graph = chain.latest_graph();
        let guest_id = graph.zone_by_role(&ZoneRole::Guest).unwrap().id;

        for policy in graph.policies.iter().filter(|p| p.from_zone == guest_id) {
            assert_eq!(
                policy.action,
                PolicyAction::Deny,
                "all guest outbound traffic must be Deny"
            );
        }
    }

    // ── Routing decision ──────────────────────────────────────────────────────

    #[test]
    fn routing_decision_always_logged() {
        let chain = engine().evaluate(&base_intent()).unwrap();
        assert!(has_fired("ROUTING_PROTOCOL_SELECTION", chain.latest_graph()));
    }

    #[test]
    fn ospf_selected_when_specified() {
        let mut intent = base_intent();
        intent.routing.protocols = vec![RoutingProtocol::Ospf];
        let chain = engine().evaluate(&intent).unwrap();
        let graph = chain.latest_graph();
        let decision = graph
            .decisions
            .iter()
            .find(|d| d.rule_id.as_deref() == Some("ROUTING_PROTOCOL_SELECTION"))
            .unwrap();
        assert!(decision.component.contains("OSPF"));
    }

    // ── Convergence ───────────────────────────────────────────────────────────

    #[test]
    fn engine_converges_in_few_passes() {
        let result = engine().evaluate(&base_intent());
        assert!(result.is_ok(), "engine must reach stable state");
    }

    #[test]
    fn evaluate_is_deterministic() {
        let a = engine().evaluate(&base_intent()).unwrap();
        let b = engine().evaluate(&base_intent()).unwrap();
        let ga = a.latest_graph();
        let gb = b.latest_graph();
        assert_eq!(ga.zones.len(), gb.zones.len());
        assert_eq!(ga.nodes.len(), gb.nodes.len());
        assert_eq!(ga.edges.len(), gb.edges.len());
        assert_eq!(ga.policies.len(), gb.policies.len());
    }

    // ── Version chain ─────────────────────────────────────────────────────────

    #[test]
    fn evaluate_produces_version_chain() {
        let chain = engine().evaluate(&base_intent()).unwrap();
        assert!(chain.len() > 1, "engine must produce multiple versions");
    }

    #[test]
    fn version_chain_is_ordered() {
        let chain = engine().evaluate(&base_intent()).unwrap();
        let versions = chain.all();
        for window in versions.windows(2) {
            assert_eq!(
                window[1].version,
                window[0].version + 1,
                "versions must be monotonically increasing"
            );
        }
    }

    #[test]
    fn each_version_has_non_empty_diff_except_v1() {
        let chain = engine().evaluate(&base_intent()).unwrap();
        let versions = chain.all();
        assert!(versions[0].diff.is_empty(), "v1 diff must be empty");
        for v in &versions[1..] {
            assert!(
                !v.diff.is_empty(),
                "version {} (produced by '{}') must have a non-empty diff",
                v.version,
                v.produced_by
            );
        }
    }

    #[test]
    fn phase_ordering_structural_before_segmentation() {
        // Zones (Structural) must exist before VLANs are assigned (Segmentation).
        let chain = engine().evaluate(&base_intent()).unwrap();
        let graph = chain.latest_graph();
        assert!(!graph.zones.is_empty());
        assert!(graph.zones.iter().all(|z| z.vlan.is_some()));
    }
}
