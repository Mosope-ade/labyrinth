use crate::{
    intent::{RedundancyTier, ResolvedIntent},
    model::*,
    routing::FailureScenario,
    topology::{AllocatedTopology, PortPurpose},
};
pub(super) struct ScenarioSet {
    pub cases: Vec<(String, FailureScenario)>,
    pub protected_set: Vec<String>,
    pub exclusions: Vec<String>,
}
pub(super) fn cases(
    intent: &ResolvedIntent,
    t: &AllocatedTopology,
    wan: &[WanConnection],
) -> ScenarioSet {
    let mut protected_set = Vec::new();
    let mut exclusions = Vec::new();
    let protected: Vec<_> = t
        .selection()
        .assignment
        .devices
        .iter()
        .filter(|d| {
            matches!(
                d.role,
                DeviceRole::CoreSwitch
                    | DeviceRole::DistributionSwitch
                    | DeviceRole::Firewall
                    | DeviceRole::WanEdge
            )
        })
        .map(|d| d.requirement.clone())
        .collect();
    if matches!(
        intent.data().redundancy,
        RedundancyTier::Tier2Device | RedundancyTier::Tier3Site
    ) {
        protected_set = protected
            .iter()
            .map(|d| format!("backbone_device:{d}"))
            .collect();
    }
    if intent.data().redundancy == RedundancyTier::Tier0None {
        exclusions.push("Tier0 requests no link/device/site/provider failure guarantee".into());
    }
    let mut cases = Vec::new();
    if intent.data().redundancy != RedundancyTier::Tier0None {
        for l in t.links() {
            protected_set.push(format!("internal_link:{}", l.id));
            let mut f = FailureScenario::default();
            f.internal.links.insert(l.id.clone());
            cases.push((format!("internal_link:{}", l.id), f));
        }
        for w in wan {
            protected_set.push(format!("wan_transport:{}", w.id));
            cases.push((
                format!("wan_transport:{}", w.id),
                FailureScenario {
                    wan_links: [w.id.clone()].into(),
                    ..Default::default()
                },
            ));
        }
        for p in t
            .ports()
            .iter()
            .filter(|p| matches!(p.purpose, PortPurpose::Provider { .. }))
        {
            protected_set.push(format!("wan_handoff:{}", p.interface));
            cases.push((
                format!("wan_handoff:{}", p.interface),
                FailureScenario {
                    interfaces: [p.interface.clone()].into(),
                    ..Default::default()
                },
            ));
        }
    }
    if matches!(
        intent.data().redundancy,
        RedundancyTier::Tier2Device | RedundancyTier::Tier3Site
    ) {
        for d in &protected {
            let mut f = FailureScenario::default();
            f.internal.devices.insert(d.clone());
            cases.push((format!("device:{d}"), f));
        }
    }
    if intent.data().redundancy == RedundancyTier::Tier3Site {
        for s in &intent.data().sites {
            protected_set.push(format!("site:{}", s.id));
            cases.push((
                format!("site:{}", s.id),
                FailureScenario {
                    sites: [s.id.clone()].into(),
                    ..Default::default()
                },
            ));
        }
        let domains: std::collections::BTreeSet<_> = intent
            .data()
            .wan_services
            .iter()
            .map(|s| s.provider_failure_domain.clone())
            .collect();
        for d in domains {
            protected_set.push(format!("provider:{d}"));
            cases.push((
                format!("provider:{d}"),
                FailureScenario {
                    providers: [d].into(),
                    ..Default::default()
                },
            ));
        }
        exclusions.push("demands with an endpoint in the failed site are excluded; all surviving site/zone backbone paths are still required".into());
    }
    protected_set.sort();
    cases.sort_by(|a, b| a.0.cmp(&b.0));
    ScenarioSet {
        cases,
        protected_set,
        exclusions,
    }
}
