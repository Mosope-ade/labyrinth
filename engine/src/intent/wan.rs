use super::*;
use crate::{
    diagnostics::{Checks, IssueCode::*},
    model::SiteId,
};
use std::collections::BTreeSet;
pub(super) fn validate(
    input: &PartialIntent,
    sites: &BTreeSet<SiteId>,
    c: &mut Checks,
    need: &mut impl FnMut(&str, &str),
) {
    let Some(services) = &input.wan_services else {
        if sites.len() > 1 {
            need(
                "/wan_services",
                "confirm explicit site/provider/bandwidth/connected-site WAN attachments; ISP names never imply connectivity",
            );
        }
        return;
    };
    let mut keys = BTreeSet::new();
    for (i, s) in services.iter().enumerate() {
        let p = format!("/wan_services/{i}");
        c.require(
            keys.insert((&s.site, &s.provider_failure_domain)),
            DuplicateId,
            &p,
            "duplicate site/provider attachment",
        );
        c.require(
            s.committed_bandwidth_mbps > 0,
            InvalidValue,
            &p,
            "committed WAN bandwidth must be positive and explicit",
        );
        if input.sites.is_some() {
            c.require(
                sites.contains(&s.site),
                UnknownReference,
                &p,
                "WAN attachment references unknown site",
            );
        }
        c.require(
            s.connected_sites.len() <= 63
                && s.connected_sites.iter().collect::<BTreeSet<_>>().len()
                    == s.connected_sites.len(),
            LimitExceeded,
            &p,
            "at most 63 distinct connected sites",
        );
        for peer in s.connected_sites.iter().take(63) {
            c.require(
                peer != &s.site && (input.sites.is_none() || sites.contains(peer)),
                UnknownReference,
                &p,
                "connected peer must be a distinct known site",
            );
            c.require(services.iter().any(|other|&other.site==peer && other.provider_failure_domain==s.provider_failure_domain && other.connected_sites.contains(&s.site)),Conflict,&p,"WAN service connectivity must be explicitly reciprocal on the same provider domain");
        }
        if let Some(site) = input
            .sites
            .as_ref()
            .and_then(|ss| ss.iter().find(|site| site.id == s.site))
            && let Some(providers) = &site.isp_failure_domains
        {
            c.require(
                providers.contains(&s.provider_failure_domain),
                Conflict,
                &p,
                "explicit attachment provider contradicts site's declared provider domains",
            );
        }
    }
    if input.redundancy == Some(RedundancyTier::Tier3Site) {
        for site in sites {
            let providers: BTreeSet<_> = services
                .iter()
                .filter(|w| &w.site == site)
                .map(|w| &w.provider_failure_domain)
                .collect();
            if providers.len() < 2 {
                need(
                    "/wan_services",
                    "Tier3 requires two confirmed provider service attachments per site; ISP-name lists do not substitute for them",
                );
            }
        }
    }
    if sites.len() > 1 {
        for site in sites {
            c.require(
                services
                    .iter()
                    .any(|s| &s.site == site && !s.connected_sites.is_empty()),
                Conflict,
                "/wan_services",
                "each multi-site design site needs a connected explicit WAN attachment",
            );
        }
    }
}
