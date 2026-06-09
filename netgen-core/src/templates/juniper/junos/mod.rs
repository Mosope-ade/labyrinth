use minijinja::{Environment, Value, context};

use crate::graph::{Interface, InterfaceRole, PolicyAction, SecurityZone};
use crate::schema::SecurityLevel;
use crate::templates::{AclContext, RoutingContext, VendorAdapter};
use crate::schema::RoutingProtocol;

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
    tmpl.render(ctx).unwrap_or_else(|e| format!("## RENDER ERROR: {e}"))
}

// ─── JuniperJunOsAdapter ──────────────────────────────────────────────────────

/// Phase 2 vendor adapter — Juniper JunOS (vMX / vSRX).
/// Config is rendered in the JunOS curly-brace (hierarchy) format.
pub struct JuniperJunOsAdapter;

impl VendorAdapter for JuniperJunOsAdapter {
    fn render_base(&self, hostname: &str) -> String {
        render(BASE_TMPL, context!(hostname => hostname))
    }

    fn render_interface(&self, iface: &Interface) -> String {
        let description = match &iface.role {
            InterfaceRole::Uplink   => "uplink",
            InterfaceRole::Access   => "access",
            InterfaceRole::Transit  => "transit",
            InterfaceRole::Loopback => "loopback",
            InterfaceRole::Mgmt     => "management",
        };

        let iface_list = vec![context!(
            name        => iface.name,
            description => description,
            ipv4_cidr   => iface.ipv4_addr.clone(),
            ipv6_cidr   => iface.ipv6_addr.clone(),
        )];

        render(IFACE_TMPL, context!(interfaces => iface_list))
    }

    fn render_routing(&self, ctx: &RoutingContext<'_>) -> String {
        let mut out = String::new();

        if ctx.routing.protocols.contains(&RoutingProtocol::Ospf) {
            let authentication = ctx.routing.ospf.as_ref().is_some_and(|o| o.authentication);

            let active: Vec<String> = ctx.node.interfaces.iter()
                .filter(|i| i.role != InterfaceRole::Loopback)
                .map(|i| i.name.clone())
                .collect();

            out.push_str(&render(OSPF_TMPL, context!(
                router_id         => ctx.router_id.clone(),
                authentication    => authentication,
                active_interfaces => active,
            )));
        }

        if ctx.routing.protocols.contains(&RoutingProtocol::Bgp) {
            if let Some(bgp) = &ctx.routing.bgp {
                if bgp.enabled {
                    let local_as = bgp.local_as.unwrap_or(65000);
                    let neighbors: Vec<Value> = Vec::new();
                    out.push_str(&render(BGP_TMPL, context!(
                        local_as  => local_as,
                        router_id => ctx.router_id.clone(),
                        neighbors => neighbors,
                    )));
                }
            }
        }

        if out.is_empty() {
            // Static routing — no protocol block needed in JunOS
            out.push_str("## static routing — no dynamic protocol configured\n");
        }

        out
    }

    fn render_acl(&self, ctx: &AclContext<'_>) -> String {
        let filter_name = format!(
            "FILTER-{}-TO-{}",
            ctx.from_zone.name.to_uppercase(),
            ctx.to_zone.name.to_uppercase()
        );
        let action = match ctx.policy.action {
            PolicyAction::Permit  => "accept",
            PolicyAction::Deny    => "discard",
            PolicyAction::Inspect => "accept", // JunOS: stateful via security policies
        };

        render(ACL_TMPL, context!(
            filter_name => filter_name,
            action      => action,
            from_subnet => ctx.from_subnet.clone(),
            to_subnet   => ctx.to_subnet.clone(),
        ))
    }

    fn render_hardening(&self, level: &SecurityLevel) -> String {
        let snmp_enabled = matches!(level, SecurityLevel::Standard | SecurityLevel::Hardened);
        render(HARDENING_TMPL, context!(snmp_enabled => snmp_enabled))
    }

