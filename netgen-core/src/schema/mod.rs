use ipnetwork::{Ipv4Network, Ipv6Network};
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};

pub const CURRENT_SCHEMA_VERSION: &str = "intent_schema_v1";

// ─── Root ─────────────────────────────────────────────────────────────────────

/// Root intent — the only thing the AI is allowed to produce.
/// Validated via serde + validator before entering the pipeline.
/// If validation fails the output is rejected; AI gets one retry.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct NetworkIntent {
    #[validate(nested)]
    pub metadata: IntentMetadata,
    #[validate(nested)]
    pub topology: TopologyIntent,
    #[validate(nested)]
    pub routing: RoutingIntent,
    #[validate(nested)]
    pub segmentation: SegmentationIntent,
    #[validate(nested)]
    pub addressing: AddressingIntent,
    pub services: ServicesIntent,
    pub constraints: ConstraintsIntent,
    #[validate(nested)]
    pub context: ContextIntent,
}

// ─── Metadata ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct IntentMetadata {
    #[validate(length(min = 1, max = 100))]
    pub name: String,
    pub environment: Environment,
    #[validate(length(min = 1))]
    pub vendor_preference: Vec<Vendor>,
    pub security_level: SecurityLevel,
    #[validate(custom(function = "validate_schema_version"))]
    pub schema_version: String,
}

