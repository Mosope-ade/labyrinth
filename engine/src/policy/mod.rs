//! Deterministic stateless service matrices: deny-overrides and default deny.
use crate::{intent::ResolvedIntent, model::*};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Concrete IP protocol class; TCP/UDP ports use 1..65535. Other protocols have no port.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ServiceClass {
    pub ip_protocol: u8,
    pub destination_port: Option<u16>,
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct PolicyDecision {
    pub permitted: bool,
    pub matching_allows: Vec<PolicyId>,
    pub matching_denies: Vec<PolicyId>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum PolicyError {
    InvalidService,
    NoPermittedService(FlowId),
}
impl std::fmt::Display for PolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for PolicyError {}
fn matches(rule: &ServiceMatch, service: &ServiceClass) -> bool {
    let protocol = match rule.protocol {
        Protocol::Any => true,
        Protocol::Tcp => service.ip_protocol == 6,
        Protocol::Udp => service.ip_protocol == 17,
        Protocol::Icmp => service.ip_protocol == 1,
    };
    protocol
        && (rule.destination_ports.is_empty()
            || service.destination_port.is_some_and(|p| {
                rule.destination_ports
                    .iter()
                    .any(|r| r.start <= p && p <= r.end)
            }))
}
/// Input intent is already validated. This classifier never assumes return traffic.
pub fn evaluate(
    intent: &ResolvedIntent,
    from: &ZoneId,
    to: &ZoneId,
    service: &ServiceClass,
) -> Result<PolicyDecision, PolicyError> {
    if matches!(service.ip_protocol, 6 | 17) != service.destination_port.is_some()
        || service.destination_port == Some(0)
    {
        return Err(PolicyError::InvalidService);
    }
    Ok(classify(&intent.data().policies, from, to, service))
}
pub fn classify(
    rules: &[FlowPolicy],
    from: &ZoneId,
    to: &ZoneId,
    service: &ServiceClass,
) -> PolicyDecision {
    let mut allows = BTreeSet::new();
    let mut denies = BTreeSet::new();
    for p in rules {
        if &p.from_zone == from && &p.to_zone == to && matches(&p.service, service) {
            match p.action {
                PolicyAction::Allow => {
                    allows.insert(p.id.clone());
                }
                PolicyAction::Deny => {
                    denies.insert(p.id.clone());
                }
            }
        }
    }
    PolicyDecision {
        permitted: !allows.is_empty() && denies.is_empty(),
        matching_allows: allows.into_iter().collect(),
        matching_denies: denies.into_iter().collect(),
    }
}
/// Partition the complete protocol/port space at every rule boundary. One
/// representative per equivalence class is sufficient for exact stateless evaluation.
pub fn service_classes(intent: &ResolvedIntent) -> Vec<ServiceClass> {
    classes_for_rules(&intent.data().policies)
}
pub fn classes_for_rules(rules: &[FlowPolicy]) -> Vec<ServiceClass> {
    let mut result = Vec::new();
    for protocol in 0..=255u8 {
        if matches!(protocol, 6 | 17) {
            let mut cuts = BTreeSet::from([1u32, 65536]);
            for p in rules {
                if matches!(p.service.protocol, Protocol::Any)
                    || (protocol == 6 && p.service.protocol == Protocol::Tcp)
                    || (protocol == 17 && p.service.protocol == Protocol::Udp)
                {
                    for r in &p.service.destination_ports {
                        cuts.insert(u32::from(r.start));
                        cuts.insert(u32::from(r.end) + 1);
                    }
                }
            }
            for cut in cuts.into_iter().take_while(|p| *p < 65536) {
                result.push(ServiceClass {
                    ip_protocol: protocol,
                    destination_port: Some(u16::try_from(cut).expect("port boundary")),
                });
            }
        } else {
            result.push(ServiceClass {
                ip_protocol: protocol,
                destination_port: None,
            });
        }
    }
    result
}
pub fn validate_demands(intent: &ResolvedIntent) -> Result<(), PolicyError> {
    for d in &intent.data().demands {
        let rules: Vec<_> = intent
            .data()
            .policies
            .iter()
            .filter(|p| p.from_zone == d.from_zone && p.to_zone == d.to_zone)
            .cloned()
            .collect();
        let classes = classes_for_rules(&rules);
        if !classes
            .iter()
            .any(|s| evaluate(intent, &d.from_zone, &d.to_zone, s).is_ok_and(|v| v.permitted))
        {
            return Err(PolicyError::NoPermittedService(d.id.clone()));
        }
    }
    Ok(())
}

/// Install the identical stateless policy on all source gateway members and all
/// destination routed ingresses, including standby peers for failure recovery.
pub fn bindings(intent: &ResolvedIntent, network: &NetworkDraft) -> Vec<PolicyBinding> {
    let mut out = Vec::new();
    for p in &intent.data().policies {
        for d in network
            .devices
            .iter()
            .filter(|d| d.role == DeviceRole::Firewall)
        {
            let source = d
                .interfaces
                .iter()
                .any(|f| matches!(&f.kind,InterfaceKind::Gateway{zone,..} if zone==&p.from_zone));
            let target = d
                .interfaces
                .iter()
                .any(|f| matches!(&f.kind,InterfaceKind::Gateway{zone,..} if zone==&p.to_zone));
            for f in &d.interfaces {
                if (source
                    && matches!(&f.kind,InterfaceKind::Gateway{zone,..} if zone==&p.from_zone))
                    || (target && matches!(f.kind, InterfaceKind::Routed { .. }))
                {
                    out.push(PolicyBinding {
                        policy: p.clone(),
                        device: d.id.clone(),
                        ingress: f.id.clone(),
                    });
                }
            }
        }
    }
    out.sort_by(|a, b| {
        (&a.policy.id, &a.device, &a.ingress).cmp(&(&b.policy.id, &b.device, &b.ingress))
    });
    out
}
