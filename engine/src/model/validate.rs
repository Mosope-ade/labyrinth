use super::*;
use crate::{
    catalog::CatalogSnapshot,
    diagnostics::{Checks, IssueCode, ValidationErrors},
};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn validate(
    n: &NetworkDraft,
    catalog: &CatalogSnapshot,
) -> Result<(), ValidationErrors> {
    use IssueCode::*;
    let mut c = Checks::default();
    for (path, len, max) in [
        ("sites", n.sites.len(), 256),
        ("zones", n.zones.len(), 4096),
        ("devices", n.devices.len(), 4096),
        ("links", n.links.len(), 65536),
        ("link_bundles", n.link_bundles.len(), 4096),
        ("domains", n.domains.len(), 4096),
        ("allocations", n.allocations.len(), 4096),
        ("routes", n.routes.len(), 65536),
        ("policies", n.policies.len(), 65536),
        ("decisions", n.decisions.len(), 16384),
        ("layer2_forwarding", n.layer2_forwarding.len(), 4096),
        ("gateways", n.gateways.len(), 4096),
        ("wan_connections", n.wan_connections.len(), 65536),
        ("forwarding_routes", n.forwarding_routes.len(), 65536),
        ("traffic_paths", n.traffic_paths.len(), 4096),
    ] {
        c.require(
            len <= max,
            LimitExceeded,
            format!("/{path}"),
            format!("at most {max} entries"),
        );
    }
    c.require(
        n.devices.iter().all(|d| {
            d.interfaces.len() <= 272
                && d.interfaces.iter().filter(|i| i.port.is_some()).count() <= 256
                && d.interfaces.iter().filter(|i| i.port.is_none()).count() <= 16
        }) && n.devices.iter().map(|d| d.interfaces.len()).sum::<usize>() <= 65536,
        LimitExceeded,
        "/devices",
        "at most 256 physical + 16 logical interfaces/device and 65536 total",
    );
    c.require(
        n.layer2_forwarding.iter().all(|s| {
            s.forwarding.len() <= 65536 && s.blocked.len() <= 65536 && s.unreachable.len() <= 4096
        }) && n
            .layer2_forwarding
            .iter()
            .map(|s| s.forwarding.len() + s.blocked.len())
            .sum::<usize>()
            <= 65536,
        LimitExceeded,
        "/layer2_forwarding",
        "at most 65536 total cable states and 4096 unreachable devices/domain",
    );
    c.require(
        n.traffic_paths
            .iter()
            .all(|p| p.devices.len() <= 128 && p.adjacencies.len() <= 127)
            && n.traffic_paths
                .iter()
                .map(|p| p.adjacencies.len())
                .sum::<usize>()
                <= 65536,
        LimitExceeded,
        "/traffic_paths",
        "bounded session path lengths and at most 65536 total hops",
    );
    c.finish()?;
    let mut c = Checks::default();
    c.require(
        n.schema_version == "network_v1",
        UnsupportedVersion,
        "/schema_version",
        "expected network_v1",
    );
    c.require(
        &n.catalog_version == catalog.version(),
        Conflict,
        "/catalog_version",
        "network must reference the exact validated catalog version/hash",
    );
    c.require(
        !n.sites.is_empty() && !n.devices.is_empty(),
        InvalidValue,
        "/devices",
        "network needs sites and devices",
    );
    let sites: BTreeSet<_> = n.sites.iter().map(|s| &s.id).collect();
    let zones: BTreeMap<_, _> = n.zones.iter().map(|z| (&z.id, z)).collect();
    let domains: BTreeMap<_, _> = n.domains.iter().map(|d| (&d.id, d)).collect();
    let devices: BTreeMap<_, _> = n.devices.iter().map(|d| (&d.id, d)).collect();
    let archetypes: BTreeMap<_, _> = catalog.archetypes().iter().map(|a| (&a.id, a)).collect();
    c.require(
        sites.len() == n.sites.len(),
        DuplicateId,
        "/sites",
        "duplicate site ID",
    );
    c.require(
        zones.len() == n.zones.len(),
        DuplicateId,
        "/zones",
        "duplicate zone ID",
    );
    c.require(
        domains.len() == n.domains.len(),
        DuplicateId,
        "/domains",
        "duplicate domain ID",
    );
    c.require(
        devices.len() == n.devices.len(),
        DuplicateId,
        "/devices",
        "duplicate device ID",
    );
    for (i, s) in n.sites.iter().enumerate() {
        c.require(
            distinct(&s.failure_domains) && s.failure_domains.len() <= 32,
            InvalidValue,
            format!("/sites/{i}/failure_domains"),
            "expected at most 32 distinct failure domains",
        );
    }
    for (i, z) in n.zones.iter().enumerate() {
        c.require(
            sites.contains(&z.site),
            UnknownReference,
            format!("/zones/{i}/site"),
            "unknown site",
        );
    }
    let mut vlan_ids = BTreeSet::new();
    for (i, d) in n.domains.iter().enumerate() {
        let p = format!("/domains/{i}");
        c.require(
            sites.contains(&d.site) && zones.get(&d.zone).is_some_and(|z| z.site == d.site),
            UnknownReference,
            &p,
            "domain site/zone must exist and agree",
        );
        c.require(
            (1..=4094).contains(&d.vlan),
            InvalidValue,
            format!("{p}/vlan"),
            "VLAN must be 1-4094",
        );
        c.require(
            vlan_ids.insert((&d.site, d.vlan)),
            Conflict,
            format!("{p}/vlan"),
            "initial contract scopes VLAN uniqueness by site",
        );
    }
    let mut interfaces = BTreeMap::new();
    let mut interface_ids = BTreeSet::new();
    let mut addresses = BTreeSet::new();
    for (i, d) in n.devices.iter().enumerate() {
        let p = format!("/devices/{i}");
        c.require(
            sites.contains(&d.site),
            UnknownReference,
            format!("{p}/site"),
            "unknown device site",
        );
        c.require(
            distinct(&d.failure_domains) && d.failure_domains.len() <= 32,
            InvalidValue,
            format!("{p}/failure_domains"),
            "expected at most 32 distinct failure domains",
        );
        let archetype = archetypes.get(&d.archetype).copied();
        c.require(
            archetype.is_some(),
            UnknownReference,
            format!("{p}/archetype"),
            "unknown catalog archetype",
        );
        let mode =
            archetype.and_then(|a| a.operating_modes.iter().find(|m| m.id == d.operating_mode));
        c.require(
            mode.is_some(),
            UnknownReference,
            format!("{p}/operating_mode"),
            "unknown archetype operating mode",
        );
        if let Some(a) = archetype {
            c.require(
                a.roles.contains(&d.role),
                Conflict,
                format!("{p}/role"),
                "role is not supported by archetype",
            );
        }
        let mut ports = BTreeSet::new();
        for (j, f) in d.interfaces.iter().enumerate() {
            let fp = format!("{p}/interfaces/{j}");
            c.require(
                interface_ids.insert(&f.id),
                DuplicateId,
                format!("{fp}/id"),
                "interface IDs must be globally unique",
            );
            interfaces.insert((&d.id, &f.id), f);
            let is_loopback = matches!(f.kind, InterfaceKind::Loopback { .. });
            let is_virtual = is_loopback || matches!(f.kind, InterfaceKind::Gateway { .. });
            if is_virtual {
                c.require(
                    f.port.is_none() && f.speed_mbps.is_none() && f.media == Media::Virtual,
                    Conflict,
                    &fp,
                    "loopback requires virtual media, no physical port or speed",
                );
            } else {
                c.require(
                    f.port.is_some() && f.speed_mbps.is_some_and(|s| s > 0),
                    InvalidValue,
                    &fp,
                    "physical interface needs port and positive configured speed",
                );
                if let Some(binding) = &f.port {
                    c.require(
                        ports.insert((&binding.group, binding.index)),
                        Conflict,
                        format!("{fp}/port"),
                        "physical catalog port reused",
                    );
                    let group = archetype
                        .and_then(|a| a.port_groups.iter().find(|g| g.id == binding.group));
                    c.require(
                        group.is_some(),
                        UnknownReference,
                        format!("{fp}/port/group"),
                        "unknown port group",
                    );
                    if let Some(g) = group {
                        c.require(
                            binding.index < g.count,
                            Conflict,
                            format!("{fp}/port/index"),
                            "zero-based port index exceeds catalog count",
                        );
                        c.require(
                            f.media == g.media
                                && f.speed_mbps.is_some_and(|s| g.speeds_mbps.contains(&s)),
                            Conflict,
                            &fp,
                            "media/configured speed unsupported by physical port group",
                        );
                    }
                }
            }
            match &f.kind {
                InterfaceKind::Routed { zone, address }
                | InterfaceKind::Loopback { zone, address }
                | InterfaceKind::Gateway { zone, address, .. } => {
                    if let InterfaceKind::Gateway { domain, .. } = &f.kind {
                        c.require(
                            domains
                                .get(domain)
                                .is_some_and(|g| &g.zone == zone && g.site == d.site),
                            UnknownReference,
                            &fp,
                            "gateway domain/zone/site must agree",
                        );
                    }
                    c.require(
                        zones.get(zone).is_some_and(|z| z.site == d.site),
                        UnknownReference,
                        format!("{fp}/kind/zone"),
                        "interface zone must exist in device site",
                    );
                    c.require(
                        addresses.insert(address.net().addr()),
                        Conflict,
                        format!("{fp}/kind/address"),
                        "duplicate IPv4 host address in the initial single routing realm",
                    );
                    c.require(
                        mode.is_some_and(|m| m.features.contains(&Feature::Routing)),
                        Conflict,
                        &fp,
                        "routed interfaces require routing in the selected mode",
                    );
                    if is_loopback {
                        c.require(
                            address.net().prefix_len() == 32,
                            InvalidValue,
                            &fp,
                            "initial loopback contract requires /32",
                        );
                    }
                }
                InterfaceKind::HaControl { peer } => {
                    c.require(
                        peer != &d.id && devices.get(peer).is_some_and(|p| p.site == d.site),
                        UnknownReference,
                        &fp,
                        "HA peer must exist in the same site",
                    );
                    c.require(
                        mode.is_some_and(|m| m.features.contains(&Feature::HighAvailability)),
                        Conflict,
                        &fp,
                        "HA control requires explicit HA capability",
                    );
                }
                InterfaceKind::Provider { .. } => {
                    c.require(
                        mode.is_some_and(|m| m.features.contains(&Feature::Routing)),
                        Conflict,
                        &fp,
                        "provider handoff requires routing capability",
                    );
                }
                InterfaceKind::Attachment { domain } => {
                    c.require(
                        domains.get(domain).is_some_and(|g| g.site == d.site),
                        UnknownReference,
                        &fp,
                        "attachment domain must exist in the device site",
                    );
                    c.require(
                        mode.is_some_and(|m| m.features.contains(&Feature::Routing)),
                        Conflict,
                        &fp,
                        "routed Ethernet attachment requires routing capability",
                    );
                }
                InterfaceKind::Access { domain } => {
                    c.require(
                        domains.get(domain).is_some_and(|g| g.site == d.site),
                        UnknownReference,
                        format!("{fp}/kind/domain"),
                        "access domain must exist in device site",
                    );
                    c.require(
                        mode.is_some_and(|m| {
                            m.features.contains(&Feature::Switching)
                                && m.features.contains(&Feature::Vlan)
                        }),
                        Conflict,
                        &fp,
                        "access interface requires switching and VLAN in mode",
                    );
                }
                InterfaceKind::Trunk { domains: ds } => {
                    c.require(
                        !ds.is_empty() && ds.len() <= 4096 && distinct(ds),
                        InvalidValue,
                        format!("{fp}/kind/domains"),
                        "trunk requires distinct domain memberships",
                    );
                    for domain in ds.iter().take(4096) {
                        c.require(
                            domains.get(domain).is_some_and(|g| g.site == d.site),
                            UnknownReference,
                            &fp,
                            "trunk domain must exist in device site",
                        );
                    }
                    c.require(
                        mode.is_some_and(|m| {
                            m.features.contains(&Feature::Switching)
                                && m.features.contains(&Feature::Vlan)
                        }),
                        Conflict,
                        &fp,
                        "trunk requires switching and VLAN in mode",
                    );
                }
            }
        }
    }
    let mut links = BTreeMap::new();
    let mut used_endpoints = BTreeSet::new();
    for (i, l) in n.links.iter().enumerate() {
        let p = format!("/links/{i}");
        c.require(
            links.insert(&l.id, l).is_none(),
            DuplicateId,
            format!("{p}/id"),
            "duplicate link ID",
        );
        c.require(
            l.from.device != l.to.device,
            Conflict,
            &p,
            "physical link must connect different devices",
        );
        c.require(
            l.failure_domains.len() <= 32 && distinct(&l.failure_domains),
            InvalidValue,
            format!("{p}/failure_domains"),
            "expected at most 32 distinct failure domains",
        );
        for ep in [&l.from, &l.to] {
            c.require(
                used_endpoints.insert(ep),
                Conflict,
                &p,
                "physical endpoint reused by multiple cables",
            );
        }
        let a = interfaces
            .get(&(&l.from.device, &l.from.interface))
            .copied();
        let b = interfaces.get(&(&l.to.device, &l.to.interface)).copied();
        c.require(
            a.is_some() && b.is_some(),
            UnknownReference,
            &p,
            "link endpoints must belong to the referenced devices",
        );
        if let (Some(a), Some(b)) = (a, b) {
            c.require(
                a.port.is_some() && b.port.is_some(),
                Conflict,
                &p,
                "loopback cannot terminate physical link",
            );
            c.require(
                a.media == b.media
                    && a.speed_mbps == b.speed_mbps
                    && a.speed_mbps
                        .is_some_and(|s| l.bandwidth_mbps > 0 && l.bandwidth_mbps <= s),
                Conflict,
                &p,
                "link media/speed must match and bandwidth fit configured ports",
            );
            match (&a.kind, &b.kind) {
                (
                    InterfaceKind::Routed { address: a, .. },
                    InterfaceKind::Routed { address: b, .. },
                ) => c.require(
                    a.net().network() == b.net().network()
                        && a.net().prefix_len() == b.net().prefix_len()
                        && a.net().addr() != b.net().addr(),
                    Conflict,
                    &p,
                    "routed link endpoints need distinct hosts in the same subnet",
                ),
                (InterfaceKind::HaControl { peer: a }, InterfaceKind::HaControl { peer: b }) => c
                    .require(
                        a == &l.to.device && b == &l.from.device,
                        Conflict,
                        &p,
                        "HA cable must join reciprocal declared peers",
                    ),
                (InterfaceKind::Attachment { domain: a }, InterfaceKind::Access { domain: b })
                | (InterfaceKind::Access { domain: a }, InterfaceKind::Attachment { domain: b })
                | (InterfaceKind::Access { domain: a }, InterfaceKind::Access { domain: b }) => c
                    .require(
                        a == b,
                        Conflict,
                        &p,
                        "connected access ports need the same broadcast domain",
                    ),
                (InterfaceKind::Trunk { domains: a }, InterfaceKind::Trunk { domains: b }) => c
                    .require(
                        a.iter().collect::<BTreeSet<_>>() == b.iter().collect(),
                        Conflict,
                        &p,
                        "trunk membership must agree on both ends",
                    ),
                _ => c.require(
                    false,
                    Conflict,
                    &p,
                    "unsupported mixed interface kinds on physical link",
                ),
            }
        }
    }
    let mut bundles = BTreeSet::new();
    let mut bundled_links = BTreeSet::new();
    for (i, b) in n.link_bundles.iter().enumerate() {
        let p = format!("/link_bundles/{i}");
        c.require(
            bundles.insert(&b.id),
            DuplicateId,
            &p,
            "duplicate bundle ID",
        );
        c.require(
            (2..=256).contains(&b.links.len()) && distinct(&b.links),
            InvalidValue,
            &p,
            "bundle needs 2-256 distinct links",
        );
        let mut pair = None;
        let mut lacp_profiles = BTreeMap::new();
        for id in b.links.iter().take(256) {
            c.require(
                bundled_links.insert(id),
                Conflict,
                &p,
                "link cannot belong to multiple bundles",
            );
            c.require(
                links.contains_key(id),
                UnknownReference,
                &p,
                "bundle references unknown link",
            );
            if let Some(l) = links.get(id) {
                let current = if l.from.device < l.to.device {
                    (&l.from.device, &l.to.device)
                } else {
                    (&l.to.device, &l.from.device)
                };
                if matches!(b.kind, BundleKind::Lacp) {
                    c.require(
                        pair.is_none_or(|previous| previous == current),
                        Conflict,
                        &p,
                        "LACP members must connect the same two devices",
                    );
                    pair = Some(current);
                }
                for ep in [&l.from, &l.to] {
                    let device = devices.get(&ep.device).copied();
                    let mode = device.and_then(|d| {
                        archetypes.get(&d.archetype).and_then(|a| {
                            a.operating_modes.iter().find(|m| m.id == d.operating_mode)
                        })
                    });
                    let feature = match b.kind {
                        BundleKind::Lacp => Feature::Lacp,
                        BundleKind::Ecmp => Feature::Ecmp,
                    };
                    c.require(
                        mode.is_some_and(|m| m.features.contains(&feature)),
                        Conflict,
                        &p,
                        "bundle capability missing in endpoint operating mode",
                    );
                    if let Some(iface) = interfaces.get(&(&ep.device, &ep.interface)) {
                        match b.kind {
                            BundleKind::Lacp => {
                                c.require(matches!(iface.kind,InterfaceKind::Access{..}|InterfaceKind::Trunk{..}|InterfaceKind::Attachment{..}),Conflict,&p,"initial LACP contract bundles L2 interfaces; routed aggregate interfaces are not modeled yet");
                                let profile = (&iface.kind, iface.speed_mbps, iface.media);
                                c.require(
                                    lacp_profiles
                                        .get(&ep.device)
                                        .is_none_or(|previous| *previous == profile),
                                    Conflict,
                                    &p,
                                    "LACP member configuration must agree",
                                );
                                lacp_profiles.insert(&ep.device, profile);
                            }
                            BundleKind::Ecmp => c.require(
                                matches!(iface.kind, InterfaceKind::Routed { .. }),
                                Conflict,
                                &p,
                                "ECMP members must be routed interfaces",
                            ),
                        }
                    }
                }
            }
        }
    }
    super::logical::validate(n, &zones, &devices, &interfaces, &mut c);
    super::forwarding_validate::validate(n, catalog, &mut c);
    super::engineering_validate::validate(n, &mut c);
    c.finish()
}

fn distinct<T: Ord>(v: &[T]) -> bool {
    v.iter().collect::<BTreeSet<_>>().len() == v.len()
}
