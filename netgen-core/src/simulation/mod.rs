use std::collections::HashSet;
use std::net::Ipv4Addr;

use ipnetwork::Ipv4Network;
use uuid::Uuid;

use crate::graph::{NodeRole, PolicyAction, TopologyGraph};
use crate::schema::{RoutingIntent, RoutingProtocol};

// ─── Route table types ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteProtocol {
    Connected,
    Static,
    Ospf,
    Bgp,
}

impl RouteProtocol {
    fn code(&self) -> &'static str {
        match self {
            Self::Connected => "C",
            Self::Static    => "S",
            Self::Ospf      => "O",
            Self::Bgp       => "B",
        }
    }

    fn admin_distance(&self) -> u32 {
        match self {
            Self::Connected => 0,
            Self::Static    => 1,
            Self::Ospf      => 110,
            Self::Bgp       => 200,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RouteEntry {
    pub prefix: String,
    pub protocol: RouteProtocol,
    /// Hop count / OSPF cost / etc. (0 for connected).
    pub metric: u32,
    /// `None` for directly-connected routes.
    pub next_hop: Option<String>,
    pub via_interface: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RouteTable {
    pub node_id: Uuid,
    pub node_label: String,
    pub entries: Vec<RouteEntry>,
}

// ─── ACL types ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum FlowDecision {
    Permit,
    Deny { reason: String },
    /// No zone policy exists for this pair — neither explicitly permit nor deny.
    NoPolicy,
}

// ─── Trace types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum HopAction {
    /// Forwarded toward destination.
    Forward { via_interface: String, next_hop: Option<String> },
    /// Blocked by a zone ACL at this device.
    AclDeny { reason: String },
    /// Packet delivered to this device (final hop).
    Delivered,
}

#[derive(Debug, Clone)]
pub struct TraceHop {
    pub node_label: String,
    pub action: HopAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TraceOutcome {
    Delivered,
    Blocked,
    Unreachable,
}

#[derive(Debug, Clone)]
pub struct TraceResult {
    pub hops: Vec<TraceHop>,
    pub outcome: TraceOutcome,
    pub reason: Option<String>,
}

impl TraceResult {
    pub fn blocked(&self) -> bool {
        self.outcome == TraceOutcome::Blocked
    }
}

// ─── RouteCalculator ──────────────────────────────────────────────────────────

/// Computes per-device routing tables deterministically from the topology graph.
pub struct RouteCalculator;

impl RouteCalculator {
    /// Compute routing tables for all nodes in `graph`.
    ///
    /// Connected routes are always included. OSPF/BGP routes are added when the
    /// corresponding protocol appears in `routing.protocols`.
    pub fn compute(&self, graph: &TopologyGraph, routing: &RoutingIntent) -> Vec<RouteTable> {
        let ospf_on = routing.protocols.contains(&RoutingProtocol::Ospf);
        let bgp_on  = routing.protocols.contains(&RoutingProtocol::Bgp);

        graph.nodes.iter()
            .map(|node| {
                let mut entries = connected_routes(node);

                if ospf_on && matches!(node.role, NodeRole::Router | NodeRole::Firewall) {
                    entries.extend(ospf_routes(node, graph));
                }

                if bgp_on && matches!(node.role, NodeRole::Router) {
                    entries.extend(bgp_routes(node, graph));
                }

                // Deduplicate: for the same prefix keep the entry with highest precedence
                // (lowest admin distance, then longest prefix).
                dedup_routes(&mut entries);

                RouteTable {
                    node_id: node.id,
                    node_label: node.label.clone(),
                    entries,
                }
            })
            .collect()
    }
}

// ─── AclEvaluator ────────────────────────────────────────────────────────────

/// Evaluates zone-level flow decisions from the zone policy matrix in the graph.
pub struct AclEvaluator;

impl AclEvaluator {
    /// Decide whether traffic from `src_ip` to `dst_ip` is permitted.
    ///
    /// Zone membership is determined by the IPv4 addressing plan in the graph.
    /// Same-zone traffic is always permitted.
    pub fn evaluate(&self, graph: &TopologyGraph, src_ip: &str, dst_ip: &str) -> FlowDecision {
        let src_zone = find_zone_for_ip(graph, src_ip);
        let dst_zone = find_zone_for_ip(graph, dst_ip);

        match (src_zone, dst_zone) {
            (None, _) | (_, None) => FlowDecision::NoPolicy,
            (Some(s), Some(d)) if s == d => FlowDecision::Permit, // intra-zone
            (Some(s), Some(d)) => match graph.policy_between(s, d) {
                None => FlowDecision::NoPolicy,
                Some(p) => match p.action {
                    PolicyAction::Permit | PolicyAction::Inspect => FlowDecision::Permit,
                    PolicyAction::Deny => {
                        let src_name = zone_label(graph, s);
                        let dst_name = zone_label(graph, d);
                        FlowDecision::Deny {
                            reason: format!(
                                "Zone policy {} \u{2192} {} = DENY",
                                src_name, dst_name
                            ),
                        }
                    }
                },
            },
        }
    }
}

// ─── PathTracer ───────────────────────────────────────────────────────────────

const MAX_HOPS: usize = 15;

/// Traces the path from one graph node to another, evaluating zone ACLs at
/// every zone boundary along the way.
pub struct PathTracer;

impl PathTracer {
    /// Trace the path from `from_id` to `to_id` using the pre-computed route tables.
    ///
    /// ACL is checked whenever the path crosses a zone boundary. The first
    /// zone boundary with a DENY policy blocks the trace there.
    pub fn trace(
        &self,
        graph: &TopologyGraph,
        tables: &[RouteTable],
        from_id: Uuid,
        to_id: Uuid,
    ) -> TraceResult {
        let mut hops: Vec<TraceHop> = Vec::new();

        let Some(dst_node) = graph.nodes.iter().find(|n| n.id == to_id) else {
            return unreachable_result(hops, "destination node not found in graph");
        };

        let Some(dst_ip) = dst_node.interfaces.iter()
            .find_map(|i| i.ipv4_addr.as_deref())
            .and_then(|c| c.split('/').next().map(str::to_string))
        else {
            return unreachable_result(hops, "destination node has no IPv4 address");
        };

        let mut current_id = from_id;
        let mut visited: HashSet<Uuid> = HashSet::new();
        let mut current_zone: Option<Uuid> =
            graph.zone_for_node(from_id).map(|z| z.id);

        for _ in 0..MAX_HOPS {
            // Loop detection
            if !visited.insert(current_id) {
                return unreachable_result(hops, "routing loop detected");
            }

            let Some(node) = graph.nodes.iter().find(|n| n.id == current_id) else {
                break;
            };

            // Zone boundary ACL check — must run BEFORE the delivery check so that
            // a deny at the destination zone boundary is correctly reported.
            let node_zone = graph.zone_for_node(current_id).map(|z| z.id);
            if node_zone != current_zone {
                if let (Some(prev_z), Some(next_z)) = (current_zone, node_zone) {
                    if let Some(policy) = graph.policy_between(prev_z, next_z) {
                        if policy.action == PolicyAction::Deny {
                            let reason = format!(
                                "Zone policy {} \u{2192} {} = DENY",
                                zone_label(graph, prev_z),
                                zone_label(graph, next_z)
                            );
                            hops.push(TraceHop {
                                node_label: node.label.clone(),
                                action: HopAction::AclDeny { reason: reason.clone() },
                            });
                            return TraceResult {
                                hops,
                                outcome: TraceOutcome::Blocked,
                                reason: Some(reason),
                            };
                        }
                    }
                }
                current_zone = node_zone;
            }

            // Delivery check — after ACL so zone-boundary denials are caught first
            if current_id == to_id {
                hops.push(TraceHop {
                    node_label: node.label.clone(),
                    action: HopAction::Delivered,
                });
                return TraceResult { hops, outcome: TraceOutcome::Delivered, reason: None };
            }

            // Route lookup and forward
            let table = tables.iter().find(|t| t.node_id == current_id);
            let best = table.and_then(|t| find_best_route(&t.entries, &dst_ip));

            match best {
                None => {
                    return unreachable_result(
                        hops,
                        &format!("no route to {} on '{}'", dst_ip, node.label),
                    );
                }
                Some(entry) if entry.next_hop.is_some() => {
                    let nh_ip = entry.next_hop.clone().unwrap();
                    hops.push(TraceHop {
                        node_label: node.label.clone(),
                        action: HopAction::Forward {
                            via_interface: entry.via_interface.clone().unwrap_or_default(),
                            next_hop: Some(nh_ip.clone()),
                        },
                    });
                    current_id = find_node_with_ip(graph, &nh_ip).unwrap_or(to_id);
                }
                Some(entry) => {
                    // Connected route — destination is directly reachable on this segment
                    hops.push(TraceHop {
                        node_label: node.label.clone(),
                        action: HopAction::Forward {
                            via_interface: entry.via_interface.clone().unwrap_or_default(),
                            next_hop: None,
                        },
                    });
                    current_id = to_id;
                }
            }
        }

        unreachable_result(hops, "max hops exceeded")
    }
}

// ─── CliRenderer ─────────────────────────────────────────────────────────────

/// Renders faithful Cisco IOS-style CLI output from graph and route table data.
pub struct CliRenderer;

impl CliRenderer {
    /// `RTR-CORE-01#show ip interface brief`
    pub fn show_ip_interface_brief(&self, graph: &TopologyGraph, node_id: Uuid) -> String {
        let Some(node) = graph.nodes.iter().find(|n| n.id == node_id) else {
            return "% Node not found".into();
        };

        let mut out = format!("{}#show ip interface brief\n\n", node.label);
        out.push_str(&format!(
            "{:<22} {:<16} {:<4} {:<7} {:<9} {}\n",
            "Interface", "IP-Address", "OK?", "Method", "Status", "Protocol"
        ));

        for iface in &node.interfaces {
            let ip = iface.ipv4_addr.as_deref()
                .and_then(|c| c.split('/').next().map(str::to_string))
                .unwrap_or_else(|| "unassigned".into());
            out.push_str(&format!(
                "{:<22} {:<16} {:<4} {:<7} {:<9} {}\n",
                iface.name, ip, "YES", "NVRAM", "up", "up"
            ));
        }

        out
    }

    /// `RTR-CORE-01#show ip route`
    pub fn show_ip_route(
        &self,
        graph: &TopologyGraph,
        node_id: Uuid,
        tables: &[RouteTable],
    ) -> String {
        let Some(node) = graph.nodes.iter().find(|n| n.id == node_id) else {
            return "% Node not found".into();
        };
        let Some(table) = tables.iter().find(|t| t.node_id == node_id) else {
            return format!("{}#show ip route\n% Routing table not computed\n", node.label);
        };

        let mut out = format!("{}#show ip route\n\n", node.label);
        out.push_str("Codes: C - connected, S - static, O - OSPF, B - BGP\n\n");

        for entry in &table.entries {
            let line = match entry.protocol {
                RouteProtocol::Connected => format!(
                    "{}    {} is directly connected, {}\n",
                    entry.protocol.code(),
                    entry.prefix,
                    entry.via_interface.as_deref().unwrap_or("?")
                ),
                RouteProtocol::Ospf => format!(
                    "{}    {} [{}/{}] via {}, {}\n",
                    entry.protocol.code(),
                    entry.prefix,
                    entry.protocol.admin_distance(),
                    entry.metric,
                    entry.next_hop.as_deref().unwrap_or("?"),
                    entry.via_interface.as_deref().unwrap_or("?")
                ),
                RouteProtocol::Bgp => format!(
                    "{}    {} [{}/{}] via {}, {}\n",
                    entry.protocol.code(),
                    entry.prefix,
                    entry.protocol.admin_distance(),
                    entry.metric,
                    entry.next_hop.as_deref().unwrap_or("?"),
                    entry.via_interface.as_deref().unwrap_or("?")
                ),
                RouteProtocol::Static => format!(
                    "{}    {} via {}\n",
                    entry.protocol.code(),
                    entry.prefix,
                    entry.next_hop.as_deref().unwrap_or("?")
                ),
            };
            out.push_str(&line);
        }

        out
    }

    /// `RTR-CORE-01#show version`
    pub fn show_version(&self, graph: &TopologyGraph, node_id: Uuid) -> String {
        let Some(node) = graph.nodes.iter().find(|n| n.id == node_id) else {
            return "% Node not found".into();
        };
        let zone = graph.zone_for_node(node_id)
            .map(|z| z.name.as_str())
            .unwrap_or("unknown");

        format!(
            "{name} uptime is 0 days, 0 hours, 0 minutes\n\
             System returned to ROM by power-on\n\
             \n\
             Cisco IOS XE Software, Version 17.x (Labyrinth simulation)\n\
             Copyright (c) 1986-2024 by Cisco Systems, Inc.\n\
             \n\
             {name} ({role:?}, zone: {zone}) with 2048960K bytes of memory.\n\
             {iface_count} interface(s) configured.\n",
            name = node.label,
            role = node.role,
            zone = zone,
            iface_count = node.interfaces.len(),
        )
    }
}

// ─── Route calculation helpers ────────────────────────────────────────────────

fn connected_routes(node: &crate::graph::NetworkNode) -> Vec<RouteEntry> {
    node.interfaces.iter()
        .filter_map(|iface| {
            let cidr = iface.ipv4_addr.as_deref()?;
            let net: Ipv4Network = cidr.parse().ok()?;
            Some(RouteEntry {
                prefix: format!("{}/{}", net.network(), net.prefix()),
                protocol: RouteProtocol::Connected,
                metric: 0,
                next_hop: None,
                via_interface: Some(iface.name.clone()),
            })
        })
        .collect()
}

fn ospf_routes(
    node: &crate::graph::NetworkNode,
    graph: &TopologyGraph,
) -> Vec<RouteEntry> {
    let own_prefixes: HashSet<String> = connected_routes(node)
        .iter()
        .map(|r| r.prefix.clone())
        .collect();

    let mut routes = Vec::new();

    let remote_nodes: Vec<&crate::graph::NetworkNode> = graph.nodes.iter()
        .filter(|n| {
            n.id != node.id
                && matches!(n.role, NodeRole::Router | NodeRole::Switch | NodeRole::Firewall)
        })
        .collect();

    for remote in remote_nodes {
        let path = match graph.shortest_path(node.id, remote.id) {
            Some(p) if p.len() >= 2 => p,
            _ => continue,
        };

        let first_hop = path[1];
        let metric = (path.len() - 1) as u32;

        let Some((next_hop_ip, via_iface)) = find_next_hop_info(graph, node.id, first_hop) else {
            continue;
        };

        for iface in &remote.interfaces {
            let cidr = match &iface.ipv4_addr {
                Some(c) => c,
                None => continue,
            };
            let net: Ipv4Network = match cidr.parse() {
                Ok(n) => n,
                Err(_) => continue,
            };
            let prefix = format!("{}/{}", net.network(), net.prefix());
            if own_prefixes.contains(&prefix) {
                continue;
            }
            routes.push(RouteEntry {
                prefix,
                protocol: RouteProtocol::Ospf,
                metric,
                next_hop: Some(next_hop_ip.clone()),
                via_interface: Some(via_iface.clone()),
            });
        }
    }

    routes
}

fn bgp_routes(
    node: &crate::graph::NetworkNode,
    graph: &TopologyGraph,
) -> Vec<RouteEntry> {
    let own_prefixes: HashSet<String> = connected_routes(node)
        .iter()
        .map(|r| r.prefix.clone())
        .collect();

    let mut routes = Vec::new();

    // BGP: learn directly-connected neighbor subnets (one-hop eBGP peers)
    for neighbor_id in graph.neighbors(node.id) {
        let Some(neighbor) = graph.nodes.iter().find(|n| n.id == neighbor_id) else {
            continue;
        };
        let Some((next_hop_ip, via_iface)) = find_next_hop_info(graph, node.id, neighbor_id) else {
            continue;
        };

        for iface in &neighbor.interfaces {
            let cidr = match &iface.ipv4_addr { Some(c) => c, None => continue };
            let net: Ipv4Network = match cidr.parse() { Ok(n) => n, Err(_) => continue };
            let prefix = format!("{}/{}", net.network(), net.prefix());
            if own_prefixes.contains(&prefix) { continue; }
            routes.push(RouteEntry {
                prefix,
                protocol: RouteProtocol::Bgp,
                metric: 0,
                next_hop: Some(next_hop_ip.clone()),
                via_interface: Some(via_iface.clone()),
            });
        }
    }

    routes
}

/// Returns (next_hop_ip, via_interface_name) for a direct edge between `from` and `toward`.
fn find_next_hop_info(graph: &TopologyGraph, from: Uuid, toward: Uuid) -> Option<(String, String)> {
    for edge in &graph.edges {
        let (my_iface_id, their_iface_id) = if edge.from_node == from && edge.to_node == toward {
            (edge.from_iface, edge.to_iface)
        } else if edge.to_node == from && edge.from_node == toward {
            (edge.to_iface, edge.from_iface)
        } else {
            continue;
        };

        let my_iface = graph.nodes.iter()
            .find(|n| n.id == from)?
            .interfaces.iter()
            .find(|i| i.id == my_iface_id)?;

        let their_iface = graph.nodes.iter()
            .find(|n| n.id == toward)?
            .interfaces.iter()
            .find(|i| i.id == their_iface_id)?;

        let next_hop_ip = their_iface.ipv4_addr.as_deref()?
            .split('/').next()?.to_string();

        return Some((next_hop_ip, my_iface.name.clone()));
    }
    None
}

fn dedup_routes(entries: &mut Vec<RouteEntry>) {
    // Sort: connected first, then by prefix specificity descending
    entries.sort_by_key(|e| {
        (
            e.protocol.admin_distance(),
            std::cmp::Reverse(e.prefix.parse::<Ipv4Network>().map_or(0, |n| n.prefix())),
        )
    });

    // Remove exact-prefix duplicates (keep first = lowest admin distance)
    let mut seen: HashSet<String> = HashSet::new();
    entries.retain(|e| seen.insert(e.prefix.clone()));
}

// ─── Trace helpers ────────────────────────────────────────────────────────────

fn find_best_route<'a>(entries: &'a [RouteEntry], dst_ip: &str) -> Option<&'a RouteEntry> {
    let dst: Ipv4Addr = dst_ip.parse().ok()?;
    let mut matching: Vec<&RouteEntry> = entries.iter()
        .filter(|e| {
            e.prefix.parse::<Ipv4Network>()
                .is_ok_and(|n| n.contains(dst))
        })
        .collect();

    if matching.is_empty() {
        return None;
    }

    // Longest prefix match, then lowest admin distance
    matching.sort_by(|a, b| {
        let ap = a.prefix.parse::<Ipv4Network>().map_or(0, |n| n.prefix());
        let bp = b.prefix.parse::<Ipv4Network>().map_or(0, |n| n.prefix());
        bp.cmp(&ap)
            .then(a.protocol.admin_distance().cmp(&b.protocol.admin_distance()))
    });

    matching.into_iter().next()
}

