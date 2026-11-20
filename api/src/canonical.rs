use serde_json::Value;

// JSON object order must not depend on a transitive serde_json preserve_order
// feature: admission_v1 and AI review identities keep stable object ordering.
pub(crate) fn value(node: Value) -> Value {
    match node {
        Value::Object(map) => {
            let sorted: std::collections::BTreeMap<_, _> = map
                .into_iter()
                .map(|(key, child)| (key, value(child)))
                .collect();
            Value::Object(sorted.into_iter().collect())
        }
        Value::Array(values) => Value::Array(values.into_iter().map(value).collect()),
        other => other,
    }
}
