mod common;

use crate_sentinel_mcp::policy::config::load_policy;
use serde::Serialize;
use std::path::Path;

#[derive(Serialize)]
struct PolicySnapshot {
    default_allow_major: bool,
    default_msrv_enforce: bool,
    strict_allow_major: bool,
    strict_msrv_enforce: bool,
    strict_perf_enforce: bool,
}

#[tokio::test]
async fn policy_contract() {
    let (default_policy, _) =
        load_policy(Path::new("tests/fixtures/basic_ws")).expect("default policy");
    let temp_dir = tempfile::tempdir().expect("tempdir");
    common::write_file(
        &temp_dir.path().join("upgrade_policy.toml"),
        r#"
[general]
allow_major = true

[msrv]
enforce = true
max_allowed = "1.85"

[performance]
enforce = true
command = "echo PERF=1"
max_slowdown_percent = 5
"#,
    );
    let (strict_policy, _) = load_policy(temp_dir.path()).expect("strict policy");
    let snapshot = PolicySnapshot {
        default_allow_major: default_policy.general.allow_major,
        default_msrv_enforce: default_policy.msrv.enforce,
        strict_allow_major: strict_policy.general.allow_major,
        strict_msrv_enforce: strict_policy.msrv.enforce,
        strict_perf_enforce: strict_policy.performance.enforce,
    };
    common::compare_with_golden(&snapshot, "tests/golden/policy.json");
}
