use super::*;
use crate::model::{AddressAllocation, AllocationPurpose, Ipv4Prefix};
use std::collections::BTreeSet;
use std::net::Ipv4Addr;

/// Largest-first, lowest-aligned-fit allocation. Pins are checked/reserved first.
/// No per-address iteration, growth, routing decisions or environmental discovery.
pub fn allocate(request: &AddressRequest) -> Result<AddressPlan, AddressError> {
    if request.subnets.is_empty() || request.subnets.len() > 4096 || request.exclusions.len() > 256
    {
        return Err(invalid(
            "require 1..4096 subnets and at most 256 exclusions",
        ));
    }
    let mut ids = BTreeSet::new();
    let mut sized = Vec::with_capacity(request.subnets.len());
    for r in &request.subnets {
        if !ids.insert(&r.id) {
            return Err(invalid("duplicate allocation ID"));
        }
        sized.push((r, size(r)?));
    }
    let pool = interval(&request.pool);
    let mut occupied: Vec<_> = request
        .exclusions
        .iter()
        .map(interval)
        .filter(|(start, end)| *start <= pool.1 && *end >= pool.0)
        .collect();
    let mut assignments = Vec::with_capacity(sized.len());
    let mut pins: Vec<_> = sized.iter().filter(|(r, _)| r.pinned.is_some()).collect();
    pins.sort_by_key(|(r, _)| &r.id);
    for (r, len) in pins {
        let pin = r.pinned.as_ref().expect("filtered pin");
        let range = interval(pin);
        let reason = if !request.pool.contains(pin) {
            Some("pin is outside pool")
        } else if pin.net().prefix_len() > *len
            || r.prefix_len.is_some_and(|p| p != pin.net().prefix_len())
        {
            Some("pin cannot satisfy capacity or exact prefix convention")
        } else if occupied.iter().any(|b| overlaps(range, *b)) {
            Some("pin overlaps exclusion or another pin")
        } else {
            None
        };
        if let Some(reason) = reason {
            return Err(AddressError::PinConflict {
                allocation: r.id.clone(),
                reason: reason.into(),
            });
        }
        occupied.push(range);
        assignments.push(assignment(r, pin.clone()));
    }
    sized.sort_by(|(a, al), (b, bl)| al.cmp(bl).then(a.id.cmp(&b.id)));
    for (r, len) in sized.into_iter().filter(|(r, _)| r.pinned.is_none()) {
        occupied.sort_unstable();
        let block = 1u64 << (32 - len);
        let mut start = align(pool.0, block);
        for &(lo, hi) in &occupied {
            if start + block - 1 < lo {
                break;
            }
            if start <= hi {
                start = align(hi + 1, block);
            }
        }
        if start + block - 1 > pool.1 {
            return Err(AddressError::Exhausted {
                allocation: r.id.clone(),
                prefix_len: len,
                pool: request.pool.clone(),
            });
        }
        let subnet = Ipv4Prefix::new(format!(
            "{}/{len}",
            Ipv4Addr::from(u32::try_from(start).expect("inside IPv4 pool"))
        ))
        .expect("aligned canonical subnet");
        occupied.push(interval(&subnet));
        assignments.push(assignment(r, subnet));
    }
    assignments.sort_by(|a, b| a.allocation.id.cmp(&b.allocation.id));
    let mut recorded_request = request.clone();
    recorded_request.subnets.sort_by(|a, b| a.id.cmp(&b.id));
    recorded_request.exclusions.sort();
    recorded_request.exclusions.dedup();
    Ok(AddressPlan {
        rules_version: "addressing_rules_v1".into(),
        request: recorded_request,
        assignments,
    })
}
fn size(r: &SubnetRequest) -> Result<u8, AddressError> {
    let hosts = u64::from(r.required_hosts) + u64::from(r.reserved_hosts);
    if hosts == 0 {
        return Err(invalid(
            "subnet needs at least one required or reserved host",
        ));
    }
    let min_len = match r.purpose {
        AllocationPurpose::Endpoints => (0..=30).rev().find(|p| (1u64 << (32 - p)) - 2 >= hosts),
        AllocationPurpose::Transit | AllocationPurpose::Loopback => {
            (0..=32).rev().find(|p| usable(*p) >= hosts)
        }
    }
    .ok_or_else(|| invalid("host count exceeds IPv4 capacity"))?;
    let len = r.prefix_len.unwrap_or(min_len);
    if len > 32 || len > min_len || (matches!(r.purpose, AllocationPurpose::Endpoints) && len > 30)
    {
        return Err(invalid(
            "requested prefix cannot fit usable host count/purpose",
        ));
    }
    Ok(len)
}
fn usable(len: u8) -> u64 {
    let total = 1u64 << (32 - len);
    if len < 31 { total - 2 } else { total }
}
fn interval(prefix: &Ipv4Prefix) -> (u64, u64) {
    let net = prefix.net();
    (
        u64::from(u32::from(net.network())),
        u64::from(u32::from(net.broadcast())),
    )
}
fn overlaps(a: (u64, u64), b: (u64, u64)) -> bool {
    a.0 <= b.1 && b.0 <= a.1
}
fn align(value: u64, block: u64) -> u64 {
    value.div_ceil(block) * block
}
fn range(start: u64, count: u32) -> Option<HostRange> {
    (count != 0).then(|| HostRange {
        first: Ipv4Addr::from(u32::try_from(start).expect("capacity checked")),
        last: Ipv4Addr::from(
            u32::try_from(start + u64::from(count) - 1).expect("capacity checked"),
        ),
        count,
    })
}
fn assignment(r: &SubnetRequest, subnet: Ipv4Prefix) -> SubnetAssignment {
    let net = subnet.net();
    let first = u64::from(u32::from(net.network())) + u64::from(net.prefix_len() < 31);
    SubnetAssignment {
        reserved: range(first, r.reserved_hosts),
        hosts: range(first + u64::from(r.reserved_hosts), r.required_hosts),
        pinned: r.pinned.is_some(),
        allocation: AddressAllocation {
            id: r.id.clone(),
            zone: r.zone.clone(),
            subnet,
            purpose: r.purpose,
            required_hosts: r.required_hosts,
            reserved_hosts: r.reserved_hosts,
        },
    }
}
