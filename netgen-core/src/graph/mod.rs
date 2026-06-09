use std::collections::HashSet;
use std::net::IpAddr;

use chrono::{DateTime, Utc};
use petgraph::{
    graph::{NodeIndex, UnGraph},
    visit::Bfs,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::schema::{Vendor, ZoneRole};

// petgraph backend — rebuilt on demand, never serialised
type PetGraph = UnGraph<Uuid, Uuid>;

// ─── TopologyGraph ────────────────────────────────────────────────────────────

/// The single source of truth for every output artifact.
/// All derived outputs (configs, diagrams, IP plans, GNS3, simulation) read from this.
/// Built by the rule engine; read by everything downstream.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TopologyGraph {
    pub nodes: Vec<NetworkNode>,
    pub edges: Vec<NetworkLink>,
    pub zones: Vec<SecurityZone>,
    pub policies: Vec<ZonePolicy>,
    pub addressing: AddressingPlan,
    pub decisions: Vec<DesignDecision>,
    pub metadata: GraphMetadata,
}

impl TopologyGraph {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            metadata: GraphMetadata {
                name: name.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    // ── Mutation ──────────────────────────────────────────────────────────────

    pub fn add_node(&mut self, node: NetworkNode) {
        self.nodes.push(node);
    }

    pub fn add_link(&mut self, link: NetworkLink) {
        self.edges.push(link);
    }

    pub fn add_zone(&mut self, zone: SecurityZone) {
        self.zones.push(zone);
    }

    pub fn add_policy(&mut self, policy: ZonePolicy) {
        self.policies.push(policy);
    }

    pub fn log_decision(&mut self, decision: DesignDecision) {
        self.decisions.push(decision);
    }

    pub fn remove_node(&mut self, id: Uuid) -> Option<NetworkNode> {
        let pos = self.nodes.iter().position(|n| n.id == id)?;
        Some(self.nodes.remove(pos))
    }

    pub fn remove_link(&mut self, id: Uuid) -> Option<NetworkLink> {
        let pos = self.edges.iter().position(|e| e.id == id)?;
        Some(self.edges.remove(pos))
    }

    // ── Lookup ────────────────────────────────────────────────────────────────

    pub fn node(&self, id: Uuid) -> Option<&NetworkNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn node_mut(&mut self, id: Uuid) -> Option<&mut NetworkNode> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    pub fn link(&self, id: Uuid) -> Option<&NetworkLink> {
        self.edges.iter().find(|e| e.id == id)
    }

    pub fn zone(&self, id: Uuid) -> Option<&SecurityZone> {
        self.zones.iter().find(|z| z.id == id)
    }

    pub fn zone_mut(&mut self, id: Uuid) -> Option<&mut SecurityZone> {
        self.zones.iter_mut().find(|z| z.id == id)
    }

    /// Returns the first zone with the given role.
    pub fn zone_by_role(&self, role: &ZoneRole) -> Option<&SecurityZone> {
        self.zones.iter().find(|z| &z.role == role)
    }

    pub fn nodes_in_zone(&self, zone_id: Uuid) -> Vec<&NetworkNode> {
        self.nodes.iter().filter(|n| n.zone_id == zone_id).collect()
    }

    pub fn zone_for_node(&self, node_id: Uuid) -> Option<&SecurityZone> {
        let zone_id = self.node(node_id)?.zone_id;
        self.zone(zone_id)
    }

    /// Returns the first matching policy from_zone → to_zone.
    /// Does NOT automatically check the reverse direction;
    /// bidirectional policies must be queried in both directions.
    pub fn policy_between(&self, from_zone: Uuid, to_zone: Uuid) -> Option<&ZonePolicy> {
        self.policies
            .iter()
            .find(|p| p.from_zone == from_zone && p.to_zone == to_zone)
    }

    // ── Petgraph-backed traversal ─────────────────────────────────────────────

    /// Builds a fresh petgraph from current domain data.
    /// Returns (graph, node_id → NodeIndex map).
    /// Called once per traversal query; fine for network-scale graphs (<= a few hundred nodes).
    fn build_petgraph(&self) -> (PetGraph, std::collections::HashMap<Uuid, NodeIndex>) {
        let mut g = PetGraph::new_undirected();
        let mut idx_map = std::collections::HashMap::new();

        for node in &self.nodes {
            let idx = g.add_node(node.id);
            idx_map.insert(node.id, idx);
        }

        for link in &self.edges {
            if let (Some(&a), Some(&b)) =
                (idx_map.get(&link.from_node), idx_map.get(&link.to_node))
            {
                g.add_edge(a, b, link.id);
            }
        }

        (g, idx_map)
    }

