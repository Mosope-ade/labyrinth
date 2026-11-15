use super::*;
use crate::model::*;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Deterministic abstract spanning tree over logical Ethernet adjacencies.
/// Parallel members form one logical adjacency; member loss retains the adjacency
/// until its final cable fails. This models steady state, not RSTP convergence timers.
pub fn l2_forwarding(
    topology: &AllocatedTopology,
    failures: &Failures,
    preferred_roots: &BTreeMap<ZoneId, DeviceId>,
) -> Result<Vec<Layer2Forwarding>, PlanningError> {
    validate_failures(topology, failures)?;
    if preferred_roots.len() > topology.zones.len()
        || preferred_roots
            .keys()
            .any(|z| !topology.zones.iter().any(|d| &d.zone == z))
    {
        return Err(PlanningError::InvalidPolicy(
            "unknown zone in preferred roots".into(),
        ));
    }
    let bp = &topology.selection.blueprint;
    let placements: BTreeMap<_, _> = bp.placements.iter().map(|p| (&p.device, p)).collect();
    let roles: BTreeMap<_, _> = topology
        .selection
        .assignment
        .devices
        .iter()
        .map(|d| (&d.requirement, d.role))
        .collect();
    let links: BTreeMap<_, _> = topology.links.iter().map(|l| (&l.id, l)).collect();
    let mut result = Vec::new();
    for zone in &topology.zones {
        let requirement = bp
            .layer2_domains
            .iter()
            .find(|r| r.zone == zone.zone)
            .expect("allocated blueprint domain");
        let nodes: BTreeSet<_> = requirement
            .links
            .iter()
            .flat_map(|id| [links[id].from.device.clone(), links[id].to.device.clone()])
            .collect();
        let candidates: BTreeSet<_> = nodes
            .iter()
            .filter(|id| roles[id] == DeviceRole::DistributionSwitch)
            .cloned()
            .collect();
        if let Some(preferred) = preferred_roots.get(&zone.zone)
            && (!candidates.contains(preferred) || placements[preferred].site != zone.site)
        {
            return Err(PlanningError::InvalidPolicy(
                "preferred L2 root must be a distribution device in this zone".into(),
            ));
        }
        let root = preferred_roots
            .get(&zone.zone)
            .filter(|d| !failures.devices.contains(*d))
            .cloned()
            .or_else(|| {
                candidates
                    .iter()
                    .find(|d| !failures.devices.contains(*d))
                    .cloned()
            });
        let mut groups: BTreeMap<(DeviceId, DeviceId), Vec<LinkId>> = BTreeMap::new();
        for id in &requirement.links {
            let l = links[id];
            if failures.links.contains(id)
                || failures.devices.contains(&l.from.device)
                || failures.devices.contains(&l.to.device)
            {
                continue;
            }
            let mut pair = [l.from.device.clone(), l.to.device.clone()];
            pair.sort();
            groups
                .entry((pair[0].clone(), pair[1].clone()))
                .or_default()
                .push(id.clone());
        }
        let mut visited = BTreeSet::new();
        let mut selected = BTreeSet::new();
        if let Some(root) = &root {
            let mut queue = VecDeque::from([root.clone()]);
            visited.insert(root.clone());
            while let Some(current) = queue.pop_front() {
                for (a, b) in groups.keys() {
                    let peer = if a == &current {
                        b
                    } else if b == &current {
                        a
                    } else {
                        continue;
                    };
                    if visited.insert(peer.clone()) {
                        selected.insert((a.clone(), b.clone()));
                        queue.push_back(peer.clone());
                    }
                }
            }
        }
        let mut forwarding = Vec::new();
        let mut blocked = Vec::new();
        for (pair, ids) in groups {
            if selected.contains(&pair) {
                forwarding.extend(ids)
            } else {
                blocked.extend(ids)
            }
        }
        forwarding.sort();
        blocked.sort();
        let unreachable = nodes
            .into_iter()
            .filter(|n| !failures.devices.contains(n) && !visited.contains(n))
            .collect();
        result.push(Layer2Forwarding {
            domain: zone.domain.clone(),
            root,
            forwarding,
            blocked,
            unreachable,
        });
    }
    Ok(result)
}
pub(super) fn validate_failures(t: &AllocatedTopology, f: &Failures) -> Result<(), PlanningError> {
    if f.devices.len() > t.selection.assignment.devices.len()
        || f.links.len() > t.links.len()
        || f.devices.iter().any(|id| {
            !t.selection
                .assignment
                .devices
                .iter()
                .any(|d| &d.requirement == id)
        })
        || f.links
            .iter()
            .any(|id| !t.links.iter().any(|l| &l.id == id))
    {
        return Err(PlanningError::InvalidPolicy(
            "failure set references unknown components".into(),
        ));
    }
    Ok(())
}
