use super::types::*;
use crate::{catalog::CatalogSnapshot, model::*, selection::*};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn stable_id(tag: &str, parts: &[&str], index: u64) -> String {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update(
            u64::try_from(part.len())
                .expect("bounded ID input")
                .to_be_bytes(),
        );
        hash.update(part.as_bytes());
    }
    format!("{tag}-{}-{index}", &format!("{:x}", hash.finalize())[..40])
}
pub(super) struct Builder<'a> {
    pub catalog: &'a CatalogSnapshot,
    pub devices: BTreeMap<DeviceId, DeviceRequirement>,
    pub connections: Vec<ConnectionRequirement>,
    pub populations: Vec<AccessPopulation>,
    pub placements: Vec<DevicePlacement>,
    pub external: Vec<ExternalAttachment>,
    pub decisions: Vec<String>,
}
impl<'a> Builder<'a> {
    pub fn new(catalog: &'a CatalogSnapshot) -> Self {
        Self {
            catalog,
            devices: BTreeMap::new(),
            connections: vec![],
            populations: vec![],
            placements: vec![],
            external: vec![],
            decisions: vec![],
        }
    }
    pub fn add(
        &mut self,
        id: DeviceId,
        site: &SiteId,
        zone: Option<&ZoneId>,
        role: DeviceRole,
        features: Vec<Feature>,
        peak: u64,
    ) {
        self.placements.push(DevicePlacement {
            device: id.clone(),
            site: site.clone(),
            zone: zone.cloned(),
        });
        self.devices.insert(
            id.clone(),
            DeviceRequirement {
                id,
                role,
                required_features: features,
                peak_mbps: peak,
                poe_watts: None,
                pinned_archetype: None,
                pinned_mode: None,
                ports: vec![],
            },
        );
    }
    pub fn speed(
        &self,
        a: &DeviceRequirement,
        b: Option<&DeviceRequirement>,
        load: u64,
    ) -> Result<(u64, Media), SelectionError> {
        let mut options = options_for(self.catalog, a, load);
        if let Some(b) = b {
            let peer = options_for(self.catalog, b, load);
            options.retain(|s| peer.contains(s));
        }
        options.into_iter().next().ok_or_else(|| {
            SelectionError::Infeasible(format!(
                "{}: no common catalog physical speed/media for declared load {load} Mbps",
                a.id
            ))
        })
    }
    pub fn connect(
        &mut self,
        a: &DeviceId,
        b: &DeviceId,
        load: u64,
        ordinal: u64,
    ) -> Result<LinkId, SelectionError> {
        let (speed, media) = self.speed(&self.devices[a], Some(&self.devices[b]), load)?;
        let id = LinkId::new(stable_id("link", &[a.as_str(), b.as_str()], ordinal))
            .expect("bounded derived ID");
        let demand =
            PortGroupId::new(stable_id("port", &[id.as_str()], 0)).expect("bounded derived ID");
        for device in [a, b] {
            self.devices
                .get_mut(device)
                .expect("known device")
                .ports
                .push(PortDemand {
                    id: demand.clone(),
                    count: 1,
                    speed_mbps: speed,
                    media,
                    min_poe_watts: None,
                });
        }
        self.connections.push(ConnectionRequirement {
            id: id.clone(),
            from: DemandEndpoint {
                device: a.clone(),
                demand: demand.clone(),
            },
            to: DemandEndpoint {
                device: b.clone(),
                demand,
            },
        });
        Ok(id)
    }
    pub fn external(
        &mut self,
        device: &DeviceId,
        provider: &FailureDomainId,
        count: u16,
    ) -> Result<(), SelectionError> {
        let req = &self.devices[device];
        let (speed, media) = self.speed(req, None, req.peak_mbps)?;
        let demand = PortGroupId::new("provider-circuits").expect("literal ID");
        self.devices
            .get_mut(device)
            .expect("known device")
            .ports
            .push(PortDemand {
                id: demand.clone(),
                count,
                speed_mbps: speed,
                media,
                min_poe_watts: None,
            });
        self.external.push(ExternalAttachment {
            device: device.clone(),
            provider_domain: provider.clone(),
            demand,
            circuit_count: count,
        });
        Ok(())
    }
}
fn options_for(
    catalog: &CatalogSnapshot,
    d: &DeviceRequirement,
    load: u64,
) -> BTreeSet<(u64, Media)> {
    catalog
        .archetypes()
        .iter()
        .filter(|a| a.roles.contains(&d.role))
        .flat_map(|a| a.port_groups.iter())
        .flat_map(|g| g.speeds_mbps.iter().map(move |s| (*s, g.media)))
        .filter(|(s, media)| *s >= load.max(1) && *media != Media::Virtual)
        .collect()
}
