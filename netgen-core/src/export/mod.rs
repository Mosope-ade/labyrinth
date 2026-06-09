use std::collections::HashMap;
use std::io::{Cursor, Write};

use serde_json::json;
use thiserror::Error;
use uuid::Uuid;
use zip::write::{FileOptions, ZipWriter};
use zip::CompressionMethod;

use crate::graph::{InterfaceRole, NodeRole, PolicyAction, TopologyGraph, TriggerSource};
use crate::schema::{Vendor, ZoneRole};

#[derive(Debug, Error)]
pub enum ExportError {
    #[error("Template render failed: {0}")]
    RenderFailed(String),
    #[error("Serialization failed: {0}")]
    SerializationFailed(String),
    #[error("ZIP write failed: {0}")]
    ZipFailed(String),
}

pub struct Gns3Exporter;
pub struct DiagramExporter;
pub struct IpPlanExporter;
pub struct DocExporter;

// ─── Layout algorithm ────────────────────────────────────────────────────────

/// Computes node positions in canvas coordinates centered at (0, 0).
/// Y layers: WAN/Transit/Cloud=50, Firewall=200, DMZ=350, LAN=500, MGMT/OOB=550, Guest=600.
fn compute_layout(graph: &TopologyGraph) -> HashMap<Uuid, (i32, i32)> {
    let layer_y = |node_id: Uuid| -> i32 {
        let node = match graph.node(node_id) {
            Some(n) => n,
            None => return 400,
        };
        match node.role {
            NodeRole::Cloud => 50,
            NodeRole::Firewall => 200,
            _ => match graph.zone_for_node(node_id).map(|z| &z.role) {
                Some(ZoneRole::Wan) | Some(ZoneRole::Transit) => 50,
                Some(ZoneRole::Dmz) => 350,
                Some(ZoneRole::Lan) => 500,
                Some(ZoneRole::Mgmt) | Some(ZoneRole::Oob) => 550,
                Some(ZoneRole::Guest) => 600,
                None => 400,
            },
        }
    };

    let mut by_layer: HashMap<i32, Vec<Uuid>> = HashMap::new();
    for node in &graph.nodes {
        by_layer.entry(layer_y(node.id)).or_default().push(node.id);
    }

    let mut positions = HashMap::new();
    let x_span = 1200i32;
    for (y, ids) in &by_layer {
        let count = ids.len() as i32;
        if count == 1 {
            positions.insert(ids[0], (0, *y));
        } else {
            let spacing = x_span / (count - 1);
            let start_x = -(x_span / 2);
            for (i, id) in ids.iter().enumerate() {
                positions.insert(*id, (start_x + spacing * i as i32, *y));
            }
        }
    }
    positions
}

// ─── GNS3 helpers ────────────────────────────────────────────────────────────

fn gns3_node_symbol(role: &NodeRole) -> &'static str {
    match role {
        NodeRole::Router => ":/symbols/router.svg",
        NodeRole::Switch => ":/symbols/ethernet_switch.svg",
        NodeRole::Firewall => ":/symbols/firewall.svg",
        NodeRole::Host | NodeRole::Ap => ":/symbols/vpcs_guest.svg",
        NodeRole::Cloud => ":/symbols/cloud.svg",
    }
}

fn gns3_node_type(role: &NodeRole) -> &'static str {
    match role {
        NodeRole::Router | NodeRole::Switch | NodeRole::Firewall => "dynamips",
        NodeRole::Host | NodeRole::Ap => "vpcs",
        NodeRole::Cloud => "cloud",
    }
}

fn gns3_port_format(role: &NodeRole, vendor: &Vendor) -> &'static str {
    match vendor {
        Vendor::Juniper => "ge-0/0/{0}",
        _ => match role {
            NodeRole::Host | NodeRole::Ap | NodeRole::Cloud => "eth{0}",
            _ => "GigabitEthernet0/{0}",
        },
    }
}

/// Returns a placeholder GNS3 appliance template ID. Configurable per installation per spec.
fn gns3_template_id(role: &NodeRole, vendor: &Vendor) -> &'static str {
    match (role, vendor) {
        (NodeRole::Router, Vendor::Cisco) => "cisco-c7200",
        (NodeRole::Switch, Vendor::Cisco) => "cisco-c3750",
        (NodeRole::Firewall, Vendor::Cisco) => "cisco-asav",
        (NodeRole::Router, Vendor::Juniper) => "juniper-vmx",
        (NodeRole::Switch, Vendor::Juniper) => "juniper-vsrx",
        (NodeRole::Router, Vendor::Mikrotik) => "mikrotik-chr",
        (NodeRole::Router | NodeRole::Firewall, Vendor::PaloAlto) => "paloalto-vm",
        (NodeRole::Host, _) | (NodeRole::Ap, _) => "vpcs",
        (NodeRole::Cloud, _) => "cloud",
        _ => "generic-device",
    }
}

