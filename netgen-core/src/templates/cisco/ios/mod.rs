use minijinja::{Environment, Value, context};

use crate::graph::{Interface, InterfaceRole, PolicyAction, SecurityZone};
use crate::schema::SecurityLevel;
use crate::templates::{AclContext, RoutingContext, VendorAdapter, cidr_to_ip_mask, cidr_to_network_wildcard};
use crate::schema::RoutingProtocol;

// Template sources embedded at compile time
const BASE_TMPL: &str      = include_str!("base.j2");
const IFACE_TMPL: &str     = include_str!("interfaces.j2");
const OSPF_TMPL: &str      = include_str!("ospf.j2");
const BGP_TMPL: &str       = include_str!("bgp.j2");
const ACL_TMPL: &str       = include_str!("acl.j2");
const HARDENING_TMPL: &str = include_str!("hardening.j2");
const VLAN_TMPL: &str      = include_str!("vlan.j2");

fn render(template_src: &str, ctx: Value) -> String {
    let mut env = Environment::new();
    env.add_template("t", template_src).expect("static template is always valid");
    let tmpl = env.get_template("t").unwrap();
    tmpl.render(ctx).unwrap_or_else(|e| format!("! RENDER ERROR: {e}"))
}

// ─── CiscoIosAdapter ─────────────────────────────────────────────────────────

/// Phase 1 vendor adapter — Cisco IOS / IOS-XE.
/// All config sections are rendered via minijinja templates embedded at compile time.
pub struct CiscoIosAdapter;

impl VendorAdapter for CiscoIosAdapter {
    fn render_base(&self, hostname: &str) -> String {
        render(BASE_TMPL, context!(hostname => hostname))
    }

    fn render_interface(&self, iface: &Interface) -> String {
        let is_loopback = iface.role == InterfaceRole::Loopback;
        let description = match &iface.role {
            InterfaceRole::Uplink   => "uplink",
            InterfaceRole::Access   => "access",
            InterfaceRole::Transit  => "transit",
            InterfaceRole::Loopback => "loopback",
            InterfaceRole::Mgmt     => "management",
        };

        let (ipv4_addr, ipv4_mask) = iface.ipv4_addr.as_deref()
            .and_then(cidr_to_ip_mask)
            .map(|(ip, mask)| (Some(ip), Some(mask)))
            .unwrap_or((None, None));

        let iface_list = vec![context!(
            name        => iface.name,
            description => description,
            ipv4_addr   => ipv4_addr,
            ipv4_mask   => ipv4_mask,
            ipv6_addr   => iface.ipv6_addr.clone(),
            is_loopback => is_loopback,
        )];

        render(IFACE_TMPL, context!(interfaces => iface_list))
    }

    fn render_routing(&self, ctx: &RoutingContext<'_>) -> String {
        let mut out = String::new();

        if ctx.routing.protocols.contains(&RoutingProtocol::Ospf) {
            let authentication = ctx.routing.ospf.as_ref().is_some_and(|o| o.authentication);

            // Build active_interfaces: all non-loopback interface names
            let active: Vec<String> = ctx.node.interfaces.iter()
                .filter(|i| i.role != InterfaceRole::Loopback)
                .map(|i| i.name.clone())
                .collect();

            let networks: Vec<Value> = ctx.ospf_networks.iter()
                .map(|(net, wildcard, area)| context!(
                    network  => net,
                    wildcard => wildcard,
                    area     => area,
                ))
                .collect();

            let ospf_ctx = context!(
                process_id         => 1u32,
                router_id          => ctx.router_id.clone(),
                authentication     => authentication,
                active_interfaces  => active,
                networks           => networks,
            );
            out.push_str(&render(OSPF_TMPL, ospf_ctx));
        }

        if ctx.routing.protocols.contains(&RoutingProtocol::Bgp) {
            if let Some(bgp) = &ctx.routing.bgp {
                if bgp.enabled {
                    let local_as = bgp.local_as.unwrap_or(65000);
                    let router_id = ctx.router_id.clone().unwrap_or_else(|| "0.0.0.0".into());
                    // Neighbors are not derivable from graph in Phase 7 — placeholder row
                    let neighbors: Vec<Value> = Vec::new();
                    let bgp_ctx = context!(
                        local_as  => local_as,
                        router_id => router_id,
                        neighbors => neighbors,
                    );
                    out.push_str(&render(BGP_TMPL, bgp_ctx));
                }
            }
        }

        if out.is_empty() {
            // Static routing — no explicit protocol block needed on IOS-XE
            out.push_str("! static routing — no dynamic protocol configured\n");
        }

        out
    }

