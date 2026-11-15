use super::*;
use crate::model::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LogicalPolicy {
    #[serde(default)]
    pub preferred_roots: BTreeMap<ZoneId, DeviceId>,
    #[serde(default)]
    pub preferred_owners: BTreeMap<ZoneId, DeviceId>,
    #[serde(default)]
    pub routing_limits: RoutingLimits,
}
/// Abstract active/standby ownership. Previous owners prevent automatic preemption.
/// Confirmed failure inputs are not a distributed failure detector or fencing system.
pub fn ha_ownership(
    t: &AllocatedTopology,
    f: &Failures,
    policy: &LogicalPolicy,
    previous: &BTreeMap<ZoneId, DeviceId>,
) -> Result<Vec<OwnershipState>, PlanningError> {
    forwarding::validate_failures(t, f)?;
    if policy.preferred_owners.len() > t.zones.len()
        || previous.len() > t.zones.len()
        || policy
            .preferred_owners
            .keys()
            .chain(previous.keys())
            .any(|z| !t.zones.iter().any(|d| &d.zone == z))
    {
        return Err(PlanningError::InvalidPolicy(
            "unknown zone in HA ownership inputs".into(),
        ));
    }
    let l2 = l2_forwarding(t, f, &policy.preferred_roots)?;
    let mut states = Vec::new();
    for z in &t.zones {
        let mut members: Vec<_> = t
            .selection
            .assignment
            .devices
            .iter()
            .filter(|d| {
                d.role == DeviceRole::Firewall
                    && t.selection
                        .blueprint
                        .placements
                        .iter()
                        .any(|p| p.device == d.requirement && p.zone.as_ref() == Some(&z.zone))
            })
            .map(|d| d.requirement.clone())
            .collect();
        members.sort();
        if policy
            .preferred_owners
            .get(&z.zone)
            .into_iter()
            .chain(previous.get(&z.zone))
            .any(|d| !members.contains(d))
        {
            return Err(PlanningError::InvalidPolicy(
                "preferred/previous owner must be a firewall member of this zone".into(),
            ));
        }
        let alive: Vec<_> = members.iter().filter(|d| !f.devices.contains(*d)).collect();
        let sync_ok = t
            .selection
            .blueprint
            .firewall_ha
            .iter()
            .find(|h| h.zone == z.zone)
            .is_none_or(|h| h.sync_links.iter().any(|id| !f.links.contains(id)));
        let state = l2
            .iter()
            .find(|s| s.domain == z.domain)
            .expect("allocated zone");
        let eligible = |d: &&DeviceId| {
            !state.unreachable.contains(d)
                && state.root.is_some()
                && t.transits.iter().any(|tr| {
                    let link = t
                        .links
                        .iter()
                        .find(|l| l.id == tr.link)
                        .expect("allocated transit");
                    (link.from.device == **d || link.to.device == **d)
                        && !f.links.contains(&link.id)
                        && !f.devices.contains(&link.from.device)
                        && !f.devices.contains(&link.to.device)
                })
        };
        let (owner, status) = if alive.len() > 1 && !sync_ok {
            (None, OwnershipStatus::SyncLost)
        } else if let Some(old) = previous.get(&z.zone).filter(|d| !f.devices.contains(*d)) {
            if eligible(&old) {
                (Some(old.clone()), OwnershipStatus::Active)
            } else {
                // A live owner may relinquish service over healthy HA control;
                // loss of its data path alone is never evidence that it is dead.
                let owner = if sync_ok {
                    alive.iter().copied().find(eligible).cloned()
                } else {
                    None
                };
                let status = if owner.is_some() {
                    OwnershipStatus::Active
                } else {
                    OwnershipStatus::Unavailable
                };
                (owner, status)
            }
        } else {
            let owner = policy
                .preferred_owners
                .get(&z.zone)
                .filter(|d| alive.contains(d) && eligible(d))
                .cloned()
                .or_else(|| alive.iter().copied().find(eligible).cloned());
            let status = if owner.is_some() {
                OwnershipStatus::Active
            } else {
                OwnershipStatus::Unavailable
            };
            (owner, status)
        };
        states.push(OwnershipState {
            zone: z.zone.clone(),
            owner,
            status,
        });
    }
    Ok(states)
}
