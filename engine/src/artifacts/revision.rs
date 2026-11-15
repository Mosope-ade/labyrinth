use super::*;
use crate::{
    catalog::CatalogSnapshot,
    intent::{self, AllocationPin, CorePin, Readiness, ResolvedIntent, UpdateMode},
    model::*,
};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BaseReference {
    pub revision: RevisionId,
    pub manifest_sha256: String,
}
/// A validated prior immutable engineering package. Loading saved bytes requires
/// an authoritative expected manifest digest from the caller, not a self-asserted hash.
#[derive(Debug, Clone)]
pub struct BaseRevision {
    reference: BaseReference,
    network: NetworkDraft,
    intent: ResolvedIntent,
    options: GenerationOptions,
}
fn conflict(message: impl Into<String>) -> GenerationError {
    GenerationError::RevisionConflict(message.into())
}
impl BaseRevision {
    pub fn from_verified(
        revision: RevisionId,
        package: &VerifiedPackage,
        catalog: &CatalogSnapshot,
    ) -> Result<Self, GenerationError> {
        Self::load_saved(
            revision,
            package.manifest_sha256(),
            package.files(),
            catalog,
        )
    }
    pub fn load_saved(
        revision: RevisionId,
        expected_manifest_sha256: &str,
        files: &BTreeMap<String, Vec<u8>>,
        catalog: &CatalogSnapshot,
    ) -> Result<Self, GenerationError> {
        const NAMES: [&str; 13] = [
            "requirements.json",
            "provenance.json",
            "options.json",
            "topology.json",
            "catalog.json",
            "ipam.json",
            "routing.json",
            "firewall.json",
            "verification.json",
            "selection.json",
            "decisions.json",
            "bom.json",
            "manifest.json",
        ];
        if files.keys().any(|name| !NAMES.contains(&name.as_str()))
            || files.len() != 13
            || files.values().any(|b| b.len() > MAX_FILE_BYTES)
            || files.values().map(Vec::len).sum::<usize>() > MAX_PACKAGE_BYTES
        {
            return Err(conflict("base package violates file/size bounds"));
        }
        let manifest_bytes = files
            .get("manifest.json")
            .ok_or_else(|| conflict("base manifest missing"))?;
        if expected_manifest_sha256.len() != 64
            || !expected_manifest_sha256
                .bytes()
                .all(|b| b.is_ascii_hexdigit())
            || encode::hash(manifest_bytes) != expected_manifest_sha256
        {
            return Err(conflict(
                "base manifest does not match the authoritative expected digest",
            ));
        }
        let manifest: PackageManifest = serde_json::from_slice(manifest_bytes)?;
        if manifest.schema_version != "engineering_package_v1"
            || manifest.engine_version != env!("CARGO_PKG_VERSION")
            || &manifest.catalog_version != catalog.version()
        {
            return Err(conflict(
                "base schema/engine/catalog identity mismatch; migration or redesign required",
            ));
        }
        for (key, expected) in [
            ("blueprint", "blueprint_rules_v3"),
            ("addressing", "addressing_rules_v1"),
            ("logical", "logical_rules_v1"),
            ("routing", "routing_sessions_v1"),
            ("security", "deny_overrides_v1"),
            ("verification", "verification_v1"),
        ] {
            if manifest.rules.get(key).map(String::as_str) != Some(expected) {
                return Err(conflict(format!("unsupported base rule version: {key}")));
            }
        }
        if manifest.files.len() + 1 != files.len() {
            return Err(conflict("base file inventory mismatch"));
        }
        for (name, digest) in &manifest.files {
            if name.contains('/')
                || name.contains('\\')
                || name.contains("..")
                || name == "manifest.json"
            {
                return Err(conflict("unsafe base file name"));
            }
            let data = files
                .get(name)
                .ok_or_else(|| conflict(format!("base payload missing: {name}")))?;
            if digest.size_bytes != data.len() as u64 || digest.sha256 != encode::hash(data) {
                return Err(conflict(format!("base payload digest mismatch: {name}")));
            }
        }
        let get = |name: &str| {
            files
                .get(name)
                .ok_or_else(|| conflict(format!("required base payload missing: {name}")))
        };
        if manifest.requirements_sha256 != encode::hash(get("requirements.json")?)
            || manifest.options_sha256 != encode::hash(get("options.json")?)
        {
            return Err(conflict("base requirements/options identity mismatch"));
        }
        if encode::json(catalog.bundle())? != *get("catalog.json")? {
            return Err(conflict("base catalog bytes contradict the pinned catalog"));
        }
        let network: NetworkDraft = serde_json::from_slice(get("topology.json")?)?;
        network
            .clone()
            .validate(catalog)
            .map_err(|e| conflict(e.to_string()))?;
        let report: crate::verification::VerificationReport =
            serde_json::from_slice(get("verification.json")?)?;
        if !report.passed()
            || [
                crate::verification::PassKind::Structural,
                crate::verification::PassKind::Addressing,
                crate::verification::PassKind::Routing,
                crate::verification::PassKind::Security,
                crate::verification::PassKind::Resilience,
            ]
            .iter()
            .any(|kind| report.passes.iter().filter(|p| &p.kind == kind).count() != 1)
        {
            return Err(conflict("base package is not five-pass verified"));
        }
        let Readiness::Ready(intent) =
            intent::resolve(serde_json::from_slice(get("requirements.json")?)?)
                .map_err(|e| conflict(e.to_string()))?
        else {
            return Err(conflict("base requirements are not resolved"));
        };
        let options: GenerationOptions = serde_json::from_slice(get("options.json")?)?;
        Ok(Self {
            reference: BaseReference {
                revision,
                manifest_sha256: expected_manifest_sha256.into(),
            },
            network,
            intent: *intent,
            options,
        })
    }
    pub fn reference(&self) -> &BaseReference {
        &self.reference
    }
    pub fn update_mode(&self) -> UpdateMode {
        let mut ipam_pins: Vec<_> = self
            .network
            .allocations
            .iter()
            .filter(|a| matches!(a.purpose, AllocationPurpose::Endpoints))
            .map(|a| AllocationPin {
                zone: a.zone.clone(),
                subnet: a.subnet.clone(),
            })
            .collect();
        ipam_pins.sort_by(|a, b| a.zone.cmp(&b.zone));
        let mut core_pins: Vec<_> = self
            .network
            .devices
            .iter()
            .filter(|d| d.role == DeviceRole::CoreSwitch)
            .map(|d| CorePin {
                device: d.id.clone(),
                site: d.site.clone(),
                archetype: d.archetype.clone(),
            })
            .collect();
        core_pins.sort_by(|a, b| a.device.cmp(&b.device));
        UpdateMode::IncrementalUpdate {
            base_revision: self.reference.revision.clone(),
            ipam_pins,
            core_pins,
        }
    }
    fn constraints(
        &self,
        intent: &ResolvedIntent,
        catalog: &CatalogSnapshot,
        options: &GenerationOptions,
    ) -> Result<PinnedConstraints, GenerationError> {
        if self.network.catalog_version != *catalog.version() {
            return Err(conflict(
                "incremental design must use its exact pinned catalog snapshot",
            ));
        }
        let UpdateMode::IncrementalUpdate {
            base_revision,
            ipam_pins,
            core_pins,
        } = &intent.data().update
        else {
            return Err(conflict(
                "incremental entry point requires explicit incremental update mode",
            ));
        };
        if base_revision != &self.reference.revision {
            return Err(conflict("stale or incorrect base revision"));
        }
        let mut supplied = UpdateMode::IncrementalUpdate {
            base_revision: base_revision.clone(),
            ipam_pins: ipam_pins.clone(),
            core_pins: core_pins.clone(),
        };
        if let UpdateMode::IncrementalUpdate {
            ipam_pins,
            core_pins,
            ..
        } = &mut supplied
        {
            ipam_pins.sort_by(|a, b| a.zone.cmp(&b.zone));
            core_pins.sort_by(|a, b| a.device.cmp(&b.device));
        }
        if supplied != self.update_mode() {
            return Err(conflict(
                "all endpoint/core pins must exactly match the authoritative base; omitted or altered pins are not relaxed",
            ));
        }
        if intent.data().redundancy != self.intent.data().redundancy
            || intent.data().growth_percent != self.intent.data().growth_percent
            || intent.data().spare_port_target_percent
                != self.intent.data().spare_port_target_percent
            || encode::json(&options.addressing)? != encode::json(&self.options.addressing)?
        {
            return Err(conflict(
                "incremental additions preserve redundancy/growth/headroom/addressing conventions; use redesign to change those policies",
            ));
        }
        for old in &self.intent.data().sites {
            let Some(site) = intent.data().sites.iter().find(|s| s.id == old.id) else {
                return Err(conflict(format!(
                    "existing site {} cannot be removed",
                    old.id
                )));
            };
            if site.role != old.role || site.location != old.location {
                return Err(conflict(
                    "existing site role/location changes require redesign",
                ));
            }
            for old_zone in &old.zones {
                let Some(zone) = site.zones.iter().find(|z| z.id == old_zone.id) else {
                    return Err(conflict(format!(
                        "existing zone {} cannot be removed or moved",
                        old_zone.id
                    )));
                };
                if zone.endpoint_count < old_zone.endpoint_count
                    || zone.endpoint_attachment != old_zone.endpoint_attachment
                {
                    return Err(conflict(
                        "incremental additions cannot shrink endpoints or change existing attachment profiles",
                    ));
                }
            }
        }
        for old in &self.intent.data().wan_services {
            let Some(service) = intent.data().wan_services.iter().find(|s| {
                s.site == old.site && s.provider_failure_domain == old.provider_failure_domain
            }) else {
                return Err(conflict("existing WAN services cannot be removed"));
            };
            if service.committed_bandwidth_mbps < old.committed_bandwidth_mbps
                || old
                    .connected_sites
                    .iter()
                    .any(|p| !service.connected_sites.contains(p))
            {
                return Err(conflict(
                    "existing WAN capacity/connectivity cannot be reduced",
                ));
            }
        }
        let mut pins = PinnedConstraints {
            allocations: self
                .network
                .allocations
                .iter()
                .map(|a| (a.id.clone(), a.clone()))
                .collect(),
            vlans: self
                .network
                .domains
                .iter()
                .map(|d| ((d.site.clone(), d.zone.clone()), d.vlan))
                .collect(),
            cores: self
                .network
                .devices
                .iter()
                .filter(|d| d.role == DeviceRole::CoreSwitch)
                .map(|d| {
                    (
                        d.id.clone(),
                        CoreAssignment {
                            site: d.site.clone(),
                            archetype: d.archetype.clone(),
                            mode: d.operating_mode.clone(),
                        },
                    )
                })
                .collect(),
            ..Default::default()
        };
        for device in self
            .network
            .devices
            .iter()
            .filter(|d| d.role == DeviceRole::AccessSwitch)
        {
            let zones: BTreeSet<_> = device
                .interfaces
                .iter()
                .filter_map(|i| {
                    if let InterfaceKind::Access { domain } = &i.kind {
                        self.network
                            .domains
                            .iter()
                            .find(|d| &d.id == domain)
                            .map(|d| d.zone.clone())
                    } else {
                        None
                    }
                })
                .collect();
            for zone in zones {
                *pins.access_counts.entry(zone).or_default() += 1;
            }
        }
        Ok(pins)
    }
}
/// Preserve every prior IP allocation/VLAN and core archetype/mode, then reverify
/// the complete proposed design. Conflicts never mutate or replace the base package.
pub fn generate_incremental(
    intent: &ResolvedIntent,
    catalog: &CatalogSnapshot,
    options: &GenerationOptions,
    base: &BaseRevision,
) -> Result<VerifiedPackage, GenerationError> {
    let pins = base.constraints(intent, catalog, options)?;
    let mut package = super::compile::compile(intent, catalog, options, Some(&pins))?;
    package.manifest.base_revision = Some(base.reference.clone());
    let bytes = encode::json(&package.manifest)?;
    package.manifest_sha256 = encode::hash(&bytes);
    package.files.insert("manifest.json".into(), bytes);
    Ok(package)
}