    fn render_acl(&self, ctx: &AclContext<'_>) -> String {
        let acl_name = format!(
            "ACL-{}-TO-{}",
            ctx.from_zone.name.to_uppercase(),
            ctx.to_zone.name.to_uppercase()
        );
        let action = match ctx.policy.action {
            PolicyAction::Permit  => "permit",
            PolicyAction::Deny    => "deny",
            PolicyAction::Inspect => "permit", // IOS-XE: Inspect maps to a permit + CBAC/ZBF
        };

        let (src_subnet, src_wildcard) = ctx.from_subnet.as_deref()
            .and_then(cidr_to_network_wildcard)
            .map(|(n, w)| (Some(n), Some(w)))
            .unwrap_or((None, None));

        let (dst_subnet, dst_wildcard) = ctx.to_subnet.as_deref()
            .and_then(cidr_to_network_wildcard)
            .map(|(n, w)| (Some(n), Some(w)))
            .unwrap_or((None, None));

        render(ACL_TMPL, context!(
            acl_name     => acl_name,
            action       => action,
            src_subnet   => src_subnet,
            src_wildcard => src_wildcard,
            dst_subnet   => dst_subnet,
            dst_wildcard => dst_wildcard,
        ))
    }

    fn render_hardening(&self, level: &SecurityLevel) -> String {
        let snmp_enabled = matches!(level, SecurityLevel::Standard | SecurityLevel::Hardened);
        render(HARDENING_TMPL, context!(snmp_enabled => snmp_enabled))
    }

    fn render_vlan(&self, zone: &SecurityZone, svi_subnet: Option<&str>) -> String {
        let vlan_id = zone.vlan.unwrap_or(1);
        let (svi_addr, svi_mask) = svi_subnet
            .and_then(cidr_to_ip_mask)
            .map(|(ip, mask)| (Some(ip), Some(mask)))
            .unwrap_or((None, None));

        render(VLAN_TMPL, context!(
            vlan_id   => vlan_id,
            vlan_name => zone.name.to_uppercase(),
            svi_addr  => svi_addr,
            svi_mask  => svi_mask,
        ))
    }

    fn interface_name(&self, role: &InterfaceRole, index: u8) -> String {
        match role {
            InterfaceRole::Loopback => format!("Loopback{index}"),
            _                       => format!("GigabitEthernet0/{index}"),
        }
    }

