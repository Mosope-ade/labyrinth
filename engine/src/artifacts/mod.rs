//! Verified vendor-neutral engineering packages with deterministic byte manifests.
mod compile;
mod encode;
mod revision;
mod types;
use crate::{
    catalog::CatalogSnapshot, intent::ResolvedIntent, topology::AllocatedTopology, verification,
};
pub use revision::{BaseReference, BaseRevision, generate_incremental};
use std::collections::BTreeMap;
pub use types::*;
pub const MAX_FILE_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_PACKAGE_BYTES: usize = 128 * 1024 * 1024;
/// Stateless generation. Incremental designs require an authoritative base context;
/// they never fall back to redesign when this entry point has no such context.
pub fn generate(
    intent: &ResolvedIntent,
    catalog: &CatalogSnapshot,
    options: &GenerationOptions,
) -> Result<VerifiedPackage, GenerationError> {
    compile::compile(intent, catalog, options, None)
}
pub(crate) fn from_allocation(
    intent: &ResolvedIntent,
    catalog: &CatalogSnapshot,
    options: &GenerationOptions,
    t: &AllocatedTopology,
) -> Result<VerifiedPackage, GenerationError> {
    let (n, _snapshot) = verification::prepare(intent, catalog, t, &options.logical)?;
    let report = verification::verify(
        intent,
        catalog,
        t,
        &options.logical,
        &n,
        &options.verification,
    );
    if !report.passed() {
        return Err(GenerationError::VerificationFailed(Box::new(report)));
    }
    let mut files = BTreeMap::new();
    let mut add = |name: &str, bytes: Vec<u8>| -> Result<(), GenerationError> {
        if bytes.len() > MAX_FILE_BYTES {
            return Err(GenerationError::PackageLimit {
                file: name.into(),
                bytes: bytes.len() as u64,
                limit: MAX_FILE_BYTES as u64,
            });
        }
        files.insert(name.to_string(), bytes);
        Ok(())
    };
    add("requirements.json", encode::requirements(intent)?)?;
    add("provenance.json", encode::provenance(intent)?)?;
    add("options.json", encode::json(options)?)?;
    add("topology.json", encode::json(&n)?)?;
    add("catalog.json", encode::json(catalog.bundle())?)?;
    add(
        "ipam.json",
        encode::json(
            &serde_json::json!({"address_plan":t.addresses(),"zones":t.zones(),"transits":t.transits(),"loopbacks":t.loopbacks()}),
        )?,
    )?;
    let mut reservations = Vec::new();
    for demand in &intent.data().demands {
        let target = crate::selection::with_growth(demand.peak_mbps, intent.data().growth_percent)
            .map_err(crate::topology::PlanningError::from)?;
        let reserved = crate::selection::reserved_peak(demand, intent.data().growth_percent)
            .map_err(crate::topology::PlanningError::from)?;
        reservations.push(serde_json::json!({"flow":demand.id,"raw_peak_mbps":demand.peak_mbps,"aggregate_growth_target_mbps":target,"reserved_peak_mbps":reserved,"session_granularity_slack_mbps":reserved-target,"distribution":demand.distribution}));
    }
    reservations.sort_by(|a, b| a["flow"].as_str().cmp(&b["flow"].as_str()));
    add(
        "routing.json",
        encode::json(
            &serde_json::json!({"destination_routes":n.forwarding_routes,"demand_reservations":reservations,"traffic_paths":n.traffic_paths,"wan_connections":n.wan_connections}),
        )?,
    )?;
    let mut requested_policies = intent.data().policies.clone();
    requested_policies.sort_by(|a, b| a.id.cmp(&b.id));
    add(
        "firewall.json",
        encode::json(
            &serde_json::json!({"semantics":"stateless_deny_overrides_default_deny","bindings":n.policies,"requested_policies":requested_policies}),
        )?,
    )?;
    add("verification.json", encode::json(&report)?)?;
    add("selection.json", encode::json(t.selection())?)?;
    add(
        "decisions.json",
        encode::json(
            &serde_json::json!({"canonical":n.decisions,"blueprint":t.selection().blueprint.decisions,"addressing_policy":t.policy(),"logical_policy":options.logical}),
        )?,
    )?;
    let mut bom = BTreeMap::new();
    for d in &n.devices {
        let key = (d.role, d.archetype.clone(), d.operating_mode.clone());
        let a = catalog
            .archetypes()
            .iter()
            .find(|a| a.id == d.archetype)
            .expect("validated catalog");
        let line = bom.entry(key).or_insert_with(|| BomLine {
            role: d.role,
            archetype: d.archetype.clone(),
            mode: d.operating_mode.clone(),
            quantity: 0,
            relative_cost_tier: a.relative_cost_tier,
            reference_hints: a.reference_hints.clone(),
        });
        line.quantity += 1;
    }
    add(
        "bom.json",
        encode::json(
            &serde_json::json!({"items":bom.into_values().collect::<Vec<_>>(),"relative_cost_total":t.selection().assignment.score.relative_cost_sum,"cost_unit":"ranking proxy, not money; generic archetypes, not purchase SKUs"}),
        )?,
    )?;
    let digests = files
        .iter()
        .map(|(name, bytes)| {
            (
                name.clone(),
                FileDigest {
                    sha256: encode::hash(bytes),
                    size_bytes: bytes.len() as u64,
                },
            )
        })
        .collect();
    let manifest = PackageManifest {
        schema_version: "engineering_package_v1".into(),
        engine_version: env!("CARGO_PKG_VERSION").into(),
        rules: BTreeMap::from([
            (
                "blueprint".into(),
                t.selection().blueprint.rules_version.clone(),
            ),
            ("addressing".into(), t.addresses().rules_version().into()),
            ("logical".into(), "logical_rules_v1".into()),
            ("routing".into(), "routing_sessions_v1".into()),
            ("security".into(), "deny_overrides_v1".into()),
            ("verification".into(), report.rules_version.clone()),
        ]),
        catalog_version: catalog.version().clone(),
        requirements_sha256: encode::hash(&files["requirements.json"]),
        options_sha256: encode::hash(&files["options.json"]),
        files: digests,
        base_revision: None,
    };
    let bytes = encode::json(&manifest)?;
    let manifest_sha256 = encode::hash(&bytes);
    files.insert("manifest.json".into(), bytes);
    let total: usize = files.values().map(Vec::len).sum();
    if total > MAX_PACKAGE_BYTES {
        return Err(GenerationError::PackageLimit {
            file: "package".into(),
            bytes: total as u64,
            limit: MAX_PACKAGE_BYTES as u64,
        });
    }
    Ok(VerifiedPackage {
        manifest,
        manifest_sha256,
        files,
        network: n,
        report,
        options: options.clone(),
    })
}