// ─── Gns3Exporter ────────────────────────────────────────────────────────────

impl Gns3Exporter {
    /// Produces a `.gns3project` ZIP archive containing:
    /// - `project.gns3` manifest (nodes, links, layered positions)
    /// - `project-files/configs/<label>.cfg` for each node that has a rendered config
    /// - `project-files/topology/nodes.json` — supplementary node/position data
    /// - `project-files/topology/links.json` — supplementary connection data
    pub fn export(&self, graph: &TopologyGraph) -> Result<Vec<u8>, ExportError> {
        let cursor = Cursor::new(Vec::<u8>::new());
        let mut zip = ZipWriter::new(cursor);
        let opts = FileOptions::default().compression_method(CompressionMethod::Deflated);

        let positions = compute_layout(graph);

        let gns3_nodes: Vec<serde_json::Value> = graph
            .nodes
            .iter()
            .map(|node| {
                let (x, y) = positions.get(&node.id).copied().unwrap_or((0, 0));
                json!({
                    "compute_id": "local",
                    "console": null,
                    "console_auto_start": false,
                    "console_type": "telnet",
                    "first_port_name": null,
                    "height": 45,
                    "label": {
                        "rotation": 0,
                        "style": "font-size: 12; font-style: normal; font-weight: bold;",
                        "text": node.label,
                        "x": -5,
                        "y": -25
                    },
                    "locked": false,
                    "name": node.label,
                    "node_id": node.id.to_string(),
                    "node_type": gns3_node_type(&node.role),
                    "port_name_format": gns3_port_format(&node.role, &node.vendor),
                    "port_segment_size": 0,
                    "symbol": gns3_node_symbol(&node.role),
                    "template_id": gns3_template_id(&node.role, &node.vendor),
                    "width": 65,
                    "x": x,
                    "y": y,
                    "z": 1
                })
            })
            .collect();

        let mut port_counters: HashMap<Uuid, u32> = HashMap::new();
        let gns3_links: Vec<serde_json::Value> = graph
            .edges
            .iter()
            .map(|edge| {
                let from_port = {
                    let c = port_counters.entry(edge.from_node).or_insert(0);
                    let p = *c;
                    *c += 1;
                    p
                };
                let to_port = {
                    let c = port_counters.entry(edge.to_node).or_insert(0);
                    let p = *c;
                    *c += 1;
                    p
                };
                json!({
                    "filters": {},
                    "link_id": edge.id.to_string(),
                    "link_style": {"color": "#000000", "type": 0, "width": 2},
                    "link_type": "ethernet",
                    "nodes": [
                        {
                            "adapter_number": 0,
                            "node_id": edge.from_node.to_string(),
                            "port_number": from_port
                        },
                        {
                            "adapter_number": 0,
                            "node_id": edge.to_node.to_string(),
                            "port_number": to_port
                        }
                    ],
                    "suspend": false
                })
            })
            .collect();

        // project.gns3 — main manifest
        let manifest = json!({
            "auto_close": false,
            "auto_open": false,
            "auto_start": false,
            "grid_size": 75,
            "name": graph.metadata.name,
            "project_id": Uuid::new_v4().to_string(),
            "revision": 10,
            "scene_height": 1000,
            "scene_width": 2000,
            "show_grid": false,
            "show_interface_labels": true,
            "topology": {
                "computes": [],
                "drawings": [],
                "links": &gns3_links,
                "nodes": &gns3_nodes
            },
            "type": "topology",
            "version": "2.8.0",
            "zoom": 75
        });
        let manifest_bytes = serde_json::to_vec_pretty(&manifest)
            .map_err(|e| ExportError::SerializationFailed(e.to_string()))?;
        zip.start_file("project.gns3", opts)
            .map_err(|e| ExportError::ZipFailed(e.to_string()))?;
        zip.write_all(&manifest_bytes)
            .map_err(|e| ExportError::ZipFailed(e.to_string()))?;

        // Per-device config files (if node has pre-rendered config)
        for node in &graph.nodes {
            if let Some(cfg) = &node.config {
                let safe_label = node.label.replace(' ', "_").replace('/', "-");
                let path = format!("project-files/configs/{safe_label}.cfg");
                zip.start_file(path.as_str(), opts)
                    .map_err(|e| ExportError::ZipFailed(e.to_string()))?;
                zip.write_all(cfg.raw.as_bytes())
                    .map_err(|e| ExportError::ZipFailed(e.to_string()))?;
            }
        }

        // Supplementary topology files
        let nodes_bytes = serde_json::to_vec_pretty(&json!({"nodes": &gns3_nodes}))
            .map_err(|e| ExportError::SerializationFailed(e.to_string()))?;
        zip.start_file("project-files/topology/nodes.json", opts)
            .map_err(|e| ExportError::ZipFailed(e.to_string()))?;
        zip.write_all(&nodes_bytes)
            .map_err(|e| ExportError::ZipFailed(e.to_string()))?;

        let links_bytes = serde_json::to_vec_pretty(&json!({"links": &gns3_links}))
            .map_err(|e| ExportError::SerializationFailed(e.to_string()))?;
        zip.start_file("project-files/topology/links.json", opts)
            .map_err(|e| ExportError::ZipFailed(e.to_string()))?;
        zip.write_all(&links_bytes)
            .map_err(|e| ExportError::ZipFailed(e.to_string()))?;

        let cursor = zip
            .finish()
            .map_err(|e| ExportError::ZipFailed(e.to_string()))?;
        Ok(cursor.into_inner())
    }
}

