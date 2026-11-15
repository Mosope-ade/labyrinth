use super::*;
use crate::{
    catalog::CatalogSnapshot,
    diagnostics::{Checks, IssueCode},
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

fn domain(kind: &InterfaceKind) -> Option<&DomainId> {
    match kind {
        InterfaceKind::Access { domain } | InterfaceKind::Attachment { domain } => Some(domain),
        _ => None,
    }
}
pub(super) fn validate(n: &NetworkDraft, catalog: &CatalogSnapshot, c: &mut Checks) {
    use IssueCode::*;
    let interfaces: BTreeMap<_, _> = n
        .devices
        .iter()
        .flat_map(|d| d.interfaces.iter().map(move |i| ((&d.id, &i.id), i)))
        .collect();
    let links: BTreeMap<_, _> = n.links.iter().map(|l| (&l.id, l)).collect();
    let bundles: BTreeMap<_, _> = n
        .link_bundles
        .iter()
        .filter(|b| matches!(b.kind, BundleKind::Lacp))
        .flat_map(|b| b.links.iter().map(move |l| (l, &b.id)))
        .collect();
    let mut domains = BTreeSet::new();
    for (i, s) in n.layer2_forwarding.iter().enumerate() {
        let p = format!("/layer2_forwarding/{i}");
        c.require(
            domains.insert(&s.domain) && n.domains.iter().any(|d| d.id == s.domain),
            UnknownReference,
            &p,
            "forwarding domain must exist and be unique",
        );
        if s.forwarding.len() + s.blocked.len() > 65536 || s.unreachable.len() > 4096 {
            c.require(false, LimitExceeded, &p, "oversized forwarding state");
            continue;
        }
        let expected: BTreeSet<_> = n
            .links
            .iter()
            .filter(|l| {
                interfaces
                    .get(&(&l.from.device, &l.from.interface))
                    .is_some_and(|f| domain(&f.kind) == Some(&s.domain) || matches!(&f.kind, InterfaceKind::Trunk {domains} if domains.contains(&s.domain)))
            })
            .map(|l| &l.id)
            .collect();
        let listed: BTreeSet<_> = s.forwarding.iter().chain(&s.blocked).collect();
        c.require(
            listed.len() == s.forwarding.len() + s.blocked.len() && listed == expected,
            Conflict,
            &p,
            "each L2 cable must be classified exactly once",
        );
        let mut parent = BTreeMap::<&DeviceId, &DeviceId>::new();
        let mut seen_groups = BTreeSet::new();
        for id in &s.forwarding {
            let Some(l) = links.get(id) else { continue };
            let key = bundles
                .get(id)
                .map_or_else(|| format!("link:{id}"), |b| format!("bundle:{b}"));
            if !seen_groups.insert(key) {
                continue;
            }
            let mut a = &l.from.device;
            let mut b = &l.to.device;
            while let Some(next) = parent.get(a) {
                a = next;
            }
            while let Some(next) = parent.get(b) {
                b = next;
            }
            c.require(
                a != b,
                Conflict,
                &p,
                "forwarding graph contains an Ethernet loop",
            );
            if a != b {
                parent.insert(a, b);
            }
        }
        let nodes: BTreeSet<_> = expected
            .iter()
            .filter_map(|id| links.get(id))
            .flat_map(|l| [&l.from.device, &l.to.device])
            .collect();
        c.require(
            s.root.as_ref().is_some_and(|r| nodes.contains(r)) && s.unreachable.is_empty(),
            Conflict,
            &p,
            "baseline tree needs a root and all domain devices reachable",
        );
        if let Some(root) = &s.root {
            let mut reached = BTreeSet::from([root]);
            let mut adjacent: BTreeMap<&DeviceId, Vec<&DeviceId>> = BTreeMap::new();
            for id in &s.forwarding {
                if let Some(l) = links.get(id) {
                    adjacent
                        .entry(&l.from.device)
                        .or_default()
                        .push(&l.to.device);
                    adjacent
                        .entry(&l.to.device)
                        .or_default()
                        .push(&l.from.device);
                }
            }
            let mut queue = VecDeque::from([root]);
            while let Some(device) = queue.pop_front() {
                for peer in adjacent.get(device).into_iter().flatten() {
                    if reached.insert(peer) {
                        queue.push_back(peer);
                    }
                }
            }
            c.require(
                nodes.is_subset(&reached),
                Conflict,
                &p,
                "forwarding tree does not span domain",
            );
        }
        if !s.blocked.is_empty() {
            for d in n
                .devices
                .iter()
                .filter(|d| nodes.contains(&d.id) && d.role != DeviceRole::Firewall)
            {
                let mode = catalog
                    .archetypes()
                    .iter()
                    .find(|a| a.id == d.archetype)
                    .and_then(|a| a.operating_modes.iter().find(|m| m.id == d.operating_mode));
                c.require(
                    mode.is_some_and(|m| m.features.contains(&Feature::RapidSpanningTree)),
                    Conflict,
                    &p,
                    "loop prevention capability missing in selected mode",
                );
            }
        }
    }
    let mut owned = BTreeSet::new();
    let mut virtuals = BTreeSet::new();
    for (i, g) in n.gateways.iter().enumerate() {
        let p = format!("/gateways/{i}");
        c.require(
            owned.insert(&g.domain) && n.domains.iter().any(|d| d.id == g.domain),
            Conflict,
            &p,
            "gateway domain must exist and be unique",
        );
        if g.members.is_empty() || g.members.len() > 2 || g.sync_links.len() > 32 {
            c.require(
                false,
                InvalidValue,
                &p,
                "gateway needs one or two members and bounded control links",
            );
            continue;
        }
        let mut members = BTreeSet::new();
        let mut subnet = None;
        for m in &g.members {
            c.require(
                members.insert(&m.device),
                Conflict,
                &p,
                "duplicate gateway member device",
            );
            match interfaces.get(&(&m.device, &m.interface)).map(|f| &f.kind) {
                Some(InterfaceKind::Gateway {
                    domain, address, ..
                }) if domain == &g.domain => {
                    c.require(
                        reserved(n, domain, address),
                        Conflict,
                        &p,
                        "gateway address must lie in reserved endpoint space",
                    );
                    let physical: Vec<_> = n
                        .devices
                        .iter()
                        .find(|d| d.id == m.device)
                        .into_iter()
                        .flat_map(|d| &d.interfaces)
                        .filter(
                            |f| matches!(&f.kind,InterfaceKind::Attachment{domain:d} if d==domain),
                        )
                        .collect();
                    let mut groups = BTreeSet::new();
                    for f in &physical {
                        let cable = n.links.iter().find(|l| {
                            (l.from.device == m.device && l.from.interface == f.id)
                                || (l.to.device == m.device && l.to.interface == f.id)
                        });
                        c.require(
                            cable.is_some(),
                            UnknownReference,
                            &p,
                            "gateway attachment needs a physical cable",
                        );
                        if let Some(l) = cable {
                            groups.insert(bundles.get(&l.id).map_or_else(
                                || format!("link:{}", l.id),
                                |b| format!("bundle:{b}"),
                            ));
                        }
                    }
                    c.require(!physical.is_empty() && groups.len()==1, Conflict,&p,"gateway must use a single attachment or LACP bundle; firewall bridging is not implied");
                    let current = (address.net().network(), address.net().prefix_len());
                    c.require(
                        subnet.is_none_or(|s| s == current),
                        Conflict,
                        &p,
                        "gateway members need the same subnet",
                    );
                    subnet = Some(current);
                    c.require(
                        n.allocations.iter().any(|a| {
                            matches!(a.purpose, AllocationPurpose::Endpoints)
                                && a.subnet.net().network() == current.0
                                && a.subnet.net().prefix_len() == current.1
                        }),
                        Conflict,
                        &p,
                        "gateway member needs an endpoint allocation",
                    );
                }
                _ => c.require(
                    false,
                    UnknownReference,
                    &p,
                    "gateway member must reference a gateway interface in its domain",
                ),
            }
        }
        c.require(
            g.owner.as_ref().is_some_and(|o| members.contains(o)),
            Conflict,
            &p,
            "baseline gateway owner must be a member",
        );
        if g.members.len() == 2 {
            c.require(
                g.virtual_address.is_some() && g.sync_links.len() >= 2,
                Conflict,
                &p,
                "HA gateway needs a virtual address and two control paths",
            );
        } else {
            c.require(
                g.virtual_address.is_none() && g.sync_links.is_empty(),
                Conflict,
                &p,
                "standalone gateway has no HA control/virtual address",
            );
        }
        if let Some(vip) = &g.virtual_address {
            let v = vip.net();
            c.require(
                reserved(n, &g.domain, vip)
                    && subnet == Some((v.network(), v.prefix_len()))
                    && virtuals.insert(v.addr())
                    && !interfaces.values().any(|f| match &f.kind {
                        InterfaceKind::Routed { address, .. }
                        | InterfaceKind::Loopback { address, .. }
                        | InterfaceKind::Gateway { address, .. } => {
                            address.net().addr() == v.addr()
                        }
                        _ => false,
                    }),
                Conflict,
                &p,
                "virtual address must be unique and in member subnet",
            );
        }
        let mut controls = BTreeSet::new();
        for id in &g.sync_links {
            c.require(
                controls.insert(id),
                Conflict,
                &p,
                "duplicate HA control cable",
            );
            c.require(
                links.get(id).is_some_and(|l| {
                    members.contains(&l.from.device)
                        && members.contains(&l.to.device)
                        && interfaces
                            .get(&(&l.from.device, &l.from.interface))
                            .is_some_and(|f| matches!(f.kind, InterfaceKind::HaControl { .. }))
                }),
                UnknownReference,
                &p,
                "HA control cable must connect gateway peers",
            );
        }
    }
    for d in &n.devices {
        for f in &d.interfaces {
            if let InterfaceKind::Gateway { domain, .. } = &f.kind {
                c.require(
                    n.gateways
                        .iter()
                        .filter(|g| &g.domain == domain)
                        .flat_map(|g| &g.members)
                        .filter(|m| m.device == d.id && m.interface == f.id)
                        .count()
                        == 1,
                    UnknownReference,
                    "/gateways",
                    "every logical gateway interface must have exactly one ownership record",
                );
            }
        }
    }
}

fn reserved(n: &NetworkDraft, domain: &DomainId, address: &Ipv4InterfaceAddress) -> bool {
    let Some(domain) = n.domains.iter().find(|d| &d.id == domain) else {
        return false;
    };
    let net = address.net();
    let host = u64::from(u32::from(net.addr()));
    n.allocations.iter().any(|a| {
        let start = u64::from(u32::from(a.subnet.net().network())) + 1;
        a.zone == domain.zone
            && matches!(a.purpose, AllocationPurpose::Endpoints)
            && a.subnet.net().prefix_len() == net.prefix_len()
            && a.subnet.net().network() == net.network()
            && host >= start
            && host < start + u64::from(a.reserved_hosts)
    })
}
