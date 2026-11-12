use super::types::*;
use crate::diagnostics::{Checks, IssueCode, ValidationErrors};
use crate::model::{Feature, Media};
use std::collections::BTreeSet;

pub(super) fn validate(bundle: &CatalogBundle) -> Result<(), ValidationErrors> {
    use IssueCode::*;
    let mut c = Checks::default();
    c.require(
        bundle.schema_version == "catalog_v1",
        UnsupportedVersion,
        "/schema_version",
        "expected catalog_v1",
    );
    c.require(
        semver::Version::parse(&bundle.version).is_ok(),
        InvalidValue,
        "/version",
        "expected semantic version",
    );
    c.require(
        (1..=256).contains(&bundle.archetypes.len()),
        LimitExceeded,
        "/archetypes",
        "expected 1-256 archetypes",
    );
    let mut ids = BTreeSet::new();
    for (i, a) in bundle.archetypes.iter().enumerate() {
        let p = format!("/archetypes/{i}");
        c.require(
            ids.insert(&a.id),
            DuplicateId,
            format!("{p}/id"),
            "duplicate archetype ID",
        );
        c.require(
            a.relative_cost_tier > 0,
            InvalidValue,
            format!("{p}/relative_cost_tier"),
            "cost tier must be positive",
        );
        c.require(
            !a.roles.is_empty() && a.roles.len() <= 9 && unique(&a.roles),
            InvalidValue,
            format!("{p}/roles"),
            "expected nonempty distinct roles",
        );
        c.require(
            a.features.len() <= 32 && unique(&a.features),
            InvalidValue,
            format!("{p}/features"),
            "expected at most 32 distinct features",
        );
        c.require(
            (1..=32).contains(&a.port_groups.len()),
            LimitExceeded,
            format!("{p}/port_groups"),
            "expected 1-32 port groups",
        );
        c.require(
            (1..=32).contains(&a.operating_modes.len()),
            LimitExceeded,
            format!("{p}/operating_modes"),
            "expected 1-32 operating modes",
        );
        c.require(
            a.reference_hints.len() <= 32,
            LimitExceeded,
            format!("{p}/reference_hints"),
            "expected at most 32 reference hints",
        );
        for (j, hint) in a.reference_hints.iter().enumerate() {
            c.require(
                !hint.description.trim().is_empty() && hint.description.len() <= 256,
                InvalidValue,
                format!("{p}/reference_hints/{j}"),
                "reference description must be 1-256 bytes",
            );
        }
        let mut groups = BTreeSet::new();
        let mut max_poe = 0u64;
        for (j, g) in a.port_groups.iter().enumerate() {
            let gp = format!("{p}/port_groups/{j}");
            c.require(
                groups.insert(&g.id),
                DuplicateId,
                format!("{gp}/id"),
                "duplicate port-group ID",
            );
            c.require(
                g.count > 0,
                InvalidValue,
                format!("{gp}/count"),
                "port count must be positive",
            );
            c.require(
                (1..=16).contains(&g.speeds_mbps.len())
                    && g.speeds_mbps.iter().all(|s| *s > 0)
                    && unique(&g.speeds_mbps),
                InvalidValue,
                format!("{gp}/speeds_mbps"),
                "expected 1-16 distinct positive speeds",
            );
            c.require(
                g.shared_capacity_mbps != Some(0),
                InvalidValue,
                format!("{gp}/shared_capacity_mbps"),
                "known shared capacity must be positive",
            );
            if let Some(watts) = g.poe_watts_per_port {
                c.require(
                    watts > 0 && g.media == Media::Copper && a.features.contains(&Feature::Poe),
                    Conflict,
                    format!("{gp}/poe_watts_per_port"),
                    "PoE needs positive per-port watts, copper media and PoE capability",
                );
                max_poe += u64::from(watts) * u64::from(g.count);
            }
            if let Some(b) = &g.breakout {
                let total = u64::from(b.children_per_port).checked_mul(b.child_speed_mbps);
                c.require(
                    b.children_per_port >= 2
                        && b.child_speed_mbps > 0
                        && total
                            .zip(g.speeds_mbps.iter().max().copied())
                            .is_some_and(|(sum, max)| sum <= max),
                    Conflict,
                    format!("{gp}/breakout"),
                    "breakout needs at least two positive child lanes fitting a parent speed",
                );
                c.require(
                    g.media == Media::Fiber && g.poe_watts_per_port.is_none(),
                    Conflict,
                    format!("{gp}/breakout"),
                    "initial breakout contract supports non-PoE fiber ports only",
                );
            }
        }
        if let Some(watts) = a.poe_budget_watts {
            c.require(
                watts > 0 && u64::from(watts) <= max_poe && a.features.contains(&Feature::Poe),
                Conflict,
                format!("{p}/poe_budget_watts"),
                "known PoE budget must fit powered ports and require PoE capability",
            );
        }
        c.require(
            max_poe == 0 || a.poe_budget_watts.is_some(),
            Conflict,
            format!("{p}/poe_budget_watts"),
            "powered port groups require an explicit total PoE budget",
        );
        let base = a.operating_modes.iter().find(|m| m.id.as_str() == "base");
        c.require(
            base.is_some(),
            InvalidValue,
            format!("{p}/operating_modes"),
            "a base operating mode is required",
        );
        let mut modes = BTreeSet::new();
        for (j, m) in a.operating_modes.iter().enumerate() {
            let mp = format!("{p}/operating_modes/{j}");
            c.require(
                modes.insert(&m.id),
                DuplicateId,
                format!("{mp}/id"),
                "duplicate mode ID",
            );
            c.require(
                m.features.len() <= 32
                    && unique(&m.features)
                    && m.features.iter().all(|f| a.features.contains(f)),
                Conflict,
                format!("{mp}/features"),
                "mode features must be distinct supported archetype features",
            );
            c.require(
                m.throughput_mbps != Some(0),
                InvalidValue,
                format!("{mp}/throughput_mbps"),
                "known throughput must be positive",
            );
            if let (Some(limit), Some(throughput)) =
                (base.and_then(|b| b.throughput_mbps), m.throughput_mbps)
            {
                c.require(
                    throughput <= limit,
                    Conflict,
                    format!("{mp}/throughput_mbps"),
                    "mode throughput exceeds declared base throughput",
                );
            }
        }
    }
    c.finish()
}
fn unique<T: Ord>(items: &[T]) -> bool {
    items.iter().collect::<BTreeSet<_>>().len() == items.len()
}
