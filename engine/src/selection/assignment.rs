use super::*;
use crate::{
    catalog::CatalogSnapshot,
    model::{ArchetypeId, DeviceRole},
};
use std::collections::{BTreeMap, BTreeSet};

/// Checked ceil(value * (100 + growth) / 100); call exactly once on raw demand.
pub fn with_growth(value: u64, growth: u8) -> Result<u64, SelectionError> {
    if growth > 100 {
        return Err(SelectionError::InvalidRequirement(
            "growth must be 0-100 percent".into(),
        ));
    }
    u64::try_from((u128::from(value) * (100 + u128::from(growth))).div_ceil(100))
        .map_err(|_| SelectionError::ArithmeticOverflow)
}

pub fn assign(
    request: &SelectionRequest,
    catalog: &CatalogSnapshot,
) -> Result<Assignment, SelectionError> {
    assign_checked(request, catalog, &mut |_| Ok(true))
}
pub(crate) fn assign_checked(
    request: &SelectionRequest,
    catalog: &CatalogSnapshot,
    accept: &mut dyn FnMut(&[SelectedDevice]) -> Result<bool, SelectionError>,
) -> Result<Assignment, SelectionError> {
    validate_request(request)?;
    let mut requirements: Vec<_> = request.devices.iter().collect();
    requirements.sort_by_key(|r| &r.id);
    let mut choices = Vec::new();
    let mut candidate_rejections = BTreeMap::new();
    for r in &requirements {
        let mut set = filter_candidates(r, catalog, request.spare_port_target_percent)?;
        if set.candidates.is_empty() {
            return Err(if r.pinned_archetype.is_some() {
                SelectionError::PinConflict(format!(
                    "{}: pinned archetype cannot meet declared capacity/ports/mode",
                    r.id
                ))
            } else {
                SelectionError::NoCandidate {
                    device: r.id.clone(),
                    required: Box::new((*r).clone()),
                    rejections: set.rejections,
                }
            });
        }
        set.candidates.sort_by_key(|c| {
            (
                c.cost_tier,
                c.spare_shortfall_ppm,
                c.archetype.clone(),
                c.mode.clone(),
            )
        });
        candidate_rejections.insert(r.id.clone(), set.rejections);
        choices.push(set.candidates);
    }
    let mut search = Search {
        requirements: &requirements,
        choices: &choices,
        best: None,
        visited: 0,
        limit: request.max_states,
        accept,
        deadline: std::time::Instant::now() + std::time::Duration::from_secs(15),
    };
    search.visit(&mut Vec::new())?;
    let (score, devices) = search.best.ok_or_else(|| {
        SelectionError::Infeasible(
            "no hard-valid assignment in this fixed candidate product".into(),
        )
    })?;
    Ok(Assignment {
        catalog_version: catalog.version().clone(),
        devices,
        score,
        visited_states: search.visited,
        candidate_rejections,
    })
}

