mod common;

use crate_sentinel_mcp::mcp::tools::{DepsApiDiffInput, deps_api_diff};
use rmcp::handler::server::wrapper::Parameters;

#[tokio::test]
async fn api_diff_contract() {
    let params: Parameters<DepsApiDiffInput> = Parameters(DepsApiDiffInput {
        workspace_path: "tests/fixtures/does_not_exist".to_string(),
        crate_name: "serde".to_string(),
        session_id: None,
    });
    let output = deps_api_diff(params).await.expect("deps_api_diff output").0;
    common::compare_with_golden(&output, "tests/golden/api_diff.json");
}
