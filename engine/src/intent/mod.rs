//! Partial/untrusted requirements and validated readiness; neither is a generated design.
mod resolve;
mod types;
mod validate;
mod wan;
pub use resolve::resolve;
pub use types::*;

pub fn intent_schema() -> schemars::Schema {
    schemars::schema_for!(PartialIntent)
}

pub use crate::model::{
    FlowPolicy, PolicyAction, PortRange, Protocol, ServiceMatch, TrafficDemand,
};

pub use crate::diagnostics::{JsonInputError, MAX_JSON_BYTES};
/// Bounded external JSON entrypoint. Ready means structurally resolved, not verified.
pub fn parse_json(input: &str) -> Result<Readiness, JsonInputError> {
    resolve(crate::diagnostics::decode_json(input)?).map_err(JsonInputError::Invalid)
}

/// Restore immutable hosted requirements and their canonical, stable-ID provenance.
/// Revalidates requirements; this does not establish feasibility or verification.
pub fn restore_snapshot(
    requirements: serde_json::Value,
    provenance: serde_json::Value,
) -> Result<ResolvedIntent, String> {
    let Readiness::Ready(mut intent) =
        resolve(serde_json::from_value(requirements).map_err(|_| "invalid saved requirements")?)
            .map_err(|_| "invalid saved requirements")?
    else {
        return Err("saved requirements need clarification".into());
    };
    let mut supplied: std::collections::BTreeMap<String, ValueSource> =
        serde_json::from_value(provenance).map_err(|_| "invalid saved provenance")?;
    for (path, source) in &mut intent.provenance {
        let mut parts: Vec<String> = path.split('/').map(str::to_owned).collect();
        if parts.get(1).is_some_and(|p| p == "sites")
            && let Some(i) = parts.get(2).and_then(|p| p.parse::<usize>().ok())
        {
            let site = &intent.data.sites[i];
            parts[2] = site.id.to_string();
            if parts.get(3).is_some_and(|p| p == "zones")
                && let Some(i) = parts.get(4).and_then(|p| p.parse::<usize>().ok())
            {
                parts[4] = site.zones[i].id.to_string();
            }
        }
        *source = supplied
            .remove(&parts.join("/"))
            .ok_or("missing saved provenance")?;
    }
    if !supplied.is_empty() {
        return Err("unexpected saved provenance".into());
    }
    Ok(*intent)
}