    fn render_vlan(&self, zone: &SecurityZone, svi_subnet: Option<&str>) -> String {
        let vlan_id = zone.vlan.unwrap_or(1);
        render(VLAN_TMPL, context!(
            vlan_id   => vlan_id,
            vlan_name => zone.name.to_uppercase(),
            svi_cidr  => svi_subnet,
        ))
    }

    fn interface_name(&self, role: &InterfaceRole, index: u8) -> String {
        match role {
            InterfaceRole::Loopback => "lo0".to_string(),
            _                       => format!("ge-0/0/{index}"),
        }
    }

    fn appliance_id(&self) -> &str {
        "juniper-vmx"
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

    fn adapter() -> JuniperJunOsAdapter {
        JuniperJunOsAdapter
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
                vendor_preference: vec![Vendor::Juniper],
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
        assert!(out.contains("host-name RTR-CORE-01"), "got: {out}");
    }

    #[test]
    fn render_base_contains_banner() {
        let out = adapter().render_base("RTR-EDGE-01");
        assert!(out.contains("Authorized access only"), "got: {out}");
    }

    #[test]
    fn render_base_junos_system_stanza() {
        let out = adapter().render_base("RTR-01");
        assert!(out.contains("system {"), "got: {out}");
        assert!(out.contains("domain-name labyrinth.network"), "got: {out}");
    }

    // ── render_interface ──────────────────────────────────────────────────────

    #[test]
    fn render_interface_with_ipv4_cidr() {
        let mut iface = Interface::new("ge-0/0/0", InterfaceRole::Access);
        iface.ipv4_addr = Some("10.10.10.1/24".into());
        let out = adapter().render_interface(&iface);
        assert!(out.contains("ge-0/0/0"), "got: {out}");
        assert!(out.contains("address 10.10.10.1/24"), "got: {out}");
        assert!(out.contains("family inet"), "got: {out}");
    }

    #[test]
    fn render_interface_with_ipv6_cidr() {
        let mut iface = Interface::new("ge-0/0/1", InterfaceRole::Uplink);
        iface.ipv6_addr = Some("fd00::1/64".into());
        let out = adapter().render_interface(&iface);
        assert!(out.contains("family inet6"), "got: {out}");
        assert!(out.contains("address fd00::1/64"), "got: {out}");
    }

    #[test]
    fn render_interface_loopback() {
        let iface = Interface::new("lo0", InterfaceRole::Loopback);
        let out = adapter().render_interface(&iface);
        assert!(out.contains("lo0"), "got: {out}");
        assert!(out.contains("unit 0"), "got: {out}");
    }

    #[test]
    fn render_interface_no_ip_is_valid() {
        let iface = Interface::new("ge-0/0/2", InterfaceRole::Access);
        let out = adapter().render_interface(&iface);
        assert!(out.contains("ge-0/0/2"), "got: {out}");
        assert!(out.contains("interfaces {"), "got: {out}");
    }

    // ── render_routing — OSPF ─────────────────────────────────────────────────

    #[test]
    fn render_routing_ospf_uses_protocols_stanza() {
        let zone_id = Uuid::new_v4();
        let node = NetworkNode::new("RTR-01", NodeRole::Router, Vendor::Juniper, zone_id);
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
            ospf_networks: vec![],
        };
        let out = adapter().render_routing(&ctx);
        assert!(out.contains("protocols {"), "got: {out}");
        assert!(out.contains("ospf {"), "got: {out}");
        assert!(out.contains("area 0.0.0.0"), "got: {out}");
    }