// ─── DiagramExporter ─────────────────────────────────────────────────────────

fn zone_fill_color(role: &ZoneRole) -> &'static str {
    match role {
        ZoneRole::Wan => "#FFEBE0",
        ZoneRole::Transit => "#E0E8FF",
        ZoneRole::Dmz => "#FFFBE0",
        ZoneRole::Lan => "#E0FFE0",
        ZoneRole::Mgmt => "#F5E0FF",
        ZoneRole::Oob => "#F0F0F0",
        ZoneRole::Guest => "#E0FFF5",
    }
}

fn zone_stroke_color(role: &ZoneRole) -> &'static str {
    match role {
        ZoneRole::Wan => "#FF6B35",
        ZoneRole::Transit => "#4A90D9",
        ZoneRole::Dmz => "#F5A623",
        ZoneRole::Lan => "#5CB85C",
        ZoneRole::Mgmt => "#9B59B6",
        ZoneRole::Oob => "#95A5A6",
        ZoneRole::Guest => "#2ECC71",
    }
}

fn node_fill_color(role: &NodeRole) -> &'static str {
    match role {
        NodeRole::Router => "#4A90D9",
        NodeRole::Switch => "#5CB85C",
        NodeRole::Firewall => "#E74C3C",
        NodeRole::Host | NodeRole::Ap => "#95A5A6",
        NodeRole::Cloud => "#BDC3C7",
    }
}

fn node_abbrev(role: &NodeRole) -> &'static str {
    match role {
        NodeRole::Router => "RTR",
        NodeRole::Switch => "SW",
        NodeRole::Firewall => "FW",
        NodeRole::Host => "HOST",
        NodeRole::Ap => "AP",
        NodeRole::Cloud => "NET",
    }
}

/// Escapes `<`, `>`, `&`, `"` for SVG/XML text content.
fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

impl DiagramExporter {
    /// Renders a professional architecture diagram as SVG with zone color-coding,
    /// layered layout (WAN top, firewall middle, access/DMZ lower), and link annotations.
    pub fn to_svg(&self, graph: &TopologyGraph) -> Result<String, ExportError> {
        const CANVAS_W: i32 = 1600;
        const CANVAS_H: i32 = 820;
        const OFFSET_X: i32 = 800; // center the (0,0) layout on canvas
        const OFFSET_Y: i32 = 120; // top margin
        const NODE_W: i32 = 90;
        const NODE_H: i32 = 44;
        const ZONE_PAD: i32 = 40;

        let positions = compute_layout(graph);
        let svg_cx = |id: Uuid| -> i32 {
            positions.get(&id).map(|(x, _)| x + OFFSET_X).unwrap_or(OFFSET_X)
        };
        let svg_cy = |id: Uuid| -> i32 {
            positions.get(&id).map(|(_, y)| y + OFFSET_Y).unwrap_or(OFFSET_Y)
        };

        let mut svg = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{CANVAS_W}\" height=\"{CANVAS_H}\" \
             viewBox=\"0 0 {CANVAS_W} {CANVAS_H}\">\n\
             <rect width=\"{CANVAS_W}\" height=\"{CANVAS_H}\" fill=\"#F8F9FA\"/>\n\
             <text x=\"{}\" y=\"40\" font-family=\"Helvetica Neue,sans-serif\" font-size=\"22\" \
             font-weight=\"bold\" fill=\"#2C3E50\" text-anchor=\"middle\">{}</text>\n\
             <text x=\"{}\" y=\"62\" font-family=\"Helvetica Neue,sans-serif\" font-size=\"12\" \
             fill=\"#7F8C8D\" text-anchor=\"middle\">Network Architecture — Labyrinth</text>\n",
            CANVAS_W / 2,
            escape_xml(&graph.metadata.name),
            CANVAS_W / 2,
        );

