//! Deterministic capacitated bipartite matching; flexible ports are not greedily wasted.
use super::{DeviceRequirement, PortAssignment, PortDemand};
use crate::catalog::{Archetype, PortGroup};
use std::collections::VecDeque;

fn compatible(d: &PortDemand, g: &PortGroup) -> bool {
    g.media == d.media
        && g.speeds_mbps.contains(&d.speed_mbps)
        && d.min_poe_watts
            .is_none_or(|watts| g.poe_watts_per_port.is_some_and(|w| w >= watts))
}

pub(super) fn allocate(
    req: &DeviceRequirement,
    a: &Archetype,
    target: u8,
) -> Option<(Vec<PortAssignment>, u32, u32)> {
    if req.ports.iter().map(|p| u32::from(p.count)).sum::<u32>() > 256 {
        return None;
    }
    let mut demands: Vec<_> = req.ports.iter().collect();
    demands.sort_by_key(|d| &d.id);
    let mut groups: Vec<_> = a
        .port_groups
        .iter()
        .filter(|g| {
            demands.iter().any(|d| compatible(d, g))
                && g.shared_capacity_mbps
                    .is_none_or(|cap| cap >= req.peak_mbps)
        })
        .collect();
    groups.sort_by_key(|g| &g.id);
    let count = 2 + demands.len() + groups.len();
    let sink = count - 1;
    let mut residual = vec![vec![0u32; count]; count];
    for (i, d) in demands.iter().enumerate() {
        residual[0][1 + i] = u32::from(d.count);
        for (j, g) in groups.iter().enumerate() {
            if compatible(d, g) {
                residual[1 + i][1 + demands.len() + j] = u32::from(d.count);
            }
        }
    }
    for (j, g) in groups.iter().enumerate() {
        residual[1 + demands.len() + j][sink] = u32::from(g.count);
    }
    let used: u32 = demands.iter().map(|d| u32::from(d.count)).sum();
    let mut total = 0;
    loop {
        let mut parents = vec![None; count];
        parents[0] = Some(0);
        let mut q = VecDeque::from([0]);
        while let Some(v) = q.pop_front() {
            for (w, cap) in residual[v].iter().enumerate() {
                if *cap > 0 && parents[w].is_none() {
                    parents[w] = Some(v);
                    q.push_back(w);
                }
            }
        }
        if parents[sink].is_none() {
            break;
        }
        let mut cursor = sink;
        let mut amount = u32::MAX;
        while cursor != 0 {
            let p = parents[cursor]?;
            amount = amount.min(residual[p][cursor]);
            cursor = p;
        }
        cursor = sink;
        while cursor != 0 {
            let p = parents[cursor]?;
            residual[p][cursor] -= amount;
            residual[cursor][p] += amount;
            cursor = p;
        }
        total += amount;
    }
    if total != used {
        return None;
    }
    let mut offsets = vec![0u16; groups.len()];
    let mut placements = Vec::new();
    for (i, d) in demands.iter().enumerate() {
        for (j, g) in groups.iter().enumerate() {
            let allocated = residual[1 + demands.len() + j][1 + i];
            if allocated > 0 {
                let allocated = u16::try_from(allocated).ok()?;
                placements.push(PortAssignment {
                    demand: d.id.clone(),
                    group: g.id.clone(),
                    first_index: offsets[j],
                    count: allocated,
                    speed_mbps: d.speed_mbps,
                    media: d.media,
                });
                offsets[j] = offsets[j].checked_add(allocated)?;
            }
        }
    }
    let installed: u32 = groups.iter().map(|g| u32::from(g.count)).sum();
    let free = installed.checked_sub(used)?;
    let available_ppm = if installed == 0 {
        0
    } else {
        u64::from(free) * 1_000_000 / u64::from(installed)
    };
    let shortage = (u64::from(target) * 10_000).saturating_sub(available_ppm);
    Some((placements, installed, u32::try_from(shortage).ok()?))
}
