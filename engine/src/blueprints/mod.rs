//! Versioned blueprint rules produce requirements; they do not verify generated networks.
mod builder;
mod types;
use crate::{catalog::CatalogSnapshot, intent::*, model::*, selection::*};
use builder::Builder;
pub(crate) use builder::stable_id;
use std::collections::{BTreeMap, BTreeSet};
pub use types::*;

pub fn plan(
    intent: &ResolvedIntent,
    catalog: &CatalogSnapshot,
    pass: u8,
) -> Result<BlueprintPlan, SelectionError> {
    HubSpokeBlueprint.plan(intent, catalog, pass)
}
pub(crate) fn plan_pinned(
    intent: &ResolvedIntent,
    catalog: &CatalogSnapshot,
    pass: u8,
    pins: Option<&PinnedConstraints>,
) -> Result<BlueprintPlan, SelectionError> {
    if !(1..=3).contains(&pass) {
        return Err(SelectionError::InvalidRequirement(
            "blueprint repair pass must be 1-3".into(),
        ));
    }
    let input = intent.data();
    if pins.is_none() && matches!(input.update, UpdateMode::IncrementalUpdate { .. }) {
        return Err(SelectionError::PinConflict("incremental role planning requires a prior-revision context, not yet implemented; pins will not be relaxed".into()));
    }
    let grown_endpoints = input
        .sites
        .iter()
        .flat_map(|s| &s.zones)
        .try_fold(0u64, |sum, z| {
            sum.checked_add(with_growth(
                u64::from(z.endpoint_count),
                input.growth_percent,
            )?)
            .ok_or(SelectionError::ArithmeticOverflow)
        })?;
    let chosen = match input.blueprint {
        BlueprintChoice::Automatic if input.sites.len() > 1 => BlueprintChoice::HubSpoke,
        BlueprintChoice::Automatic if input.sites[0].role == SiteRole::DataCenter => {
            BlueprintChoice::SpineLeaf
        }
        BlueprintChoice::Automatic if grown_endpoints > 512 => BlueprintChoice::ThreeTierCampus,
        BlueprintChoice::Automatic => BlueprintChoice::HubSpoke,
        explicit => explicit,
    };
    if chosen != BlueprintChoice::HubSpoke {
        return Err(SelectionError::UnsupportedBlueprint(format!(
            "{chosen:?} recommended/requested; only hub-spoke role planning is implemented; no silent fallback"
        )));
    }
    if input.sites.len() > 1 && !input.sites.iter().any(|s| s.role == SiteRole::Hub) {
        return Err(SelectionError::ClarificationRequired(
            "multi-site hub-spoke needs an explicitly designated hub site".into(),
        ));
    }
    let resilient = input.redundancy != RedundancyTier::Tier0None;
    let device_ha = matches!(
        input.redundancy,
        RedundancyTier::Tier2Device | RedundancyTier::Tier3Site
    );
    let peers = if device_ha { 2 } else { 1 };
    let parallel = if resilient && !device_ha { 2 } else { 1 };
    let scale = 1u64 << (pass - 1);
    let role_count = input.sites.iter().try_fold(0u64, |sum, site| {
        let access = site.zones.iter().try_fold(0u64, |total, z| {
            total
                .checked_add(
                    with_growth(u64::from(z.endpoint_count), input.growth_percent)?.min(
                        scale
                            * pins
                                .and_then(|p| p.access_counts.get(&z.id))
                                .copied()
                                .unwrap_or(1),
                    ),
                )
                .ok_or(SelectionError::ArithmeticOverflow)
        })?;
        let zones = u64::try_from(site.zones.len()).expect("bounded sites");
        let providers = input
            .wan_services
            .iter()
            .filter(|w| w.site == site.id)
            .count();
        let edges = if providers == 0 {
            0
        } else {
            u64::try_from(providers)
                .expect("bounded providers")
                .max(peers)
        };
        sum.checked_add(peers + zones * peers * 2 + access + edges)
            .ok_or(SelectionError::ArithmeticOverflow)
    })?;
    if role_count > 128 {
        return Err(SelectionError::InvalidRequirement(
            "blueprint exceeds current 128-device selection admission limit".into(),
        ));
    }
    let mut b = Builder::new(catalog);
    b.decisions.push(format!("blueprint_rules_v3 selected HubSpoke: sites={}, grown_endpoints={grown_endpoints}, tier={:?}, explicit={}",input.sites.len(),input.redundancy,input.blueprint!=BlueprintChoice::Automatic));
    b.decisions.push(format!("repair pass {pass}: {scale} access partitions per zone (bounded by endpoint count); growth applied once; full-zone peak retained per access device as conservative endpoint-distribution bound"));
    let mut layer2_domains = Vec::new();
    let mut firewall_ha = Vec::new();
    let zone_sites: BTreeMap<_, _> = input
        .sites
        .iter()
        .flat_map(|s| s.zones.iter().map(move |z| (&z.id, &s.id)))
        .collect();
    let mut sites: Vec<_> = input.sites.iter().collect();
    sites.sort_by_key(|s| &s.id);
    for site in sites {
        let site_load = sum_demands(input, |d| {
            zone_sites[&d.from_zone] == &site.id
                || zone_sites[&d.to_zone] == &site.id
                || (site.role == SiteRole::Hub
                    && zone_sites[&d.from_zone] != zone_sites[&d.to_zone])
        })?;
        let mut infrastructure = vec![Feature::Routing, Feature::Switching, Feature::Vlan];
        infrastructure.extend(
            site.zones
                .iter()
                .flat_map(|z| z.required_features.iter().copied())
                .filter(|f| {
                    matches!(
                        f,
                        Feature::Ospf
                            | Feature::Bgp
                            | Feature::StaticRouting
                            | Feature::Ecmp
                            | Feature::SessionSteering
                    )
                }),
        );
        let core_features = features(infrastructure, resilient, device_ha);
        let mut cores = Vec::new();
        for i in 0..peers {
            let id = DeviceId::new(stable_id("core", &[site.id.as_str()], i)).expect("derived ID");
            b.add(
                id.clone(),
                &site.id,
                None,
                DeviceRole::CoreSwitch,
                core_features.clone(),
                site_load,
            );
            cores.push(id);
        }
        let mut zones: Vec<_> = site.zones.iter().collect();
        zones.sort_by_key(|z| &z.id);
        for zone in zones {
            let mut l2_links = Vec::new();
            let zone_load = sum_demands(input, |d| d.from_zone == zone.id || d.to_zone == zone.id)?;
            let firewall_features: Vec<_> = zone
                .required_features
                .iter()
                .copied()
                .filter(|f| {
                    matches!(
                        f,
                        Feature::Firewall
                            | Feature::Ips
                            | Feature::Vpn
                            | Feature::Nat
                            | Feature::HighAvailability
                            | Feature::Ospf
                            | Feature::Bgp
                            | Feature::StaticRouting
                            | Feature::Lacp
                            | Feature::SessionSteering
                    )
                })
                .chain([Feature::Routing, Feature::Firewall])
                .collect();
            let mut firewall_features = features(firewall_features, resilient, device_ha);
            if parallel > 1 {
                firewall_features.push(Feature::Lacp);
            }
            firewall_features.sort();
            firewall_features.dedup();
            let mut distributions = Vec::new();
            let mut distribution_features = vec![Feature::Switching, Feature::Vlan];
            distribution_features.extend(
                zone.required_features
                    .iter()
                    .copied()
                    .filter(|f| matches!(f, Feature::Lacp | Feature::RapidSpanningTree)),
            );
            if resilient {
                distribution_features.push(Feature::Lacp);
            }
            if device_ha {
                distribution_features.push(Feature::RapidSpanningTree);
            }
            distribution_features.sort();
            distribution_features.dedup();
            for i in 0..peers {
                let id = DeviceId::new(stable_id("dist", &[site.id.as_str(), zone.id.as_str()], i))
                    .expect("derived ID");
                b.add(
                    id.clone(),
                    &site.id,
                    Some(&zone.id),
                    DeviceRole::DistributionSwitch,
                    distribution_features.clone(),
                    zone_load,
                );
                distributions.push(id);
            }
            if device_ha {
                for p in 0..2 {
                    l2_links.push(b.connect(&distributions[0], &distributions[1], zone_load, p)?);
                }
            }
            let mut firewalls = Vec::new();
            for i in 0..peers {
                let id = DeviceId::new(stable_id("fw", &[site.id.as_str(), zone.id.as_str()], i))
                    .expect("derived ID");
                b.add(
                    id.clone(),
                    &site.id,
                    Some(&zone.id),
                    DeviceRole::Firewall,
                    firewall_features.clone(),
                    if device_ha {
                        zone_load
                            .checked_mul(2)
                            .ok_or(SelectionError::ArithmeticOverflow)?
                    } else {
                        zone_load
                    },
                );
                for core in &cores {
                    for p in 0..parallel {
                        b.connect(&id, core, zone_load, p)?;
                    }
                }
                // Each firewall has one attachment (or Tier1 parallel bundle) to
                // its distribution peer. No implicit bridging inside a firewall.
                for p in 0..parallel {
                    l2_links.push(b.connect(
                        &id,
                        &distributions[usize::try_from(i).expect("small peer index")],
                        zone_load,
                        p,
                    )?);
                }
                firewalls.push(id);
            }
            if device_ha {
                let mut sync_links = Vec::new();
                for p in 0..2 {
                    sync_links.push(b.connect(&firewalls[0], &firewalls[1], zone_load, p)?);
                }
                firewall_ha.push(FirewallHaRequirement {
                    zone: zone.id.clone(),
                    devices: firewalls.clone(),
                    sync_links,
                });
            }
            let endpoints = with_growth(u64::from(zone.endpoint_count), input.growth_percent)?;
            let access_count = (scale
                * pins
                    .and_then(|p| p.access_counts.get(&zone.id))
                    .copied()
                    .unwrap_or(1))
            .min(endpoints);
            for i in 0..access_count {
                let population = endpoints / access_count + u64::from(i < endpoints % access_count);
                let count = u16::try_from(population).map_err(|_| {
                    SelectionError::Infeasible(format!(
                        "zone {} exceeds representable access-port density in pass {pass}",
                        zone.id
                    ))
                })?;
                let id = DeviceId::new(stable_id("acc", &[site.id.as_str(), zone.id.as_str()], i))
                    .expect("derived ID");
                let mut fs = vec![Feature::Switching, Feature::Routing, Feature::Vlan];
                fs.extend(zone.required_features.iter().copied().filter(|f| {
                    matches!(
                        f,
                        Feature::Switching
                            | Feature::Routing
                            | Feature::Vlan
                            | Feature::Poe
                            | Feature::Lacp
                            | Feature::Ospf
                            | Feature::Bgp
                            | Feature::StaticRouting
                            | Feature::Ecmp
                            | Feature::RapidSpanningTree
                    )
                }));
                if parallel > 1 {
                    fs.push(Feature::Lacp);
                }
                if device_ha {
                    fs.push(Feature::RapidSpanningTree);
                }
                b.add(
                    id.clone(),
                    &site.id,
                    Some(&zone.id),
                    DeviceRole::AccessSwitch,
                    features(fs, false, false),
                    zone_load,
                );
                let req = b.devices.get_mut(&id).expect("known device");
                req.ports.push(PortDemand {
                    id: PortGroupId::new("endpoints").expect("literal ID"),
                    count,
                    speed_mbps: zone.endpoint_attachment.speed_mbps,
                    media: zone.endpoint_attachment.media,
                    min_poe_watts: zone.endpoint_attachment.poe_watts_per_endpoint,
                });
                if let Some(watts) = zone.poe_load_watts {
                    let per_port =
                        zone.endpoint_attachment
                            .poe_watts_per_endpoint
                            .ok_or_else(|| {
                                SelectionError::ClarificationRequired(format!(
                                    "zone {} needs per-endpoint PoE watts",
                                    zone.id
                                ))
                            })?;
                    req.poe_watts = Some(
                        with_growth(u64::from(watts), input.growth_percent)?
                            .min(population * u64::from(per_port)),
                    );
                }
                b.populations.push(AccessPopulation {
                    device: id.clone(),
                    site: site.id.clone(),
                    zone: zone.id.clone(),
                    endpoints: population,
                });
                for dist in &distributions {
                    for p in 0..parallel {
                        l2_links.push(b.connect(&id, dist, zone_load, p)?);
                    }
                }
            }
            l2_links.sort();
            layer2_domains.push(ZoneLayer2Requirement {
                site: site.id.clone(),
                zone: zone.id.clone(),
                links: l2_links,
                loop_prevention: device_ha.then_some(Feature::RapidSpanningTree),
            });
        }
        let mut providers: Vec<_> = input
            .wan_services
            .iter()
            .filter(|w| w.site == site.id)
            .map(|w| w.provider_failure_domain.clone())
            .collect();
        providers.sort();
        providers.dedup();
        if !providers.is_empty() {
            let edge_count = providers
                .len()
                .max(usize::try_from(peers).expect("small peer count"));
            for i in 0..edge_count {
                let id = DeviceId::new(stable_id(
                    "edge",
                    &[site.id.as_str()],
                    u64::try_from(i).expect("bounded edge count"),
                ))
                .expect("derived ID");
                b.add(
                    id.clone(),
                    &site.id,
                    None,
                    DeviceRole::WanEdge,
                    features(
                        site.zones
                            .iter()
                            .flat_map(|z| z.required_features.iter().copied())
                            .filter(|f| {
                                matches!(
                                    f,
                                    Feature::Ospf
                                        | Feature::Bgp
                                        | Feature::StaticRouting
                                        | Feature::Ecmp
                                        | Feature::SessionSteering
                                )
                            })
                            .chain([Feature::Routing])
                            .collect(),
                        resilient,
                        false,
                    ),
                    site_load,
                );
                for core in &cores {
                    for p in 0..parallel {
                        b.connect(&id, core, site_load, p)?;
                    }
                }
                let service = input
                    .wan_services
                    .iter()
                    .find(|w| {
                        w.site == site.id
                            && w.provider_failure_domain == providers[i % providers.len()]
                    })
                    .expect("explicit service attachment");
                b.devices.get_mut(&id).expect("edge requirement").peak_mbps =
                    site_load.max(service.committed_bandwidth_mbps);
                b.external(
                    &id,
                    &providers[i % providers.len()],
                    if resilient { 2 } else { 1 },
                )?;
            }
        }
    }
    if b.devices.len() > 128 {
        return Err(SelectionError::InvalidRequirement(
            "blueprint exceeds current 128-device selection admission limit".into(),
        ));
    }
    for d in b.devices.values_mut() {
        d.ports.sort_by_key(|p| p.id.clone());
    }
    b.connections.sort_by_key(|l| l.id.clone());
    b.populations.sort_by_key(|p| p.device.clone());
    b.placements.sort_by_key(|p| p.device.clone());
    b.external.sort_by_key(|e| e.device.clone());
    layer2_domains.sort_by(|a, b| (&a.site, &a.zone).cmp(&(&b.site, &b.zone)));
    firewall_ha.sort_by(|a, b| a.zone.cmp(&b.zone));
    b.decisions.push("zone distribution switches aggregate access VLANs; dual-homed access requires RSTP; parallel L2 members require LACP; HA sync cables are reserved separately; these requirements are not forwarding/HA proofs".into());
    b.decisions.push("every zone crosses dedicated firewall roles before reaching shared cores; actual forwarding/isolation proofs remain required; external provider attachments are requirements, not carrier connectivity evidence".into());
    Ok(BlueprintPlan {
        rules_version: "blueprint_rules_v3".into(),
        blueprint: chosen,
        pass,
        requirements: SelectionRequest {
            devices: b.devices.into_values().collect(),
            connections: b.connections,
            spare_port_target_percent: input.spare_port_target_percent,
            max_states: 100_000,
        },
        access_populations: b.populations,
        placements: b.placements,
        external_attachments: b.external,
        decisions: b.decisions,
        layer2_domains,
        firewall_ha,
    })
}
fn features(fs: Vec<Feature>, resilient: bool, ha: bool) -> Vec<Feature> {
    let mut fs: BTreeSet<_> = fs.into_iter().collect();
    if resilient {
        fs.insert(Feature::Ecmp);
    }
    if ha {
        fs.insert(Feature::HighAvailability);
    }
    fs.into_iter().collect()
}
fn sum_demands(
    input: &IntentData,
    include: impl Fn(&TrafficDemand) -> bool,
) -> Result<u64, SelectionError> {
    input
        .demands
        .iter()
        .filter(|d| include(d))
        .try_fold(0u64, |sum, d| {
            sum.checked_add(crate::selection::reserved_peak(d, input.growth_percent)?)
                .ok_or(SelectionError::ArithmeticOverflow)
        })
}

/// Blueprint boundary: scenario-specific rules emit the same typed requirements,
/// placements and L2/HA declarations consumed by the independent engine stages.
/// V1 registers hub-spoke only; other named scenarios fail explicitly.
pub trait Blueprint {
    fn kind(&self) -> BlueprintChoice;
    fn plan(
        &self,
        intent: &ResolvedIntent,
        catalog: &CatalogSnapshot,
        pass: u8,
    ) -> Result<BlueprintPlan, SelectionError>;
}
#[derive(Debug, Clone, Copy, Default)]
pub struct HubSpokeBlueprint;
impl Blueprint for HubSpokeBlueprint {
    fn kind(&self) -> BlueprintChoice {
        BlueprintChoice::HubSpoke
    }
    fn plan(
        &self,
        intent: &ResolvedIntent,
        catalog: &CatalogSnapshot,
        pass: u8,
    ) -> Result<BlueprintPlan, SelectionError> {
        plan_pinned(intent, catalog, pass, None)
    }
}
