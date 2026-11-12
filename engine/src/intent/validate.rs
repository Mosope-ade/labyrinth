use super::types::*;
use crate::{
    diagnostics::{Checks, IssueCode::*},
    model::{Feature, Protocol, SiteId, ZoneId},
};
use std::collections::BTreeSet;

pub(super) fn sites(
    input: &PartialIntent,
    c: &mut Checks,
    need: &mut impl FnMut(&str, &str),
) -> (BTreeSet<SiteId>, BTreeSet<ZoneId>, bool) {
    let mut site_ids = BTreeSet::new();
    let mut zone_ids = BTreeSet::new();
    let mut zones_complete = true;
    if let Some(sites) = &input.sites {
        c.require(
            !sites.is_empty(),
            InvalidValue,
            "/sites",
            "at least one site is required",
        );
        for (i, site) in sites.iter().enumerate() {
            let p = format!("/sites/{i}");
            c.require(
                site_ids.insert(site.id.clone()),
                DuplicateId,
                format!("{p}/id"),
                "duplicate site ID",
            );
            if let Some(location) = &site.location {
                c.require(
                    !location.trim().is_empty() && location.len() <= 128,
                    InvalidValue,
                    format!("{p}/location"),
                    "location must contain 1-128 nonblank bytes",
                );
            }
            if let Some(providers) = &site.isp_failure_domains {
                c.require(
                    providers.len() <= 16
                        && providers.iter().collect::<BTreeSet<_>>().len() == providers.len(),
                    InvalidValue,
                    format!("{p}/isp_failure_domains"),
                    "provider domains must be distinct (at most 16)",
                );
            }
            match &site.zones {
                None => {
                    zones_complete = false;
                    need(&format!("{p}/zones"), "declare the site's security zones");
                }
                Some(zones) => {
                    c.require(
                        (1..=64).contains(&zones.len()),
                        LimitExceeded,
                        format!("{p}/zones"),
                        "expected 1-64 zones per site",
                    );
                    for (j, zone) in zones.iter().take(64).enumerate() {
                        let zp = format!("{p}/zones/{j}");
                        c.require(
                            zone_ids.insert(zone.id.clone()),
                            DuplicateId,
                            format!("{zp}/id"),
                            "zone IDs must be globally unique within intent",
                        );
                        match zone.endpoint_count {
                            None => {
                                need(&format!("{zp}/endpoint_count"), "provide endpoint capacity")
                            }
                            Some(n) => c.require(
                                n > 0,
                                InvalidValue,
                                format!("{zp}/endpoint_count"),
                                "endpoint count must be positive",
                            ),
                        }
                        if let Some(attachment) = &zone.endpoint_attachment {
                            c.require(attachment.speed_mbps>0 && attachment.media!=crate::model::Media::Virtual && attachment.poe_watts_per_endpoint!=Some(0),InvalidValue,format!("{zp}/endpoint_attachment"),"endpoint attachment needs positive speed, physical media and positive known PoE watts");
                            c.require(
                                attachment.poe_watts_per_endpoint.is_none()
                                    || attachment.media == crate::model::Media::Copper,
                                Conflict,
                                format!("{zp}/endpoint_attachment"),
                                "powered endpoint attachment requires copper",
                            );
                        }
                        if let (Some(watts), Some(maximum), Some(count)) = (
                            zone.poe_load_watts,
                            zone.endpoint_attachment
                                .as_ref()
                                .and_then(|a| a.poe_watts_per_endpoint),
                            zone.endpoint_count,
                        ) {
                            c.require(u64::from(watts)<=u64::from(maximum)*u64::from(count),Conflict,format!("{zp}/poe_load_watts"),"aggregate PoE load exceeds endpoint count times declared per-port maximum");
                        }
                        match &zone.required_features {
                            None => need(
                                &format!("{zp}/required_features"),
                                "declare operating/security capabilities, or explicitly provide an empty list",
                            ),
                            Some(features) => {
                                c.require(
                                    features.len() <= 32
                                        && features.iter().collect::<BTreeSet<_>>().len()
                                            == features.len(),
                                    InvalidValue,
                                    format!("{zp}/required_features"),
                                    "features must be distinct (at most 32)",
                                );
                                if features.contains(&Feature::Poe)
                                    && zone
                                        .endpoint_attachment
                                        .as_ref()
                                        .and_then(|a| a.poe_watts_per_endpoint)
                                        .is_none()
                                {
                                    need(
                                        &format!("{zp}/endpoint_attachment/poe_watts_per_endpoint"),
                                        "declare maximum power required by each endpoint port",
                                    );
                                }
                                c.require(
                                    zone.endpoint_attachment
                                        .as_ref()
                                        .and_then(|a| a.poe_watts_per_endpoint)
                                        .is_none()
                                        || features.contains(&Feature::Poe),
                                    Conflict,
                                    format!("{zp}/endpoint_attachment"),
                                    "per-port power requires PoE capability",
                                );
                                if features.contains(&Feature::Poe) && zone.poe_load_watts.is_none()
                                {
                                    need(
                                        &format!("{zp}/poe_load_watts"),
                                        "provide simultaneous PoE load",
                                    );
                                }
                                if let Some(watts) = zone.poe_load_watts {
                                    c.require(
                                        features.contains(&Feature::Poe) && watts > 0,
                                        Conflict,
                                        format!("{zp}/poe_load_watts"),
                                        "PoE load must be positive and require PoE capability",
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    } else {
        zones_complete = false;
        need("/sites", "provide sites and zones");
    }
    (site_ids, zone_ids, zones_complete)
}
pub(super) fn traffic(
    input: &PartialIntent,
    zone_ids: &BTreeSet<ZoneId>,
    zones_complete: bool,
    c: &mut Checks,
    need: &mut impl FnMut(&str, &str),
) {
    match &input.demands {
        None => need(
            "/demands",
            "declare simultaneous directional peak demand; explicit empty list means no inter-zone demand",
        ),
        Some(demands) => {
            let mut ids = BTreeSet::new();
            for (i, d) in demands.iter().enumerate() {
                let p = format!("/demands/{i}");
                c.require(
                    ids.insert(&d.id),
                    DuplicateId,
                    format!("{p}/id"),
                    "duplicate demand ID",
                );
                c.require(
                    d.peak_mbps > 0,
                    InvalidValue,
                    format!("{p}/peak_mbps"),
                    "peak demand must be positive integer Mbps",
                );
                c.require(
                    d.from_zone != d.to_zone,
                    InvalidValue,
                    &p,
                    "inter-zone demand endpoints must differ",
                );
                if zones_complete {
                    for (field, z) in [("from_zone", &d.from_zone), ("to_zone", &d.to_zone)] {
                        c.require(
                            zone_ids.contains(z),
                            UnknownReference,
                            format!("{p}/{field}"),
                            "zone does not exist",
                        );
                    }
                }
            }
        }
    }
    match &input.policies {
        None => need(
            "/policies",
            "declare allowed/isolated flows; explicit empty list means default deny",
        ),
        Some(policies) => {
            let mut ids = BTreeSet::new();
            for (i, p) in policies.iter().enumerate() {
                let pp = format!("/policies/{i}");
                c.require(
                    ids.insert(&p.id),
                    DuplicateId,
                    format!("{pp}/id"),
                    "duplicate policy ID",
                );
                c.require(
                    p.from_zone != p.to_zone,
                    InvalidValue,
                    &pp,
                    "inter-zone policy endpoints must differ",
                );
                if zones_complete {
                    for (field, z) in [("from_zone", &p.from_zone), ("to_zone", &p.to_zone)] {
                        c.require(
                            zone_ids.contains(z),
                            UnknownReference,
                            format!("{pp}/{field}"),
                            "zone does not exist",
                        );
                    }
                }
                let ranges = &p.service.destination_ports;
                c.require(
                    ranges.len() <= 128,
                    LimitExceeded,
                    format!("{pp}/service/destination_ports"),
                    "at most 128 port ranges",
                );
                c.require(
                    matches!(p.service.protocol, Protocol::Tcp | Protocol::Udp)
                        || ranges.is_empty(),
                    Conflict,
                    format!("{pp}/service"),
                    "only TCP/UDP can match destination ports",
                );
                for (j, r) in ranges.iter().take(128).enumerate() {
                    c.require(
                        r.start > 0 && r.start <= r.end,
                        InvalidValue,
                        format!("{pp}/service/destination_ports/{j}"),
                        "range must satisfy 1 <= start <= end <= 65535",
                    );
                }
            }
        }
    }
}
pub(super) fn tier3(input: &PartialIntent, c: &mut Checks, need: &mut impl FnMut(&str, &str)) {
    if input.redundancy == Some(RedundancyTier::Tier3Site)
        && let Some(sites) = &input.sites
    {
        let hubs: Vec<_> = sites.iter().filter(|s| s.role == SiteRole::Hub).collect();
        if hubs.len() < 2 {
            need(
                "/sites",
                "Tier3 needs at least two hub sites with distinct locations and independent connectivity",
            );
        }
        let mut locations = BTreeSet::new();
        for (i, site) in sites.iter().enumerate() {
            if site.location.is_none() {
                need(
                    &format!("/sites/{i}/location"),
                    "Tier3 requires explicit site failure-domain locations",
                );
            }
            if site.role == SiteRole::Hub
                && let Some(l) = &site.location
            {
                c.require(
                    locations.insert(l.trim().to_ascii_lowercase()),
                    Conflict,
                    format!("/sites/{i}/location"),
                    "hub locations must be distinct",
                );
            }
            if site
                .isp_failure_domains
                .as_ref()
                .is_none_or(|p| p.len() < 2)
            {
                need(
                    &format!("/sites/{i}/isp_failure_domains"),
                    "Tier3 needs at least two explicitly identified provider domains",
                );
            }
        }
    }
}
