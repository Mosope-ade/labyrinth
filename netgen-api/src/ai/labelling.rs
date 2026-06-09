use serde::{Deserialize, Serialize};

// ─── Industry ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Industry {
    Healthcare,
    Retail,
    Education,
    Industrial,
    Enterprise,
}

impl Industry {
    /// The compliance profile auto-loaded when this industry is detected.
    pub fn compliance_profile(&self) -> &'static str {
        match self {
            Industry::Healthcare => "HIPAA",
            Industry::Retail => "PCI-DSS",
            Industry::Education => "None",
            Industry::Industrial => "IEC 62443",
            Industry::Enterprise => "ISO 27001",
        }
    }

    /// Zone names the AI pre-populates when this industry is detected.
    pub fn suggested_zones(&self) -> &'static [&'static str] {
        match self {
            Industry::Healthcare => &["Clinical", "Non-Clinical", "IoMT", "Admin", "Guest"],
            Industry::Retail => &["POS", "Back-Office", "Guest"],
            Industry::Education => &["Student", "Staff", "Admin", "Guest"],
            Industry::Industrial => &["IT", "OT", "SCADA", "Historian", "DMZ"],
            Industry::Enterprise => &["LAN", "DMZ", "MGMT", "Guest"],
        }
    }
}

// ─── Industry detection ───────────────────────────────────────────────────────

/// Detects industry context from accumulated conversation text.
/// Returns the strongest match, or `None` if no signals are found.
/// Earlier signals (more specific keywords) take priority.
pub fn detect_industry(text: &str) -> Option<Industry> {
    let lower = text.to_lowercase();

    // Healthcare — most specific first so it wins over generic "enterprise"
    if contains_any(
        &lower,
        &[
            "hospital", "clinic", "icu", "patient", "hipaa", "clinical",
            "radiology", "ehr", "medical", "healthcare", "iomt", "nurse",
            "pharmacy", "ward", "physician",
        ],
    ) {
        return Some(Industry::Healthcare);
    }

    // Industrial / OT — before retail/enterprise to catch SCADA correctly
    if contains_any(
        &lower,
        &[
            "scada", "plc", "ot network", "industrial", "modbus", "dnp3",
            "iec 62443", "historian", "control system", "factory", "ics",
        ],
    ) {
        return Some(Industry::Industrial);
    }

    // Retail / PCI
    if contains_any(
        &lower,
        &[
            "pos", "point of sale", "retail", "pci", "checkout",
            "payment card", "storefront", "merchant",
        ],
    ) {
        return Some(Industry::Retail);
    }

    // Education
    if contains_any(
        &lower,
        &[
            "school", "university", "college", "student", "campus",
            "classroom", "faculty", "education", "dorm",
        ],
    ) {
        return Some(Industry::Education);
    }

    // Enterprise — broadest, only if an explicit signal is present
    if contains_any(
        &lower,
        &["enterprise", "corporate", "headquarter", "hq", "iso 27001", "branch office"],
    ) {
        return Some(Industry::Enterprise);
    }

    None
}

fn contains_any(text: &str, keywords: &[&str]) -> bool {
    keywords.iter().any(|kw| text.contains(kw))
}

// ─── Zone semantic labels ─────────────────────────────────────────────────────

/// Maps a raw zone name (from user input or AI) to the semantic label
/// that appears in diagrams, reports, and documentation.
pub fn zone_semantic_label(raw: &str) -> String {
    match raw.to_lowercase().replace(['-', '_', ' '], "").as_str() {
        "icu" => "Clinical-Critical".into(),
        "radiology" | "imaging" => "Clinical-Imaging".into(),
        "admin" | "administration" => "Corporate".into(),
        "guestwifi" | "guest" => "Untrusted-Access".into(),
        "mgmt" | "management" | "oob" => "OOB-Management".into(),
        "pos" | "pointofsale" => "Payment-Zone".into(),
        "scada" => "OT-Control".into(),
        "ot" | "otnetwork" => "OT-Network".into(),
        "dmz" => "DMZ".into(),
        "wan" => "WAN".into(),
        "lan" => "LAN".into(),
        "transit" => "Transit".into(),
        "clinical" => "Clinical".into(),
        "nonclinical" => "Non-Clinical".into(),
        "iomt" => "IoMT".into(),
        "staff" => "Staff".into(),
        "student" => "Student".into(),
        _ => titlecase(raw),
    }
}