        // Zone background boxes (drawn first — nodes sit on top)
        for zone in &graph.zones {
            let node_ids: Vec<Uuid> =
                graph.nodes_in_zone(zone.id).iter().map(|n| n.id).collect();
            if node_ids.is_empty() {
                continue;
            }
            let xs: Vec<i32> = node_ids.iter().map(|&id| svg_cx(id)).collect();
            let ys: Vec<i32> = node_ids.iter().map(|&id| svg_cy(id)).collect();
            let min_x = xs.iter().min().unwrap() - NODE_W / 2 - ZONE_PAD;
            let min_y = ys.iter().min().unwrap() - NODE_H / 2 - ZONE_PAD;
            let max_x = xs.iter().max().unwrap() + NODE_W / 2 + ZONE_PAD;
            let max_y = ys.iter().max().unwrap() + NODE_H / 2 + ZONE_PAD + 20;
            let w = max_x - min_x;
            let h = max_y - min_y;
            let fill = zone_fill_color(&zone.role);
            let stroke = zone_stroke_color(&zone.role);
            svg.push_str(&format!(
                "<rect x=\"{min_x}\" y=\"{min_y}\" width=\"{w}\" height=\"{h}\" rx=\"10\" \
                 fill=\"{fill}\" stroke=\"{stroke}\" stroke-width=\"2\" stroke-dasharray=\"6,3\"/>\n\
                 <text x=\"{}\" y=\"{}\" font-family=\"Helvetica Neue,sans-serif\" font-size=\"13\" \
                 font-weight=\"bold\" fill=\"{stroke}\">{}</text>\n",
                min_x + 10,
                min_y + 18,
                escape_xml(&zone.label)
            ));
        }

        // Links (drawn before nodes so nodes occlude line ends)
        for edge in &graph.edges {
            let x1 = svg_cx(edge.from_node);
            let y1 = svg_cy(edge.from_node);
            let x2 = svg_cx(edge.to_node);
            let y2 = svg_cy(edge.to_node);
            svg.push_str(&format!(
                "<line x1=\"{x1}\" y1=\"{y1}\" x2=\"{x2}\" y2=\"{y2}\" \
                 stroke=\"#666\" stroke-width=\"2\"/>\n"
            ));
            if let Some(bw) = edge.bandwidth {
                let mid_x = (x1 + x2) / 2;
                let mid_y = (y1 + y2) / 2;
                svg.push_str(&format!(
                    "<text x=\"{mid_x}\" y=\"{mid_y}\" font-family=\"monospace\" font-size=\"10\" \
                     fill=\"#555\" text-anchor=\"middle\" dy=\"-4\">{bw}M</text>\n"
                ));
            }
        }

        // Nodes
        for node in &graph.nodes {
            let cx = svg_cx(node.id);
            let cy = svg_cy(node.id);
            let rx = cx - NODE_W / 2;
            let ry = cy - NODE_H / 2;
            let fill = node_fill_color(&node.role);
            let abbrev = node_abbrev(&node.role);
            svg.push_str(&format!(
                "<rect x=\"{rx}\" y=\"{ry}\" width=\"{NODE_W}\" height=\"{NODE_H}\" rx=\"6\" \
                 fill=\"{fill}\" stroke=\"#2C3E50\" stroke-width=\"1.5\"/>\n\
                 <text x=\"{cx}\" y=\"{}\" font-family=\"monospace\" font-size=\"9\" \
                 fill=\"rgba(255,255,255,0.6)\" text-anchor=\"middle\">{abbrev}</text>\n\
                 <text x=\"{cx}\" y=\"{}\" font-family=\"Helvetica Neue,sans-serif\" font-size=\"11\" \
                 font-weight=\"bold\" fill=\"white\" text-anchor=\"middle\" \
                 dominant-baseline=\"middle\">{}</text>\n\
                 <text x=\"{cx}\" y=\"{}\" font-family=\"monospace\" font-size=\"9\" \
                 fill=\"#BDC3C7\" text-anchor=\"middle\">{} iface(s)</text>\n",
                ry + 12,
                cy + 5,
                escape_xml(&node.label),
                cy + NODE_H / 2 + 14,
                node.interfaces.len()
            ));
        }

        svg.push_str("</svg>\n");
        Ok(svg)
    }
}

// ─── IpPlanExporter ──────────────────────────────────────────────────────────

impl IpPlanExporter {
    /// Exports the full address table as CSV.
    /// Columns: Zone, Zone Role, VLAN, Device, Device Role, Interface, Interface Role, IPv4, IPv6, Purpose
    /// Rows with no IP addresses (neither v4 nor v6) are omitted.
    pub fn to_csv(&self, graph: &TopologyGraph) -> Result<String, ExportError> {
        let mut csv = String::from(
            "Zone,Zone Role,VLAN,Device,Device Role,Interface,Interface Role,IPv4,IPv6,Purpose\n",
        );

        for node in &graph.nodes {
            let zone = graph.zone_for_node(node.id);
            let zone_name = zone.map_or("", |z| z.name.as_str());
            let zone_role = zone.map_or("", |z| zone_role_label(&z.role));
            let vlan = zone
                .and_then(|z| z.vlan)
                .map(|v| v.to_string())
                .unwrap_or_default();
            let dev_role = node_role_label(&node.role);

            for iface in &node.interfaces {
                let ipv4 = iface.ipv4_addr.as_deref().unwrap_or("");
                let ipv6 = iface.ipv6_addr.as_deref().unwrap_or("");
                if ipv4.is_empty() && ipv6.is_empty() {
                    continue;
                }
                let iface_role = iface_role_label(&iface.role);
                let purpose = format!("{} {iface_role}", node.label);
                csv.push_str(&format!(
                    "{zone_name},{zone_role},{vlan},{},{dev_role},{},{iface_role},{ipv4},{ipv6},{}\n",
                    csv_field(&node.label),
                    csv_field(&iface.name),
                    csv_field(&purpose),
                ));
            }
        }

        Ok(csv)
    }
}

