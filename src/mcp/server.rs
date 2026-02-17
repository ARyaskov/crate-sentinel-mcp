use anyhow::Error as AnyError;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::{Json, tool, tool_handler, tool_router};
use rmcp::{
    ServerHandler, ServiceExt,
    model::{ServerCapabilities, ServerInfo},
    transport::stdio,
};
use tracing::info;

use crate::error::{AppError, AppResult};
use crate::mcp::tools::{
    CiReportInput, CiReportOutput, DepsApiDiffInput, DepsApiDiffOutput, DepsApplyUpgradeInput,
    DepsApplyUpgradeOutput, DepsCheckUpdatesInput, DepsCheckUpdatesOutput, DepsScanInput,
    DepsScanOutput, DepsTryUpgradeInput, DepsTryUpgradeOutput, PingInput, PingOutput,
    RefactorPlanInput, RefactorPlanOutput, RefactorValidateInput, RefactorValidateOutput,
    SessionEndInput, SessionEndOutput, SessionRecoverInput, SessionRecoverOutput,
    SessionStartInput, SessionStartOutput, SessionStatusInput, SessionStatusOutput, ci_report,
    deps_api_diff, deps_apply_upgrade, deps_check_updates, deps_scan, deps_try_upgrade, ping,
    refactor_plan, refactor_validate, session_end, session_recover, session_start, session_status,
};

#[derive(Debug, Clone)]
pub struct SentinelServer {
    tool_router: rmcp::handler::server::tool::ToolRouter<Self>,
}

impl SentinelServer {
    fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl SentinelServer {
    #[tool(
        name = "session.start",
        description = "Start an upgrade governance session for a workspace."
    )]
    async fn session_start(
        &self,
        params: Parameters<SessionStartInput>,
    ) -> Result<Json<SessionStartOutput>, String> {
        session_start(params).await
    }

    #[tool(
        name = "session.status",
        description = "Get current status for a governance session."
    )]
    async fn session_status(
        &self,
        params: Parameters<SessionStatusInput>,
    ) -> Result<Json<SessionStatusOutput>, String> {
        session_status(params).await
    }

    #[tool(
        name = "session.end",
        description = "End a governance session and release workspace lock."
    )]
    async fn session_end(
        &self,
        params: Parameters<SessionEndInput>,
    ) -> Result<Json<SessionEndOutput>, String> {
        session_end(params).await
    }

    #[tool(
        name = "session.recover",
        description = "Recover persisted sessions for a workspace."
    )]
    async fn session_recover(
        &self,
        params: Parameters<SessionRecoverInput>,
    ) -> Result<Json<SessionRecoverOutput>, String> {
        session_recover(params).await
    }

    #[tool(
        name = "refactor.plan",
        description = "Generate deterministic refactor actions from API diff results."
    )]
    async fn refactor_plan(
        &self,
        params: Parameters<RefactorPlanInput>,
    ) -> Result<Json<RefactorPlanOutput>, String> {
        refactor_plan(params).await
    }

    #[tool(
        name = "refactor.validate",
        description = "Validate a proposed unified diff patch in an isolated workspace."
    )]
    async fn refactor_validate(
        &self,
        params: Parameters<RefactorValidateInput>,
    ) -> Result<Json<RefactorValidateOutput>, String> {
        refactor_validate(params).await
    }

    #[tool(
        name = "ci.report",
        description = "Generate deterministic CI summary for dependency policy status."
    )]
    async fn ci_report(
        &self,
        params: Parameters<CiReportInput>,
    ) -> Result<Json<CiReportOutput>, String> {
        ci_report(params).await
    }

    #[tool(description = "Basic liveness tool that echoes a message.")]
    async fn ping(&self, params: Parameters<PingInput>) -> Result<Json<PingOutput>, String> {
        ping(params).await
    }

    #[tool(
        name = "deps.scan",
        description = "Scan a Rust workspace for external crates."
    )]
    async fn deps_scan(
        &self,
        params: Parameters<DepsScanInput>,
    ) -> Result<Json<DepsScanOutput>, String> {
        deps_scan(params).await
    }

    #[tool(
        name = "deps.check_updates",
        description = "Check crates.io for latest versions of workspace dependencies."
    )]
    async fn deps_check_updates(
        &self,
        params: Parameters<DepsCheckUpdatesInput>,
    ) -> Result<Json<DepsCheckUpdatesOutput>, String> {
        deps_check_updates(params).await
    }

    #[tool(
        name = "deps.try_upgrade",
        description = "Safely simulate a crate upgrade in an isolated temporary workspace."
    )]
    async fn deps_try_upgrade(
        &self,
        params: Parameters<DepsTryUpgradeInput>,
    ) -> Result<Json<DepsTryUpgradeOutput>, String> {
        deps_try_upgrade(params).await
    }

    #[tool(
        name = "deps.api_diff",
        description = "Analyze semantic API differences between current and latest crate versions."
    )]
    async fn deps_api_diff(
        &self,
        params: Parameters<DepsApiDiffInput>,
    ) -> Result<Json<DepsApiDiffOutput>, String> {
        deps_api_diff(params).await
    }

    #[tool(
        name = "deps.apply_upgrade",
        description = "Apply a policy-approved dependency upgrade to the real workspace."
    )]
    async fn deps_apply_upgrade(
        &self,
        params: Parameters<DepsApplyUpgradeInput>,
    ) -> Result<Json<DepsApplyUpgradeOutput>, String> {
        deps_apply_upgrade(params).await
    }
}

#[tool_handler]
impl ServerHandler for SentinelServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some("Crate Sentinel MCP server".to_string()),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

pub async fn run() -> AppResult<()> {
    info!("starting mcp server over stdio");
    let server = SentinelServer::new()
        .serve(stdio())
        .await
        .map_err(AnyError::from)
        .map_err(AppError::from)?;

    server
        .waiting()
        .await
        .map_err(AnyError::from)
        .map_err(AppError::from)?;
    Ok(())
}
