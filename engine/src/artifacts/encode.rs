use super::*;
use crate::intent::ResolvedIntent;
use serde_json::Value;
use sha2::{Digest, Sha256};
pub(super) fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(super) fn json<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, GenerationError> {
    fn sorted(v: Value) -> Value {
        match v {
            Value::Object(o) => {
                let ordered: std::collections::BTreeMap<_, _> = o.into_iter().collect();
                Value::Object(ordered.into_iter().map(|(k, v)| (k, sorted(v))).collect())
            }
            Value::Array(a) => Value::Array(a.into_iter().map(sorted).collect()),
            v => v,
        }
    }
    let mut bytes = serde_json::to_vec_pretty(&sorted(serde_json::to_value(value)?))?;
    bytes.push(b'\n');
    Ok(bytes)
}
fn sort(v: &mut Value, key: impl Fn(&Value) -> String) {
    if let Some(a) = v.as_array_mut() {
        a.sort_by_key(key);
    }
}
pub(super) fn requirements(intent: &ResolvedIntent) -> Result<Vec<u8>, GenerationError> {
    let mut value = serde_json::to_value(intent.data())?;
    for site in value["sites"].as_array_mut().expect("resolved sites") {
        sort(&mut site["isp_failure_domains"], Value::to_string);
        for zone in site["zones"].as_array_mut().expect("resolved zones") {
            sort(&mut zone["required_features"], Value::to_string);
        }
        sort(&mut site["zones"], |z| z["id"].to_string());
    }
    sort(&mut value["sites"], |s| s["id"].to_string());
    sort(&mut value["demands"], |d| d["id"].to_string());
    for policy in value["policies"].as_array_mut().expect("resolved policies") {
        sort(&mut policy["service"]["destination_ports"], |p| {
            format!(
                "{:05}/{:05}",
                p["start"].as_u64().unwrap_or(0),
                p["end"].as_u64().unwrap_or(0)
            )
        });
    }
    sort(&mut value["policies"], |p| p["id"].to_string());
    for w in value["wan_services"].as_array_mut().expect("resolved WAN") {
        sort(&mut w["connected_sites"], Value::to_string);
    }
    sort(&mut value["wan_services"], |w| {
        format!("{}/{}", w["site"], w["provider_failure_domain"])
    });
    sort(&mut value["excluded_ipv4"], Value::to_string);
    if value["update"]["mode"] == "incremental_update" {
        sort(&mut value["update"]["ipam_pins"], |p| p["zone"].to_string());
        sort(&mut value["update"]["core_pins"], |p| {
            p["device"].to_string()
        });
    }
    json(&value)
}
pub(super) fn provenance(intent: &ResolvedIntent) -> Result<Vec<u8>, GenerationError> {
    let mut mapped = std::collections::BTreeMap::new();
    for (path, source) in intent.provenance() {
        let mut parts: Vec<String> = path.split('/').map(str::to_owned).collect();
        if parts.get(1).is_some_and(|p| p == "sites")
            && let Some(index) = parts.get(2).and_then(|p| p.parse::<usize>().ok())
        {
            let site = &intent.data().sites[index];
            parts[2] = site.id.to_string();
            if parts.get(3).is_some_and(|p| p == "zones")
                && let Some(index) = parts.get(4).and_then(|p| p.parse::<usize>().ok())
            {
                parts[4] = site.zones[index].id.to_string();
            }
        }
        mapped.insert(parts.join("/"), source);
    }
    json(&mapped)
}