/// Quotes a CSV field if it contains commas, quotes, or newlines.
fn csv_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

// ─── DocExporter ─────────────────────────────────────────────────────────────

impl DocExporter {
    /// Generates a full network design document as Markdown, including topology summary,
    /// zone policies, IP addressing plan, device list, and the complete decision log.
    pub fn to_markdown(&self, graph: &TopologyGraph) -> Result<String, ExportError> {
        let mut md = String::new();

        md.push_str(&format!("# Network Design: {}\n\n", graph.metadata.name));
        if !graph.metadata.schema_version.is_empty() {
            md.push_str(&format!(
                "> Schema `{}` | Rules `{}` | Policies `{}`\n\n",
                graph.metadata.schema_version,
                graph.metadata.rules_version,
                graph.metadata.policy_version
            ));
        }

        // Overview
        md.push_str("## Overview\n\n");
        md.push_str(&format!(
            "| Metric | Count |\n\
             |--------|-------|\n\
             | Devices | {} |\n\
             | Links | {} |\n\
             | Zones | {} |\n\
             | Zone Policies | {} |\n\
             | Design Decisions | {} |\n\n",
            graph.nodes.len(),
            graph.edges.len(),
            graph.zones.len(),
            graph.policies.len(),
            graph.decisions.len()
        ));

        // Network Zones
        md.push_str("## Network Zones\n\n");
        md.push_str("| Zone | Role | VLAN | Trust | Devices |\n");
        md.push_str("|------|------|------|-------|---------|\n");
        for zone in &graph.zones {
            let node_labels: Vec<&str> = graph
                .nodes_in_zone(zone.id)
                .iter()
                .map(|n| n.label.as_str())
                .collect();
            let devices = if node_labels.is_empty() {
                "-".to_string()
            } else {
                node_labels.join(", ")
            };
            md.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                zone.name,
                zone_role_label(&zone.role),
                zone.vlan.map(|v| v.to_string()).unwrap_or_else(|| "-".into()),
                zone.trust_level,
                devices
            ));
        }
        md.push('\n');

        // Zone Policies
        md.push_str("## Zone Policies\n\n");
        md.push_str("| From | To | Action | Services |\n");
        md.push_str("|------|----|--------|----------|\n");
        for policy in &graph.policies {
            let from = graph.zone(policy.from_zone).map_or("?", |z| z.name.as_str());
            let to = graph.zone(policy.to_zone).map_or("?", |z| z.name.as_str());
            let action = policy_action_label(&policy.action);
            let svcs: Vec<&str> = policy.services.iter().map(|s| s.name.as_str()).collect();
            let services = if svcs.is_empty() {
                "Any".to_string()
            } else {
                svcs.join(", ")
            };
            md.push_str(&format!("| {from} | {to} | {action} | {services} |\n"));
        }
        md.push('\n');

        // IPv4 Addressing
        if !graph.addressing.ipv4_allocations.is_empty() {
            md.push_str("## IPv4 Addressing\n\n");
            md.push_str("| Zone | Subnet | Purpose |\n");
            md.push_str("|------|--------|----------|\n");
            for alloc in &graph.addressing.ipv4_allocations {
                let zone_name = graph.zone(alloc.zone_id).map_or("?", |z| z.name.as_str());
                md.push_str(&format!(
                    "| {zone_name} | {} | {} |\n",
                    alloc.subnet, alloc.purpose
                ));
            }
            md.push('\n');
        }

        // IPv6 Addressing
        if !graph.addressing.ipv6_allocations.is_empty() {
            md.push_str("## IPv6 Addressing\n\n");
            md.push_str("| Zone | Prefix | Purpose |\n");
            md.push_str("|------|--------|----------|\n");
            for alloc in &graph.addressing.ipv6_allocations {
                let zone_name = graph.zone(alloc.zone_id).map_or("?", |z| z.name.as_str());
                md.push_str(&format!(
                    "| {zone_name} | {} | {} |\n",
                    alloc.prefix, alloc.purpose
                ));
            }
            md.push('\n');
        }

        // Device Summary
        md.push_str("## Devices\n\n");
        md.push_str("| Device | Role | Vendor | Zone | Interfaces |\n");
        md.push_str("|--------|------|--------|------|------------|\n");
        for node in &graph.nodes {
            let zone = graph.zone_for_node(node.id).map_or("-", |z| z.name.as_str());
            md.push_str(&format!(
                "| {} | {} | {:?} | {zone} | {} |\n",
                node.label,
                node_role_label(&node.role),
                node.vendor,
                node.interfaces.len()
            ));
        }
        md.push('\n');

        // Design Decisions
        md.push_str("## Design Decisions\n\n");
        md.push_str("| # | Component | Decision | Source | Rule | Override |\n");
        md.push_str("|---|-----------|----------|--------|------|----------|\n");
        for (i, d) in graph.decisions.iter().enumerate() {
            let rule = d.rule_id.as_deref().unwrap_or("-");
            let source = trigger_source_label(&d.triggered_by);
            let overridable = if d.overridable { "Yes" } else { "No" };
            md.push_str(&format!(
                "| {} | {} | {} | {source} | {rule} | {overridable} |\n",
                i + 1,
                d.component,
                d.reason
            ));
        }
        md.push('\n');

        Ok(md)
    }
}

