mod common;

use crate_sentinel_mcp::crates_io::client::set_mock_registry;
use crate_sentinel_mcp::mcp::tools::{DepsCheckUpdatesInput, deps_check_updates};
use rmcp::handler::server::wrapper::Parameters;

#[tokio::test]
async fn deps_check_updates_contract() {
    set_mock_registry(true);
    let params: Parameters<DepsCheckUpdatesInput> = Parameters(DepsCheckUpdatesInput {
        workspace_path: common::fixture_workspace(),
        session_id: None,
    });
    let output = deps_check_updates(params)
        .await
        .expect("deps_check_updates ok")
        .0;
    common::compare_with_golden(&output, "tests/golden/deps_check_updates.json");
}
