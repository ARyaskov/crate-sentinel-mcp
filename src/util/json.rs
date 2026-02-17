use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub fn ensure_stable_json<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    let raw = serde_json::to_value(value)?;
    let stable = normalize_value(raw);
    serde_json::to_string_pretty(&stable)
}

pub fn strip_timestamps(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.remove("timestamp");
            map.remove("last_updated");
            for nested in map.values_mut() {
                strip_timestamps(nested);
            }
        }
        Value::Array(items) => {
            for item in items {
                strip_timestamps(item);
            }
        }
        _ => {}
    }
}

fn normalize_value(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let sorted = map
                .into_iter()
                .map(|(key, value)| (key, normalize_value(value)))
                .collect::<BTreeMap<_, _>>();
            Value::Object(sorted.into_iter().collect::<Map<_, _>>())
        }
        Value::Array(items) => Value::Array(items.into_iter().map(normalize_value).collect()),
        other => other,
    }
}