// ─── Label helpers ────────────────────────────────────────────────────────────

fn zone_role_label(role: &ZoneRole) -> &'static str {
    match role {
        ZoneRole::Wan => "wan",
        ZoneRole::Transit => "transit",
        ZoneRole::Dmz => "dmz",
        ZoneRole::Lan => "lan",
        ZoneRole::Mgmt => "mgmt",
        ZoneRole::Oob => "oob",
        ZoneRole::Guest => "guest",
    }
}

fn node_role_label(role: &NodeRole) -> &'static str {
    match role {
        NodeRole::Router => "router",
        NodeRole::Switch => "switch",
        NodeRole::Firewall => "firewall",
        NodeRole::Host => "host",
        NodeRole::Ap => "ap",
        NodeRole::Cloud => "cloud",
    }
}

fn iface_role_label(role: &InterfaceRole) -> &'static str {
    match role {
        InterfaceRole::Uplink => "uplink",
        InterfaceRole::Access => "access",
        InterfaceRole::Transit => "transit",
        InterfaceRole::Loopback => "loopback",
        InterfaceRole::Mgmt => "mgmt",
    }
}

fn policy_action_label(action: &PolicyAction) -> &'static str {
    match action {
        PolicyAction::Permit => "permit",
        PolicyAction::Deny => "deny",
        PolicyAction::Inspect => "inspect",
    }
}

