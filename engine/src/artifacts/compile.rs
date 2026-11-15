use super::*;
use crate::{
    catalog::CatalogSnapshot,
    intent::ResolvedIntent,
    model::*,
    selection::{self, Assignment, SelectionError},
    topology::{RepairAttempt, SelectedPlan},
};
/// Hardware-dependent capacity is a hard predicate inside assignment ranking.
/// The selected winner receives the complete independent five-pass proof before
/// publication. Candidate enumeration never substitutes a partial proof for it.
pub(super) fn compile(
    intent: &ResolvedIntent,
    catalog: &CatalogSnapshot,
    options: &GenerationOptions,
    pins: Option<&PinnedConstraints>,
) -> Result<VerifiedPackage, GenerationError> {
    crate::pipeline::validate_addressing(intent, &options.addressing)?;
    let limits = &options.logical.routing_limits;
    if limits.max_search_states == 0
        || limits.max_search_states > 1_000_000
        || limits.max_paths_per_demand == 0
        || limits.max_paths_per_demand > 256
        || limits.timeout_ms == 0
        || limits.timeout_ms > 15_000
    {
        return Err(GenerationError::Routing(
            crate::routing::RoutingError::InvalidLimits,
        ));
    }
    if options.verification.max_scenarios > 4096
        || options.verification.max_service_classes == 0
        || options.verification.max_service_classes > 1_000_000
    {
        return Err(GenerationError::Configuration(
            "verification limits exceed the supported bounds".into(),
        ));
    }
    let mut attempts = Vec::new();
    let mut bottlenecks = Vec::new();
    let mut last = None;
    for pass in 1..=3 {
        let blueprint = match crate::pipeline::prepared_plan(
            intent,
            catalog,
            pass,
            options.selection_search_states,
            options.addressing.allocate_loopbacks,
            pins,
        ) {
            Ok(p) => p,
            Err(e @ SelectionError::Infeasible(_)) => {
                last = Some(GenerationError::Planning(e.clone().into()));
                attempts.push(RepairAttempt {
                    pass,
                    outcome: e.to_string(),
                });
                continue;
            }
            Err(e) => return Err(GenerationError::Planning(e.into())),
        };
        let mut fatal = None;
        let mut validate =
            |selected: &[crate::selection::SelectedDevice]| -> Result<bool, SelectionError> {
                let assignment = Assignment {
                    catalog_version: catalog.version().clone(),
                    devices: selected.to_vec(),
                    score: selection::score(selected),
                    visited_states: 0,
                    candidate_rejections: Default::default(),
                };
                let candidate = SelectedPlan {
                    blueprint: blueprint.clone(),
                    assignment,
                    attempts: attempts.clone(),
                    repair_bottlenecks: bottlenecks.clone(),
                };
                let t = match crate::topology::build(
                    intent,
                    candidate,
                    &options.addressing,
                    catalog,
                    pins,
                ) {
                    Ok(t) => t,
                    Err(e) => {
                        last = Some(GenerationError::Planning(e));
                        return Ok(false);
                    }
                };
                let errors = match crate::verification::assignment_capacity(
                    intent,
                    catalog,
                    &t,
                    &options.logical,
                    &options.verification,
                ) {
                    Ok(errors) => errors,
                    Err(e) => {
                        let limited = e.resource_limit_code().is_some();
                        last = Some(GenerationError::Routing(e.clone()));
                        if limited {
                            fatal = Some(GenerationError::Routing(e));
                            return Err(SelectionError::SearchExhausted {
                                visited: 0,
                                limit: options.selection_search_states,
                            });
                        }
                        return Ok(false);
                    }
                };
                if !errors.is_empty() {
                    let limited = errors
                        .iter()
                        .any(|e| e.code.contains("budget") || e.code.contains("timeout"));
                    let error = GenerationError::CapacityRejected(errors);
                    if limited {
                        fatal = Some(error);
                        return Err(SelectionError::SearchExhausted {
                            visited: 0,
                            limit: options.selection_search_states,
                        });
                    }
                    last = Some(error);
                    return Ok(false);
                }
                Ok(true)
            };
        match selection::assign_checked(&blueprint.requirements, catalog, &mut validate) {
            Ok(assignment) => {
                attempts.push(RepairAttempt{pass,outcome:"hard-valid complete assignment ranked after capacity/security/resilience checks".into()});
                let selected = SelectedPlan {
                    blueprint,
                    assignment,
                    attempts,
                    repair_bottlenecks: bottlenecks,
                };
                let t =
                    crate::topology::build(intent, selected, &options.addressing, catalog, pins)?;
                return super::from_allocation(intent, catalog, options, &t);
            }
            Err(e) => {
                if let Some(fatal) = fatal {
                    return Err(fatal);
                }
                if matches!(
                    e,
                    SelectionError::SearchExhausted { .. }
                        | SelectionError::Timeout
                        | SelectionError::PinConflict(_)
                        | SelectionError::InvalidRequirement(_)
                ) {
                    return Err(GenerationError::Planning(e.into()));
                }
                attempts.push(RepairAttempt {
                    pass,
                    outcome: e.to_string(),
                });
                if let SelectionError::NoCandidate {
                    device, rejections, ..
                } = &e
                {
                    bottlenecks.push(crate::selection::Bottleneck {
                        pass,
                        device: Some(device.clone()),
                        message: "no candidate in this repair variant".into(),
                        rejections: rejections.clone(),
                    });
                }
                if last.is_none() {
                    last = Some(GenerationError::Planning(e.into()));
                }
            }
        }
    }
    Err(GenerationError::RepairExhausted {
        passes: 3,
        bottlenecks,
        last: Box::new(last.unwrap_or_else(|| {
            GenerationError::RevisionConflict("no accepted repair variant".into())
        })),
    })
}
