mod common;

use crate_sentinel_mcp::mcp::tools::{DepsScanInput, deps_scan};
use rmcp::handler::server::wrapper::Parameters;

#[tokio::test]
async fn deps_scan_contract() {
    let params: Parameters<DepsScanInput> = Parameters(DepsScanInput {
        workspace_path: common::fixture_workspace(),
        session_id: None,
    });
    let output = deps_scan(params).await.expect("deps_scan ok").0;
    common::compare_with_golden(&output, "tests/golden/deps_scan.json");
}