    #[test]
    fn render_routing_ospf_router_id_in_routing_options() {
        let zone_id = Uuid::new_v4();
        let node = NetworkNode::new("RTR-01", NodeRole::Router, Vendor::Juniper, zone_id);
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
            ospf_networks: vec![],
        };
        let out = adapter().render_routing(&ctx);
        assert!(out.contains("routing-options {"), "got: {out}");
        assert!(out.contains("router-id 10.255.255.1"), "got: {out}");
    }

    #[test]
    fn render_routing_ospf_authentication_uses_md5() {
        let zone_id = Uuid::new_v4();
        let mut node = NetworkNode::new("RTR-01", NodeRole::Router, Vendor::Juniper, zone_id);
        node.interfaces.push(Interface::new("ge-0/0/0", InterfaceRole::Access));
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
        assert!(out.contains("md5 1 key"), "got: {out}");
    }

    #[test]
    fn render_routing_bgp_uses_as_and_routing_options() {
        let zone_id = Uuid::new_v4();
        let node = NetworkNode::new("RTR-01", NodeRole::Router, Vendor::Juniper, zone_id);
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
        assert!(out.contains("autonomous-system 65001"), "got: {out}");
        assert!(out.contains("bgp {"), "got: {out}");
    }

    #[test]
    fn render_routing_static_emits_comment() {
        let zone_id = Uuid::new_v4();
        let node = NetworkNode::new("RTR-01", NodeRole::Router, Vendor::Juniper, zone_id);
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
    fn render_acl_uses_firewall_filter() {
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
        assert!(out.contains("firewall {"), "got: {out}");
        assert!(out.contains("filter FILTER-LAN-TO-DMZ"), "got: {out}");
        assert!(out.contains("source-address 10.10.10.0/24"), "got: {out}");
        assert!(out.contains("destination-address 10.10.20.0/27"), "got: {out}");
        assert!(out.contains("then accept"), "got: {out}");
    }

    #[test]
    fn render_acl_deny_uses_discard() {
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
        assert!(out.contains("FILTER-GUEST-TO-LAN"), "got: {out}");
        assert!(out.contains("then discard"), "got: {out}");
    }

    #[test]
    fn render_acl_always_has_deny_all_term() {
        let lan = make_zone("LAN", ZoneRole::Lan, 80);
        let mgmt = make_zone("MGMT", ZoneRole::Mgmt, 90);
        let policy = ZonePolicy::new(lan.id, mgmt.id, PolicyAction::Permit);
        let ctx = AclContext {
            policy: &policy,
            from_zone: &lan,
            to_zone: &mgmt,
            from_subnet: None,
            to_subnet: None,
        };
        let out = adapter().render_acl(&ctx);
        assert!(out.contains("term DENY-ALL"), "got: {out}");
        assert!(out.contains("syslog"), "got: {out}");
    }

    // ── render_hardening ──────────────────────────────────────────────────────

    #[test]
    fn render_hardening_ssh_root_login_deny() {
        let out = adapter().render_hardening(&SecurityLevel::Basic);
        assert!(out.contains("root-login deny"), "got: {out}");
        assert!(out.contains("protocol-version v2"), "got: {out}");
    }

    #[test]
    fn render_hardening_no_snmp_for_basic() {
        let out = adapter().render_hardening(&SecurityLevel::Basic);
        assert!(!out.contains("snmp {"), "got: {out}");
    }

    #[test]
    fn render_hardening_snmpv3_for_standard() {
        let out = adapter().render_hardening(&SecurityLevel::Standard);
        assert!(out.contains("snmp {"), "got: {out}");
        assert!(out.contains("authentication-sha"), "got: {out}");
        assert!(out.contains("privacy-aes128"), "got: {out}");
    }

    #[test]
    fn render_hardening_syslog_configured() {
        let out = adapter().render_hardening(&SecurityLevel::Basic);
        assert!(out.contains("syslog {"), "got: {out}");
    }

    // ── render_vlan ───────────────────────────────────────────────────────────

    #[test]
    fn render_vlan_uses_irb_for_svi() {
        let mut zone = SecurityZone::new("LAN", ZoneRole::Lan, 80);
        zone.vlan = Some(10);
        let out = adapter().render_vlan(&zone, Some("10.10.10.1/24"));
        assert!(out.contains("vlan-id 10"), "got: {out}");
        assert!(out.contains("l3-interface irb.10"), "got: {out}");
        assert!(out.contains("irb {"), "got: {out}");
        assert!(out.contains("address 10.10.10.1/24"), "got: {out}");
    }

    #[test]
    fn render_vlan_without_svi_omits_irb() {
        let mut zone = SecurityZone::new("MGMT", ZoneRole::Mgmt, 90);
        zone.vlan = Some(200);
        let out = adapter().render_vlan(&zone, None);
        assert!(out.contains("vlan-id 200"), "got: {out}");
        assert!(!out.contains("irb"), "IRB should be absent without SVI subnet, got: {out}");
    }

    // ── interface_name ────────────────────────────────────────────────────────

    #[test]
    fn interface_name_loopback_is_lo0() {
        assert_eq!(adapter().interface_name(&InterfaceRole::Loopback, 0), "lo0");
    }

    #[test]
    fn interface_name_physical_uses_ge_format() {
        assert_eq!(adapter().interface_name(&InterfaceRole::Uplink, 2), "ge-0/0/2");
    }

    #[test]
    fn interface_name_loopback_ignores_index() {
        assert_eq!(adapter().interface_name(&InterfaceRole::Loopback, 3), "lo0");
    }

    #[test]
    fn appliance_id_is_juniper_vmx() {
        assert_eq!(adapter().appliance_id(), "juniper-vmx");
    }

    // ── ConfigGenerator with JunOS adapter ───────────────────────────────────

    fn build_graph_with_router() -> (TopologyGraph, Uuid) {
        use crate::graph::{Ipv4Allocation, SecurityZone};
        let mut g = TopologyGraph::new("junos-gen-test");
        let zone = SecurityZone::new("LAN", ZoneRole::Lan, 80);
        let zone_id = zone.id;
        g.add_zone(zone);

        let mut router = NetworkNode::new("RTR-LAN-01", NodeRole::Router, Vendor::Juniper, zone_id);
        let mut lo = Interface::new("lo0", InterfaceRole::Loopback);
        lo.ipv4_addr = Some("10.255.255.1/32".into());
        let mut ge0 = Interface::new("ge-0/0/0", InterfaceRole::Access);
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
    fn config_generator_produces_base_interfaces_hardening() {
        let (graph, node_id) = build_graph_with_router();
        let intent = base_intent();
        let objects = ConfigGenerator.generate(
            &graph,
            &intent.routing,
            &intent.metadata.security_level,
            &adapter(),
        ).unwrap();

        let caps: Vec<&str> = objects.iter()
            .filter(|o| o.graph_object == node_id)
            .map(|o| o.capability.as_str())
            .collect();

        assert!(caps.contains(&"base"), "missing base, got: {caps:?}");
        assert!(caps.contains(&"interfaces"), "missing interfaces, got: {caps:?}");
        assert!(caps.contains(&"hardening"), "missing hardening, got: {caps:?}");
    }

    #[test]
    fn config_objects_carry_juniper_vmx_appliance_id() {
        let (graph, _) = build_graph_with_router();
        let intent = base_intent();
        let objects = ConfigGenerator.generate(
            &graph,
            &intent.routing,
            &intent.metadata.security_level,
            &adapter(),
        ).unwrap();
        assert!(objects.iter().all(|o| o.generated_by == "juniper-vmx"));
    }

    #[test]
    fn stitched_junos_config_contains_host_name() {
        let (graph, node_id) = build_graph_with_router();
        let intent = base_intent();
        let objects = ConfigGenerator.generate(
            &graph,
            &intent.routing,
            &intent.metadata.security_level,
            &adapter(),
        ).unwrap();
        let full = ConfigGenerator::stitch_device_config(&objects, node_id);
        assert!(full.contains("host-name RTR-LAN-01"), "stitched: {full}");
        assert!(full.contains("root-login deny"), "stitched: {full}");
    }

    #[test]
    fn junos_config_generation_is_deterministic() {
        let (graph, node_id) = build_graph_with_router();
        let intent = base_intent();
        let a = ConfigGenerator.generate(
            &graph, &intent.routing, &intent.metadata.security_level, &adapter(),
        ).unwrap();
        let b = ConfigGenerator.generate(
            &graph, &intent.routing, &intent.metadata.security_level, &adapter(),
        ).unwrap();
        assert_eq!(
            ConfigGenerator::stitch_device_config(&a, node_id),
            ConfigGenerator::stitch_device_config(&b, node_id),
        );
    }
}
