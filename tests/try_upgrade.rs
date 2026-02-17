mod common;

use crate_sentinel_mcp::mcp::tools::{DepsTryUpgradeInput, deps_try_upgrade};
use rmcp::handler::server::wrapper::Parameters;

#[tokio::test]
async fn try_upgrade_contract() {
    let params: Parameters<DepsTryUpgradeInput> = Parameters(DepsTryUpgradeInput {
        session_id: "missing-session".to_string(),
        workspace_path: common::fixture_workspace(),
        crate_name: "serde".to_string(),
    });
    let output = deps_try_upgrade(params)
        .await
        .expect("deps_try_upgrade output")
        .0;
    common::compare_with_golden(&output, "tests/golden/try_upgrade.json");
}
