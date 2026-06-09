use std::net::{Ipv4Addr, Ipv6Addr};

use ipnetwork::{Ipv4Network, Ipv6Network};
use thiserror::Error;
use uuid::Uuid;

use crate::graph::{
    AddressingPlan, DesignDecision, Ipv4Allocation, Ipv6Allocation, TopologyGraph, TriggerSource,
};
use crate::schema::{AddressingMode, Ipv4Config, Ipv6Config, NetworkIntent, ZoneRole};

// ─── Errors ───────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum AddressingError {
    #[error("Address space exhausted for zone '{zone}' — adjust base_range")]
    Exhausted { zone: String },
    #[error("Invalid base range '{range}': {reason}")]
    InvalidRange { range: String, reason: String },
    #[error("Subnet conflict: {0}")]
    Conflict(String),
}

// ─── AddressingEngine ─────────────────────────────────────────────────────────

/// Deterministic IPv4 and IPv6 allocator.
///
/// Subnet sizing defaults by zone role:
///   LAN /24 | DMZ /27 | MGMT /28 | OOB /28 | WAN/Transit /30 | Guest /23
///
/// Zone processing order is deterministic: sorted by VLAN ID (zones with
/// VLANs first, in VLAN order), then alphabetically by name.
///
/// Idempotent: clears previous allocations before each run.
pub struct AddressingEngine;

impl AddressingEngine {
    /// Allocate addresses for all zones and their interfaces.
    ///
    /// Reads `intent.addressing` for base ranges and mode.
    /// Writes to `graph.addressing` and `graph.nodes[*].interfaces[*].{ipv4,ipv6}_addr`.
    pub fn allocate(
        &self,
        graph: &mut TopologyGraph,
        intent: &NetworkIntent,
    ) -> Result<(), AddressingError> {
        // Clear previous state for idempotency
        graph.addressing = AddressingPlan::default();
        for node in &mut graph.nodes {
            for iface in &mut node.interfaces {
                iface.ipv4_addr = None;
                iface.ipv6_addr = None;
            }
        }

        let mode = &intent.addressing.mode;

        if matches!(mode, AddressingMode::Ipv4Only | AddressingMode::DualStack) {
            if let Some(ipv4) = &intent.addressing.ipv4 {
                allocate_ipv4(graph, ipv4)?;
            }
        }

        if matches!(mode, AddressingMode::Ipv6Only | AddressingMode::DualStack) {
            if let Some(ipv6) = &intent.addressing.ipv6 {
                allocate_ipv6(graph, ipv6)?;
            }
        }

        Ok(())
    }
}

// ─── IPv4 allocation ──────────────────────────────────────────────────────────

fn allocate_ipv4(graph: &mut TopologyGraph, config: &Ipv4Config) -> Result<(), AddressingError> {
    let base: Ipv4Network = config.base_range.parse().map_err(|_| {
        AddressingError::InvalidRange {
            range: config.base_range.clone(),
            reason: "not a valid CIDR block".into(),
        }
    })?;

    let zone_order = sorted_zone_ids(graph);
    let mut current = u32::from(base.network());

    for zone_id in zone_order {
        let (zone_name, zone_role) = {
            let z = graph.zones.iter().find(|z| z.id == zone_id).unwrap();
            (z.name.clone(), z.role.clone())
        };

        let prefix_len = default_prefix_len(&zone_role);
        let subnet = next_aligned_v4(current, prefix_len, base).ok_or_else(|| {
            AddressingError::Exhausted { zone: zone_name.clone() }
        })?;

        graph.addressing.ipv4_allocations.push(Ipv4Allocation {
            zone_id,
            subnet: subnet.to_string(),
            purpose: format!("{zone_name} zone subnet"),
        });

        assign_ipv4_to_interfaces(graph, zone_id, &zone_name, subnet)?;

        // Advance cursor past the allocated subnet
        let broadcast = u32::from(subnet.broadcast());
        current = broadcast.checked_add(1).ok_or_else(|| {
            AddressingError::Exhausted { zone: zone_name.clone() }
        })?;
    }

    Ok(())
}

