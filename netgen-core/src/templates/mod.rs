pub mod cisco;
pub mod juniper;

use std::net::Ipv4Addr;

use ipnetwork::Ipv4Network;
use thiserror::Error;
use uuid::Uuid;

use crate::graph::{
    AddressingPlan, Interface, InterfaceRole, NetworkNode, NodeRole, SecurityZone, TopologyGraph,
    ZonePolicy,
};
use crate::schema::{RoutingIntent, RoutingProtocol, SecurityLevel, Vendor};

/// Returns the correct vendor adapter based on the first entry in `vendor_preference`.
/// Falls back to Cisco IOS if the list is empty or the vendor has no adapter yet.
pub fn select_adapter(vendor_preference: &[Vendor]) -> Box<dyn VendorAdapter> {
    match vendor_preference.first() {
        Some(Vendor::Juniper) => Box::new(juniper::junos::JuniperJunOsAdapter),
        _ => Box::new(cisco::ios::CiscoIosAdapter),
    }
}

// ─── Context structs (data passed into adapter render methods) ────────────────

/// Routing data assembled by ConfigGenerator for a single router node.
pub struct RoutingContext<'a> {
    pub node: &'a NetworkNode,
    pub routing: &'a RoutingIntent,
    /// Loopback IP without prefix (used as router-id), e.g. `"10.255.255.1"`.
    pub router_id: Option<String>,
    /// (network_addr, wildcard_mask, area) tuples for OSPF network statements.
    pub ospf_networks: Vec<(String, String, u32)>,
}

/// ACL data assembled for one ZonePolicy.
pub struct AclContext<'a> {
    pub policy: &'a ZonePolicy,
    pub from_zone: &'a SecurityZone,
    pub to_zone: &'a SecurityZone,
    /// IPv4 subnet for the source zone (CIDR notation), if available.
    pub from_subnet: Option<String>,
    /// IPv4 subnet for the destination zone (CIDR notation), if available.
    pub to_subnet: Option<String>,
}

// ─── VendorAdapter trait ──────────────────────────────────────────────────────

/// All vendor config rendering goes through this trait.
/// No vendor-specific logic leaks into the engine — adapters are the only
/// place where vendor syntax is known.
pub trait VendorAdapter {
    /// Renders the base section: hostname, domain, banner.
    fn render_base(&self, hostname: &str) -> String;
    /// Renders config for a single interface.
    fn render_interface(&self, iface: &Interface) -> String;
    /// Renders the routing protocol section for a router node.
    fn render_routing(&self, ctx: &RoutingContext<'_>) -> String;
    /// Renders one ACL entry derived from a zone-to-zone policy.
    fn render_acl(&self, ctx: &AclContext<'_>) -> String;
    /// Renders the security hardening baseline (SSH, SNMPv3, no telnet).
    fn render_hardening(&self, level: &SecurityLevel) -> String;
    /// Renders VLAN database entry and SVI config for a switch zone.
    fn render_vlan(&self, zone: &SecurityZone, svi_subnet: Option<&str>) -> String;
    /// Returns the canonical interface name for a given role and index.
    fn interface_name(&self, role: &InterfaceRole, index: u8) -> String;
    /// GNS3 appliance template ID for this vendor platform.
    fn appliance_id(&self) -> &str;
}

// ─── ConfigObject — traceability record ──────────────────────────────────────

/// A single rendered config fragment with a full audit trail.
#[derive(Debug, Clone)]
pub struct ConfigObject {
    /// Rendered IOS/JunOS/etc. config text.
    pub content: String,
    /// Adapter that produced this (e.g. `"cisco-iosv"`).
    pub generated_by: String,
    /// Capability being configured (e.g. `"interfaces"`, `"ospf"`, `"acl"`).
    pub capability: String,
    /// Graph node this config belongs to.
    pub graph_object: Uuid,
    /// Security policy that required this config fragment (ACLs only).
    pub policy_ref: Option<String>,
}

// ─── ConfigError ─────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Template render error for node '{node}' section '{section}': {reason}")]
    RenderFailed { node: String, section: String, reason: String },
    #[error("No vendor adapter available for node '{node}' (vendor: {vendor})")]
    NoAdapter { node: String, vendor: String },
}

// ─── ConfigGenerator ─────────────────────────────────────────────────────────

/// Drives per-device config generation for every node in the topology.
///
/// Walk order: for each node, render base → interfaces → routing → ACLs → VLANs → hardening.
/// Each fragment becomes a `ConfigObject` with full traceability metadata.
pub struct ConfigGenerator;

impl ConfigGenerator {
    /// Generate config fragments for every node in `graph`.
    ///
    /// Returns all `ConfigObject`s in node-then-section order. The caller is
    /// responsible for stitching per-node fragments into a final device config.
    pub fn generate(
        &self,
        graph: &TopologyGraph,
        routing: &RoutingIntent,
        security_level: &SecurityLevel,
        adapter: &dyn VendorAdapter,
    ) -> Result<Vec<ConfigObject>, ConfigError> {
        let mut objects: Vec<ConfigObject> = Vec::new();

        for node in &graph.nodes {
            let appliance = adapter.appliance_id().to_string();

            // 1. Base
            objects.push(ConfigObject {
                content: adapter.render_base(&node.label),
                generated_by: appliance.clone(),
                capability: "base".into(),
                graph_object: node.id,
                policy_ref: None,
            });

            // 2. Interfaces
            for iface in &node.interfaces {
                objects.push(ConfigObject {
                    content: adapter.render_interface(iface),
                    generated_by: appliance.clone(),
                    capability: "interfaces".into(),
                    graph_object: node.id,
                    policy_ref: None,
                });
            }

            // 3. Routing (routers only)
            if node.role == NodeRole::Router {
                let ctx = build_routing_ctx(node, routing, graph);
                objects.push(ConfigObject {
                    content: adapter.render_routing(&ctx),
                    generated_by: appliance.clone(),
                    capability: routing_capability(routing),
                    graph_object: node.id,
                    policy_ref: None,
                });
            }

            // 4. ACLs — emit one ACL per policy that touches this node's zone
            if let Some(zone) = graph.zone_for_node(node.id) {
                for policy in graph.policies.iter().filter(|p| {
                    p.from_zone == zone.id || p.to_zone == zone.id
                }) {
                    let from_zone = graph.zones.iter().find(|z| z.id == policy.from_zone);
                    let to_zone = graph.zones.iter().find(|z| z.id == policy.to_zone);
                    if let (Some(fz), Some(tz)) = (from_zone, to_zone) {
                        let acl_ctx = AclContext {
                            policy,
                            from_zone: fz,
                            to_zone: tz,
                            from_subnet: zone_subnet(&graph.addressing, fz.id),
                            to_subnet: zone_subnet(&graph.addressing, tz.id),
                        };
                        let policy_label = format!(
                            "{}-to-{}",
                            fz.name.to_uppercase(),
                            tz.name.to_uppercase()
                        );
                        objects.push(ConfigObject {
                            content: adapter.render_acl(&acl_ctx),
                            generated_by: appliance.clone(),
                            capability: "acl".into(),
                            graph_object: node.id,
                            policy_ref: Some(policy_label),
                        });
                    }
                }

                // 5. VLANs (switches only)
                if node.role == NodeRole::Switch {
                    let svi = zone_subnet(&graph.addressing, zone.id);
                    objects.push(ConfigObject {
                        content: adapter.render_vlan(zone, svi.as_deref()),
                        generated_by: appliance.clone(),
                        capability: "vlan".into(),
                        graph_object: node.id,
                        policy_ref: None,
                    });
                }
            }

            // 6. Hardening (all devices)
            objects.push(ConfigObject {
                content: adapter.render_hardening(security_level),
                generated_by: appliance.clone(),
                capability: "hardening".into(),
                graph_object: node.id,
                policy_ref: None,
            });
        }

        Ok(objects)
    }

