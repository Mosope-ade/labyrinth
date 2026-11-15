//! Incremental pipeline slices; no verified engineering package is produced yet.
pub use crate::topology::{PlanningError, RepairAttempt, SelectedPlan};
use crate::{
    blueprints,
    catalog::CatalogSnapshot,
    intent::ResolvedIntent,
    selection::{self, Bottleneck, SelectionError},
};
/// First feasible blueprint repair variant; exact ranking is limited to its fixed candidate set.
pub fn select_devices(
    intent: &ResolvedIntent,
    catalog: &CatalogSnapshot,
    max_states: u64,
) -> Result<SelectedPlan, SelectionError> {
    select_with_loopbacks(intent, catalog, max_states, false, None)
}
fn select_with_loopbacks(
    intent: &ResolvedIntent,
    catalog: &CatalogSnapshot,
    max_states: u64,
    loopbacks: bool,
    pins: Option<&crate::model::PinnedConstraints>,
) -> Result<SelectedPlan, SelectionError> {
    let mut attempts = Vec::new();
    let mut bottlenecks = Vec::new();
    for pass in 1..=3 {
        let blueprint = match prepared_plan(intent, catalog, pass, max_states, loopbacks, pins) {
            Ok(p) => p,
            Err(SelectionError::Infeasible(reason)) => {
                attempts.push(RepairAttempt {
                    pass,
                    outcome: reason.clone(),
                });
                bottlenecks.push(Bottleneck {
                    pass,
                    device: None,
                    message: reason,
                    rejections: vec![],
                });
                continue;
            }
            Err(error) => return Err(error),
        };
        match selection::assign(&blueprint.requirements, catalog) {
            Ok(assignment) => {
                attempts.push(RepairAttempt {
                    pass,
                    outcome: "fixed-topology assignment selected; not a verified network".into(),
                });
                return Ok(SelectedPlan {
                    blueprint,
                    assignment,
                    attempts,
                    repair_bottlenecks: bottlenecks,
                });
            }
            Err(SelectionError::NoCandidate {
                device,
                required: _,
                rejections,
            }) => {
                attempts.push(RepairAttempt {
                    pass,
                    outcome: format!("{device}: no catalog candidate for this role demand"),
                });
                bottlenecks.push(Bottleneck{pass,device:Some(device),message:"no mode/port assignment in this repair variant; not a global infeasibility proof".into(),rejections});
            }
            Err(SelectionError::Infeasible(reason)) => {
                attempts.push(RepairAttempt {
                    pass,
                    outcome: reason.clone(),
                });
                bottlenecks.push(Bottleneck {
                    pass,
                    device: None,
                    message: reason,
                    rejections: vec![],
                });
            }
            Err(error) => return Err(error),
        }
    }
    Err(SelectionError::RepairExhausted {
        passes: 3,
        bottlenecks,
    })
}

/// Select devices, bind actual catalog ports and allocate IPv4/VLANs. This stage
/// cannot be exported as a verified network; forwarding and HA realization follow.
pub fn allocate_topology(
    intent: &ResolvedIntent,
    catalog: &CatalogSnapshot,
    policy: &crate::topology::AddressingPolicy,
    max_states: u64,
) -> Result<crate::topology::AllocatedTopology, PlanningError> {
    allocate_with_pins(intent, catalog, policy, max_states, None)
}
pub(crate) fn allocate_with_pins(
    intent: &ResolvedIntent,
    catalog: &CatalogSnapshot,
    policy: &crate::topology::AddressingPolicy,
    max_states: u64,
    pins: Option<&crate::model::PinnedConstraints>,
) -> Result<crate::topology::AllocatedTopology, PlanningError> {
    validate_addressing(intent, policy)?;
    let selected =
        select_with_loopbacks(intent, catalog, max_states, policy.allocate_loopbacks, pins)?;
    crate::topology::build(intent, selected, policy, catalog, pins)
}

/// Assemble baseline canonical entities; route/policy synthesis and five-pass
/// verification are separate stages and are not granted by this return type.
pub fn assemble_network(
    intent: &ResolvedIntent,
    catalog: &CatalogSnapshot,
    addressing: &crate::topology::AddressingPolicy,
    logical: &crate::topology::LogicalPolicy,
    max_states: u64,
) -> Result<crate::model::ValidatedNetwork, PlanningError> {
    let allocated = allocate_topology(intent, catalog, addressing, max_states)?;
    crate::topology::assemble(&allocated, catalog, logical)
}

pub(crate) fn prepared_plan(
    intent: &ResolvedIntent,
    catalog: &CatalogSnapshot,
    pass: u8,
    max_states: u64,
    loopbacks: bool,
    pins: Option<&crate::model::PinnedConstraints>,
) -> Result<crate::blueprints::BlueprintPlan, SelectionError> {
    let mut blueprint = blueprints::plan_pinned(intent, catalog, pass, pins)?;
    blueprint.requirements.max_states = max_states;
    if intent.data().demands.iter().any(|d| {
        matches!(
            d.distribution,
            Some(crate::model::TrafficDistribution::Sessions { .. })
        )
    }) {
        for device in &mut blueprint.requirements.devices {
            if matches!(
                device.role,
                crate::model::DeviceRole::Firewall
                    | crate::model::DeviceRole::CoreSwitch
                    | crate::model::DeviceRole::WanEdge
                    | crate::model::DeviceRole::Router
            ) {
                device
                    .required_features
                    .push(crate::model::Feature::SessionSteering);
                device.required_features.sort();
                device.required_features.dedup();
            }
        }
        blueprint.decisions.push("session traffic requires controlled session steering in every selected data-plane routing mode; ECMP/LACP alone is insufficient".into());
    }
    if loopbacks {
        for device in &mut blueprint.requirements.devices {
            device
                .required_features
                .push(crate::model::Feature::Routing);
            device.required_features.sort();
            device.required_features.dedup();
        }
        blueprint.decisions.push("addressing policy requests /32 loopbacks: routing capability required on every selected mode".into());
    }
    if let Some(pins) = pins {
        for (id, core) in &pins.cores {
            let Some(req) = blueprint
                .requirements
                .devices
                .iter_mut()
                .find(|d| &d.id == id)
            else {
                return Err(SelectionError::PinConflict(format!(
                    "existing core {id} would be removed"
                )));
            };
            if req.role != crate::model::DeviceRole::CoreSwitch
                || !blueprint
                    .placements
                    .iter()
                    .any(|p| &p.device == id && p.site == core.site)
            {
                return Err(SelectionError::PinConflict(format!(
                    "existing core {id} changes role/site"
                )));
            }
            req.pinned_archetype = Some(core.archetype.clone());
            req.pinned_mode = Some(core.mode.clone());
        }
    }
    Ok(blueprint)
}

pub(crate) fn validate_addressing(
    intent: &ResolvedIntent,
    policy: &crate::topology::AddressingPolicy,
) -> Result<(), PlanningError> {
    let requires_ha = matches!(
        intent.data().redundancy,
        crate::intent::RedundancyTier::Tier2Device | crate::intent::RedundancyTier::Tier3Site
    );
    if (requires_ha && policy.redundant_gateway_reservations < 3)
        || policy.single_gateway_reservations == 0
        || policy.redundant_gateway_reservations == 0
        || policy.transit_prefix_len > 31
    {
        return Err(crate::ipam::AddressError::InvalidRequest {
            reason:
                "gateway reservations must be positive and HA needs at least three; transit prefix must fit two hosts (0..31)"
                    .into(),
        }
        .into());
    }
    crate::ipam::allocate_vlans(&[], &policy.vlans)?;
    Ok(())
}
