use super::*;
use crate::{catalog::CatalogSnapshot, model::Feature};
use std::collections::BTreeSet;

pub fn filter_candidates(
    req: &DeviceRequirement,
    catalog: &CatalogSnapshot,
    target: u8,
) -> Result<CandidateSet, SelectionError> {
    validate_requirement(req, target)?;
    let mut candidates = Vec::new();
    let mut rejections = Vec::new();
    let mut archetypes: Vec<_> = catalog.archetypes().iter().collect();
    archetypes.sort_by_key(|a| &a.id);
    for a in archetypes {
        let mut modes: Vec<_> = a.operating_modes.iter().collect();
        modes.sort_by_key(|m| &m.id);
        for m in modes {
            let mut reasons = Vec::new();
            if req.pinned_mode.as_ref().is_some_and(|pin| pin != &m.id) {
                reasons.push(RejectReason::PinConflict);
            }
            if !a.roles.contains(&req.role) {
                reasons.push(RejectReason::Role);
            }
            if req.pinned_archetype.as_ref().is_some_and(|p| p != &a.id) {
                reasons.push(RejectReason::PinConflict);
            }
            if !req.required_features.iter().all(|f| m.features.contains(f)) {
                reasons.push(RejectReason::ModeFeatures);
            }
            match m.throughput_mbps {
                None => reasons.push(RejectReason::UnknownThroughput),
                Some(cap) if cap < req.peak_mbps => reasons.push(RejectReason::Throughput),
                _ => {}
            }
            if req.poe_watts.is_some_and(|w| {
                !m.features.contains(&Feature::Poe)
                    || a.poe_budget_watts.is_none_or(|b| u64::from(b) < w)
            }) {
                reasons.push(RejectReason::PoeBudget);
            }
            let allocation = super::ports::allocate(req, a, target);
            if allocation.is_none() {
                reasons.push(RejectReason::PortCompatibility);
            }
            if reasons.is_empty() {
                let (port_assignments, compatible_port_count, spare_shortfall_ppm) =
                    allocation.expect("allocation checked");
                candidates.push(Candidate {
                    archetype: a.id.clone(),
                    mode: m.id.clone(),
                    cost_tier: a.relative_cost_tier,
                    used_port_count: req.ports.iter().map(|p| u32::from(p.count)).sum(),
                    port_assignments,
                    compatible_port_count,
                    spare_shortfall_ppm,
                });
            } else {
                rejections.push(CandidateRejection {
                    archetype: a.id.clone(),
                    mode: m.id.clone(),
                    reasons,
                });
            }
        }
    }
    Ok(CandidateSet {
        candidates,
        rejections,
    })
}
fn validate_requirement(req: &DeviceRequirement, target: u8) -> Result<(), SelectionError> {
    if target >= 100
        || req.ports.is_empty()
        || req.ports.len() > 64
        || req.required_features.len() > 32
        || req.required_features.iter().collect::<BTreeSet<_>>().len()
            != req.required_features.len()
        || req
            .ports
            .iter()
            .map(|p| &p.id)
            .collect::<BTreeSet<_>>()
            .len()
            != req.ports.len()
        || req
            .ports
            .iter()
            .any(|p| p.count == 0 || p.speed_mbps == 0 || p.min_poe_watts == Some(0))
        || (req.required_features.contains(&Feature::Poe)
            && (req.poe_watts.is_none() || !req.ports.iter().any(|p| p.min_poe_watts.is_some())))
        || req.poe_watts == Some(0)
        || (req.poe_watts.is_some() && !req.required_features.contains(&Feature::Poe))
        || (req.ports.iter().any(|p| p.min_poe_watts.is_some())
            && (req.poe_watts.is_none() || !req.required_features.contains(&Feature::Poe)))
    {
        return Err(SelectionError::InvalidRequirement(format!(
            "{}: require distinct port demands/features, positive port demands (candidates are capped at 256 physical ports), explicit PoE bounds, target below 100%",
            req.id
        )));
    }
    Ok(())
}