fn validate_schema_version(version: &str) -> Result<(), ValidationError> {
    if version == CURRENT_SCHEMA_VERSION {
        Ok(())
    } else {
        let mut err = ValidationError::new("schema_version_mismatch");
        err.message = Some(
            format!(
                "expected '{}', got '{version}'",
                CURRENT_SCHEMA_VERSION
            )
            .into(),
        );
        Err(err)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Environment {
    Lab,
    Staging,
    Production,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Vendor {
    Cisco,
    Juniper,
    Mikrotik,
    PaloAlto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SecurityLevel {
    Basic,
    Standard,
    Hardened,
}

// ─── Topology ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct TopologyIntent {
    /// Number of network sites. 1–50.
    #[validate(range(min = 1, max = 50))]
    pub sites: u8,
    pub topology_type: TopologyType,
    pub redundancy_level: RedundancyLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TopologyType {
    HubSpoke,
    FullMesh,
    PartialMesh,
    PointToPoint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RedundancyLevel {
    None,
    Standard,
    High,
}

// ─── Routing ──────────────────────────────────────────────────────────────────

/// If OSPF is in protocols, ospf must be Some.
/// If BGP is in protocols, bgp must be Some.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[validate(schema(function = "validate_routing_consistency"))]
pub struct RoutingIntent {
    #[validate(length(min = 1))]
    pub protocols: Vec<RoutingProtocol>,
    #[validate(nested)]
    pub ospf: Option<OspfConfig>,
    #[validate(nested)]
    pub bgp: Option<BgpConfig>,
}

fn validate_routing_consistency(intent: &RoutingIntent) -> Result<(), ValidationError> {
    if intent.protocols.contains(&RoutingProtocol::Ospf) && intent.ospf.is_none() {
        return Err(ValidationError::new("ospf_config_required"));
    }
    if intent.protocols.contains(&RoutingProtocol::Bgp) && intent.bgp.is_none() {
        return Err(ValidationError::new("bgp_config_required"));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RoutingProtocol {
    Ospf,
    Bgp,
    Static,
    IsIs,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct OspfConfig {
    /// Number of OSPF areas. Area 0 (backbone) is always required.
    #[validate(range(min = 1, max = 50))]
    pub areas: u8,
    pub authentication: bool,
    pub area_types: Vec<AreaType>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AreaType {
    Backbone,
    Stub,
    Nssa,
    TotallyStub,
}

/// iBGP: local_as required.
/// eBGP: local_as and peer_as both required.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[validate(schema(function = "validate_bgp_config"))]
pub struct BgpConfig {
    pub enabled: bool,
    pub bgp_type: BgpType,
    pub local_as: Option<u32>,
    pub peer_as: Option<u32>,
}

fn validate_bgp_config(cfg: &BgpConfig) -> Result<(), ValidationError> {
    if !cfg.enabled {
        return Ok(());
    }
    if cfg.local_as.is_none() {
        return Err(ValidationError::new("local_as_required"));
    }
    if cfg.bgp_type == BgpType::External && cfg.peer_as.is_none() {
        return Err(ValidationError::new("peer_as_required_for_ebgp"));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BgpType {
    Internal,
    External,
}

// ─── Segmentation ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SegmentationIntent {
    pub dmz: bool,
    pub vlan_count: u8,
    #[validate(length(min = 1), nested)]
    pub zones: Vec<ZoneIntent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ZoneIntent {
    #[validate(length(min = 1, max = 64))]
    pub name: String,
    pub role: ZoneRole,
    /// 0–100; the engine derives inter-zone policy from the delta between zones
    #[validate(range(min = 0, max = 100))]
    pub trust_level: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ZoneRole {
    Lan,
    Wan,
    Dmz,
    Mgmt,
    Transit,
    Oob,
    Guest,
}

// ─── Addressing ───────────────────────────────────────────────────────────────

/// Ipv4Only → ipv4 must be Some.
/// Ipv6Only → ipv6 must be Some.
/// DualStack → both must be Some.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[validate(schema(function = "validate_addressing_consistency"))]
pub struct AddressingIntent {
    pub mode: AddressingMode,
    #[validate(nested)]
    pub ipv4: Option<Ipv4Config>,
    #[validate(nested)]
    pub ipv6: Option<Ipv6Config>,
}

fn validate_addressing_consistency(intent: &AddressingIntent) -> Result<(), ValidationError> {
    let needs_v4 = matches!(intent.mode, AddressingMode::Ipv4Only | AddressingMode::DualStack);
    let needs_v6 = matches!(intent.mode, AddressingMode::Ipv6Only | AddressingMode::DualStack);
    if needs_v4 && intent.ipv4.is_none() {
        return Err(ValidationError::new("ipv4_config_required"));
    }
    if needs_v6 && intent.ipv6.is_none() {
        return Err(ValidationError::new("ipv6_config_required"));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddressingMode {
    Ipv4Only,
    DualStack,
    Ipv6Only,
}

/// base_range must be a valid, non-host IPv4 CIDR (e.g. "10.0.0.0/8").
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[validate(schema(function = "validate_ipv4_config"))]
pub struct Ipv4Config {
    pub base_range: String,
    pub scheme: AddressingScheme,
}

fn validate_ipv4_config(cfg: &Ipv4Config) -> Result<(), ValidationError> {
    cfg.base_range
        .parse::<Ipv4Network>()
        .map_err(|_| ValidationError::new("invalid_ipv4_cidr"))?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddressingScheme {
    CidrOptimized,
    Classful,
}

/// ula_prefix and gua_prefix, when provided, must be valid IPv6 CIDR prefixes.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[validate(schema(function = "validate_ipv6_config"))]
pub struct Ipv6Config {
    /// Auto-generated per RFC 4193 if absent — logged in decision log
    pub ula_prefix: Option<String>,
    pub gua_prefix: Option<String>,
    /// Always true in practice; non-negotiable per RFC 4291
    pub link_local: bool,
}

fn validate_ipv6_config(cfg: &Ipv6Config) -> Result<(), ValidationError> {
    if let Some(ref prefix) = cfg.ula_prefix {
        prefix
            .parse::<Ipv6Network>()
            .map_err(|_| ValidationError::new("invalid_ula_prefix"))?;
    }
    if let Some(ref prefix) = cfg.gua_prefix {
        prefix
            .parse::<Ipv6Network>()
            .map_err(|_| ValidationError::new("invalid_gua_prefix"))?;
    }
    Ok(())
}

// ─── Services, Constraints, Context ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServicesIntent {
    pub internet_gateway: bool,
    pub vpn: bool,
    pub nat: bool,
    pub dns: bool,
    pub ntp: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintsIntent {
    pub no_single_point_of_failure: bool,
    pub strict_security: bool,
    /// Downgrades Block/AutoFix security actions to Warn; allows insecure lab exports
    pub lab_mode: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ContextIntent {
    pub industry: Option<Industry>,
    pub compliance_profiles: Vec<Compliance>,
    pub departments: Vec<String>,
    #[validate(nested)]
    pub custom_labels: Vec<CustomLabel>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Industry {
    Healthcare,
    Education,
    Retail,
    Industrial,
    Enterprise,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Compliance {
    Hipaa,
    PciDss,
    Iso27001,
    Iec62443,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CustomLabel {
    #[validate(length(min = 1))]
    pub target: String,
    #[validate(length(min = 1))]
    pub label: String,
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use validator::Validate;

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn minimal_valid_intent() -> NetworkIntent {
        NetworkIntent {
            metadata: IntentMetadata {
                name: "test-network".into(),
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
                zones: vec![
                    ZoneIntent { name: "LAN".into(), role: ZoneRole::Lan, trust_level: 80 },
                    ZoneIntent { name: "MGMT".into(), role: ZoneRole::Mgmt, trust_level: 90 },
                ],
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

    // ── Valid intents ─────────────────────────────────────────────────────────

    #[test]
    fn valid_minimal_lab_intent() {
        assert!(minimal_valid_intent().validate().is_ok());
    }

    #[test]
    fn valid_full_production_intent() {
        let intent = NetworkIntent {
            metadata: IntentMetadata {
                name: "hospital-network".into(),
                environment: Environment::Production,
                vendor_preference: vec![Vendor::Cisco, Vendor::Juniper],
                security_level: SecurityLevel::Hardened,
                schema_version: CURRENT_SCHEMA_VERSION.into(),
            },
            topology: TopologyIntent {
                sites: 3,
                topology_type: TopologyType::HubSpoke,
                redundancy_level: RedundancyLevel::High,
            },
            routing: RoutingIntent {
                protocols: vec![RoutingProtocol::Ospf],
                ospf: Some(OspfConfig {
                    areas: 2,
                    authentication: true,
                    area_types: vec![AreaType::Backbone, AreaType::Stub],
                }),
                bgp: None,
            },
            segmentation: SegmentationIntent {
                dmz: true,
                vlan_count: 6,
                zones: vec![
                    ZoneIntent { name: "ICU".into(), role: ZoneRole::Lan, trust_level: 95 },
                    ZoneIntent { name: "Radiology".into(), role: ZoneRole::Dmz, trust_level: 70 },
                    ZoneIntent { name: "Admin".into(), role: ZoneRole::Lan, trust_level: 60 },
                    ZoneIntent { name: "Guest".into(), role: ZoneRole::Guest, trust_level: 10 },
                    ZoneIntent { name: "MGMT".into(), role: ZoneRole::Mgmt, trust_level: 100 },
                ],
            },
            addressing: AddressingIntent {
                mode: AddressingMode::DualStack,
                ipv4: Some(Ipv4Config {
                    base_range: "10.10.0.0/16".into(),
                    scheme: AddressingScheme::CidrOptimized,
                }),
                ipv6: Some(Ipv6Config {
                    ula_prefix: Some("fd12:3456:789a::/48".into()),
                    gua_prefix: None,
                    link_local: true,
                }),
            },
            services: ServicesIntent {
                internet_gateway: true,
                vpn: false,
                nat: true,
                dns: true,
                ntp: true,
            },
            constraints: ConstraintsIntent {
                no_single_point_of_failure: true,
                strict_security: true,
                lab_mode: false,
            },
            context: ContextIntent {
                industry: Some(Industry::Healthcare),
                compliance_profiles: vec![Compliance::Hipaa],
                departments: vec!["ICU".into(), "Radiology".into(), "Admin".into()],
                custom_labels: vec![],
            },
        };
        assert!(intent.validate().is_ok());
    }

    #[test]
    fn valid_ebgp_intent() {
        let mut intent = minimal_valid_intent();
        intent.routing = RoutingIntent {
            protocols: vec![RoutingProtocol::Bgp],
            ospf: None,
            bgp: Some(BgpConfig {
                enabled: true,
                bgp_type: BgpType::External,
                local_as: Some(65001),
                peer_as: Some(65002),
            }),
        };
        assert!(intent.validate().is_ok());
    }

    #[test]
    fn valid_ibgp_no_peer_as() {
        let mut intent = minimal_valid_intent();
        intent.routing = RoutingIntent {
            protocols: vec![RoutingProtocol::Bgp],
            ospf: None,
            bgp: Some(BgpConfig {
                enabled: true,
                bgp_type: BgpType::Internal,
                local_as: Some(65001),
                peer_as: None, // iBGP doesn't require a separate peer AS
            }),
        };
        assert!(intent.validate().is_ok());
    }

    #[test]
    fn valid_ipv6_only() {
        let mut intent = minimal_valid_intent();
        intent.addressing = AddressingIntent {
            mode: AddressingMode::Ipv6Only,
            ipv4: None,
            ipv6: Some(Ipv6Config {
                ula_prefix: Some("fd00::/48".into()),
                gua_prefix: None,
                link_local: true,
            }),
        };
        assert!(intent.validate().is_ok());
    }

    // ── Invalid: metadata ─────────────────────────────────────────────────────

    #[test]
    fn invalid_empty_name() {
        let mut intent = minimal_valid_intent();
        intent.metadata.name = "".into();
        let err = intent.validate().unwrap_err();
        assert!(err.to_string().contains("name"));
    }

    #[test]
    fn invalid_schema_version() {
        let mut intent = minimal_valid_intent();
        intent.metadata.schema_version = "intent_schema_v0".into();
        let err = intent.validate().unwrap_err();
        assert!(err.to_string().contains("schema_version"));
    }

    #[test]
    fn invalid_empty_vendor_preference() {
        let mut intent = minimal_valid_intent();
        intent.metadata.vendor_preference = vec![];
        assert!(intent.validate().is_err());
    }

    // ── Invalid: topology ─────────────────────────────────────────────────────

    #[test]
    fn invalid_zero_sites() {
        let mut intent = minimal_valid_intent();
        intent.topology.sites = 0;
        assert!(intent.validate().is_err());
    }

    #[test]
    fn invalid_too_many_sites() {
        let mut intent = minimal_valid_intent();
        intent.topology.sites = 51;
        assert!(intent.validate().is_err());
    }

    // ── Invalid: routing ──────────────────────────────────────────────────────

    #[test]
    fn invalid_ospf_in_protocols_without_config() {
        let mut intent = minimal_valid_intent();
        intent.routing = RoutingIntent {
            protocols: vec![RoutingProtocol::Ospf],
            ospf: None,
            bgp: None,
        };
        let err = intent.validate().unwrap_err();
        assert!(err.to_string().contains("ospf_config_required"));
    }

    #[test]
    fn invalid_bgp_in_protocols_without_config() {
        let mut intent = minimal_valid_intent();
        intent.routing = RoutingIntent {
            protocols: vec![RoutingProtocol::Bgp],
            ospf: None,
            bgp: None,
        };
        assert!(intent.validate().is_err());
    }

    #[test]
    fn invalid_ebgp_missing_peer_as() {
        let mut intent = minimal_valid_intent();
        intent.routing = RoutingIntent {
            protocols: vec![RoutingProtocol::Bgp],
            ospf: None,
            bgp: Some(BgpConfig {
                enabled: true,
                bgp_type: BgpType::External,
                local_as: Some(65001),
                peer_as: None, // eBGP requires peer_as
            }),
        };
        let err = intent.validate().unwrap_err();
        assert!(err.to_string().contains("peer_as_required_for_ebgp"));
    }

    #[test]
    fn invalid_empty_protocols() {
        let mut intent = minimal_valid_intent();
        intent.routing.protocols = vec![];
        assert!(intent.validate().is_err());
    }

    // ── Invalid: segmentation ─────────────────────────────────────────────────

    #[test]
    fn invalid_zero_zones() {
        let mut intent = minimal_valid_intent();
        intent.segmentation.zones = vec![];
        assert!(intent.validate().is_err());
    }

    #[test]
    fn invalid_trust_level_over_100() {
        let mut intent = minimal_valid_intent();
        intent.segmentation.zones[0].trust_level = 101;
        assert!(intent.validate().is_err());
    }

    #[test]
    fn invalid_empty_zone_name() {
        let mut intent = minimal_valid_intent();
        intent.segmentation.zones[0].name = "".into();
        assert!(intent.validate().is_err());
    }

    // ── Invalid: addressing ───────────────────────────────────────────────────

    #[test]
    fn invalid_ipv4only_without_ipv4_config() {
        let mut intent = minimal_valid_intent();
        intent.addressing = AddressingIntent {
            mode: AddressingMode::Ipv4Only,
            ipv4: None,
            ipv6: None,
        };
        let err = intent.validate().unwrap_err();
        assert!(err.to_string().contains("ipv4_config_required"));
    }

    #[test]
    fn invalid_dualstack_missing_ipv6_config() {
        let mut intent = minimal_valid_intent();
        intent.addressing = AddressingIntent {
            mode: AddressingMode::DualStack,
            ipv4: Some(Ipv4Config {
                base_range: "10.0.0.0/8".into(),
                scheme: AddressingScheme::CidrOptimized,
            }),
            ipv6: None,
        };
        let err = intent.validate().unwrap_err();
        assert!(err.to_string().contains("ipv6_config_required"));
    }

    #[test]
    fn invalid_ipv4_cidr_notation() {
        let mut intent = minimal_valid_intent();
        intent.addressing.ipv4 = Some(Ipv4Config {
            base_range: "not-a-cidr".into(),
            scheme: AddressingScheme::CidrOptimized,
        });
        let err = intent.validate().unwrap_err();
        assert!(err.to_string().contains("invalid_ipv4_cidr"));
    }

    #[test]
    fn invalid_ula_prefix_format() {
        let mut intent = minimal_valid_intent();
        intent.addressing = AddressingIntent {
            mode: AddressingMode::DualStack,
            ipv4: Some(Ipv4Config {
                base_range: "10.0.0.0/8".into(),
                scheme: AddressingScheme::CidrOptimized,
            }),
            ipv6: Some(Ipv6Config {
                ula_prefix: Some("not-an-ipv6-prefix".into()),
                gua_prefix: None,
                link_local: true,
            }),
        };
        let err = intent.validate().unwrap_err();
        assert!(err.to_string().contains("invalid_ula_prefix"));
    }

    // ── Serialisation round-trip ──────────────────────────────────────────────

    #[test]
    fn serde_round_trip() {
        let original = minimal_valid_intent();
        let json = serde_json::to_string(&original).expect("serialise");
        let restored: NetworkIntent = serde_json::from_str(&json).expect("deserialise");
        assert!(restored.validate().is_ok());
        assert_eq!(restored.metadata.name, original.metadata.name);
        assert_eq!(restored.metadata.schema_version, CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn unknown_field_in_json_is_rejected() {
        // The AI may inject extra fields; serde must reject them.
        let bad_json = r#"{
            "metadata": {},
            "unknown_field": "should not be here"
        }"#;
        let result = serde_json::from_str::<NetworkIntent>(bad_json);
        assert!(result.is_err());
    }
}