    /// Directly connected neighbours (one hop) of the given node.
    pub fn neighbors(&self, node_id: Uuid) -> Vec<Uuid> {
        let (g, idx_map) = self.build_petgraph();
        let Some(&idx) = idx_map.get(&node_id) else {
            return vec![];
        };
        g.neighbors(idx)
            .map(|n| *g.node_weight(n).unwrap())
            .collect()
    }

    /// Shortest path (by hop count) between two nodes.
    /// Returns the ordered list of node IDs including both endpoints,
    /// or None if no path exists.
    pub fn shortest_path(&self, from: Uuid, to: Uuid) -> Option<Vec<Uuid>> {
        let (g, idx_map) = self.build_petgraph();
        let &from_idx = idx_map.get(&from)?;
        let &to_idx = idx_map.get(&to)?;

        let (_cost, path) =
            petgraph::algo::astar(&g, from_idx, |n| n == to_idx, |_| 1usize, |_| 0usize)?;

        Some(
            path.into_iter()
                .map(|idx| *g.node_weight(idx).unwrap())
                .collect(),
        )
    }

    /// All nodes reachable from `node_id` via any path (BFS).
    /// Includes `node_id` itself.
    pub fn reachable_from(&self, node_id: Uuid) -> Vec<Uuid> {
        let (g, idx_map) = self.build_petgraph();
        let Some(&start) = idx_map.get(&node_id) else {
            return vec![];
        };

        let mut bfs = Bfs::new(&g, start);
        let mut visited = vec![];
        while let Some(nx) = bfs.next(&g) {
            visited.push(*g.node_weight(nx).unwrap());
        }
        visited
    }

    /// True if every node in the graph can reach every other node.
    /// An empty or single-node graph is considered connected.
    pub fn is_fully_connected(&self) -> bool {
        if self.nodes.len() <= 1 {
            return true;
        }
        let start = self.nodes[0].id;
        self.reachable_from(start).len() == self.nodes.len()
    }