    /// Stitches all `ConfigObject`s for a given node into a single device config string.
    pub fn stitch_device_config(objects: &[ConfigObject], node_id: Uuid) -> String {
        objects
            .iter()
            .filter(|o| o.graph_object == node_id)
            .map(|o| o.content.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

fn build_routing_ctx<'a>(
    node: &'a NetworkNode,
    routing: &'a RoutingIntent,
    _graph: &'a TopologyGraph,
) -> RoutingContext<'a> {
    let router_id = node.interfaces.iter()
        .find(|i| i.role == InterfaceRole::Loopback)
        .and_then(|i| i.ipv4_addr.as_deref())
        .and_then(|cidr| cidr.split('/').next().map(str::to_string));

    // Build OSPF network statements from all interfaces with IPv4 addresses
    let ospf_networks = node.interfaces.iter()
        .filter_map(|iface| {
            let cidr = iface.ipv4_addr.as_deref()?;
            let (net_addr, wildcard) = cidr_to_network_wildcard(cidr)?;
            Some((net_addr, wildcard, 0u32)) // area 0 for Phase 7
        })
        .collect();

    RoutingContext { node, routing, router_id, ospf_networks }
}

fn routing_capability(routing: &RoutingIntent) -> String {
    let has_ospf = routing.protocols.contains(&RoutingProtocol::Ospf);
    let has_bgp = routing.protocols.contains(&RoutingProtocol::Bgp);
    match (has_ospf, has_bgp) {
        (true, true) => "ospf+bgp".into(),
        (true, false) => "ospf".into(),
        (false, true) => "bgp".into(),
        _ => "static".into(),
    }
}

fn zone_subnet(plan: &AddressingPlan, zone_id: Uuid) -> Option<String> {
    plan.ipv4_allocations
        .iter()
        .find(|a| a.zone_id == zone_id)
        .map(|a| a.subnet.clone())
}

/// Converts a CIDR address (e.g. `"10.0.1.1/24"`) into (network_addr, wildcard_mask).
pub fn cidr_to_network_wildcard(cidr: &str) -> Option<(String, String)> {
    let net: Ipv4Network = cidr.parse().ok()?;
    let wildcard = u32::from(net.mask()) ^ u32::MAX;
    Some((net.network().to_string(), Ipv4Addr::from(wildcard).to_string()))
}

/// Splits a CIDR address (e.g. `"10.0.1.1/24"`) into (host_ip, subnet_mask).
pub fn cidr_to_ip_mask(cidr: &str) -> Option<(String, String)> {
    let net: Ipv4Network = cidr.parse().ok()?;
    Some((net.ip().to_string(), net.mask().to_string()))
}
