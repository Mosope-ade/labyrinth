use super::*;
use crate::diagnostics::{Checks, IssueCode};
use std::collections::{BTreeMap, BTreeSet};
use std::net::Ipv4Addr;
pub(super) fn validate(
    n: &NetworkDraft,
    zones: &BTreeMap<&ZoneId, &SecurityZone>,
    devices: &BTreeMap<&DeviceId, &Device>,
    interfaces: &BTreeMap<(&DeviceId, &InterfaceId), &Interface>,
    c: &mut Checks,
) {
    use IssueCode::*;
    let mut ids = BTreeSet::new();
    for (i, a) in n.allocations.iter().enumerate() {
        let p = format!("/allocations/{i}");
        c.require(
            ids.insert(&a.id),
            DuplicateId,
            &p,
            "duplicate allocation ID",
        );
        c.require(
            zones.contains_key(&a.zone),
            UnknownReference,
            &p,
            "unknown allocation zone",
        );
        let total = 1u64 << (32 - a.subnet.net().prefix_len());
        let usable = if a.subnet.net().prefix_len() < 31 {
            total - 2
        } else {
            total
        };
        c.require(
            u64::from(a.required_hosts) + u64::from(a.reserved_hosts) <= usable,
            Conflict,
            &p,
            "allocation cannot fit required and reserved usable hosts",
        );
        for previous in n.allocations.iter().take(i) {
            c.require(
                !a.subnet.overlaps(&previous.subnet),
                Conflict,
                &p,
                "allocated sibling subnets overlap in initial single routing realm",
            );
        }
    }
    for (i, r) in n.routes.iter().enumerate() {
        let p = format!("/routes/{i}");
        c.require(
            devices.contains_key(&r.device),
            UnknownReference,
            &p,
            "unknown route device",
        );
        let iface = interfaces.get(&(&r.device, &r.egress));
        c.require(
            iface.is_some(),
            UnknownReference,
            &p,
            "route egress must belong to device",
        );
        let hop = r.next_hop.parse::<Ipv4Addr>();
        c.require(
            hop.is_ok(),
            InvalidValue,
            format!("{p}/next_hop"),
            "expected IPv4 next hop",
        );
        if let (Some(iface), Ok(hop)) = (iface, hop) {
            match &iface.kind {
                InterfaceKind::Routed { address, .. } => c.require(
                    address.net().contains(&hop)
                        && hop != address.net().addr()
                        && (address.net().prefix_len() >= 31
                            || (hop != address.net().network()
                                && hop != address.net().broadcast())),
                    Conflict,
                    &p,
                    "next hop must be a distinct usable host on routed egress subnet",
                ),
                _ => c.require(
                    false,
                    Conflict,
                    &p,
                    "initial route contract requires routed egress",
                ),
            }
        }
    }
    let mut policies = BTreeSet::new();
    let mut bindings = BTreeSet::new();
    let mut definitions = BTreeMap::new();
    for (i, b) in n.policies.iter().enumerate() {
        let p = format!("/policies/{i}");
        policies.insert(&b.policy.id);
        if let Some(previous) = definitions.insert(&b.policy.id, &b.policy) {
            c.require(
                previous == &b.policy,
                Conflict,
                &p,
                "same policy ID has contradictory definitions",
            );
        }
        c.require(
            bindings.insert((&b.policy.id, &b.device, &b.ingress)),
            DuplicateId,
            &p,
            "duplicate policy binding ID",
        );
        c.require(
            zones.contains_key(&b.policy.from_zone) && zones.contains_key(&b.policy.to_zone),
            UnknownReference,
            &p,
            "unknown policy zone",
        );
        c.require(
            interfaces.contains_key(&(&b.device, &b.ingress)),
            UnknownReference,
            &p,
            "policy ingress must belong to enforcement device",
        );
        let service = &b.policy.service;
        c.require(
            service.destination_ports.len() <= 128
                && service
                    .destination_ports
                    .iter()
                    .all(|r| r.start > 0 && r.start <= r.end)
                && (matches!(service.protocol, Protocol::Tcp | Protocol::Udp)
                    || service.destination_ports.is_empty()),
            InvalidValue,
            &p,
            "invalid policy protocol/port match",
        );
    }
    let site_ids: BTreeSet<_> = n.sites.iter().map(|s| &s.id).collect();
    let interface_ids: BTreeSet<_> = interfaces.keys().map(|(_, id)| *id).collect();
    let link_ids: BTreeSet<_> = n.links.iter().map(|l| &l.id).collect();
    let domain_ids: BTreeSet<_> = n.domains.iter().map(|d| &d.id).collect();
    let bundle_ids: BTreeSet<_> = n.link_bundles.iter().map(|b| &b.id).collect();
    let mut decisions = BTreeSet::new();
    for (i, d) in n.decisions.iter().enumerate() {
        let p = format!("/decisions/{i}");
        c.require(
            decisions.insert(&d.id),
            DuplicateId,
            &p,
            "duplicate decision ID",
        );
        c.require(
            !d.reason.trim().is_empty()
                && d.reason.len() <= 2048
                && d.component_ids.len() <= 128
                && !d.component_ids.is_empty()
                && d.rule_id
                    .as_ref()
                    .is_none_or(|r| !r.is_empty() && r.len() <= 128),
            InvalidValue,
            &p,
            "decision needs bounded component references, reason and optional rule ID",
        );
        for component in d.component_ids.iter().take(128) {
            let exists = match component {
                ComponentRef::Site(id) => site_ids.contains(id),
                ComponentRef::Zone(id) => zones.contains_key(id),
                ComponentRef::Device(id) => devices.contains_key(id),
                ComponentRef::Interface(id) => interface_ids.contains(id),
                ComponentRef::Link(id) => link_ids.contains(id),
                ComponentRef::Domain(id) => domain_ids.contains(id),
                ComponentRef::Allocation(id) => ids.contains(id),
                ComponentRef::Bundle(id) => bundle_ids.contains(id),
                ComponentRef::Policy(id) => policies.contains(id),
            };
            c.require(
                exists,
                UnknownReference,
                format!("{p}/component_ids"),
                "decision references unknown component",
            );
        }
    }
}
