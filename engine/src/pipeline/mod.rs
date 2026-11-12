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
    let mut attempts = Vec::new();
    let mut bottlenecks = Vec::new();
    for pass in 1..=3 {
        let mut blueprint = match blueprints::plan(intent, catalog, pass) {
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
        blueprint.requirements.max_states = max_states;
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
    let selected = select_devices(intent, catalog, max_states)?;
    crate::topology::build(intent, selected, policy)
}