fn assign_ipv4_to_interfaces(
    graph: &mut TopologyGraph,
    zone_id: Uuid,
    zone_name: &str,
    subnet: Ipv4Network,
) -> Result<(), AddressingError> {
    let prefix_len = subnet.prefix();
    let net = u32::from(subnet.network());
    let bcast = u32::from(subnet.broadcast());

    let (first_host, last_host) = if prefix_len < 31 {
        (net + 1, bcast - 1)
    } else {
        (net, bcast) // /31 and /32: all addresses usable
    };

    let node_ids: Vec<Uuid> = graph
        .nodes
        .iter()
        .filter(|n| n.zone_id == zone_id)
        .map(|n| n.id)
        .collect();

    let mut cursor = first_host;

    for node_id in node_ids {
        if let Some(node) = graph.node_mut(node_id) {
            for iface in &mut node.interfaces {
                if iface.ipv4_addr.is_some() {
                    continue;
                }
                if cursor > last_host {
                    return Err(AddressingError::Exhausted { zone: zone_name.to_string() });
                }
                iface.ipv4_addr = Some(format!("{}/{prefix_len}", Ipv4Addr::from(cursor)));
                cursor += 1;
            }
        }
    }

    Ok(())
}

// ─── IPv6 allocation ──────────────────────────────────────────────────────────

fn allocate_ipv6(graph: &mut TopologyGraph, config: &Ipv6Config) -> Result<(), AddressingError> {
    let ula = resolve_ula(graph, config)?;
    let base = u128::from(ula.network());

    let zone_order = sorted_zone_ids(graph);

    for (idx, zone_id) in zone_order.into_iter().enumerate() {
        if idx > 0xFFFF {
            let zone_name = graph
                .zones
                .iter()
                .find(|z| z.id == zone_id)
                .map(|z| z.name.as_str())
                .unwrap_or("?")
                .to_string();
            return Err(AddressingError::Exhausted { zone: zone_name });
        }

        let (zone_name, zone_role) = {
            let z = graph.zones.iter().find(|z| z.id == zone_id).unwrap();
            (z.name.clone(), z.role.clone())
        };

        // RFC 6164: point-to-point WAN/Transit links use /127; all other zones use /64.
        let prefix_len: u8 = match zone_role {
            ZoneRole::Wan | ZoneRole::Transit => 127,
            _ => 64,
        };

        // Carve a subnet from the /48: set the 4th group (bits 79–64) to `idx`
        let subnet_addr = Ipv6Addr::from(base | ((idx as u128) << 64));
        let prefix = Ipv6Network::new(subnet_addr, prefix_len).map_err(|e| {
            AddressingError::InvalidRange { range: ula.to_string(), reason: e.to_string() }
        })?;

        graph.addressing.ipv6_allocations.push(Ipv6Allocation {
            zone_id,
            prefix: prefix.to_string(),
            purpose: format!("{zone_name} zone /{prefix_len}"),
        });

        assign_ipv6_to_interfaces(graph, zone_id, prefix);
    }

    Ok(())
}

fn resolve_ula(
    graph: &mut TopologyGraph,
    config: &Ipv6Config,
) -> Result<Ipv6Network, AddressingError> {
    if let Some(ref prefix_str) = config.ula_prefix {
        prefix_str.parse::<Ipv6Network>().map_err(|_| AddressingError::InvalidRange {
            range: prefix_str.clone(),
            reason: "not a valid IPv6 CIDR prefix".into(),
        })
    } else {
        let generated = generate_ula_prefix(&graph.metadata.name);
        graph.log_decision(DesignDecision::new(
            "addressing",
            format!(
                "Auto-generated ULA prefix {generated} for network '{}' (none provided in intent)",
                graph.metadata.name
            ),
            TriggerSource::RuleEngine,
            false,
        ));
        Ok(generated)
    }
}

