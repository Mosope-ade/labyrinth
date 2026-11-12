use super::*;
use std::collections::{BTreeMap, BTreeSet};

/// Per-site or global uniqueness; reserve all pins before assigning lowest free IDs.
pub fn allocate_vlans(
    requests: &[VlanRequest],
    policy: &VlanPolicy,
) -> Result<Vec<VlanAssignment>, AddressError> {
    if requests.len() > 4096
        || policy.first == 0
        || policy.last > 4094
        || policy.first > policy.last
        || policy.excluded.len() > 4094
        || policy.excluded.iter().any(|v| *v == 0 || *v > 4094)
    {
        return Err(invalid(
            "invalid VLAN range/exclusions or more than 4096 requests",
        ));
    }
    let mut sorted: Vec<_> = requests.iter().collect();
    sorted.sort_by(|a, b| (&a.site, &a.zone).cmp(&(&b.site, &b.zone)));
    let mut keys = BTreeSet::new();
    let excluded: BTreeSet<_> = policy.excluded.iter().copied().collect();
    let mut used: BTreeMap<Option<&crate::model::SiteId>, BTreeSet<u16>> = BTreeMap::new();
    for r in &sorted {
        if !keys.insert((&r.site, &r.zone)) {
            return Err(invalid("duplicate site/zone VLAN request"));
        }
        if let Some(pin) = r.pinned {
            let scope = policy.reuse_across_sites.then_some(&r.site);
            if !(policy.first..=policy.last).contains(&pin)
                || excluded.contains(&pin)
                || !used.entry(scope).or_default().insert(pin)
            {
                return Err(invalid("VLAN pin conflicts with scope, range or exclusion"));
            }
        }
    }
    let mut result = Vec::with_capacity(sorted.len());
    for r in sorted {
        let occupied = used
            .entry(policy.reuse_across_sites.then_some(&r.site))
            .or_default();
        let vlan = match r.pinned {
            Some(pin) => pin,
            None => (policy.first..=policy.last)
                .find(|v| !excluded.contains(v) && !occupied.contains(v))
                .ok_or_else(|| AddressError::VlanExhausted {
                    site: r.site.clone(),
                })?,
        };
        occupied.insert(vlan);
        result.push(VlanAssignment {
            site: r.site.clone(),
            zone: r.zone.clone(),
            vlan,
        });
    }
    Ok(result)
}
