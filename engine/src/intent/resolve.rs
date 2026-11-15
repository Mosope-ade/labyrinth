use super::types::*;
use crate::diagnostics::{Checks, IssueCode, ValidationErrors};
use crate::model::Ipv4Prefix;
use std::collections::{BTreeMap, BTreeSet};

/// Validate supplied values before returning clarification; never default critical facts.
pub fn resolve(input: PartialIntent) -> Result<Readiness, ValidationErrors> {
    use IssueCode::*;
    let mut c = Checks::default();
    c.require(
        input.schema_version == INTENT_SCHEMA_VERSION,
        UnsupportedVersion,
        "/schema_version",
        "expected intent_v1",
    );
    c.require(
        input.sites.as_ref().is_none_or(|s| s.len() <= 64),
        LimitExceeded,
        "/sites",
        "at most 64 sites per intent boundary",
    );
    c.require(
        input.demands.as_ref().is_none_or(|d| d.len() <= 4096),
        LimitExceeded,
        "/demands",
        "at most 4096 demands",
    );
    c.require(
        input.policies.as_ref().is_none_or(|p| p.len() <= 4096),
        LimitExceeded,
        "/policies",
        "at most 4096 policies",
    );
    c.require(
        input.excluded_ipv4.as_ref().is_none_or(|e| e.len() <= 1024),
        LimitExceeded,
        "/excluded_ipv4",
        "at most 1024 excluded ranges",
    );
    c.require(
        input.wan_services.as_ref().is_none_or(|s| s.len() <= 1024),
        LimitExceeded,
        "/wan_services",
        "at most 1024 explicit WAN attachments",
    );
    c.finish()?;
    let exclusions = input.excluded_ipv4.clone().unwrap_or_default();
    let mut c = Checks::default();
    let mut missing = Vec::new();
    let mut need = |path: &str, reason: &str| {
        missing.push(MissingField {
            path: path.into(),
            reason: reason.into(),
        })
    };
    match &input.name {
        None => need("/name", "name the design"),
        Some(n) => c.require(
            !n.trim().is_empty() && n.len() <= 100,
            InvalidValue,
            "/name",
            "name must contain 1-100 nonblank bytes",
        ),
    }
    if input.redundancy.is_none() {
        need(
            "/redundancy",
            "select a failure tier; no resilience tier is inferred",
        );
    }
    if input.update.is_none() {
        need(
            "/update",
            "choose redesign or incremental update explicitly",
        );
    }
    let (site_ids, zone_ids, zones_complete) = super::validate::sites(&input, &mut c, &mut need);
    super::wan::validate(&input, &site_ids, &mut c, &mut need);
    super::validate::traffic(&input, &zone_ids, zones_complete, &mut c, &mut need);
    if let Some(g) = input.growth_percent {
        c.require(
            g <= 100,
            InvalidValue,
            "/growth_percent",
            "supported growth is 0-100 percent",
        );
    }
    if let Some(p) = input.spare_port_target_percent {
        c.require(
            p < 100,
            InvalidValue,
            "/spare_port_target_percent",
            "spare-port target must be 0-99 percent",
        );
    }
    let pool = input
        .ipv4_pool
        .clone()
        .unwrap_or_else(|| Ipv4Prefix::new("10.0.0.0/8").expect("valid documented default"));
    for (i, e) in exclusions.iter().enumerate() {
        c.require(
            !e.contains(&pool),
            Conflict,
            format!("/excluded_ipv4/{i}"),
            "excluded range covers the entire requested/default pool",
        );
    }
    if let Some(UpdateMode::IncrementalUpdate {
        ipam_pins,
        core_pins,
        ..
    }) = &input.update
    {
        c.require(
            !ipam_pins.is_empty() && !core_pins.is_empty(),
            Conflict,
            "/update",
            "incremental update requires existing allocation and core-device pins",
        );
        c.require(
            ipam_pins.len() <= 4096 && core_pins.len() <= 4096,
            LimitExceeded,
            "/update",
            "at most 4096 pins per class",
        );
        let mut pinned_zones = BTreeSet::new();
        for (i, pin) in ipam_pins.iter().take(4096).enumerate() {
            let p = format!("/update/ipam_pins/{i}");
            c.require(
                pinned_zones.insert(&pin.zone),
                DuplicateId,
                &p,
                "duplicate zone allocation pin",
            );
            if zones_complete {
                c.require(
                    zone_ids.contains(&pin.zone),
                    UnknownReference,
                    &p,
                    "pinned zone must be present in the full updated intent",
                );
            }
            c.require(
                pool.contains(&pin.subnet) && !exclusions.iter().any(|e| e.overlaps(&pin.subnet)),
                Conflict,
                &p,
                "pin must fit pool and avoid excluded space",
            );
            for previous in ipam_pins.iter().take(i.min(4096)) {
                c.require(
                    !pin.subnet.overlaps(&previous.subnet),
                    Conflict,
                    &p,
                    "pinned allocations overlap",
                );
            }
        }
        let mut pinned_devices = BTreeSet::new();
        for (i, pin) in core_pins.iter().take(4096).enumerate() {
            let p = format!("/update/core_pins/{i}");
            c.require(
                pinned_devices.insert(&pin.device),
                DuplicateId,
                &p,
                "duplicate core device pin",
            );
            c.require(
                site_ids.contains(&pin.site),
                UnknownReference,
                &p,
                "pinned core site must exist",
            );
        }
    }
    super::validate::tier3(&input, &mut c, &mut need);
    c.finish()?;
    if !missing.is_empty() {
        return Ok(Readiness::NeedsClarification(missing));
    }
    // Only this branch constructs immutable resolved input; all critical values were checked.
    let mut provenance = BTreeMap::new();
    for (path, provided) in [
        ("/growth_percent", input.growth_percent.is_some()),
        (
            "/spare_port_target_percent",
            input.spare_port_target_percent.is_some(),
        ),
        ("/ipv4_pool", input.ipv4_pool.is_some()),
        ("/blueprint", input.blueprint.is_some()),
        ("/excluded_ipv4", input.excluded_ipv4.is_some()),
        ("/wan_services", input.wan_services.is_some()),
    ] {
        provenance.insert(
            path.into(),
            if provided {
                ValueSource::Provided
            } else {
                ValueSource::EngineDefault
            },
        );
    }
    for path in [
        "/name",
        "/sites",
        "/demands",
        "/policies",
        "/redundancy",
        "/update",
    ] {
        provenance.insert(path.into(), ValueSource::Provided);
    }
    let sites = input
        .sites
        .expect("readiness invariant")
        .into_iter()
        .enumerate()
        .map(|(i, s)| {
            let zones = s
                .zones
                .expect("readiness invariant")
                .into_iter()
                .enumerate()
                .map(|(j, z)| {
                    for field in ["id", "endpoint_count", "required_features"] {
                        provenance.insert(
                            format!("/sites/{i}/zones/{j}/{field}"),
                            ValueSource::Provided,
                        );
                    }
                    if z.poe_load_watts.is_some() {
                        provenance.insert(
                            format!("/sites/{i}/zones/{j}/poe_load_watts"),
                            ValueSource::Provided,
                        );
                    }
                    provenance.insert(
                        format!("/sites/{i}/zones/{j}/endpoint_attachment"),
                        if z.endpoint_attachment.is_some() {
                            ValueSource::Provided
                        } else {
                            ValueSource::EngineDefault
                        },
                    );
                    ResolvedZone {
                        id: z.id,
                        endpoint_count: z.endpoint_count.expect("readiness invariant"),
                        required_features: z.required_features.expect("readiness invariant"),
                        poe_load_watts: z.poe_load_watts,
                        endpoint_attachment: z.endpoint_attachment.unwrap_or_default(),
                    }
                })
                .collect();
            ResolvedSite {
                id: s.id,
                role: s.role,
                location: s.location,
                isp_failure_domains: s.isp_failure_domains,
                zones,
            }
        })
        .collect();
    let data = IntentData {
        schema_version: input.schema_version,
        name: input.name.expect("readiness invariant"),
        sites,
        demands: input.demands.expect("readiness invariant"),
        policies: input.policies.expect("readiness invariant"),
        wan_services: input.wan_services.unwrap_or_default(),
        redundancy: input.redundancy.expect("readiness invariant"),
        blueprint: input.blueprint.unwrap_or(BlueprintChoice::Automatic),
        growth_percent: input.growth_percent.unwrap_or(20),
        spare_port_target_percent: input.spare_port_target_percent.unwrap_or(20),
        ipv4_pool: pool,
        excluded_ipv4: exclusions,
        update: input.update.expect("readiness invariant"),
    };
    Ok(Readiness::Ready(Box::new(ResolvedIntent {
        data,
        provenance,
    })))
}