fn assign_ipv6_to_interfaces(graph: &mut TopologyGraph, zone_id: Uuid, prefix: Ipv6Network) {
    let node_ids: Vec<Uuid> = graph
        .nodes
        .iter()
        .filter(|n| n.zone_id == zone_id)
        .map(|n| n.id)
        .collect();

    let base = u128::from(prefix.network());
    let mut iface_id: u128 = 1; // ::1, ::2, … within the /64

    for node_id in node_ids {
        if let Some(node) = graph.node_mut(node_id) {
            for iface in &mut node.interfaces {
                if iface.ipv6_addr.is_some() {
                    continue;
                }
                iface.ipv6_addr = Some(format!("{}/{}", Ipv6Addr::from(base | iface_id), prefix.prefix()));
                iface_id += 1;
            }
        }
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Zone IDs in deterministic order: by VLAN (zones without VLAN sort last),
/// then alphabetically by name within the same VLAN bucket.
fn sorted_zone_ids(graph: &TopologyGraph) -> Vec<Uuid> {
    let mut zones: Vec<(Uuid, Option<u16>, String)> = graph
        .zones
        .iter()
        .map(|z| (z.id, z.vlan, z.name.clone()))
        .collect();
    zones.sort_by_key(|(_, vlan, name)| (vlan.unwrap_or(u16::MAX), name.clone()));
    zones.into_iter().map(|(id, _, _)| id).collect()
}

/// Default IPv4 prefix length for each zone role.
fn default_prefix_len(role: &ZoneRole) -> u8 {
    match role {
        ZoneRole::Guest => 23,
        ZoneRole::Lan => 24,
        ZoneRole::Dmz => 27,
        ZoneRole::Mgmt | ZoneRole::Oob => 28,
        ZoneRole::Wan | ZoneRole::Transit => 30,
    }
}

/// Find the smallest aligned IPv4 subnet of `prefix_len` whose network address
/// is >= `current`, and that fits entirely within `base`.
fn next_aligned_v4(current: u32, prefix_len: u8, base: Ipv4Network) -> Option<Ipv4Network> {
    let subnet_size = 1u32 << (32 - prefix_len);
    let mask = subnet_size - 1;

    // Round `current` up to the nearest alignment boundary
    let aligned = if current & mask == 0 { current } else { (current | mask).checked_add(1)? };

    let subnet_end = aligned.checked_add(subnet_size - 1)?;
    let base_net = u32::from(base.network());
    let base_bcast = u32::from(base.broadcast());

    if aligned < base_net || subnet_end > base_bcast {
        return None;
    }

    Ipv4Network::new(Ipv4Addr::from(aligned), prefix_len).ok()
}

/// Generate a deterministic ULA /48 prefix from a network name using FNV-1a.
/// The result is in fd00::/8 with a 40-bit Global ID derived from the name hash.
fn generate_ula_prefix(name: &str) -> Ipv6Network {
    let hash = name.bytes().fold(0xcbf29ce484222325u64, |h, b| {
        h.wrapping_mul(0x100000001b3) ^ b as u64
    });
    let global_id = hash & 0x00ff_ffff_ffff; // 40 bits
    // fd (bits 127–120) | global_id (bits 119–80) | zeros (bits 79–0)
    let addr = Ipv6Addr::from(0xfd00_0000_0000_0000_0000_0000_0000_0000u128
        | ((global_id as u128) << 80));
    Ipv6Network::new(addr, 48).unwrap()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Interface, InterfaceRole, NetworkNode, NodeRole, SecurityZone};
    use crate::schema::{
        AddressingMode, AddressingScheme, ConstraintsIntent, ContextIntent, Environment,
        IntentMetadata, Ipv4Config, Ipv6Config, NetworkIntent, RedundancyLevel, RoutingIntent,
        RoutingProtocol, SegmentationIntent, SecurityLevel, ServicesIntent, TopologyIntent,
        TopologyType, Vendor, ZoneIntent, ZoneRole, CURRENT_SCHEMA_VERSION,
    };

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn make_v4_intent(base: &str) -> NetworkIntent {
        make_intent(AddressingMode::Ipv4Only, Some(base), None)
    }

    fn make_v6_intent(ula: Option<&str>) -> NetworkIntent {
        make_intent(AddressingMode::Ipv6Only, None, Some(ula))
    }

    fn make_dual_intent(base: &str, ula: Option<&str>) -> NetworkIntent {
        make_intent(AddressingMode::DualStack, Some(base), Some(ula))
    }

    fn make_intent(
        mode: AddressingMode,
        v4_base: Option<&str>,
        v6_ula: Option<Option<&str>>,
    ) -> NetworkIntent {
        NetworkIntent {
            metadata: IntentMetadata {
                name: "addr-test".into(),
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
                vlan_count: 2,
                zones: vec![ZoneIntent { name: "LAN".into(), role: ZoneRole::Lan, trust_level: 80 }],
            },
            addressing: crate::schema::AddressingIntent {
                mode,
                ipv4: v4_base.map(|b| Ipv4Config {
                    base_range: b.into(),
                    scheme: AddressingScheme::CidrOptimized,
                }),
                ipv6: v6_ula.map(|ula| Ipv6Config {
                    ula_prefix: ula.map(str::to_owned),
                    gua_prefix: None,
                    link_local: true,
                }),
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

    fn graph_with_zones(roles: &[(&str, ZoneRole)]) -> TopologyGraph {
        let mut g = TopologyGraph::new("addr-test");
        for (name, role) in roles {
            g.add_zone(SecurityZone::new(*name, role.clone(), 50));
        }
        g
    }

    fn add_node_with_ifaces(g: &mut TopologyGraph, zone_id: Uuid, count: usize) -> Uuid {
        let mut node = NetworkNode::new("RTR-01", NodeRole::Router, Vendor::Cisco, zone_id);
        for i in 0..count {
            node.interfaces.push(Interface::new(format!("Gi0/{i}"), InterfaceRole::Access));
        }
        let id = node.id;
        g.add_node(node);
        id
    }

    fn subnet_for_zone(g: &TopologyGraph, zone_id: Uuid) -> Option<String> {
        g.addressing
            .ipv4_allocations
            .iter()
            .find(|a| a.zone_id == zone_id)
            .map(|a| a.subnet.clone())
    }

    fn prefix_for_zone_v6(g: &TopologyGraph, zone_id: Uuid) -> Option<String> {
        g.addressing
            .ipv6_allocations
            .iter()
            .find(|a| a.zone_id == zone_id)
            .map(|a| a.prefix.clone())
    }

    // ── IPv4: basic allocation ────────────────────────────────────────────────

    #[test]
    fn all_zones_get_ipv4_allocations() {
        let mut g = graph_with_zones(&[
            ("LAN", ZoneRole::Lan),
            ("MGMT", ZoneRole::Mgmt),
            ("DMZ", ZoneRole::Dmz),
        ]);
        let intent = make_v4_intent("10.0.0.0/8");
        AddressingEngine.allocate(&mut g, &intent).unwrap();
        assert_eq!(g.addressing.ipv4_allocations.len(), 3);
    }

    #[test]
    fn correct_prefix_lengths_per_role() {
        let mut g = graph_with_zones(&[
            ("LAN", ZoneRole::Lan),
            ("MGMT", ZoneRole::Mgmt),
            ("DMZ", ZoneRole::Dmz),
            ("Guest", ZoneRole::Guest),
            ("WAN", ZoneRole::Wan),
        ]);
        let zone_ids: Vec<(Uuid, ZoneRole)> = g
            .zones
            .iter()
            .map(|z| (z.id, z.role.clone()))
            .collect();
        let intent = make_v4_intent("10.0.0.0/8");
        AddressingEngine.allocate(&mut g, &intent).unwrap();

        for (zone_id, role) in zone_ids {
            let subnet: Ipv4Network = subnet_for_zone(&g, zone_id).unwrap().parse().unwrap();
            assert_eq!(subnet.prefix(), default_prefix_len(&role),
                "wrong prefix for {:?}", role);
        }
    }

    #[test]
    fn subnets_are_non_overlapping() {
        let mut g = graph_with_zones(&[
            ("LAN", ZoneRole::Lan),
            ("MGMT", ZoneRole::Mgmt),
            ("DMZ", ZoneRole::Dmz),
            ("Guest", ZoneRole::Guest),
        ]);
        let intent = make_v4_intent("10.0.0.0/8");
        AddressingEngine.allocate(&mut g, &intent).unwrap();

        let nets: Vec<Ipv4Network> = g
            .addressing
            .ipv4_allocations
            .iter()
            .map(|a| a.subnet.parse().unwrap())
            .collect();

        for i in 0..nets.len() {
            for j in (i + 1)..nets.len() {
                assert!(
                    !nets[i].contains(nets[j].network()) && !nets[j].contains(nets[i].network()),
                    "subnets overlap: {} and {}",
                    nets[i],
                    nets[j]
                );
            }
        }
    }

    #[test]
    fn all_subnets_within_base_range() {
        let mut g = graph_with_zones(&[
            ("LAN", ZoneRole::Lan),
            ("MGMT", ZoneRole::Mgmt),
            ("DMZ", ZoneRole::Dmz),
        ]);
        let base: Ipv4Network = "192.168.0.0/16".parse().unwrap();
        let intent = make_v4_intent("192.168.0.0/16");
        AddressingEngine.allocate(&mut g, &intent).unwrap();

        for alloc in &g.addressing.ipv4_allocations {
            let subnet: Ipv4Network = alloc.subnet.parse().unwrap();
            assert!(base.contains(subnet.network()), "{} not within {}", subnet, base);
            assert!(base.contains(subnet.broadcast()), "{} broadcast outside {}", subnet, base);
        }
    }

    #[test]
    fn interfaces_get_ips_within_zone_subnet() {
        let mut g = graph_with_zones(&[("LAN", ZoneRole::Lan)]);
        let lan_id = g.zones[0].id;
        add_node_with_ifaces(&mut g, lan_id, 3);

        let intent = make_v4_intent("10.0.0.0/8");
        AddressingEngine.allocate(&mut g, &intent).unwrap();

        let subnet: Ipv4Network = subnet_for_zone(&g, lan_id).unwrap().parse().unwrap();
        for node in &g.nodes {
            for iface in &node.interfaces {
                let addr: Ipv4Addr = iface
                    .ipv4_addr
                    .as_ref()
                    .unwrap()
                    .split('/')
                    .next()
                    .unwrap()
                    .parse()
                    .unwrap();
                assert!(subnet.contains(addr), "{addr} not in {subnet}");
            }
        }
    }

    #[test]
    fn interfaces_in_different_zones_get_different_subnets() {
        let mut g = graph_with_zones(&[("LAN", ZoneRole::Lan), ("MGMT", ZoneRole::Mgmt)]);
        let lan_id = g.zones.iter().find(|z| z.role == ZoneRole::Lan).unwrap().id;
        let mgmt_id = g.zones.iter().find(|z| z.role == ZoneRole::Mgmt).unwrap().id;
        add_node_with_ifaces(&mut g, lan_id, 2);
        add_node_with_ifaces(&mut g, mgmt_id, 2);

        let intent = make_v4_intent("10.0.0.0/8");
        AddressingEngine.allocate(&mut g, &intent).unwrap();

        let lan_net: Ipv4Network = subnet_for_zone(&g, lan_id).unwrap().parse().unwrap();
        let mgmt_net: Ipv4Network = subnet_for_zone(&g, mgmt_id).unwrap().parse().unwrap();
        assert_ne!(lan_net, mgmt_net);

        for node in g.nodes.iter().filter(|n| n.zone_id == mgmt_id) {
            for iface in &node.interfaces {
                let addr: Ipv4Addr = iface
                    .ipv4_addr
                    .as_ref()
                    .unwrap()
                    .split('/')
                    .next()
                    .unwrap()
                    .parse()
                    .unwrap();
                assert!(mgmt_net.contains(addr), "{addr} should be in MGMT subnet {mgmt_net}");
                assert!(!lan_net.contains(addr), "{addr} should NOT be in LAN subnet {lan_net}");
            }
        }
    }

    #[test]
    fn allocation_is_deterministic() {
        let mut g1 = graph_with_zones(&[("LAN", ZoneRole::Lan), ("DMZ", ZoneRole::Dmz)]);
        let mut g2 = graph_with_zones(&[("LAN", ZoneRole::Lan), ("DMZ", ZoneRole::Dmz)]);
        // Override IDs to ensure graphs are identical
        g2.zones[0].id = g1.zones[0].id;
        g2.zones[1].id = g1.zones[1].id;

        let intent = make_v4_intent("10.0.0.0/8");
        AddressingEngine.allocate(&mut g1, &intent).unwrap();
        AddressingEngine.allocate(&mut g2, &intent).unwrap();

        for (a1, a2) in g1.addressing.ipv4_allocations.iter()
            .zip(g2.addressing.ipv4_allocations.iter())
        {
            assert_eq!(a1.subnet, a2.subnet);
        }
    }

    #[test]
    fn allocation_is_idempotent() {
        let mut g = graph_with_zones(&[("LAN", ZoneRole::Lan)]);
        let intent = make_v4_intent("10.0.0.0/8");
        AddressingEngine.allocate(&mut g, &intent).unwrap();
        let first_subnet = subnet_for_zone(&g, g.zones[0].id).unwrap();
        AddressingEngine.allocate(&mut g, &intent).unwrap();
        let second_subnet = subnet_for_zone(&g, g.zones[0].id).unwrap();
        assert_eq!(first_subnet, second_subnet, "second run must produce same result");
        assert_eq!(g.addressing.ipv4_allocations.len(), 1, "no duplicates after second run");
    }

    // ── IPv4: error cases ─────────────────────────────────────────────────────

    #[test]
    fn invalid_base_range_returns_error() {
        let mut g = graph_with_zones(&[("LAN", ZoneRole::Lan)]);
        let mut intent = make_v4_intent("10.0.0.0/8");
        intent.addressing.ipv4.as_mut().unwrap().base_range = "not-a-cidr".into();
        assert!(matches!(
            AddressingEngine.allocate(&mut g, &intent),
            Err(AddressingError::InvalidRange { .. })
        ));
    }

    #[test]
    fn base_range_too_small_returns_exhausted() {
        // /30 base only has 4 addresses — can't fit a /24 LAN
        let mut g = graph_with_zones(&[("LAN", ZoneRole::Lan)]);
        let intent = make_v4_intent("10.0.0.0/30");
        assert!(matches!(
            AddressingEngine.allocate(&mut g, &intent),
            Err(AddressingError::Exhausted { .. })
        ));
    }

    // ── IPv4: mode gating ─────────────────────────────────────────────────────

    #[test]
    fn ipv4_only_mode_produces_no_ipv6_allocations() {
        let mut g = graph_with_zones(&[("LAN", ZoneRole::Lan)]);
        let intent = make_v4_intent("10.0.0.0/8");
        AddressingEngine.allocate(&mut g, &intent).unwrap();
        assert!(g.addressing.ipv6_allocations.is_empty());
    }

    #[test]
    fn ipv6_only_mode_produces_no_ipv4_allocations() {
        let mut g = graph_with_zones(&[("LAN", ZoneRole::Lan)]);
        let intent = make_v6_intent(Some("fd12:3456:789a::/48"));
        AddressingEngine.allocate(&mut g, &intent).unwrap();
        assert!(g.addressing.ipv4_allocations.is_empty());
    }

    // ── IPv6: allocation ──────────────────────────────────────────────────────

    #[test]
    fn all_zones_get_ipv6_prefixes() {
        let mut g = graph_with_zones(&[
            ("LAN", ZoneRole::Lan),
            ("MGMT", ZoneRole::Mgmt),
            ("DMZ", ZoneRole::Dmz),
        ]);
        let intent = make_v6_intent(Some("fd12:3456:789a::/48"));
        AddressingEngine.allocate(&mut g, &intent).unwrap();
        assert_eq!(g.addressing.ipv6_allocations.len(), 3);
    }

    #[test]
    fn ipv6_zone_prefixes_are_slash64() {
        let mut g = graph_with_zones(&[("LAN", ZoneRole::Lan), ("MGMT", ZoneRole::Mgmt)]);
        let intent = make_v6_intent(Some("fd12:3456:789a::/48"));
        AddressingEngine.allocate(&mut g, &intent).unwrap();
        for alloc in &g.addressing.ipv6_allocations {
            let net: Ipv6Network = alloc.prefix.parse().unwrap();
            assert_eq!(net.prefix(), 64);
        }
    }

    #[test]
    fn wan_transit_zones_get_slash127_per_rfc6164() {
        let mut g = graph_with_zones(&[
            ("LAN", ZoneRole::Lan),
            ("WAN", ZoneRole::Wan),
            ("TRANSIT", ZoneRole::Transit),
        ]);
        let intent = make_v6_intent(Some("fd12:3456:789a::/48"));
        AddressingEngine.allocate(&mut g, &intent).unwrap();

        for alloc in &g.addressing.ipv6_allocations {
            let net: Ipv6Network = alloc.prefix.parse().unwrap();
            let zone = g.zones.iter().find(|z| z.id == alloc.zone_id).unwrap();
            match zone.role {
                ZoneRole::Wan | ZoneRole::Transit => {
                    assert_eq!(net.prefix(), 127, "WAN/Transit zone '{}' must get /127", zone.name);
                }
                _ => {
                    assert_eq!(net.prefix(), 64, "non-WAN zone '{}' must get /64", zone.name);
                }
            }
        }
    }

    #[test]
    fn ipv6_uses_explicit_ula_prefix() {
        let ula = "fd00:dead:beef::/48";
        let mut g = graph_with_zones(&[("LAN", ZoneRole::Lan)]);
        let intent = make_v6_intent(Some(ula));
        AddressingEngine.allocate(&mut g, &intent).unwrap();
        let prefix = prefix_for_zone_v6(&g, g.zones[0].id).unwrap();
        assert!(
            prefix.starts_with("fd00:dead:beef:"),
            "prefix {prefix} should be from {ula}"
        );
    }

    #[test]
    fn ipv6_auto_generates_ula_when_not_provided() {
        let mut g = graph_with_zones(&[("LAN", ZoneRole::Lan)]);
        let intent = make_v6_intent(None);
        AddressingEngine.allocate(&mut g, &intent).unwrap();
        assert!(!g.addressing.ipv6_allocations.is_empty());
        // Auto-generated ULA must start with fd
        let prefix = prefix_for_zone_v6(&g, g.zones[0].id).unwrap();
        assert!(prefix.starts_with("fd"), "auto-generated ULA must start with fd, got {prefix}");
    }

    #[test]
    fn auto_generated_ula_is_deterministic() {
        let intent = make_v6_intent(None);
        let mut g1 = graph_with_zones(&[("LAN", ZoneRole::Lan)]);
        let mut g2 = graph_with_zones(&[("LAN", ZoneRole::Lan)]);
        g2.zones[0].id = g1.zones[0].id;
        AddressingEngine.allocate(&mut g1, &intent).unwrap();
        AddressingEngine.allocate(&mut g2, &intent).unwrap();
        assert_eq!(
            prefix_for_zone_v6(&g1, g1.zones[0].id),
            prefix_for_zone_v6(&g2, g2.zones[0].id)
        );
    }

    #[test]
    fn auto_generated_ula_logs_decision() {
        let mut g = graph_with_zones(&[("LAN", ZoneRole::Lan)]);
        let intent = make_v6_intent(None);
        AddressingEngine.allocate(&mut g, &intent).unwrap();
        assert!(
            g.decisions.iter().any(|d| d.component == "addressing"),
            "auto-generated ULA must be logged in decision log"
        );
    }

    #[test]
    fn ipv6_interfaces_get_addresses_within_zone_prefix() {
        let mut g = graph_with_zones(&[("LAN", ZoneRole::Lan)]);
        let lan_id = g.zones[0].id;
        add_node_with_ifaces(&mut g, lan_id, 2);

        let intent = make_v6_intent(Some("fd12:3456:789a::/48"));
        AddressingEngine.allocate(&mut g, &intent).unwrap();

        let prefix: Ipv6Network = prefix_for_zone_v6(&g, lan_id).unwrap().parse().unwrap();
        for node in &g.nodes {
            for iface in &node.interfaces {
                let addr: Ipv6Addr = iface
                    .ipv6_addr
                    .as_ref()
                    .unwrap()
                    .split('/')
                    .next()
                    .unwrap()
                    .parse()
                    .unwrap();
                assert!(prefix.contains(addr), "{addr} not in {prefix}");
            }
        }
    }

    // ── DualStack ─────────────────────────────────────────────────────────────

    #[test]
    fn dualstack_produces_both_v4_and_v6_allocations() {
        let mut g = graph_with_zones(&[("LAN", ZoneRole::Lan), ("MGMT", ZoneRole::Mgmt)]);
        let intent = make_dual_intent("10.0.0.0/8", Some("fd12:3456:789a::/48"));
        AddressingEngine.allocate(&mut g, &intent).unwrap();
        assert_eq!(g.addressing.ipv4_allocations.len(), 2);
        assert_eq!(g.addressing.ipv6_allocations.len(), 2);
    }

    #[test]
    fn dualstack_assigns_both_addr_types_to_interfaces() {
        let mut g = graph_with_zones(&[("LAN", ZoneRole::Lan)]);
        let lan_id = g.zones[0].id;
        add_node_with_ifaces(&mut g, lan_id, 1);

        let intent = make_dual_intent("10.0.0.0/8", Some("fd12:3456:789a::/48"));
        AddressingEngine.allocate(&mut g, &intent).unwrap();

        let iface = &g.nodes[0].interfaces[0];
        assert!(iface.ipv4_addr.is_some(), "interface must have IPv4 address");
        assert!(iface.ipv6_addr.is_some(), "interface must have IPv6 address");
    }
}