fn titlecase(s: &str) -> String {
    s.split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── detect_industry ───────────────────────────────────────────────────────

    #[test]
    fn detects_healthcare_from_hospital() {
        assert_eq!(detect_industry("Design a hospital network"), Some(Industry::Healthcare));
    }

    #[test]
    fn detects_healthcare_from_icu() {
        assert_eq!(detect_industry("ICU and radiology zones needed"), Some(Industry::Healthcare));
    }

    #[test]
    fn detects_healthcare_from_hipaa() {
        assert_eq!(detect_industry("must be HIPAA compliant"), Some(Industry::Healthcare));
    }

    #[test]
    fn detects_industrial_from_scada() {
        assert_eq!(detect_industry("SCADA and historian separation"), Some(Industry::Industrial));
    }

    #[test]
    fn detects_industrial_from_ot_network() {
        assert_eq!(detect_industry("OT network with PLC isolation"), Some(Industry::Industrial));
    }

    #[test]
    fn detects_retail_from_pos() {
        assert_eq!(detect_industry("POS terminals and back-office"), Some(Industry::Retail));
    }

    #[test]
    fn detects_retail_from_pci() {
        assert_eq!(detect_industry("need PCI-DSS compliance"), Some(Industry::Retail));
    }

    #[test]
    fn detects_education_from_campus() {
        assert_eq!(detect_industry("university campus student wifi"), Some(Industry::Education));
    }

    #[test]
    fn detects_enterprise_from_hq() {
        assert_eq!(detect_industry("enterprise HQ with branch offices"), Some(Industry::Enterprise));
    }

    #[test]
    fn returns_none_for_generic_text() {
        assert_eq!(detect_industry("3 routers 2 switches"), None);
    }

    #[test]
    fn healthcare_beats_enterprise_when_both_present() {
        // Healthcare keywords should win over generic enterprise
        let text = "enterprise hospital network with HIPAA compliance";
        assert_eq!(detect_industry(text), Some(Industry::Healthcare));
    }

    #[test]
    fn case_insensitive_detection() {
        assert_eq!(detect_industry("HOSPITAL NETWORK"), Some(Industry::Healthcare));
        assert_eq!(detect_industry("Retail Store Network"), Some(Industry::Retail));
    }

    // ── zone_semantic_label ───────────────────────────────────────────────────

    #[test]
    fn icu_maps_to_clinical_critical() {
        assert_eq!(zone_semantic_label("icu"), "Clinical-Critical");
    }

    #[test]
    fn radiology_maps_correctly() {
        assert_eq!(zone_semantic_label("radiology"), "Clinical-Imaging");
    }

    #[test]
    fn mgmt_maps_to_oob_management() {
        assert_eq!(zone_semantic_label("mgmt"), "OOB-Management");
        assert_eq!(zone_semantic_label("management"), "OOB-Management");
    }

    #[test]
    fn guest_maps_to_untrusted_access() {
        assert_eq!(zone_semantic_label("guest"), "Untrusted-Access");
    }

    #[test]
    fn scada_maps_to_ot_control() {
        assert_eq!(zone_semantic_label("scada"), "OT-Control");
    }

    #[test]
    fn unknown_zone_is_titlecased() {
        assert_eq!(zone_semantic_label("sales floor"), "Sales Floor");
        assert_eq!(zone_semantic_label("server room"), "Server Room");
    }

    #[test]
    fn industry_compliance_profiles_are_correct() {
        assert_eq!(Industry::Healthcare.compliance_profile(), "HIPAA");
        assert_eq!(Industry::Retail.compliance_profile(), "PCI-DSS");
        assert_eq!(Industry::Industrial.compliance_profile(), "IEC 62443");
    }

    #[test]
    fn industry_suggested_zones_are_nonempty() {
        for industry in [
            Industry::Healthcare,
            Industry::Retail,
            Industry::Education,
            Industry::Industrial,
            Industry::Enterprise,
        ] {
            assert!(!industry.suggested_zones().is_empty());
        }
    }
}
