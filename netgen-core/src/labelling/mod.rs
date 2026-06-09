use std::collections::HashMap;

use crate::graph::{NodeRole, TopologyGraph};
use crate::schema::ZoneRole;

/// Applies deterministic device naming and zone semantic labels to the graph.
///
/// Called once, after the rule engine produces the final topology and before
/// config generation. All names produced here are stable — same graph always
/// produces the same names.
pub struct LabellingEngine;

impl LabellingEngine {
    /// Apply the naming convention to every node and assign semantic labels to
    /// every zone. Mutates `graph` in place.
    ///
    /// Node format: `{TYPE}-{ZONE_ABBREV}-{INDEX:02}`
    /// Zone label: semantic label derived from zone name (e.g. "icu" → "Clinical-Critical")
    pub fn apply(&self, graph: &mut TopologyGraph) {
        // ── 1. Assign semantic labels to zones ───────────────────────────────
        for zone in &mut graph.zones {
            zone.label = semantic_zone_label(&zone.name, &zone.role);
        }

        // ── 2. Rename nodes ──────────────────────────────────���───────────────
        // Build a (type_abbrev, zone_id) → next_index counter so indices are
        // sequential within each (type, zone) bucket.
        let mut counters: HashMap<(String, uuid::Uuid), u8> = HashMap::new();

        // We need the zone name for each node — collect zone_id → abbrev first.
        let zone_abbrevs: HashMap<uuid::Uuid, String> = graph
            .zones
            .iter()
            .map(|z| (z.id, zone_abbrev(&z.name)))
            .collect();

        for node in &mut graph.nodes {
            let type_abbrev = node_type_abbrev(&node.role);
            let zone_abbrev = zone_abbrevs
                .get(&node.zone_id)
                .cloned()
                .unwrap_or_else(|| "UNK".to_string());

            let key = (type_abbrev.clone(), node.zone_id);
            let count = counters.entry(key).or_insert(0);
            *count += 1;

            node.label = format!("{type_abbrev}-{zone_abbrev}-{count:02}");
        }
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Abbreviation for a node type, matching the naming convention in the plan.
fn node_type_abbrev(role: &NodeRole) -> String {
    match role {
        NodeRole::Router => "RTR",
        NodeRole::Switch => "SW",
        NodeRole::Firewall => "FW",
        NodeRole::Ap => "AP",
        NodeRole::Host => "SRV",
        NodeRole::Cloud => "CLOUD",
    }
    .to_string()
}

/// Short uppercase abbreviation for a zone name.
/// Uses the first meaningful word, capped at 6 characters.
///
/// "ICU"       → "ICU"
/// "Guest Wifi" → "GUEST"
/// "Management" → "MGMT"
/// "Radiology"  → "RAD"
fn zone_abbrev(name: &str) -> String {
    let first_word = name.split_whitespace().next().unwrap_or(name);
    let upper = first_word.to_uppercase();

    // Common abbreviation overrides
    match upper.as_str() {
        "MANAGEMENT" | "MGMT" | "OOB" => "MGMT".to_string(),
        "GUEST" | "GUESTWIFI" | "WIFI" => "GUEST".to_string(),
        "DEMILITARIZED" | "DMZ" => "DMZ".to_string(),
        "TRANSIT" => "TRANSIT".to_string(),
        "CORE" => "CORE".to_string(),
        "WAN" => "WAN".to_string(),
        "LAN" => "LAN".to_string(),
        _ => upper.chars().take(6).collect(),
    }
}

/// Derives a human-readable semantic zone label from the raw zone name and role.
///
/// This mirrors the AI's `zone_semantic_label` inference but is deterministic —
/// it runs in `netgen-core` with no AI involvement.
pub fn semantic_zone_label(name: &str, role: &ZoneRole) -> String {
    let lower = name.to_lowercase();

    // Clinical / healthcare zones
    if lower.contains("icu") || lower.contains("intensive care") {
        return "Clinical-Critical".to_string();
    }
    if lower.contains("radiology") || lower.contains("imaging") {
        return "Clinical-Imaging".to_string();
    }
    if lower.contains("clinical") || lower.contains("patient") || lower.contains("medical") {
        return "Clinical".to_string();
    }
    if lower.contains("iomt") || lower.contains("medical device") || lower.contains("biomedical") {
        return "IoMT-Devices".to_string();
    }

    // Industrial / OT zones
    if lower.contains("scada") || lower.contains("ot-control") {
        return "OT-Control".to_string();
    }
    if lower.contains("historian") {
        return "OT-Historian".to_string();
    }
    if lower.contains("ot") || lower.contains("industrial") || lower.contains("operational") {
        return "OT-Network".to_string();
    }

    // Management / OOB
    if lower.contains("mgmt")
        || lower.contains("management")
        || lower.contains("oob")
        || lower.contains("out-of-band")
    {
        return "OOB-Management".to_string();
    }

    // Internet / guest / untrusted
    if lower.contains("guest") || lower.contains("wifi") || lower.contains("visitor") {
        return "Untrusted-Access".to_string();
    }
    if lower.contains("internet") || lower.contains("wan") {
        return "Internet".to_string();
    }

    // DMZ
    if lower.contains("dmz") || lower.contains("demilitarized") {
        return "DMZ".to_string();
    }

    // Transit / WAN interconnect
    if lower.contains("transit") || lower.contains("interconnect") {
        return "Transit".to_string();
    }

    // POS / retail
    if lower.contains("pos") || lower.contains("point of sale") || lower.contains("checkout") {
        return "POS-Network".to_string();
    }

    // Admin / corporate
    if lower.contains("admin") || lower.contains("corporate") || lower.contains("office") {
        return "Corporate".to_string();
    }

    // Fall back to role-derived label if name gives no signal
    match role {
        ZoneRole::Lan => titlecase(name),
        ZoneRole::Wan => "Internet".to_string(),
        ZoneRole::Dmz => "DMZ".to_string(),
        ZoneRole::Mgmt => "OOB-Management".to_string(),
        ZoneRole::Transit => "Transit".to_string(),
        ZoneRole::Oob => "OOB-Management".to_string(),
        ZoneRole::Guest => "Untrusted-Access".to_string(),
    }
}

fn titlecase(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{NetworkNode, SecurityZone, TopologyGraph};
    use crate::schema::{Vendor, ZoneRole};


    fn make_graph_with_nodes() -> TopologyGraph {
        let mut graph = TopologyGraph::new("test");

        let dmz_zone = SecurityZone::new("DMZ", ZoneRole::Dmz, 30);
        let lan_zone = SecurityZone::new("ICU", ZoneRole::Lan, 80);
        let mgmt_zone = SecurityZone::new("Management", ZoneRole::Mgmt, 90);

        let dmz_id = dmz_zone.id;
        let lan_id = lan_zone.id;
        let mgmt_id = mgmt_zone.id;

        graph.add_zone(dmz_zone);
        graph.add_zone(lan_zone);
        graph.add_zone(mgmt_zone);

        graph.add_node(NetworkNode::new("placeholder", NodeRole::Firewall, Vendor::Cisco, dmz_id));
        graph.add_node(NetworkNode::new("placeholder", NodeRole::Router, Vendor::Cisco, lan_id));
        graph.add_node(NetworkNode::new("placeholder", NodeRole::Switch, Vendor::Cisco, lan_id));
        graph.add_node(NetworkNode::new("placeholder", NodeRole::Switch, Vendor::Cisco, lan_id));
        graph.add_node(NetworkNode::new("placeholder", NodeRole::Router, Vendor::Cisco, mgmt_id));

        graph
    }

    #[test]
    fn firewall_in_dmz_gets_fw_dmz_label() {
        let mut graph = make_graph_with_nodes();
        LabellingEngine.apply(&mut graph);

        let fw = graph.nodes.iter().find(|n| n.role == NodeRole::Firewall).unwrap();
        assert_eq!(fw.label, "FW-DMZ-01");
    }

    #[test]
    fn switches_in_same_zone_get_sequential_indices() {
        let mut graph = make_graph_with_nodes();
        LabellingEngine.apply(&mut graph);

        let switches: Vec<_> = graph.nodes.iter()
            .filter(|n| n.role == NodeRole::Switch)
            .collect();

        assert_eq!(switches.len(), 2);
        let labels: std::collections::HashSet<_> = switches.iter().map(|n| n.label.as_str()).collect();
        assert!(labels.contains("SW-ICU-01"));
        assert!(labels.contains("SW-ICU-02"));
    }

    #[test]
    fn router_in_mgmt_zone_gets_mgmt_abbrev() {
        let mut graph = make_graph_with_nodes();
        LabellingEngine.apply(&mut graph);

        let rtr = graph.nodes.iter()
            .find(|n| n.role == NodeRole::Router && {
                let zone = graph.zones.iter().find(|z| z.id == n.zone_id).unwrap();
                zone.role == ZoneRole::Mgmt
            })
            .unwrap();

        assert_eq!(rtr.label, "RTR-MGMT-01");
    }

    #[test]
    fn icu_zone_gets_clinical_critical_label() {
        let mut graph = make_graph_with_nodes();
        LabellingEngine.apply(&mut graph);

        let icu = graph.zones.iter().find(|z| z.name == "ICU").unwrap();
        assert_eq!(icu.label, "Clinical-Critical");
    }

    #[test]
    fn management_zone_gets_oob_label() {
        let mut graph = make_graph_with_nodes();
        LabellingEngine.apply(&mut graph);

        let mgmt = graph.zones.iter().find(|z| z.name == "Management").unwrap();
        assert_eq!(mgmt.label, "OOB-Management");
    }

    #[test]
    fn dmz_zone_gets_dmz_label() {
        let mut graph = make_graph_with_nodes();
        LabellingEngine.apply(&mut graph);

        let dmz = graph.zones.iter().find(|z| z.name == "DMZ").unwrap();
        assert_eq!(dmz.label, "DMZ");
    }

    #[test]
    fn guest_wifi_zone_gets_untrusted_label() {
        assert_eq!(
            semantic_zone_label("Guest Wifi", &ZoneRole::Lan),
            "Untrusted-Access"
        );
    }

    #[test]
    fn scada_zone_gets_ot_control_label() {
        assert_eq!(
            semantic_zone_label("SCADA", &ZoneRole::Lan),
            "OT-Control"
        );
    }

    #[test]
    fn admin_zone_gets_corporate_label() {
        assert_eq!(
            semantic_zone_label("Admin", &ZoneRole::Lan),
            "Corporate"
        );
    }

    #[test]
    fn zone_abbrev_trims_long_names() {
        let abbrev = zone_abbrev("Radiology");
        assert!(abbrev.len() <= 6);
        assert_eq!(abbrev, abbrev.to_uppercase());
    }

    #[test]
    fn empty_graph_apply_does_not_panic() {
        let mut graph = TopologyGraph::new("empty");
        LabellingEngine.apply(&mut graph);
    }

    #[test]
    fn ap_role_gets_ap_prefix() {
        assert_eq!(node_type_abbrev(&NodeRole::Ap), "AP");
    }

    #[test]
    fn indices_restart_per_zone_and_type() {
        let mut graph = TopologyGraph::new("test");

        let zone_a = SecurityZone::new("LAN", ZoneRole::Lan, 70);
        let zone_b = SecurityZone::new("DMZ", ZoneRole::Dmz, 30);
        let a_id = zone_a.id;
        let b_id = zone_b.id;
        graph.add_zone(zone_a);
        graph.add_zone(zone_b);

        graph.add_node(NetworkNode::new("x", NodeRole::Router, Vendor::Cisco, a_id));
        graph.add_node(NetworkNode::new("x", NodeRole::Router, Vendor::Cisco, b_id));

        LabellingEngine.apply(&mut graph);

        let labels: Vec<_> = graph.nodes.iter().map(|n| n.label.clone()).collect();
        assert!(labels.contains(&"RTR-LAN-01".to_string()));
        assert!(labels.contains(&"RTR-DMZ-01".to_string()));
    }
}
