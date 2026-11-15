use super::*;
use crate::diagnostics::{Checks, IssueCode::*};
use std::collections::{BTreeMap, BTreeSet};
pub(super) fn validate(n: &NetworkDraft, c: &mut Checks) {
    let interfaces: BTreeMap<_, _> = n
        .devices
        .iter()
        .flat_map(|d| d.interfaces.iter().map(move |i| ((&d.id, &i.id), i)))
        .collect();
    let devices: BTreeMap<_, _> = n.devices.iter().map(|d| (&d.id, d)).collect();
    let mut ids: BTreeSet<_> = n.links.iter().map(|l| &l.id).collect();
    for (i, w) in n.wan_connections.iter().enumerate() {
        let p = format!("/wan_connections/{i}");
        c.require(
            ids.insert(&w.id),
            DuplicateId,
            &p,
            "WAN/internal link identities must be unique",
        );
        let a = devices.get(&w.from.device);
        let b = devices.get(&w.to.device);
        c.require(
            a.zip(b).is_some_and(|(a, b)| a.site != b.site),
            Conflict,
            &p,
            "WAN transport must connect distinct known sites",
        );
        for end in [&w.from, &w.to] {
            c.require(interfaces.get(&(&end.device,&end.interface)).is_some_and(|f|matches!(&f.kind,InterfaceKind::Provider{failure_domain} if failure_domain==&w.provider_failure_domain)&&f.speed_mbps.is_some_and(|s|w.bandwidth_mbps>0&&w.bandwidth_mbps<=s)),Conflict,&p,"WAN endpoint must match provider and configured physical bandwidth");
        }
    }
    let mut keys = BTreeSet::new();
    for (i, r) in n.forwarding_routes.iter().enumerate() {
        let p = format!("/forwarding_routes/{i}");
        c.require(
            keys.insert((&r.device, &r.destination)),
            DuplicateId,
            &p,
            "one selected next hop per device/destination",
        );
        c.require(
            devices.contains_key(&r.device),
            UnknownReference,
            &p,
            "unknown route device",
        );
        match &r.via {
   ForwardingAdjacency::LocalGateway(id)=>c.require(r.next_hop.is_none()&&interfaces.get(&(&r.device,id)).is_some_and(|f|matches!(&f.kind,InterfaceKind::Gateway{address,..} if address.net().network()==r.destination.net().network()&&address.net().prefix_len()==r.destination.net().prefix_len())),Conflict,&p,"local route must terminate at the matching gateway interface"),
   ForwardingAdjacency::Internal(id)=>c.require(n.links.iter().find(|l|&l.id==id).is_some_and(|l|{
    let peer=if l.from.device==r.device{Some(&l.to.device)}else if l.to.device==r.device{Some(&l.from.device)}else{None};
    peer==r.next_hop.as_ref()&&peer.is_some()&& [&l.from,&l.to].iter().all(|e|interfaces.get(&(&e.device,&e.interface)).is_some_and(|f|matches!(f.kind,InterfaceKind::Routed{..})))
   }),Conflict,&p,"route next hop must be the adjacent peer on a routed internal cable"),
   ForwardingAdjacency::Wan(id)=>c.require(n.wan_connections.iter().find(|w|&w.id==id).is_some_and(|w|{
    let peer=if w.from.device==r.device{Some(&w.to.device)}else if w.to.device==r.device{Some(&w.from.device)}else{None};peer==r.next_hop.as_ref()&&peer.is_some()
   }),Conflict,&p,"WAN route next hop must match its declared logical adjacency"),
  }
    }
    for (i, path) in n.traffic_paths.iter().enumerate() {
        let p = format!("/traffic_paths/{i}");
        c.require(
            path.sessions > 0
                && path.session_peak_mbps > 0
                && u128::from(path.peak_mbps)
                    == u128::from(path.sessions) * u128::from(path.session_peak_mbps),
            Conflict,
            &p,
            "invalid session bandwidth reservation",
        );
        c.require(
            path.from_zone != path.to_zone
                && n.zones.iter().any(|z| z.id == path.from_zone)
                && n.zones.iter().any(|z| z.id == path.to_zone),
            UnknownReference,
            &p,
            "traffic zones must exist and differ",
        );
        c.require(
            !path.devices.is_empty()
                && path.devices.len() == path.adjacencies.len() + 1
                && path.devices.iter().collect::<BTreeSet<_>>().len() == path.devices.len()
                && path.devices.iter().all(|d| devices.contains_key(d)),
            Conflict,
            &p,
            "traffic path must be a loop-free sequence of known devices",
        );
        for (hop, pair) in path.adjacencies.iter().zip(path.devices.windows(2)) {
            let matches = |from: &DeviceId, to: &DeviceId| {
                (&pair[0] == from && &pair[1] == to) || (&pair[0] == to && &pair[1] == from)
            };
            let valid = match hop {
                ForwardingAdjacency::Internal(id) => n
                    .links
                    .iter()
                    .any(|l| &l.id == id && matches(&l.from.device, &l.to.device)),
                ForwardingAdjacency::Wan(id) => n
                    .wan_connections
                    .iter()
                    .any(|w| &w.id == id && matches(&w.from.device, &w.to.device)),
                ForwardingAdjacency::LocalGateway(_) => false,
            };
            c.require(
                valid,
                UnknownReference,
                &p,
                "traffic hop must reference a matching adjacent transport",
            );
        }
    }
}