fn validate_request(request: &SelectionRequest) -> Result<(), SelectionError> {
    let invalid = |s: &str| SelectionError::InvalidRequirement(s.into());
    if request.devices.is_empty()
        || request.devices.len() > 128
        || request.connections.len() > 4096
        || request.max_states == 0
        || request.max_states > 1_000_000
        || request.spare_port_target_percent >= 100
    {
        return Err(invalid(
            "require 1-128 devices, at most 4096 connections and 1-1000000 search states",
        ));
    }
    let devices: BTreeMap<_, _> = request.devices.iter().map(|r| (&r.id, r)).collect();
    if devices.len() != request.devices.len() {
        return Err(invalid("duplicate device requirement ID"));
    }
    let mut links = BTreeSet::new();
    let mut uses = BTreeMap::new();
    for link in &request.connections {
        if !links.insert(&link.id) || link.from.device == link.to.device {
            return Err(invalid(
                "connections need distinct link IDs and different devices",
            ));
        }
        let from = devices
            .get(&link.from.device)
            .and_then(|d| d.ports.iter().find(|p| p.id == link.from.demand));
        let to = devices
            .get(&link.to.device)
            .and_then(|d| d.ports.iter().find(|p| p.id == link.to.demand));
        let (Some(a), Some(b)) = (from, to) else {
            return Err(invalid("connection references unknown device/port demand"));
        };
        if a.speed_mbps != b.speed_mbps || a.media != b.media {
            return Err(invalid(
                "connection ends need matching configured speed/media",
            ));
        }
        for (endpoint, ports) in [(&link.from, a), (&link.to, b)] {
            let count = uses.entry(endpoint).or_insert(0u16);
            *count = count
                .checked_add(1)
                .ok_or(SelectionError::ArithmeticOverflow)?;
            if *count > ports.count {
                return Err(invalid("connections overuse a port-demand count"));
            }
        }
    }
    Ok(())
}
struct Search<'a, 'b> {
    requirements: &'a [&'a DeviceRequirement],
    choices: &'a [Vec<Candidate>],
    best: Option<(AssignmentScore, Vec<SelectedDevice>)>,
    visited: u64,
    limit: u64,
    accept: &'b mut dyn FnMut(&[SelectedDevice]) -> Result<bool, SelectionError>,
    deadline: std::time::Instant,
}
impl Search<'_, '_> {
    fn visit(&mut self, selected: &mut Vec<SelectedDevice>) -> Result<(), SelectionError> {
        if std::time::Instant::now() >= self.deadline {
            return Err(SelectionError::Timeout);
        }
        if self.visited >= self.limit {
            return Err(SelectionError::SearchExhausted {
                visited: self.visited,
                limit: self.limit,
            });
        }
        self.visited += 1;
        let current = score(selected);
        if let Some((best, _)) = &self.best {
            let minimum_remaining_cost: u64 = self
                .choices
                .iter()
                .skip(selected.len())
                .map(|options| u64::from(options[0].cost_tier))
                .sum();
            let lower = (
                current.standardization_penalty,
                current.relative_cost_sum + minimum_remaining_cost,
                current.spare_shortfall_ppm,
            );
            if lower
                > (
                    best.standardization_penalty,
                    best.relative_cost_sum,
                    best.spare_shortfall_ppm,
                )
            {
                return Ok(());
            }
        }
        if selected.len() == self.requirements.len() {
            if self.best.as_ref().is_none_or(|(best, _)| &current < best) {
                let accepted = (self.accept)(selected)?;
                if std::time::Instant::now() >= self.deadline {
                    return Err(SelectionError::Timeout);
                }
                if accepted {
                    self.best = Some((current, selected.clone()));
                }
            }
            return Ok(());
        }
        let i = selected.len();
        for c in &self.choices[i] {
            selected.push(SelectedDevice {
                requirement: self.requirements[i].id.clone(),
                role: self.requirements[i].role,
                candidate: c.clone(),
            });
            self.visit(selected)?;
            selected.pop();
        }
        Ok(())
    }
}
pub(crate) fn score(devices: &[SelectedDevice]) -> AssignmentScore {
    let mut groups: BTreeMap<DeviceRole, BTreeSet<&ArchetypeId>> = BTreeMap::new();
    for d in devices {
        groups
            .entry(d.role)
            .or_default()
            .insert(&d.candidate.archetype);
    }
    AssignmentScore {
        standardization_penalty: groups
            .values()
            .map(|g| u32::try_from(g.len().saturating_sub(1)).expect("bounded devices"))
            .sum(),
        relative_cost_sum: devices
            .iter()
            .map(|d| u64::from(d.candidate.cost_tier))
            .sum(),
        spare_shortfall_ppm: devices
            .iter()
            .map(|d| u64::from(d.candidate.spare_shortfall_ppm))
            .sum(),
        device_count: u32::try_from(devices.len()).expect("bounded devices"),
        stable_tiebreak: devices
            .iter()
            .map(|d| {
                (
                    d.requirement.clone(),
                    d.candidate.archetype.clone(),
                    d.candidate.mode.clone(),
                )
            })
            .collect(),
    }
}