fn trigger_source_label(source: &TriggerSource) -> &'static str {
    match source {
        TriggerSource::UserIntent => "user",
        TriggerSource::RuleEngine => "rule-engine",
        TriggerSource::SecurityPolicy => "security",
        TriggerSource::AiInference => "ai",
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{
        DesignDecision, DeviceConfig, Interface, InterfaceRole, LinkType, NetworkLink,
        NetworkNode, NodeRole, SecurityZone, TopologyGraph, TriggerSource,
    };
    use crate::schema::{Vendor, ZoneRole};

    fn build_test_graph() -> TopologyGraph {
        let mut g = TopologyGraph::new("Test-Network");

        let wan_zone = SecurityZone::new("WAN", ZoneRole::Wan, 10);
        let lan_zone = SecurityZone::new("LAN", ZoneRole::Lan, 80);
        let wan_id = wan_zone.id;
        let lan_id = lan_zone.id;
        g.add_zone(wan_zone);
        g.add_zone(lan_zone);

        let mut router = NetworkNode::new("RTR-CORE-01", NodeRole::Router, Vendor::Cisco, wan_id);
        let mut iface_r = Interface::new("GigabitEthernet0/0", InterfaceRole::Uplink);
        iface_r.ipv4_addr = Some("192.168.1.1/30".into());
        iface_r.ipv6_addr = Some("2001:db8::1/64".into());
        let iface_r_id = iface_r.id;
        router.interfaces.push(iface_r);
        let router_id = router.id;
        g.add_node(router);

        let mut switch = NetworkNode::new("SW-ACC-01", NodeRole::Switch, Vendor::Cisco, lan_id);
        let mut iface_s = Interface::new("GigabitEthernet0/0", InterfaceRole::Uplink);
        iface_s.ipv4_addr = Some("10.0.0.1/24".into());
        let iface_s_id = iface_s.id;
        switch.interfaces.push(iface_s);
        let switch_id = switch.id;
        g.add_node(switch);

        let link = NetworkLink::new(router_id, iface_r_id, switch_id, iface_s_id, LinkType::Ethernet);
        g.add_link(link);

        g
    }

    fn build_graph_with_config() -> TopologyGraph {
        let mut g = build_test_graph();
        let id = g.nodes[0].id;
        if let Some(node) = g.node_mut(id) {
            node.config = Some(DeviceConfig {
                raw: "hostname RTR-CORE-01\n!".into(),
                template_version: "1.0".into(),
            });
        }
        g
    }

    // ── GNS3 ────────────────────────────────────────────────────────────────

    #[test]
    fn gns3_export_returns_nonempty_bytes() {
        let bytes = Gns3Exporter.export(&build_test_graph()).expect("export failed");
        assert!(!bytes.is_empty());
    }

    #[test]
    fn gns3_export_is_valid_zip() {
        let bytes = Gns3Exporter.export(&build_test_graph()).expect("export failed");
        zip::ZipArchive::new(Cursor::new(bytes)).expect("not a valid ZIP");
    }

    #[test]
    fn gns3_zip_contains_required_files() {
        let bytes = Gns3Exporter.export(&build_test_graph()).expect("export failed");
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(names.contains(&"project.gns3".to_string()), "missing project.gns3: {names:?}");
        assert!(
            names.contains(&"project-files/topology/nodes.json".to_string()),
            "missing nodes.json: {names:?}"
        );
        assert!(
            names.contains(&"project-files/topology/links.json".to_string()),
            "missing links.json: {names:?}"
        );
    }

    #[test]
    fn gns3_manifest_contains_all_node_labels() {
        let bytes = Gns3Exporter.export(&build_test_graph()).expect("export failed");
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut f = archive.by_name("project.gns3").unwrap();
        let mut s = String::new();
        std::io::Read::read_to_string(&mut f, &mut s).unwrap();
        assert!(s.contains("RTR-CORE-01"), "RTR-CORE-01 missing from manifest");
        assert!(s.contains("SW-ACC-01"), "SW-ACC-01 missing from manifest");
    }

    #[test]
    fn gns3_manifest_contains_link() {
        let bytes = Gns3Exporter.export(&build_test_graph()).expect("export failed");
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut f = archive.by_name("project.gns3").unwrap();
        let mut s = String::new();
        std::io::Read::read_to_string(&mut f, &mut s).unwrap();
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        let links = v["topology"]["links"].as_array().unwrap();
        assert_eq!(links.len(), 1);
    }

    #[test]
    fn gns3_config_files_included_when_node_has_config() {
        let bytes = Gns3Exporter.export(&build_graph_with_config()).expect("export failed");
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(names.iter().any(|n| n.ends_with(".cfg")), "no .cfg files found: {names:?}");
    }

    #[test]
    fn gns3_port_counters_start_at_zero() {
        let bytes = Gns3Exporter.export(&build_test_graph()).expect("export failed");
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut f = archive.by_name("project.gns3").unwrap();
        let mut s = String::new();
        std::io::Read::read_to_string(&mut f, &mut s).unwrap();
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        let link = &v["topology"]["links"][0];
        let ports: Vec<u64> = link["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["port_number"].as_u64().unwrap())
            .collect();
        // Both endpoints are the first connection on their respective nodes
        assert_eq!(ports, vec![0, 0]);
    }

    #[test]
    fn gns3_empty_graph_produces_valid_zip_with_three_files() {
        let bytes = Gns3Exporter.export(&TopologyGraph::new("Empty")).expect("export failed");
        let archive = zip::ZipArchive::new(Cursor::new(bytes)).expect("not a valid ZIP");
        assert!(archive.len() >= 3);
    }

    // ── SVG ─────────────────────────────────────────────────────────────────

    #[test]
    fn svg_has_xml_declaration_and_closing_tag() {
        let svg = DiagramExporter.to_svg(&build_test_graph()).expect("svg failed");
        assert!(svg.starts_with("<?xml"), "missing XML declaration");
        assert!(svg.trim_end().ends_with("</svg>"), "missing </svg>");
    }

    #[test]
    fn svg_contains_all_node_labels() {
        let svg = DiagramExporter.to_svg(&build_test_graph()).expect("svg failed");
        assert!(svg.contains("RTR-CORE-01"));
        assert!(svg.contains("SW-ACC-01"));
    }

    #[test]
    fn svg_contains_zone_names() {
        let svg = DiagramExporter.to_svg(&build_test_graph()).expect("svg failed");
        assert!(svg.contains("WAN"));
        assert!(svg.contains("LAN"));
    }

    #[test]
    fn svg_contains_network_title() {
        let svg = DiagramExporter.to_svg(&build_test_graph()).expect("svg failed");
        assert!(svg.contains("Test-Network"));
    }

    #[test]
    fn svg_escapes_xml_special_chars() {
        let mut g = TopologyGraph::new("Net & <Test>");
        let zone = SecurityZone::new("Z&one", ZoneRole::Lan, 50);
        let zid = zone.id;
        g.add_zone(zone);
        g.add_node(NetworkNode::new("Node<1>", NodeRole::Router, Vendor::Cisco, zid));
        let svg = DiagramExporter.to_svg(&g).unwrap();
        assert!(svg.contains("Net &amp; &lt;Test&gt;"), "title not escaped: {svg}");
        assert!(!svg.contains("Net & <Test>"), "raw special chars in SVG");
    }

    #[test]
    fn svg_empty_graph_is_valid() {
        let svg = DiagramExporter.to_svg(&TopologyGraph::new("Blank")).unwrap();
        assert!(svg.contains("Blank"));
        assert!(svg.trim_end().ends_with("</svg>"));
    }

    // ── CSV ──────────────────────────────────────────────────────────────────

    #[test]
    fn csv_first_line_is_header() {
        let csv = IpPlanExporter.to_csv(&build_test_graph()).expect("csv failed");
        let header = csv.lines().next().unwrap();
        assert!(header.contains("Zone"));
        assert!(header.contains("IPv4"));
        assert!(header.contains("IPv6"));
        assert!(header.contains("Interface"));
    }

    #[test]
    fn csv_contains_ipv4_addresses() {
        let csv = IpPlanExporter.to_csv(&build_test_graph()).expect("csv failed");
        assert!(csv.contains("192.168.1.1/30"));
        assert!(csv.contains("10.0.0.1/24"));
    }

    #[test]
    fn csv_contains_ipv6_address() {
        let csv = IpPlanExporter.to_csv(&build_test_graph()).expect("csv failed");
        assert!(csv.contains("2001:db8::1/64"));
    }

    #[test]
    fn csv_skips_unaddressed_interfaces() {
        let mut g = build_test_graph();
        let node_id = g.nodes[0].id;
        g.node_mut(node_id)
            .unwrap()
            .interfaces
            .push(Interface::new("Loopback0", InterfaceRole::Loopback));
        let csv = IpPlanExporter.to_csv(&g).expect("csv failed");
        // RTR has 1 addressed iface + 1 unaddressed; SW has 1 addressed → 2 data rows
        assert_eq!(csv.lines().skip(1).count(), 2, "unexpected rows:\n{csv}");
    }

    #[test]
    fn csv_empty_graph_has_only_header() {
        let csv = IpPlanExporter.to_csv(&TopologyGraph::new("Empty")).expect("csv failed");
        assert_eq!(csv.lines().count(), 1);
    }

    // ── Markdown ──────────────────────────────────────────────────────────────

    #[test]
    fn markdown_starts_with_h1_title() {
        let md = DocExporter.to_markdown(&build_test_graph()).expect("doc failed");
        assert!(md.starts_with("# Network Design: Test-Network"));
    }

    #[test]
    fn markdown_has_zones_section() {
        let md = DocExporter.to_markdown(&build_test_graph()).expect("doc failed");
        assert!(md.contains("## Network Zones"));
        assert!(md.contains("WAN"));
        assert!(md.contains("LAN"));
    }

    #[test]
    fn markdown_has_device_table() {
        let md = DocExporter.to_markdown(&build_test_graph()).expect("doc failed");
        assert!(md.contains("RTR-CORE-01"));
        assert!(md.contains("SW-ACC-01"));
    }

    #[test]
    fn markdown_has_decisions_section() {
        let md = DocExporter.to_markdown(&build_test_graph()).expect("doc failed");
        assert!(md.contains("## Design Decisions"));
    }

    #[test]
    fn markdown_decisions_content_when_logged() {
        let mut g = build_test_graph();
        g.log_decision(DesignDecision::new(
            "topology",
            "Added redundant uplink for HA",
            TriggerSource::RuleEngine,
            false,
        ));
        let md = DocExporter.to_markdown(&g).expect("doc failed");
        assert!(md.contains("Added redundant uplink for HA"));
        assert!(md.contains("rule-engine"));
    }

    #[test]
    fn markdown_overview_counts_match() {
        let md = DocExporter.to_markdown(&build_test_graph()).expect("doc failed");
        assert!(md.contains("| Devices | 2 |"));
        assert!(md.contains("| Links | 1 |"));
        assert!(md.contains("| Zones | 2 |"));
    }

    // ── Layout ───────────────────────────────────────────────────────────────

    #[test]
    fn layout_all_nodes_get_positions() {
        let g = build_test_graph();
        let positions = compute_layout(&g);
        for node in &g.nodes {
            assert!(positions.contains_key(&node.id), "no position for {}", node.label);
        }
    }

    #[test]
    fn layout_wan_node_above_lan_node() {
        let g = build_test_graph();
        let positions = compute_layout(&g);
        let rtr = g.nodes.iter().find(|n| n.label == "RTR-CORE-01").unwrap();
        let sw = g.nodes.iter().find(|n| n.label == "SW-ACC-01").unwrap();
        // RTR in WAN zone → y=50; SW in LAN zone → y=500
        assert!(
            positions[&rtr.id].1 < positions[&sw.id].1,
            "WAN node should have lower y than LAN node"
        );
    }

    #[test]
    fn layout_single_node_layer_is_centered() {
        let g = build_test_graph();
        let positions = compute_layout(&g);
        // Each zone has exactly one node, so both should be at x=0
        for node in &g.nodes {
            assert_eq!(
                positions[&node.id].0, 0,
                "single-node layer should be at x=0 for {}",
                node.label
            );
        }
    }
}