fn find_node_with_ip(graph: &TopologyGraph, ip: &str) -> Option<Uuid> {
    graph.nodes.iter().find(|n| {
        n.interfaces.iter().any(|i| {
            i.ipv4_addr.as_deref()
                .and_then(|c| c.split('/').next())
                .is_some_and(|a| a == ip)
        })
    }).map(|n| n.id)
}

fn find_zone_for_ip(graph: &TopologyGraph, ip: &str) -> Option<Uuid> {
    let addr: Ipv4Addr = ip.parse().ok()?;
    graph.addressing.ipv4_allocations.iter().find_map(|alloc| {
        let net: Ipv4Network = alloc.subnet.parse().ok()?;
        if net.contains(addr) { Some(alloc.zone_id) } else { None }
    })
}

fn zone_label(graph: &TopologyGraph, id: Uuid) -> String {
    graph.zones.iter()
        .find(|z| z.id == id)
        .map(|z| z.name.clone())
        .unwrap_or_else(|| id.to_string())
}

fn unreachable_result(hops: Vec<TraceHop>, reason: &str) -> TraceResult {
    TraceResult {
        hops,
        outcome: TraceOutcome::Unreachable,
        reason: Some(reason.into()),
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
    use crate::schema::{RoutingProtocol, ZoneRole};

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn static_routing() -> RoutingIntent {
        RoutingIntent {
            protocols: vec![RoutingProtocol::Static],
            ospf: None,
            bgp: None,
        }
    }

    fn ospf_routing() -> RoutingIntent {
        RoutingIntent {
            protocols: vec![RoutingProtocol::Ospf],
            ospf: Some(crate::schema::OspfConfig {
                areas: 1,
                authentication: false,
                area_types: vec![crate::schema::AreaType::Backbone],
            }),
            bgp: None,
        }
    }

    /// Build a two-router graph connected by a single link with full addressing:
    ///
    /// RTR-A (LAN, 10.10.10.1/24) -- GE0/0 ↔ GE0/0 -- RTR-B (MGMT, 10.10.20.1/28)
    ///
    /// Zone policies: LAN→MGMT=Permit, MGMT→LAN=Deny.
    fn two_router_graph() -> (TopologyGraph, Uuid, Uuid) {
        let mut g = TopologyGraph::new("sim-test");

        let lan = SecurityZone::new("LAN", ZoneRole::Lan, 80);
        let mgmt = SecurityZone::new("MGMT", ZoneRole::Mgmt, 90);
        let lan_id = lan.id;
        let mgmt_id = mgmt.id;
        g.add_zone(lan);
        g.add_zone(mgmt);
        g.add_policy(ZonePolicy::new(lan_id, mgmt_id, PolicyAction::Permit));
        g.add_policy(ZonePolicy::new(mgmt_id, lan_id, PolicyAction::Deny));

        // RTR-A in LAN
        let mut rtr_a = NetworkNode::new("RTR-A", NodeRole::Router,
            crate::schema::Vendor::Cisco, lan_id);
        let mut a_ge0 = Interface::new("GigabitEthernet0/0", InterfaceRole::Uplink);
        a_ge0.ipv4_addr = Some("10.10.10.1/24".into());
        rtr_a.interfaces.push(a_ge0);
        let a_iface_id = rtr_a.interfaces[0].id;
        let a_id = rtr_a.id;
        g.add_node(rtr_a);

        // RTR-B in MGMT
        let mut rtr_b = NetworkNode::new("RTR-B", NodeRole::Router,
            crate::schema::Vendor::Cisco, mgmt_id);
        let mut b_ge0 = Interface::new("GigabitEthernet0/0", InterfaceRole::Uplink);
        b_ge0.ipv4_addr = Some("10.10.20.1/28".into());
        rtr_b.interfaces.push(b_ge0);
        let b_iface_id = rtr_b.interfaces[0].id;
        let b_id = rtr_b.id;
        g.add_node(rtr_b);

        g.edges.push(NetworkLink::new(a_id, a_iface_id, b_id, b_iface_id, LinkType::Ethernet));

        g.addressing.ipv4_allocations.push(Ipv4Allocation {
            zone_id: lan_id,
            subnet: "10.10.10.0/24".into(),
            purpose: "LAN".into(),
        });
        g.addressing.ipv4_allocations.push(Ipv4Allocation {
            zone_id: mgmt_id,
            subnet: "10.10.20.0/28".into(),
            purpose: "MGMT".into(),
        });

        (g, a_id, b_id)
    }

    // ── RouteCalculator ───────────────────────────────────────────────────────

    #[test]
    fn route_calculator_produces_table_per_node() {
        let (g, _, _) = two_router_graph();
        let tables = RouteCalculator.compute(&g, &static_routing());
        assert_eq!(tables.len(), 2);
    }

    #[test]
    fn connected_routes_appear_for_all_interfaces_with_ip() {
        let (g, a_id, _) = two_router_graph();
        let tables = RouteCalculator.compute(&g, &static_routing());
        let a_table = tables.iter().find(|t| t.node_id == a_id).unwrap();
        assert!(
            a_table.entries.iter().any(|e| {
                e.protocol == RouteProtocol::Connected && e.prefix == "10.10.10.0/24"
            }),
            "got: {:?}", a_table.entries
        );
    }

    #[test]
    fn ospf_routes_computed_for_remote_routers() {
        let (g, a_id, _) = two_router_graph();
        let tables = RouteCalculator.compute(&g, &ospf_routing());
        let a_table = tables.iter().find(|t| t.node_id == a_id).unwrap();
        assert!(
            a_table.entries.iter().any(|e| {
                e.protocol == RouteProtocol::Ospf && e.prefix.starts_with("10.10.20")
            }),
            "expected OSPF route to MGMT subnet, got: {:?}", a_table.entries
        );
    }

    #[test]
    fn ospf_route_has_correct_next_hop() {
        let (g, a_id, _) = two_router_graph();
        let tables = RouteCalculator.compute(&g, &ospf_routing());
        let a_table = tables.iter().find(|t| t.node_id == a_id).unwrap();
        let ospf_entry = a_table.entries.iter()
            .find(|e| e.protocol == RouteProtocol::Ospf)
            .expect("expected OSPF entry");
        // Next-hop should be RTR-B's IP (10.10.20.1)
        assert_eq!(ospf_entry.next_hop.as_deref(), Some("10.10.20.1"),
            "got: {:?}", ospf_entry);
    }

    #[test]
    fn static_routing_produces_no_ospf_routes() {
        let (g, a_id, _) = two_router_graph();
        let tables = RouteCalculator.compute(&g, &static_routing());
        let a_table = tables.iter().find(|t| t.node_id == a_id).unwrap();
        assert!(
            !a_table.entries.iter().any(|e| e.protocol == RouteProtocol::Ospf),
            "should have no OSPF routes for static intent"
        );
    }

    #[test]
    fn empty_graph_produces_no_tables() {
        let g = TopologyGraph::new("empty");
        assert!(RouteCalculator.compute(&g, &static_routing()).is_empty());
    }

    // ── AclEvaluator ─────────────────────────────────────────────────────────

    #[test]
    fn acl_same_zone_is_always_permit() {
        let (g, _, _) = two_router_graph();
        let result = AclEvaluator.evaluate(&g, "10.10.10.1", "10.10.10.2");
        assert_eq!(result, FlowDecision::Permit);
    }

    #[test]
    fn acl_permit_policy_returns_permit() {
        let (g, _, _) = two_router_graph();
        // LAN→MGMT is Permit
        let result = AclEvaluator.evaluate(&g, "10.10.10.1", "10.10.20.1");
        assert_eq!(result, FlowDecision::Permit);
    }

    #[test]
    fn acl_deny_policy_returns_deny_with_reason() {
        let (g, _, _) = two_router_graph();
        // MGMT→LAN is Deny
        let result = AclEvaluator.evaluate(&g, "10.10.20.1", "10.10.10.1");
        assert!(matches!(result, FlowDecision::Deny { .. }));
        if let FlowDecision::Deny { reason } = result {
            assert!(reason.contains("MGMT"), "reason: {reason}");
            assert!(reason.contains("LAN"), "reason: {reason}");
        }
    }

    #[test]
    fn acl_unknown_ip_returns_no_policy() {
        let (g, _, _) = two_router_graph();
        let result = AclEvaluator.evaluate(&g, "192.168.1.1", "10.10.10.1");
        assert_eq!(result, FlowDecision::NoPolicy);
    }

    // ── PathTracer ────────────────────────────────────────────────────────────

    #[test]
    fn trace_delivers_when_directly_connected() {
        let (g, a_id, b_id) = two_router_graph();
        // A and B are directly connected (one hop)
        // RTR-A has no route to RTR-B's subnet via OSPF (static routing)
        // but with OSPF it will
        let tables = RouteCalculator.compute(&g, &ospf_routing());
        let result = PathTracer.trace(&g, &tables, a_id, b_id);
        assert_eq!(
            result.outcome, TraceOutcome::Delivered,
            "hops: {:?}, reason: {:?}", result.hops, result.reason
        );
    }

    #[test]
    fn trace_blocked_at_zone_boundary_with_deny_policy() {
        // MGMT→LAN is Deny; tracing from RTR-B (MGMT) to RTR-A (LAN) should be blocked
        let (g, a_id, b_id) = two_router_graph();
        let tables = RouteCalculator.compute(&g, &ospf_routing());
        let result = PathTracer.trace(&g, &tables, b_id, a_id);
        assert_eq!(result.outcome, TraceOutcome::Blocked,
            "hops: {:?}, reason: {:?}", result.hops, result.reason);
        // The denial should name both zones
        let deny_hop = result.hops.iter().find(|h| matches!(h.action, HopAction::AclDeny { .. }));
        assert!(deny_hop.is_some(), "expected AclDeny hop");
    }

    #[test]
    fn trace_unreachable_when_no_route() {
        let (g, a_id, b_id) = two_router_graph();
        // Static routing → RTR-A has no route to RTR-B's subnet
        let tables = RouteCalculator.compute(&g, &static_routing());
        let result = PathTracer.trace(&g, &tables, a_id, b_id);
        assert_eq!(result.outcome, TraceOutcome::Unreachable,
            "reason: {:?}", result.reason);
    }

    #[test]
    fn trace_self_delivers_immediately() {
        let (g, a_id, _) = two_router_graph();
        let tables = RouteCalculator.compute(&g, &ospf_routing());
        let result = PathTracer.trace(&g, &tables, a_id, a_id);
        assert_eq!(result.outcome, TraceOutcome::Delivered);
        assert_eq!(result.hops.len(), 1);
    }

    #[test]
    fn trace_hops_contain_node_labels() {
        let (g, a_id, b_id) = two_router_graph();
        let tables = RouteCalculator.compute(&g, &ospf_routing());
        let result = PathTracer.trace(&g, &tables, a_id, b_id);
        let labels: Vec<&str> = result.hops.iter().map(|h| h.node_label.as_str()).collect();
        assert!(labels.contains(&"RTR-A"), "hops: {:?}", labels);
    }

    // ── CliRenderer ───────────────────────────────────────────────────────────

    #[test]
    fn show_ip_interface_brief_contains_header() {
        let (g, a_id, _) = two_router_graph();
        let out = CliRenderer.show_ip_interface_brief(&g, a_id);
        assert!(out.contains("RTR-A#show ip interface brief"), "got: {out}");
        assert!(out.contains("Interface"), "got: {out}");
        assert!(out.contains("IP-Address"), "got: {out}");
    }

    #[test]
    fn show_ip_interface_brief_lists_interfaces() {
        let (g, a_id, _) = two_router_graph();
        let out = CliRenderer.show_ip_interface_brief(&g, a_id);
        assert!(out.contains("GigabitEthernet0/0"), "got: {out}");
        assert!(out.contains("10.10.10.1"), "got: {out}");
    }

    #[test]
    fn show_ip_route_contains_header() {
        let (g, a_id, _) = two_router_graph();
        let tables = RouteCalculator.compute(&g, &ospf_routing());
        let out = CliRenderer.show_ip_route(&g, a_id, &tables);
        assert!(out.contains("RTR-A#show ip route"), "got: {out}");
        assert!(out.contains("Codes:"), "got: {out}");
    }

    #[test]
    fn show_ip_route_lists_connected_and_ospf() {
        let (g, a_id, _) = two_router_graph();
        let tables = RouteCalculator.compute(&g, &ospf_routing());
        let out = CliRenderer.show_ip_route(&g, a_id, &tables);
        assert!(out.contains("C    10.10.10.0/24 is directly connected"), "got: {out}");
        assert!(out.contains("O    10.10.20"), "got: {out}");
    }

    #[test]
    fn show_version_contains_hostname_and_ios_xe() {
        let (g, a_id, _) = two_router_graph();
        let out = CliRenderer.show_version(&g, a_id);
        assert!(out.contains("RTR-A"), "got: {out}");
        assert!(out.contains("Cisco IOS XE"), "got: {out}");
    }

    #[test]
    fn show_ip_interface_brief_unknown_node() {
        let g = TopologyGraph::new("empty");
        let out = CliRenderer.show_ip_interface_brief(&g, Uuid::new_v4());
        assert!(out.contains("% Node not found"), "got: {out}");
    }
}