    fn appliance_id(&self) -> &str {
        "cisco-iosv"
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    use crate::graph::{
        Interface, InterfaceRole, NetworkNode, NodeRole, PolicyAction, SecurityZone, ZonePolicy,
        TopologyGraph,
    };
    use crate::schema::{
        AddressingIntent, AddressingMode, AddressingScheme, BgpConfig, BgpType, ConstraintsIntent,
        ContextIntent, Environment, IntentMetadata, Ipv4Config, NetworkIntent, OspfConfig,
        RedundancyLevel, RoutingIntent, SecurityLevel, SegmentationIntent, ServicesIntent,
        TopologyIntent, TopologyType, Vendor, ZoneIntent, ZoneRole, CURRENT_SCHEMA_VERSION,
    };
    use crate::templates::{AclContext, ConfigGenerator, RoutingContext};

    fn adapter() -> CiscoIosAdapter {
        CiscoIosAdapter
    }

    fn base_routing() -> RoutingIntent {
        RoutingIntent {
            protocols: vec![crate::schema::RoutingProtocol::Static],
            ospf: None,
            bgp: None,
        }
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
            routing: base_routing(),
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

    // ── render_base ───────────────────────────────────────────────────────────

    #[test]
    fn render_base_contains_hostname() {
        let out = adapter().render_base("RTR-CORE-01");
        assert!(out.contains("hostname RTR-CORE-01"), "got: {out}");
    }

    #[test]
    fn render_base_contains_banner() {
        let out = adapter().render_base("RTR-EDGE-01");
        assert!(out.contains("banner motd"), "got: {out}");
        assert!(out.contains("Authorized access only"), "got: {out}");
    }

    // ── render_interface ──────────────────────────────────────────────────────

    #[test]
    fn render_interface_with_ipv4() {
        let mut iface = Interface::new("GigabitEthernet0/0", InterfaceRole::Access);
        iface.ipv4_addr = Some("10.10.10.1/24".into());
        let out = adapter().render_interface(&iface);
        assert!(out.contains("GigabitEthernet0/0"), "got: {out}");
        assert!(out.contains("ip address 10.10.10.1 255.255.255.0"), "got: {out}");
        assert!(out.contains("no shutdown"), "got: {out}");
    }

    #[test]
    fn render_interface_with_ipv6() {
        let mut iface = Interface::new("GigabitEthernet0/1", InterfaceRole::Uplink);
        iface.ipv6_addr = Some("fd00::1/64".into());
        let out = adapter().render_interface(&iface);
        assert!(out.contains("ipv6 address fd00::1/64"), "got: {out}");
        assert!(out.contains("ipv6 enable"), "got: {out}");
    }

    #[test]
    fn render_interface_loopback() {
        let mut iface = Interface::new("Loopback0", InterfaceRole::Loopback);
        iface.ipv4_addr = Some("10.255.255.1/32".into());
        let out = adapter().render_interface(&iface);
        assert!(out.contains("Loopback0"), "got: {out}");
        assert!(out.contains("no ip proxy-arp"), "got: {out}");
    }

    #[test]
    fn render_interface_no_ip_is_valid() {
        let iface = Interface::new("GigabitEthernet0/2", InterfaceRole::Access);
        let out = adapter().render_interface(&iface);
        assert!(out.contains("GigabitEthernet0/2"), "got: {out}");
        assert!(out.contains("no shutdown"), "got: {out}");
    }

    // ── render_routing — OSPF ─────────────────────────────────────────────────

    #[test]
    fn render_routing_ospf_contains_router_ospf() {
        let zone_id = Uuid::new_v4();
        let node = NetworkNode::new("RTR-01", NodeRole::Router, Vendor::Cisco, zone_id);
        let routing = RoutingIntent {
            protocols: vec![crate::schema::RoutingProtocol::Ospf],
            ospf: Some(OspfConfig {
                areas: 1,
                authentication: false,
                area_types: vec![crate::schema::AreaType::Backbone],
            }),
            bgp: None,
        };
        let ctx = RoutingContext {
            node: &node,
            routing: &routing,
            router_id: Some("10.255.255.1".into()),
            ospf_networks: vec![
                ("10.10.10.0".into(), "0.0.0.255".into(), 0),
            ],
        };
        let out = adapter().render_routing(&ctx);
        assert!(out.contains("router ospf 1"), "got: {out}");
        assert!(out.contains("router-id 10.255.255.1"), "got: {out}");
        assert!(out.contains("network 10.10.10.0 0.0.0.255 area 0"), "got: {out}");
    }

    #[test]
    fn render_routing_ospf_authentication_flag() {
        let zone_id = Uuid::new_v4();
        let node = NetworkNode::new("RTR-01", NodeRole::Router, Vendor::Cisco, zone_id);
        let routing = RoutingIntent {
            protocols: vec![crate::schema::RoutingProtocol::Ospf],
            ospf: Some(OspfConfig {
                areas: 1,
                authentication: true,
                area_types: vec![crate::schema::AreaType::Backbone],
            }),
            bgp: None,
        };
        let ctx = RoutingContext {
            node: &node,
            routing: &routing,
            router_id: Some("10.0.0.1".into()),
            ospf_networks: vec![],
        };
        let out = adapter().render_routing(&ctx);
        assert!(out.contains("authentication message-digest"), "got: {out}");
    }

    #[test]
    fn render_routing_bgp_contains_router_bgp() {
        let zone_id = Uuid::new_v4();
        let node = NetworkNode::new("RTR-01", NodeRole::Router, Vendor::Cisco, zone_id);
        let routing = RoutingIntent {
            protocols: vec![crate::schema::RoutingProtocol::Bgp],
            ospf: None,
            bgp: Some(BgpConfig {
                enabled: true,
                bgp_type: BgpType::Internal,
                local_as: Some(65001),
                peer_as: None,
            }),
        };
        let ctx = RoutingContext {
            node: &node,
            routing: &routing,
            router_id: Some("10.0.0.1".into()),
            ospf_networks: vec![],
        };
        let out = adapter().render_routing(&ctx);
        assert!(out.contains("router bgp 65001"), "got: {out}");
        assert!(out.contains("bgp router-id 10.0.0.1"), "got: {out}");
    }

    #[test]
    fn render_routing_static_emits_comment() {
        let zone_id = Uuid::new_v4();
        let node = NetworkNode::new("RTR-01", NodeRole::Router, Vendor::Cisco, zone_id);
        let routing = base_routing();
        let ctx = RoutingContext {
            node: &node,
            routing: &routing,
            router_id: None,
            ospf_networks: vec![],
        };
        let out = adapter().render_routing(&ctx);
        assert!(out.contains("static routing"), "got: {out}");
    }

    // ── render_acl ────────────────────────────────────────────────────────────

    fn make_zone(name: &str, role: ZoneRole, trust: u8) -> SecurityZone {
        SecurityZone::new(name, role, trust)
    }

    #[test]
    fn render_acl_permit_with_subnets() {
        let lan = make_zone("LAN", ZoneRole::Lan, 80);
        let dmz = make_zone("DMZ", ZoneRole::Dmz, 40);
        let policy = ZonePolicy::new(lan.id, dmz.id, PolicyAction::Permit);
        let ctx = AclContext {
            policy: &policy,
            from_zone: &lan,
            to_zone: &dmz,
            from_subnet: Some("10.10.10.0/24".into()),
            to_subnet: Some("10.10.20.0/27".into()),
        };
        let out = adapter().render_acl(&ctx);
        assert!(out.contains("ip access-list extended ACL-LAN-TO-DMZ"), "got: {out}");
        assert!(out.contains("permit ip"), "got: {out}");
        assert!(out.contains("10.10.10.0 0.0.0.255"), "got: {out}");
        assert!(out.contains("10.10.20.0 0.0.0.31"), "got: {out}");
        assert!(out.contains("deny ip any any log"), "got: {out}");
    }

    #[test]
    fn render_acl_deny_policy() {
        let lan = make_zone("LAN", ZoneRole::Lan, 80);
        let guest = make_zone("GUEST", ZoneRole::Guest, 10);
        let policy = ZonePolicy::new(guest.id, lan.id, PolicyAction::Deny);
        let ctx = AclContext {
            policy: &policy,
            from_zone: &guest,
            to_zone: &lan,
            from_subnet: None,
            to_subnet: None,
        };
        let out = adapter().render_acl(&ctx);
        assert!(out.contains("ACL-GUEST-TO-LAN"), "got: {out}");
        assert!(out.contains("deny ip any any"), "got: {out}");
    }

    #[test]
    fn render_acl_no_subnets_uses_any() {
        let lan = make_zone("LAN", ZoneRole::Lan, 80);
        let mgmt = make_zone("MGMT", ZoneRole::Mgmt, 90);
        let policy = ZonePolicy::new(lan.id, mgmt.id, PolicyAction::Deny);
        let ctx = AclContext {
            policy: &policy,
            from_zone: &lan,
            to_zone: &mgmt,
            from_subnet: None,
            to_subnet: None,
        };
        let out = adapter().render_acl(&ctx);
        assert!(out.contains("deny ip any any"), "got: {out}");
    }

    // ── render_hardening ──────────────────────────────────────────────────────

    #[test]
    fn render_hardening_no_telnet() {
        let out = adapter().render_hardening(&SecurityLevel::Basic);
        assert!(out.contains("no service telnet"), "got: {out}");
    }

    #[test]
    fn render_hardening_ssh_only() {
        let out = adapter().render_hardening(&SecurityLevel::Basic);
        assert!(out.contains("transport input ssh"), "got: {out}");
        assert!(out.contains("ip ssh version 2"), "got: {out}");
    }

    #[test]
    fn render_hardening_snmp_disabled_for_basic() {
        let out = adapter().render_hardening(&SecurityLevel::Basic);
        assert!(out.contains("no snmp-server"), "got: {out}");
        assert!(!out.contains("snmp-server group"), "got: {out}");
    }

    #[test]
    fn render_hardening_snmpv3_enabled_for_standard() {
        let out = adapter().render_hardening(&SecurityLevel::Standard);
        assert!(out.contains("snmp-server group SNMPV3GROUP v3 priv"), "got: {out}");
    }

    #[test]
    fn render_hardening_service_password_encryption() {
        let out = adapter().render_hardening(&SecurityLevel::Basic);
        assert!(out.contains("service password-encryption"), "got: {out}");
    }

    // ── render_vlan ───────────────────────────────────────────────────────────

    #[test]
    fn render_vlan_with_svi() {
        let mut zone = SecurityZone::new("LAN", ZoneRole::Lan, 80);
        zone.vlan = Some(10);
        let out = adapter().render_vlan(&zone, Some("10.10.10.1/24"));
        assert!(out.contains("vlan 10"), "got: {out}");
        assert!(out.contains("name LAN"), "got: {out}");
        assert!(out.contains("interface Vlan10"), "got: {out}");
        assert!(out.contains("ip address 10.10.10.1 255.255.255.0"), "got: {out}");
    }

    #[test]
    fn render_vlan_without_svi() {
        let mut zone = SecurityZone::new("MGMT", ZoneRole::Mgmt, 90);
        zone.vlan = Some(200);
        let out = adapter().render_vlan(&zone, None);
        assert!(out.contains("vlan 200"), "got: {out}");
        assert!(!out.contains("interface Vlan"), "SVI should be absent without subnet, got: {out}");
    }

    // ── interface_name ────────────────────────────────────────────────────────

    #[test]
    fn interface_name_loopback() {
        assert_eq!(adapter().interface_name(&InterfaceRole::Loopback, 0), "Loopback0");
    }

    #[test]
    fn interface_name_gigabit() {
        assert_eq!(adapter().interface_name(&InterfaceRole::Uplink, 2), "GigabitEthernet0/2");
    }

    #[test]
    fn appliance_id_is_cisco_iosv() {
        assert_eq!(adapter().appliance_id(), "cisco-iosv");
    }

    // ── ConfigGenerator ───────────────────────────────────────────────────────

    fn build_graph_with_router() -> (TopologyGraph, Uuid) {
        use crate::graph::{Ipv4Allocation, SecurityZone};
        let mut g = TopologyGraph::new("gen-test");
        let zone = SecurityZone::new("LAN", ZoneRole::Lan, 80);
        let zone_id = zone.id;
        g.add_zone(zone);

        let mut router = NetworkNode::new("RTR-CORE-01", NodeRole::Router, Vendor::Cisco, zone_id);
        let mut lo = Interface::new("Loopback0", InterfaceRole::Loopback);
        lo.ipv4_addr = Some("10.255.255.1/32".into());
        let mut ge0 = Interface::new("GigabitEthernet0/0", InterfaceRole::Access);
        ge0.ipv4_addr = Some("10.10.10.1/24".into());
        router.interfaces.push(lo);
        router.interfaces.push(ge0);
        let node_id = router.id;
        g.add_node(router);

        g.addressing.ipv4_allocations.push(Ipv4Allocation {
            zone_id,
            subnet: "10.10.10.0/24".into(),
            purpose: "LAN zone /24".into(),
        });

        (g, node_id)
    }

    #[test]
    fn config_generator_produces_objects_for_all_sections() {
        let (graph, node_id) = build_graph_with_router();
        let intent = base_intent();
        let gen = ConfigGenerator;
        let objects = gen.generate(&graph, &intent.routing, &intent.metadata.security_level, &adapter()).unwrap();

        let caps: Vec<&str> = objects.iter()
            .filter(|o| o.graph_object == node_id)
            .map(|o| o.capability.as_str())
            .collect();

        assert!(caps.contains(&"base"), "missing base, got: {caps:?}");
        assert!(caps.contains(&"interfaces"), "missing interfaces, got: {caps:?}");
        assert!(caps.contains(&"hardening"), "missing hardening, got: {caps:?}");
    }

    #[test]
    fn config_generator_objects_carry_appliance_id() {
        let (graph, _) = build_graph_with_router();
        let intent = base_intent();
        let objects = ConfigGenerator.generate(&graph, &intent.routing, &intent.metadata.security_level, &adapter()).unwrap();
        assert!(objects.iter().all(|o| o.generated_by == "cisco-iosv"));
    }

    #[test]
    fn stitch_device_config_concatenates_sections() {
        let (graph, node_id) = build_graph_with_router();
        let intent = base_intent();
        let objects = ConfigGenerator.generate(&graph, &intent.routing, &intent.metadata.security_level, &adapter()).unwrap();
        let full = ConfigGenerator::stitch_device_config(&objects, node_id);
        assert!(full.contains("hostname RTR-CORE-01"), "stitched config: {full}");
        assert!(full.contains("no service telnet"), "stitched config: {full}");
    }

    #[test]
    fn full_stitched_config_is_deterministic() {
        let (graph, node_id) = build_graph_with_router();
        let intent = base_intent();
        let a = ConfigGenerator.generate(&graph, &intent.routing, &intent.metadata.security_level, &adapter()).unwrap();
        let b = ConfigGenerator.generate(&graph, &intent.routing, &intent.metadata.security_level, &adapter()).unwrap();
        let cfg_a = ConfigGenerator::stitch_device_config(&a, node_id);
        let cfg_b = ConfigGenerator::stitch_device_config(&b, node_id);
        assert_eq!(cfg_a, cfg_b, "config generation must be deterministic");
    }

    #[test]
    fn config_generator_ospf_section_present_when_ospf_in_intent() {
        let (graph, node_id) = build_graph_with_router();
        let mut intent = base_intent();
        intent.routing.protocols = vec![crate::schema::RoutingProtocol::Ospf];
        intent.routing.ospf = Some(OspfConfig {
            areas: 1,
            authentication: false,
            area_types: vec![crate::schema::AreaType::Backbone],
        });
        let objects = ConfigGenerator.generate(&graph, &intent.routing, &intent.metadata.security_level, &adapter()).unwrap();
        let routing_obj = objects.iter()
            .find(|o| o.graph_object == node_id && o.capability == "ospf");
        assert!(routing_obj.is_some(), "expected ospf capability object");
        assert!(routing_obj.unwrap().content.contains("router ospf"));
    }
}