    /// Zones that are directly adjacent to `zone_id` via at least one physical link.
    /// A zone is adjacent if any node in it connects to any node in `zone_id`.
    pub fn adjacent_zones(&self, zone_id: Uuid) -> Vec<Uuid> {
        let zone_nodes: HashSet<Uuid> = self
            .nodes_in_zone(zone_id)
            .into_iter()
            .map(|n| n.id)
            .collect();

        let mut adjacent: HashSet<Uuid> = HashSet::new();

        for link in &self.edges {
            let a_in_zone = zone_nodes.contains(&link.from_node);
            let b_in_zone = zone_nodes.contains(&link.to_node);

            if a_in_zone && !b_in_zone {
                if let Some(z) = self.zone_for_node(link.to_node) {
                    adjacent.insert(z.id);
                }
            } else if b_in_zone && !a_in_zone {
                if let Some(z) = self.zone_for_node(link.from_node) {
                    adjacent.insert(z.id);
                }
            }
        }

        adjacent.into_iter().collect()
    }
}

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkNode {
    pub id: Uuid,
    pub label: String,
    pub role: NodeRole,
    pub vendor: Vendor,
    pub zone_id: Uuid,
    pub interfaces: Vec<Interface>,
    pub config: Option<DeviceConfig>,
}

impl NetworkNode {
    pub fn new(
        label: impl Into<String>,
        role: NodeRole,
        vendor: Vendor,
        zone_id: Uuid,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            label: label.into(),
            role,
            vendor,
            zone_id,
            interfaces: vec![],
            config: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeRole {
    Router,
    Switch,
    Firewall,
    Host,
    Cloud,
    Ap,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkLink {
    pub id: Uuid,
    pub from_node: Uuid,
    pub from_iface: Uuid,
    pub to_node: Uuid,
    pub to_iface: Uuid,
    /// Bandwidth in Mbps
    pub bandwidth: Option<u32>,
    pub link_type: LinkType,
}

impl NetworkLink {
    pub fn new(
        from_node: Uuid,
        from_iface: Uuid,
        to_node: Uuid,
        to_iface: Uuid,
        link_type: LinkType,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            from_node,
            from_iface,
            to_node,
            to_iface,
            bandwidth: None,
            link_type,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkType {
    Ethernet,
    Serial,
    Fiber,
    Tunnel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityZone {
    pub id: Uuid,
    pub name: String,
    pub role: ZoneRole,
    /// 0–100; used to derive inter-zone policy from trust delta
    pub trust_level: u8,
    pub vlan: Option<u16>,
    /// Semantic label e.g. "Clinical-Critical", "OOB-Management"
    pub label: String,
}

impl SecurityZone {
    pub fn new(name: impl Into<String>, role: ZoneRole, trust_level: u8) -> Self {
        let name = name.into();
        let label = name.clone();
        Self {
            id: Uuid::new_v4(),
            name,
            role,
            trust_level,
            vlan: None,
            label,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZonePolicy {
    pub from_zone: Uuid,
    pub to_zone: Uuid,
    pub action: PolicyAction,
    pub services: Vec<Service>,
    pub direction: PolicyDirection,
}

impl ZonePolicy {
    pub fn new(from_zone: Uuid, to_zone: Uuid, action: PolicyAction) -> Self {
        Self {
            from_zone,
            to_zone,
            action,
            services: vec![],
            direction: PolicyDirection::Unidirectional,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PolicyAction {
    Permit,
    Deny,
    Inspect,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyDirection {
    Unidirectional,
    Bidirectional,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Service {
    pub name: String,
    pub protocol: String,
    pub port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Interface {
    pub id: Uuid,
    pub name: String,
    pub ipv4_addr: Option<String>,
    pub ipv6_addr: Option<String>,
    pub role: InterfaceRole,
}

impl Interface {
    pub fn new(name: impl Into<String>, role: InterfaceRole) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            ipv4_addr: None,
            ipv6_addr: None,
            role,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InterfaceRole {
    Uplink,
    Access,
    Transit,
    Loopback,
    Mgmt,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AddressingPlan {
    pub ipv4_allocations: Vec<Ipv4Allocation>,
    pub ipv6_allocations: Vec<Ipv6Allocation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ipv4Allocation {
    pub zone_id: Uuid,
    pub subnet: String,
    pub purpose: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ipv6Allocation {
    pub zone_id: Uuid,
    pub prefix: String,
    pub purpose: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceConfig {
    pub raw: String,
    pub template_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesignDecision {
    pub id: Uuid,
    pub component: String,
    pub reason: String,
    pub rule_id: Option<String>,
    pub triggered_by: TriggerSource,
    pub overridable: bool,
    pub timestamp: DateTime<Utc>,
}

impl DesignDecision {
    pub fn new(
        component: impl Into<String>,
        reason: impl Into<String>,
        triggered_by: TriggerSource,
        overridable: bool,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            component: component.into(),
            reason: reason.into(),
            rule_id: None,
            triggered_by,
            overridable,
            timestamp: Utc::now(),
        }
    }

    /// Chain a rule_id onto a decision.
    pub fn with_rule(mut self, rule_id: impl Into<String>) -> Self {
        self.rule_id = Some(rule_id.into());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerSource {
    UserIntent,
    RuleEngine,
    SecurityPolicy,
    AiInference,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GraphMetadata {
    pub name: String,
    pub schema_version: String,
    pub rules_version: String,
    pub policy_version: String,
    pub generated_at: Option<DateTime<Utc>>,
}

// ─── Immutable graph versioning ───────────────────────────────────────────────

/// A single change from one graph snapshot to the next.
/// Every rule transform declares exactly which changes it made;
/// the version chain preserves these for audit and rollback.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GraphChange {
    NodeAdded(NetworkNode),
    NodeRemoved { id: Uuid },
    EdgeAdded(NetworkLink),
    EdgeRemoved { id: Uuid },
    ZoneAdded(SecurityZone),
    ZoneRemoved { id: Uuid },
    PolicyAdded(ZonePolicy),
    PolicyRemoved { from_zone: Uuid, to_zone: Uuid },
    AddressAssigned { node: Uuid, interface: Uuid, addr: IpAddr },
    DecisionLogged(DesignDecision),
}

/// One immutable snapshot of the topology graph, tagged with what produced it
/// and what changed since the previous version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphVersion {
    /// Monotonically increasing; v1 is the initial empty graph.
    pub version: u32,
    /// Full graph snapshot at this version.
    pub graph: TopologyGraph,
    /// Human-readable name of the rule, phase, or engine that produced this version.
    pub produced_by: String,
    pub timestamp: DateTime<Utc>,
    /// Exact set of changes applied to produce this version from the previous one.
    /// Empty for v1.
    pub diff: Vec<GraphChange>,
}

impl GraphVersion {
    pub fn initial(graph: TopologyGraph, produced_by: impl Into<String>) -> Self {
        Self {
            version: 1,
            graph,
            produced_by: produced_by.into(),
            timestamp: Utc::now(),
            diff: vec![],
        }
    }

    pub fn next(
        &self,
        graph: TopologyGraph,
        produced_by: impl Into<String>,
        diff: Vec<GraphChange>,
    ) -> Self {
        Self {
            version: self.version + 1,
            graph,
            produced_by: produced_by.into(),
            timestamp: Utc::now(),
            diff,
        }
    }
}

/// Ordered sequence of graph versions from v1 through the final state.
/// The chain is never empty — it always contains at least the initial version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphVersionChain {
    versions: Vec<GraphVersion>,
}

impl GraphVersionChain {
    /// Start a new chain from the given graph.
    pub fn new(initial_graph: TopologyGraph, produced_by: impl Into<String>) -> Self {
        Self {
            versions: vec![GraphVersion::initial(initial_graph, produced_by)],
        }
    }

    /// Append a new version to the chain.
    pub fn push(&mut self, graph: TopologyGraph, produced_by: impl Into<String>, diff: Vec<GraphChange>) {
        let next = self.latest().next(graph, produced_by, diff);
        self.versions.push(next);
    }

    /// The most recent graph version.
    pub fn latest(&self) -> &GraphVersion {
        self.versions.last().expect("version chain is never empty")
    }

    /// The most recent graph (convenience accessor).
    pub fn latest_graph(&self) -> &TopologyGraph {
        &self.latest().graph
    }

    /// Look up a specific version by number (1-based).
    pub fn get(&self, version: u32) -> Option<&GraphVersion> {
        self.versions.iter().find(|v| v.version == version)
    }

    /// Total number of versions in the chain.
    pub fn len(&self) -> usize {
        self.versions.len()
    }

    pub fn is_empty(&self) -> bool {
        false // invariant: chain always has at least v1
    }

    /// All versions in chronological order.
    pub fn all(&self) -> &[GraphVersion] {
        &self.versions
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Vendor, ZoneRole};

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn zone(role: ZoneRole, name: &str) -> SecurityZone {
        SecurityZone::new(name, role, 80)
    }

    fn router(zone_id: Uuid, label: &str) -> NetworkNode {
        NetworkNode::new(label, NodeRole::Router, Vendor::Cisco, zone_id)
    }

    fn link(a: &NetworkNode, b: &NetworkNode) -> NetworkLink {
        let a_iface = Uuid::new_v4();
        let b_iface = Uuid::new_v4();
        NetworkLink::new(a.id, a_iface, b.id, b_iface, LinkType::Ethernet)
    }

    fn linear_graph() -> (TopologyGraph, [Uuid; 3]) {
        // A ── B ── C  (all in one zone)
        let mut g = TopologyGraph::new("test");
        let z = zone(ZoneRole::Lan, "LAN");
        g.add_zone(z.clone());

        let a = router(z.id, "RTR-A");
        let b = router(z.id, "RTR-B");
        let c = router(z.id, "RTR-C");
        let ids = [a.id, b.id, c.id];

        let ab = link(&a, &b);
        let bc = link(&b, &c);

        g.add_node(a);
        g.add_node(b);
        g.add_node(c);
        g.add_link(ab);
        g.add_link(bc);

        (g, ids)
    }

    // ── Construction & lookup ─────────────────────────────────────────────────

    #[test]
    fn add_and_lookup_node() {
        let mut g = TopologyGraph::new("test");
        let z = zone(ZoneRole::Lan, "LAN");
        let z_id = z.id;
        g.add_zone(z);

        let n = router(z_id, "RTR-01");
        let n_id = n.id;
        g.add_node(n);

        assert!(g.node(n_id).is_some());
        assert_eq!(g.node(n_id).unwrap().label, "RTR-01");
    }

    #[test]
    fn add_and_lookup_zone() {
        let mut g = TopologyGraph::new("test");
        let z = zone(ZoneRole::Dmz, "DMZ");
        let z_id = z.id;
        g.add_zone(z);

        assert!(g.zone(z_id).is_some());
        assert_eq!(g.zone(z_id).unwrap().role, ZoneRole::Dmz);
    }

    #[test]
    fn zone_by_role_found() {
        let mut g = TopologyGraph::new("test");
        g.add_zone(zone(ZoneRole::Mgmt, "MGMT"));
        g.add_zone(zone(ZoneRole::Lan, "LAN"));
        assert!(g.zone_by_role(&ZoneRole::Mgmt).is_some());
        assert!(g.zone_by_role(&ZoneRole::Dmz).is_none());
    }

    #[test]
    fn nodes_in_zone() {
        let mut g = TopologyGraph::new("test");
        let z1 = zone(ZoneRole::Lan, "LAN");
        let z2 = zone(ZoneRole::Dmz, "DMZ");
        let z1_id = z1.id;
        g.add_zone(z1);
        g.add_zone(z2.clone());

        g.add_node(router(z1_id, "RTR-01"));
        g.add_node(router(z1_id, "RTR-02"));
        g.add_node(router(z2.id, "FW-01"));

        assert_eq!(g.nodes_in_zone(z1_id).len(), 2);
        assert_eq!(g.nodes_in_zone(z2.id).len(), 1);
    }

    #[test]
    fn zone_for_node() {
        let mut g = TopologyGraph::new("test");
        let z = zone(ZoneRole::Lan, "LAN");
        let z_id = z.id;
        g.add_zone(z);

        let n = router(z_id, "RTR-01");
        let n_id = n.id;
        g.add_node(n);

        let found_zone = g.zone_for_node(n_id).unwrap();
        assert_eq!(found_zone.id, z_id);
    }

    #[test]
    fn policy_between_lookup() {
        let mut g = TopologyGraph::new("test");
        let z1 = zone(ZoneRole::Lan, "LAN");
        let z2 = zone(ZoneRole::Dmz, "DMZ");
        let (id1, id2) = (z1.id, z2.id);
        g.add_zone(z1);
        g.add_zone(z2);

        g.add_policy(ZonePolicy::new(id1, id2, PolicyAction::Permit));

        assert!(g.policy_between(id1, id2).is_some());
        assert_eq!(g.policy_between(id1, id2).unwrap().action, PolicyAction::Permit);
        // reverse direction has no explicit policy
        assert!(g.policy_between(id2, id1).is_none());
    }

    #[test]
    fn remove_node() {
        let mut g = TopologyGraph::new("test");
        let z = zone(ZoneRole::Lan, "LAN");
        let z_id = z.id;
        g.add_zone(z);

        let n = router(z_id, "RTR-01");
        let n_id = n.id;
        g.add_node(n);

        assert!(g.remove_node(n_id).is_some());
        assert!(g.node(n_id).is_none());
        assert!(g.remove_node(n_id).is_none()); // idempotent
    }

    #[test]
    fn decision_log() {
        let mut g = TopologyGraph::new("test");
        let d = DesignDecision::new(
            "DMZ zone",
            "internet_gateway=true requires DMZ",
            TriggerSource::RuleEngine,
            false,
        )
        .with_rule("DMZ_REQUIRED_FOR_INTERNET");

        g.log_decision(d);

        assert_eq!(g.decisions.len(), 1);
        assert_eq!(
            g.decisions[0].rule_id.as_deref(),
            Some("DMZ_REQUIRED_FOR_INTERNET")
        );
        assert!(!g.decisions[0].overridable);
    }

    // ── Traversal ─────────────────────────────────────────────────────────────

    #[test]
    fn neighbors_middle_node() {
        // A ── B ── C  →  neighbors of B = {A, C}
        let (g, [a_id, b_id, c_id]) = linear_graph();
        let mut nbrs = g.neighbors(b_id);
        nbrs.sort();
        let mut expected = vec![a_id, c_id];
        expected.sort();
        assert_eq!(nbrs, expected);
    }

    #[test]
    fn neighbors_endpoint() {
        // A ── B ── C  →  neighbors of A = {B}
        let (g, [a_id, b_id, _c_id]) = linear_graph();
        assert_eq!(g.neighbors(a_id), vec![b_id]);
    }

    #[test]
    fn neighbors_unknown_node() {
        let g = TopologyGraph::new("empty");
        assert!(g.neighbors(Uuid::new_v4()).is_empty());
    }

    #[test]
    fn shortest_path_found() {
        // A ── B ── C  →  A→C = [A, B, C]
        let (g, [a_id, b_id, c_id]) = linear_graph();
        let path = g.shortest_path(a_id, c_id).unwrap();
        assert_eq!(path, vec![a_id, b_id, c_id]);
    }

    #[test]
    fn shortest_path_direct() {
        // A ── B  →  A→B = [A, B]
        let (g, [a_id, b_id, _]) = linear_graph();
        let path = g.shortest_path(a_id, b_id).unwrap();
        assert_eq!(path, vec![a_id, b_id]);
    }

    #[test]
    fn shortest_path_none_disconnected() {
        let mut g = TopologyGraph::new("test");
        let z = zone(ZoneRole::Lan, "LAN");
        g.add_zone(z.clone());
        let a = router(z.id, "RTR-A");
        let b = router(z.id, "RTR-B");
        let (a_id, b_id) = (a.id, b.id);
        g.add_node(a);
        g.add_node(b);
        // no link added — disconnected

        assert!(g.shortest_path(a_id, b_id).is_none());
    }

    #[test]
    fn is_fully_connected_true() {
        let (g, _) = linear_graph();
        assert!(g.is_fully_connected());
    }

    #[test]
    fn is_fully_connected_false() {
        let mut g = TopologyGraph::new("test");
        let z = zone(ZoneRole::Lan, "LAN");
        g.add_zone(z.clone());
        // Two isolated nodes, no link
        g.add_node(router(z.id, "RTR-A"));
        g.add_node(router(z.id, "RTR-B"));

        assert!(!g.is_fully_connected());
    }

    #[test]
    fn is_fully_connected_empty() {
        let g = TopologyGraph::new("empty");
        assert!(g.is_fully_connected());
    }

    #[test]
    fn reachable_from_all() {
        let (g, [a_id, _, _]) = linear_graph();
        let reached = g.reachable_from(a_id);
        assert_eq!(reached.len(), 3);
    }

    #[test]
    fn reachable_from_isolated() {
        let mut g = TopologyGraph::new("test");
        let z = zone(ZoneRole::Lan, "LAN");
        g.add_zone(z.clone());
        let a = router(z.id, "RTR-A");
        let a_id = a.id;
        g.add_node(a);
        g.add_node(router(z.id, "RTR-B")); // no link to A

        let reached = g.reachable_from(a_id);
        assert_eq!(reached, vec![a_id]); // only itself
    }

    #[test]
    fn adjacent_zones() {
        // LAN node ── DMZ node
        let mut g = TopologyGraph::new("test");
        let lan = zone(ZoneRole::Lan, "LAN");
        let dmz = zone(ZoneRole::Dmz, "DMZ");
        let (lan_id, dmz_id) = (lan.id, dmz.id);
        g.add_zone(lan);
        g.add_zone(dmz);

        let rtr = router(lan_id, "RTR-01");
        let fw = router(dmz_id, "FW-01");
        let l = link(&rtr, &fw);
        g.add_node(rtr);
        g.add_node(fw);
        g.add_link(l);

        let adj = g.adjacent_zones(lan_id);
        assert_eq!(adj, vec![dmz_id]);

        // Internal link within LAN — does not appear as adjacent
        let rtr2 = router(lan_id, "RTR-02");
        let rtr3 = router(lan_id, "RTR-03");
        let internal = link(&rtr2, &rtr3);
        g.add_node(rtr2);
        g.add_node(rtr3);
        g.add_link(internal);

        let adj2 = g.adjacent_zones(lan_id);
        assert_eq!(adj2, vec![dmz_id]); // still just DMZ
    }

    // ── Serde round-trip ──────────────────────────────────────────────────────

    #[test]
    fn graph_serde_round_trip() {
        let (original, [a_id, _, _]) = linear_graph();
        let json = serde_json::to_string(&original).unwrap();
        let restored: TopologyGraph = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.nodes.len(), original.nodes.len());
        assert_eq!(restored.edges.len(), original.edges.len());
        assert!(restored.node(a_id).is_some());

        // Traversal still works after restoring from JSON
        assert!(restored.is_fully_connected());
    }

    // ── GraphVersionChain ─────────────────────────────────────────────────────

    #[test]
    fn version_chain_starts_at_v1() {
        let g = TopologyGraph::new("test");
        let chain = GraphVersionChain::new(g, "initial");
        assert_eq!(chain.len(), 1);
        assert_eq!(chain.latest().version, 1);
        assert_eq!(chain.latest().produced_by, "initial");
        assert!(chain.latest().diff.is_empty());
    }

    #[test]
    fn version_chain_push_increments_version() {
        let g1 = TopologyGraph::new("test");
        let mut chain = GraphVersionChain::new(g1, "init");

        let mut g2 = chain.latest_graph().clone();
        g2.add_zone(SecurityZone::new("LAN", ZoneRole::Lan, 80));
        let zone_id = g2.zones[0].id;

        chain.push(
            g2,
            "ZONES_FROM_INTENT",
            vec![GraphChange::ZoneAdded(SecurityZone::new("LAN", ZoneRole::Lan, 80))],
        );

        assert_eq!(chain.len(), 2);
        assert_eq!(chain.latest().version, 2);
        assert_eq!(chain.latest().produced_by, "ZONES_FROM_INTENT");
        assert_eq!(chain.latest().diff.len(), 1);
        assert!(chain.latest_graph().zone(zone_id).is_some());
    }

    #[test]
    fn version_chain_get_by_version_number() {
        let g = TopologyGraph::new("test");
        let mut chain = GraphVersionChain::new(g, "v1");
        chain.push(TopologyGraph::new("test"), "v2", vec![]);
        chain.push(TopologyGraph::new("test"), "v3", vec![]);

        assert_eq!(chain.get(1).unwrap().produced_by, "v1");
        assert_eq!(chain.get(2).unwrap().produced_by, "v2");
        assert_eq!(chain.get(3).unwrap().produced_by, "v3");
        assert!(chain.get(99).is_none());
    }

    #[test]
    fn version_chain_all_returns_ordered_slice() {
        let g = TopologyGraph::new("test");
        let mut chain = GraphVersionChain::new(g, "a");
        chain.push(TopologyGraph::new("test"), "b", vec![]);
        chain.push(TopologyGraph::new("test"), "c", vec![]);

        let all = chain.all();
        assert_eq!(all.len(), 3);
        assert!(all[0].version < all[1].version);
        assert!(all[1].version < all[2].version);
    }

    #[test]
    fn version_chain_is_not_empty() {
        let chain = GraphVersionChain::new(TopologyGraph::new("test"), "init");
        assert!(!chain.is_empty());
    }

    #[test]
    fn graph_change_variants_are_constructible() {
        use std::net::IpAddr;
        let z = SecurityZone::new("LAN", ZoneRole::Lan, 80);
        let z_id = z.id;
        let n = NetworkNode::new("R1", NodeRole::Router, Vendor::Cisco, z_id);
        let n_id = n.id;
        let iface_id = Uuid::new_v4();

        let _changes: Vec<GraphChange> = vec![
            GraphChange::ZoneAdded(z.clone()),
            GraphChange::ZoneRemoved { id: z_id },
            GraphChange::NodeAdded(n.clone()),
            GraphChange::NodeRemoved { id: n_id },
            GraphChange::PolicyAdded(ZonePolicy::new(z_id, z_id, PolicyAction::Deny)),
            GraphChange::PolicyRemoved { from_zone: z_id, to_zone: z_id },
            GraphChange::AddressAssigned {
                node: n_id,
                interface: iface_id,
                addr: IpAddr::V4("10.0.0.1".parse().unwrap()),
            },
        ];
    }

    #[test]
    fn graph_version_chain_serde_round_trip() {
        let g = TopologyGraph::new("test");
        let mut chain = GraphVersionChain::new(g, "init");
        chain.push(TopologyGraph::new("test"), "phase-1", vec![]);

        let json = serde_json::to_string(&chain).unwrap();
        let restored: GraphVersionChain = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.len(), 2);
        assert_eq!(restored.latest().produced_by, "phase-1");
    }
}
