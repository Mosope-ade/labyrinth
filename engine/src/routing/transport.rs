use crate::{
    blueprints::stable_id,
    intent::{ResolvedIntent, SiteRole},
    model::*,
    topology::{AllocatedTopology, PortPurpose},
};
/// Build only declared reciprocal provider adjacencies chosen by hub-spoke rules.
pub fn build_transport(intent: &ResolvedIntent, t: &AllocatedTopology) -> Vec<WanConnection> {
    let sites = &intent.data().sites;
    let placements = &t.selection().blueprint.placements;
    let mut out = Vec::new();
    for a in &intent.data().wan_services {
        for b in &intent.data().wan_services {
            if a.site >= b.site
                || a.provider_failure_domain != b.provider_failure_domain
                || !a.connected_sites.contains(&b.site)
                || !b.connected_sites.contains(&a.site)
            {
                continue;
            }
            if !sites
                .iter()
                .any(|s| (s.id == a.site || s.id == b.site) && s.role == SiteRole::Hub)
            {
                continue;
            }
            let side = |site: &SiteId| {
                t.ports().iter().filter(|p|placements.iter().any(|d|d.device==p.device&&&d.site==site) && matches!(&p.purpose,PortPurpose::Provider{failure_domain} if failure_domain==&a.provider_failure_domain)).collect::<Vec<_>>()
            };
            let from = side(&a.site);
            let to = side(&b.site);
            for p in &from {
                for q in &to {
                    // Multiple physical handoffs are real reservations. Logical edges share
                    // one committed attachment budget; their capacities must never be summed.
                    let id = LinkId::new(stable_id(
                        "wan",
                        &[
                            a.site.as_str(),
                            b.site.as_str(),
                            a.provider_failure_domain.as_str(),
                            p.interface.as_str(),
                            q.interface.as_str(),
                        ],
                        0,
                    ))
                    .expect("derived ID");
                    out.push(WanConnection {
                        id,
                        from: LinkEndpoint {
                            device: p.device.clone(),
                            interface: p.interface.clone(),
                        },
                        to: LinkEndpoint {
                            device: q.device.clone(),
                            interface: q.interface.clone(),
                        },
                        provider_failure_domain: a.provider_failure_domain.clone(),
                        bandwidth_mbps: a
                            .committed_bandwidth_mbps
                            .min(b.committed_bandwidth_mbps)
                            .min(p.speed_mbps)
                            .min(q.speed_mbps),
                    });
                }
            }
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}
