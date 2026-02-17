#![allow(dead_code)]

use crate_sentinel_mcp::util::json::{ensure_stable_json, strip_timestamps};
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

pub fn fixture_workspace() -> String {
    PathBuf::from("tests/fixtures/basic_ws")
        .to_string_lossy()
        .to_string()
}

pub fn compare_with_golden<T: Serialize>(value: &T, golden_path: &str) {
    let actual = ensure_stable_json(value).expect("stable json");
    let expected = fs::read_to_string(golden_path).expect("read golden");
    let mut actual_json: Value = serde_json::from_str(&actual).expect("parse actual");
    let mut expected_json: Value = serde_json::from_str(&expected).expect("parse expected");
    strip_timestamps(&mut actual_json);
    strip_timestamps(&mut expected_json);
    normalize_dynamic_fields(&mut actual_json);
    normalize_dynamic_fields(&mut expected_json);
    assert_eq!(
        actual_json, expected_json,
        "golden mismatch for {}",
        golden_path
    );
}

pub fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(path, contents).expect("write file");
}

fn normalize_dynamic_fields(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if map.contains_key("workspace_root") {
                map.insert(
                    "workspace_root".to_string(),
                    Value::String("<workspace_root>".to_string()),
                );
            }
            if map.contains_key("session_id") {
                map.insert(
                    "session_id".to_string(),
                    Value::String("<session_id>".to_string()),
                );
            }
            if map.contains_key("git_commit") {
                map.insert(
                    "git_commit".to_string(),
                    Value::String("<git_commit>".to_string()),
                );
            }
            for nested in map.values_mut() {
                normalize_dynamic_fields(nested);
            }
        }
        Value::Array(items) => {
            for item in items {
                normalize_dynamic_fields(item);
            }
        }
        _ => {}
    }
}
