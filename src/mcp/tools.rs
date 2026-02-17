use rmcp::Json;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::schemars;
use rmcp::schemars::JsonSchema;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use tracing::info;

use crate::api_diff::analyzer;
use crate::cargo::metadata;
use crate::ci;
use crate::ci::report as ci_report;
use crate::crates_io::client::CratesIoClient;
use crate::error::AppError;
use crate::guards::msrv;
use crate::policy::config::load_policy;
use crate::policy::evaluator::{is_update_allowed, should_include_dependency};
use crate::refactor::{planner, validator};
use crate::session::manager::session_manager;
use crate::session::model::SessionState;
use crate::upgrade::applier;
use crate::upgrade::simulator;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PingInput {
    pub message: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct PingOutput {
    pub response: String,
    pub echo: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DepsScanInput {
    pub workspace_path: String,
    pub session_id: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DepsScanOutput {
    pub workspace_root: String,
    pub crates: Vec<DepsScanCrate>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DepsScanCrate {
    pub name: String,
    pub version: String,
    pub kind: String,
    pub source: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DepsCheckUpdatesInput {
    pub workspace_path: String,
    pub session_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DepsTryUpgradeInput {
    pub session_id: String,
    pub workspace_path: String,
    pub crate_name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DepsApiDiffInput {
    pub workspace_path: String,
    pub crate_name: String,
    pub session_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DepsApplyUpgradeInput {
    pub session_id: String,
    pub workspace_path: String,
    pub crate_name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SessionStartInput {
    pub workspace_path: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SessionStatusInput {
    pub session_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SessionEndInput {
    pub session_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SessionRecoverInput {
    pub workspace_path: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RefactorPlanInput {
    pub session_id: String,
    pub crate_name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RefactorValidateInput {
    pub session_id: String,
    pub patch: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DepsCheckUpdatesOutput {
    pub workspace_root: String,
    pub updates: Vec<DepsCheckUpdateEntry>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DepsCheckUpdateEntry {
    pub name: String,
    pub current_version: String,
    pub latest_version: String,
    pub update_type: String,
    pub allowed: bool,
    pub yanked: bool,
    pub release_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msrv_required: Option<String>,
    pub msrv_ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disallowed_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DepsTryUpgradeOutput {
    #[serde(rename = "crate")]
    pub crate_name: String,
    pub compiles: bool,
    pub tests_pass: bool,
    pub check_exit_code: i32,
    pub test_exit_code: i32,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub error: String,
    pub stderr: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slowdown_percent: Option<f64>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DepsApiDiffOutput {
    #[serde(rename = "crate")]
    pub crate_name: String,
    pub current_version: String,
    pub latest_version: String,
    pub breaking: bool,
    pub summary: DepsApiDiffSummary,
    pub details: Vec<DepsApiDiffDetail>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DepsApiDiffSummary {
    pub removed_items: usize,
    pub changed_signatures: usize,
    pub trait_bound_changes: usize,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DepsApiDiffDetail {
    #[serde(rename = "type")]
    pub detail_type: String,
    pub path: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DepsApplyUpgradeOutput {
    #[serde(rename = "crate")]
    pub crate_name: String,
    pub previous_version: String,
    pub new_version: String,
    pub applied: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slowdown_percent: Option<f64>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SessionStartOutput {
    pub session_id: String,
    pub workspace_root: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SessionStatusOutput {
    pub session_id: String,
    pub workspace_root: String,
    pub current_crate: String,
    pub state: String,
    pub audit_events: usize,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SessionEndOutput {
    pub ended: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SessionRecoverOutput {
    pub recovered_sessions: usize,
    pub inconsistent_sessions: usize,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RefactorPlanOutput {
    #[serde(rename = "crate")]
    pub crate_name: String,
    pub breaking: bool,
    pub refactor_actions: Vec<RefactorActionOutput>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RefactorActionOutput {
    #[serde(rename = "type")]
    pub action_type: String,
    pub old_path: String,
    pub new_path: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RefactorValidateOutput {
    pub compiles: bool,
    pub tests_pass: bool,
    pub errors: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CiReportInput {
    pub workspace_path: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct CiReportOutput {
    pub updates_total: usize,
    pub updates_allowed: usize,
    pub updates_disallowed: usize,
    pub disallowed: Vec<CiDisallowedOutput>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct CiDisallowedOutput {
    pub name: String,
    pub reason: String,
}

#[derive(Debug, Clone)]
struct TryUpgradeRecord {
    latest_version: String,
    success: bool,
}

#[derive(Debug, Clone)]
struct ApiDiffRecord {
    breaking: bool,
    details: Vec<(String, String)>,
}

#[derive(Debug, Clone, Default)]
struct RefactorGateRecord {
    api_diff_done: bool,
    plan_done: bool,
    validate_done: bool,
}

static TRY_UPGRADE_CACHE: OnceLock<Mutex<HashMap<String, TryUpgradeRecord>>> = OnceLock::new();
static API_DIFF_CACHE: OnceLock<Mutex<HashMap<String, ApiDiffRecord>>> = OnceLock::new();
static REFACTOR_GATE_CACHE: OnceLock<Mutex<HashMap<String, RefactorGateRecord>>> = OnceLock::new();

pub async fn ping(params: Parameters<PingInput>) -> Result<Json<PingOutput>, String> {
    info!(tool = "ping", "executing tool");
    validate_ping_input(&params.0).map_err(|error| error.to_string())?;

    Ok(Json(PingOutput {
        response: "pong".to_string(),
        echo: params.0.message,
    }))
}

pub async fn session_start(
    params: Parameters<SessionStartInput>,
) -> Result<Json<SessionStartOutput>, String> {
    let scan =
        metadata::scan_workspace(Path::new(&params.0.workspace_path)).map_err(|e| e.to_string())?;
    let session = session_manager()
        .start_session(&scan.workspace_root)
        .await
        .map_err(|e| e.to_string())?;
    Ok(Json(SessionStartOutput {
        session_id: session.session_id,
        workspace_root: scan.workspace_root,
    }))
}

pub async fn session_status(
    params: Parameters<SessionStatusInput>,
) -> Result<Json<SessionStatusOutput>, String> {
    let session = session_manager()
        .get_session(&params.0.session_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(Json(SessionStatusOutput {
        session_id: session.session_id,
        workspace_root: session.workspace_root,
        current_crate: session.current_crate.unwrap_or_default(),
        state: session.state.as_str().to_string(),
        audit_events: session.audit.len(),
    }))
}

pub async fn session_end(
    params: Parameters<SessionEndInput>,
) -> Result<Json<SessionEndOutput>, String> {
    let ended = session_manager()
        .end_session(&params.0.session_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(Json(SessionEndOutput { ended }))
}

pub async fn session_recover(
    params: Parameters<SessionRecoverInput>,
) -> Result<Json<SessionRecoverOutput>, String> {
    let scan =
        metadata::scan_workspace(Path::new(&params.0.workspace_path)).map_err(|e| e.to_string())?;
    let report = session_manager()
        .recover_workspace(&scan.workspace_root)
        .await
        .map_err(|e| e.to_string())?;
    Ok(Json(SessionRecoverOutput {
        recovered_sessions: report.recovered_sessions,
        inconsistent_sessions: report.inconsistent_sessions,
    }))
}

pub async fn ci_report(params: Parameters<CiReportInput>) -> Result<Json<CiReportOutput>, String> {
    let updates_output = deps_check_updates(Parameters(DepsCheckUpdatesInput {
        workspace_path: params.0.workspace_path.clone(),
        session_id: None,
    }))
    .await
    .map_err(|e| e.to_string())?;

    let rows = updates_output
        .0
        .updates
        .iter()
        .map(|entry| ci_report::UpdateRow {
            name: entry.name.clone(),
            allowed: entry.allowed,
            disallowed_reason: entry.disallowed_reason.clone(),
        })
        .collect::<Vec<_>>();
    let summary = ci_report::summarize(&rows);
    let scan =
        metadata::scan_workspace(Path::new(&params.0.workspace_path)).map_err(|e| e.to_string())?;
    let (policy, _) = load_policy(Path::new(&scan.workspace_root)).map_err(|e| e.to_string())?;
    if policy.ci.mode && policy.ci.fail_on_any_disallowed_update && summary.updates_disallowed > 0 {
        if ci::ci_cli_mode() {
            return Err("{\"error\":\"ci_disallowed_updates\"}".to_string());
        }
        return Err("ci_disallowed_updates".to_string());
    }

    Ok(Json(CiReportOutput {
        updates_total: summary.updates_total,
        updates_allowed: summary.updates_allowed,
        updates_disallowed: summary.updates_disallowed,
        disallowed: summary
            .disallowed
            .into_iter()
            .map(|item| CiDisallowedOutput {
                name: item.name,
                reason: item.reason,
            })
            .collect(),
    }))
}

pub async fn deps_scan(params: Parameters<DepsScanInput>) -> Result<Json<DepsScanOutput>, String> {
    let session_id = params.0.session_id.clone();
    info!(
        tool = "deps.scan",
        workspace_path = %params.0.workspace_path,
        "executing tool"
    );

    let result =
        metadata::scan_workspace(Path::new(&params.0.workspace_path)).map_err(|e| e.to_string())?;
    let crates = result
        .crates
        .into_iter()
        .map(|krate| DepsScanCrate {
            name: krate.name,
            version: krate.version,
            kind: krate.kind,
            source: krate.source,
        })
        .collect::<Vec<_>>();

    info!(
        tool = "deps.scan",
        workspace_path = %params.0.workspace_path,
        external_crates = crates.len(),
        "tool completed"
    );

    if let Some(value) = session_id
        && session_manager()
            .verify_workspace(&value, &result.workspace_root)
            .await
            .is_ok()
    {
        let _ = session_manager()
            .record_event(&value, "scan", "", "success", Some(SessionState::Analyzed))
            .await;
    }

    Ok(Json(DepsScanOutput {
        workspace_root: result.workspace_root,
        crates,
    }))
}

pub async fn deps_check_updates(
    params: Parameters<DepsCheckUpdatesInput>,
) -> Result<Json<DepsCheckUpdatesOutput>, String> {
    let session_id = params.0.session_id.clone();
    let workspace_path = params.0.workspace_path;
    let scan_result =
        metadata::scan_workspace(Path::new(&workspace_path)).map_err(|e| e.to_string())?;
    let workspace_root = scan_result.workspace_root.clone();
    let (policy, policy_loaded) =
        load_policy(Path::new(&workspace_root)).map_err(|e| e.to_string())?;
    info!(
        tool = "deps.check_updates",
        policy_loaded, "policy resolved"
    );

    let crates_io = CratesIoClient::new().map_err(|e| e.to_string())?;
    let dependency_crates = scan_result
        .crates
        .into_iter()
        .filter(|entry| should_include_dependency(&policy, &entry.kind))
        .collect::<Vec<_>>();
    let analyzed_count = dependency_crates.len();
    let mut updates = Vec::with_capacity(analyzed_count);

    for crate_entry in dependency_crates {
        let current_is_prerelease = Version::parse(&crate_entry.version)
            .map(|version| !version.pre.is_empty())
            .unwrap_or(false);

        let update = match crates_io
            .fetch_release_info(&crate_entry.name, current_is_prerelease)
            .await
        {
            Ok(release) => {
                let update_type =
                    classify_update_type(&crate_entry.version, &release.latest_version);
                let mut disallowed_reason = None;
                let mut msrv_required = None;
                let mut msrv_ok = true;
                let mut allowed = is_update_allowed(&policy, &crate_entry.name, &update_type);
                if update_type == "none" {
                    disallowed_reason = Some("no_update".to_string());
                } else if update_type == "unknown" {
                    disallowed_reason = Some("unknown_update".to_string());
                } else if !allowed {
                    disallowed_reason = Some("policy_disallowed".to_string());
                }
                match msrv::evaluate_msrv(
                    &crates_io,
                    &crate_entry.name,
                    &release.latest_version,
                    &policy.msrv,
                )
                .await
                {
                    Ok(decision) => {
                        msrv_required = decision.required;
                        msrv_ok = decision.ok;
                        if !decision.ok {
                            allowed = false;
                            disallowed_reason = decision.disallowed_reason;
                        }
                    }
                    Err(_) if policy.msrv.enforce => {
                        allowed = false;
                        msrv_ok = false;
                        disallowed_reason = Some("msrv_unknown".to_string());
                    }
                    Err(_) => {}
                }
                DepsCheckUpdateEntry {
                    allowed,
                    name: crate_entry.name,
                    current_version: crate_entry.version,
                    latest_version: release.latest_version,
                    update_type,
                    yanked: release.yanked,
                    release_date: release.release_date,
                    msrv_required,
                    msrv_ok,
                    disallowed_reason,
                    error: None,
                }
            }
            Err(error) => DepsCheckUpdateEntry {
                name: crate_entry.name,
                current_version: crate_entry.version,
                latest_version: String::new(),
                update_type: "unknown".to_string(),
                allowed: false,
                yanked: false,
                release_date: None,
                msrv_required: None,
                msrv_ok: !policy.msrv.enforce,
                disallowed_reason: Some("registry_unavailable".to_string()),
                error: Some(error.to_string()),
            },
        };

        updates.push(update);
    }

    updates.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then(left.current_version.cmp(&right.current_version))
    });
    let updates_available = updates
        .iter()
        .filter(|entry| entry.update_type != "none" && entry.update_type != "unknown")
        .count();
    let updates_allowed = updates.iter().filter(|entry| entry.allowed).count();

    info!(
        tool = "deps.check_updates",
        analyzed_crates = analyzed_count,
        crates_with_updates = updates_available,
        updates_allowed,
        "tool completed"
    );

    if let Some(value) = session_id
        && session_manager()
            .verify_workspace(&value, &workspace_root)
            .await
            .is_ok()
    {
        let _ = session_manager()
            .record_event(
                &value,
                "check_updates",
                "",
                "success",
                Some(SessionState::Analyzed),
            )
            .await;
    }

    Ok(Json(DepsCheckUpdatesOutput {
        workspace_root,
        updates,
    }))
}

pub async fn deps_try_upgrade(
    params: Parameters<DepsTryUpgradeInput>,
) -> Result<Json<DepsTryUpgradeOutput>, String> {
    let session_id = params.0.session_id;
    let workspace_path = params.0.workspace_path;
    let crate_name = params.0.crate_name;
    info!(
        tool = "deps.try_upgrade",
        workspace_path = %workspace_path,
        crate_name = %crate_name,
        "executing tool"
    );

    let scan_result = match metadata::scan_workspace(Path::new(&workspace_path)) {
        Ok(value) => value,
        Err(error) => {
            return Ok(Json(failed_upgrade_output(
                crate_name,
                error.to_string(),
                String::new(),
            )));
        }
    };

    let Some(crate_entry) = scan_result
        .crates
        .iter()
        .find(|entry| entry.name == crate_name)
        .cloned()
    else {
        return Ok(Json(failed_upgrade_output(
            crate_name,
            "crate not found in workspace dependencies".to_string(),
            String::new(),
        )));
    };

    let workspace_root = scan_result.workspace_root;
    if let Err(error) = session_manager()
        .assert_mutation_allowed(&session_id, &workspace_root)
        .await
    {
        return Ok(Json(failed_upgrade_output(
            crate_name,
            error.to_string(),
            String::new(),
        )));
    }
    let (policy, _) = match load_policy(Path::new(&workspace_root)) {
        Ok(value) => value,
        Err(error) => {
            return Ok(Json(failed_upgrade_output(
                crate_name,
                error.to_string(),
                String::new(),
            )));
        }
    };

    if !should_include_dependency(&policy, &crate_entry.kind) {
        return Ok(Json(failed_upgrade_output(
            crate_name,
            "crate is excluded by policy include_dev_dependencies setting".to_string(),
            String::new(),
        )));
    }

    let crates_io = match CratesIoClient::new() {
        Ok(client) => client,
        Err(error) => {
            return Ok(Json(failed_upgrade_output(
                crate_name,
                error.to_string(),
                String::new(),
            )));
        }
    };
    let current_is_prerelease = Version::parse(&crate_entry.version)
        .map(|version| !version.pre.is_empty())
        .unwrap_or(false);
    let release = match crates_io
        .fetch_release_info(&crate_entry.name, current_is_prerelease)
        .await
    {
        Ok(value) => value,
        Err(error) => {
            return Ok(Json(failed_upgrade_output(
                crate_name,
                error.to_string(),
                String::new(),
            )));
        }
    };
    if let Ok(decision) = msrv::evaluate_msrv(
        &crates_io,
        &crate_entry.name,
        &release.latest_version,
        &policy.msrv,
    )
    .await
        && !decision.ok
    {
        remember_try_upgrade(&workspace_root, &crate_name, &release.latest_version, false);
        return Ok(Json(failed_upgrade_output(
            crate_name,
            decision
                .disallowed_reason
                .unwrap_or_else(|| "msrv_exceeds_policy".to_string()),
            String::new(),
        )));
    }

    let update_type = classify_update_type(&crate_entry.version, &release.latest_version);
    let allowed = is_update_allowed(&policy, &crate_name, &update_type);
    if !allowed {
        remember_try_upgrade(&workspace_root, &crate_name, &release.latest_version, false);
        let _ = session_manager()
            .record_event(
                &session_id,
                "try_upgrade",
                &crate_name,
                "failure",
                Some(SessionState::Idle),
            )
            .await;
        return Ok(Json(failed_upgrade_output(
            crate_name,
            format!("upgrade not allowed by policy for update type '{update_type}'"),
            String::new(),
        )));
    }

    let result = match simulator::simulate_upgrade(
        Path::new(&workspace_root),
        &crate_name,
        &policy.performance,
    ) {
        Ok(value) => value,
        Err(error) => {
            remember_try_upgrade(&workspace_root, &crate_name, &release.latest_version, false);
            let _ = session_manager()
                .record_event(
                    &session_id,
                    "try_upgrade",
                    &crate_name,
                    "failure",
                    Some(SessionState::Idle),
                )
                .await;
            return Ok(Json(failed_upgrade_output(
                crate_name,
                error.to_string(),
                String::new(),
            )));
        }
    };

    remember_try_upgrade(
        &workspace_root,
        &crate_name,
        &release.latest_version,
        result.compiles && result.tests_pass && result.error.is_empty(),
    );
    let _ = session_manager()
        .record_event(
            &session_id,
            "try_upgrade",
            &crate_name,
            if result.compiles && result.tests_pass && result.error.is_empty() {
                "success"
            } else {
                "failure"
            },
            Some(
                if result.compiles && result.tests_pass && result.error.is_empty() {
                    SessionState::Simulated
                } else {
                    SessionState::Idle
                },
            ),
        )
        .await;

    Ok(Json(DepsTryUpgradeOutput {
        crate_name,
        compiles: result.compiles,
        tests_pass: result.tests_pass,
        check_exit_code: result.check_exit_code,
        test_exit_code: result.test_exit_code,
        errors: result.errors,
        warnings: result.warnings,
        error: result.error,
        stderr: result.stderr,
        slowdown_percent: result.slowdown_percent,
    }))
}

pub async fn deps_api_diff(
    params: Parameters<DepsApiDiffInput>,
) -> Result<Json<DepsApiDiffOutput>, String> {
    let session_id = params.0.session_id.clone();
    let workspace_path = params.0.workspace_path;
    let crate_name = params.0.crate_name;
    info!(
        tool = "deps.api_diff",
        workspace_path = %workspace_path,
        crate_name = %crate_name,
        "executing tool"
    );

    let scan_result = match metadata::scan_workspace(Path::new(&workspace_path)) {
        Ok(value) => value,
        Err(error) => {
            return Ok(Json(failed_api_diff_output(
                crate_name,
                String::new(),
                String::new(),
                error.to_string(),
            )));
        }
    };

    let current_version = match resolve_current_version(&scan_result.crates, &crate_name) {
        Some(version) => version,
        None => {
            return Ok(Json(failed_api_diff_output(
                crate_name,
                String::new(),
                String::new(),
                "crate not found in workspace dependencies".to_string(),
            )));
        }
    };

    let crates_io = match CratesIoClient::new() {
        Ok(value) => value,
        Err(error) => {
            return Ok(Json(failed_api_diff_output(
                crate_name,
                current_version,
                String::new(),
                error.to_string(),
            )));
        }
    };

    let current_is_prerelease = Version::parse(&current_version)
        .map(|version| !version.pre.is_empty())
        .unwrap_or(false);
    let latest_version = match crates_io
        .fetch_release_info(&crate_name, current_is_prerelease)
        .await
    {
        Ok(release) => release.latest_version,
        Err(error) => {
            return Ok(Json(failed_api_diff_output(
                crate_name,
                current_version,
                String::new(),
                error.to_string(),
            )));
        }
    };

    let analysis = match analyzer::analyze(&crate_name, &current_version, &latest_version) {
        Ok(value) => value,
        Err(error) => {
            return Ok(Json(failed_api_diff_output(
                crate_name,
                current_version,
                latest_version,
                error.to_string(),
            )));
        }
    };

    info!(
        tool = "deps.api_diff",
        crate_name = %crate_name,
        current_version = %current_version,
        latest_version = %latest_version,
        breaking = analysis.breaking,
        "analysis completed"
    );

    if let Some(ref value) = session_id
        && session_manager()
            .verify_workspace(value, &scan_result.workspace_root)
            .await
            .is_ok()
    {
        let _ = session_manager()
            .record_event(
                value,
                "api_diff",
                &crate_name,
                "success",
                Some(SessionState::Analyzed),
            )
            .await;
    }

    let detail_pairs = analysis
        .details
        .iter()
        .map(|detail| (detail.kind.clone(), detail.path.clone()))
        .collect::<Vec<_>>();
    let details = detail_pairs
        .iter()
        .map(|(kind, path)| DepsApiDiffDetail {
            detail_type: kind.clone(),
            path: path.clone(),
        })
        .collect();

    if let Some(value) = session_id {
        cache_api_diff(
            &value,
            &scan_result.workspace_root,
            &crate_name,
            analysis.breaking,
            detail_pairs,
        );
    }

    Ok(Json(DepsApiDiffOutput {
        crate_name,
        current_version,
        latest_version,
        breaking: analysis.breaking,
        summary: DepsApiDiffSummary {
            removed_items: analysis.summary.removed_items,
            changed_signatures: analysis.summary.changed_signatures,
            trait_bound_changes: analysis.summary.trait_bound_changes,
        },
        details,
        error: None,
    }))
}

pub async fn refactor_plan(
    params: Parameters<RefactorPlanInput>,
) -> Result<Json<RefactorPlanOutput>, String> {
    let session = session_manager()
        .get_session(&params.0.session_id)
        .await
        .map_err(|e| e.to_string())?;
    let cache_key = workflow_key(
        &params.0.session_id,
        &session.workspace_root,
        &params.0.crate_name,
    );
    let cached = match get_api_diff_cache(&cache_key) {
        Some(value) => value,
        None => {
            return Err(
                "deps.api_diff must be executed first for this session and crate".to_string(),
            );
        }
    };

    let plan = planner::build_plan(&cached.details, cached.breaking);
    mark_refactor_plan_done(&cache_key);
    let _ = session_manager()
        .record_event(
            &params.0.session_id,
            "refactor_plan",
            &params.0.crate_name,
            "success",
            Some(SessionState::Analyzed),
        )
        .await;

    Ok(Json(RefactorPlanOutput {
        crate_name: params.0.crate_name,
        breaking: plan.breaking,
        refactor_actions: plan
            .actions
            .into_iter()
            .map(|action| RefactorActionOutput {
                action_type: action.action_type,
                old_path: action.old_path,
                new_path: action.new_path,
            })
            .collect(),
    }))
}

pub async fn refactor_validate(
    params: Parameters<RefactorValidateInput>,
) -> Result<Json<RefactorValidateOutput>, String> {
    let base_session = session_manager()
        .get_session(&params.0.session_id)
        .await
        .map_err(|e| e.to_string())?;
    let session = session_manager()
        .assert_mutation_allowed(&params.0.session_id, &base_session.workspace_root)
        .await
        .map_err(|e| e.to_string())?;
    let crate_name = session.current_crate.clone().unwrap_or_default();
    if crate_name.is_empty() {
        return Err("session has no current crate; run deps.api_diff first".to_string());
    }

    let cache_key = workflow_key(&params.0.session_id, &session.workspace_root, &crate_name);
    if !refactor_plan_completed(&cache_key) {
        return Err("refactor.plan must be executed before refactor.validate".to_string());
    }

    let result =
        match validator::validate_patch(Path::new(&session.workspace_root), &params.0.patch) {
            Ok(value) => value,
            Err(error) => {
                let _ = session_manager()
                    .record_event(
                        &params.0.session_id,
                        "refactor_validate",
                        &crate_name,
                        "failure",
                        Some(SessionState::Analyzed),
                    )
                    .await;
                return Ok(Json(RefactorValidateOutput {
                    compiles: false,
                    tests_pass: false,
                    errors: vec!["patch validation failed".to_string()],
                    error: Some(error.to_string()),
                }));
            }
        };
    let success = result.compiles && result.tests_pass && result.error.is_none();
    if success {
        mark_refactor_validate_done(&cache_key);
    }
    let _ = session_manager()
        .record_event(
            &params.0.session_id,
            "refactor_validate",
            &crate_name,
            if success { "success" } else { "failure" },
            Some(if success {
                SessionState::Refactored
            } else {
                SessionState::Analyzed
            }),
        )
        .await;

    Ok(Json(RefactorValidateOutput {
        compiles: result.compiles,
        tests_pass: result.tests_pass,
        errors: result.errors,
        error: result.error,
    }))
}

pub async fn deps_apply_upgrade(
    params: Parameters<DepsApplyUpgradeInput>,
) -> Result<Json<DepsApplyUpgradeOutput>, String> {
    let session_id = params.0.session_id;
    let workspace_path = params.0.workspace_path;
    let crate_name = params.0.crate_name;
    info!(
        tool = "deps.apply_upgrade",
        workspace_path = %workspace_path,
        crate_name = %crate_name,
        "executing tool"
    );

    let scan_result = match metadata::scan_workspace(Path::new(&workspace_path)) {
        Ok(value) => value,
        Err(error) => {
            return Ok(Json(failed_apply_output(
                crate_name,
                String::new(),
                String::new(),
                error.to_string(),
            )));
        }
    };

    let Some(crate_entry) = scan_result
        .crates
        .iter()
        .find(|entry| entry.name == crate_name)
        .cloned()
    else {
        return Ok(Json(failed_apply_output(
            crate_name,
            String::new(),
            String::new(),
            "crate not found in workspace dependencies".to_string(),
        )));
    };

    let workspace_root = scan_result.workspace_root;
    if let Err(error) = session_manager()
        .assert_mutation_allowed(&session_id, &workspace_root)
        .await
    {
        return Ok(Json(failed_apply_output(
            crate_name,
            String::new(),
            String::new(),
            error.to_string(),
        )));
    }
    let previous_version = crate_entry.version.clone();
    let (policy, _) = match load_policy(Path::new(&workspace_root)) {
        Ok(value) => value,
        Err(error) => {
            return Ok(Json(failed_apply_output(
                crate_name,
                previous_version,
                String::new(),
                error.to_string(),
            )));
        }
    };

    if !should_include_dependency(&policy, &crate_entry.kind) {
        return Ok(Json(failed_apply_output(
            crate_name,
            previous_version,
            String::new(),
            "crate is excluded by policy include_dev_dependencies setting".to_string(),
        )));
    }

    let crates_io = match CratesIoClient::new() {
        Ok(client) => client,
        Err(error) => {
            return Ok(Json(failed_apply_output(
                crate_name,
                previous_version,
                String::new(),
                error.to_string(),
            )));
        }
    };
    let current_is_prerelease = Version::parse(&crate_entry.version)
        .map(|version| !version.pre.is_empty())
        .unwrap_or(false);
    let release = match crates_io
        .fetch_release_info(&crate_entry.name, current_is_prerelease)
        .await
    {
        Ok(value) => value,
        Err(error) => {
            return Ok(Json(failed_apply_output(
                crate_name,
                previous_version,
                String::new(),
                error.to_string(),
            )));
        }
    };
    if let Ok(decision) = msrv::evaluate_msrv(
        &crates_io,
        &crate_entry.name,
        &release.latest_version,
        &policy.msrv,
    )
    .await
        && !decision.ok
    {
        return Ok(Json(failed_apply_output(
            crate_name,
            previous_version,
            release.latest_version,
            decision
                .disallowed_reason
                .unwrap_or_else(|| "msrv_exceeds_policy".to_string()),
        )));
    }

    let update_type = classify_update_type(&crate_entry.version, &release.latest_version);
    if !is_update_allowed(&policy, &crate_name, &update_type) {
        let _ = session_manager()
            .record_event(
                &session_id,
                "apply_upgrade",
                &crate_name,
                "failure",
                Some(SessionState::Idle),
            )
            .await;
        return Ok(Json(failed_apply_output(
            crate_name,
            previous_version,
            release.latest_version,
            format!("upgrade not allowed by policy for update type '{update_type}'"),
        )));
    }

    if !has_successful_try_upgrade(&workspace_root, &crate_name, &release.latest_version) {
        let _ = session_manager()
            .record_event(
                &session_id,
                "apply_upgrade",
                &crate_name,
                "failure",
                Some(SessionState::Idle),
            )
            .await;
        return Ok(Json(failed_apply_output(
            crate_name,
            previous_version,
            release.latest_version,
            "deps.try_upgrade must succeed for this crate and workspace before apply".to_string(),
        )));
    }
    let gate_key = workflow_key(&session_id, &workspace_root, &crate_name);
    if !refactor_gates_satisfied(&gate_key) {
        let _ = session_manager()
            .record_event(
                &session_id,
                "apply_upgrade",
                &crate_name,
                "failure",
                Some(SessionState::Idle),
            )
            .await;
        return Ok(Json(failed_apply_output(
            crate_name,
            previous_version,
            release.latest_version,
            "refactor gates not satisfied: require api_diff, refactor_plan, and refactor_validate"
                .to_string(),
        )));
    }

    let _ = session_manager().set_apply_commit(&session_id, None).await;

    let apply_result = match applier::apply_upgrade(
        Path::new(&workspace_root),
        &crate_name,
        &release.latest_version,
        &policy.performance,
    ) {
        Ok(value) => value,
        Err(error) => {
            let _ = session_manager()
                .record_event(
                    &session_id,
                    "apply_upgrade",
                    &crate_name,
                    "failure",
                    Some(SessionState::Idle),
                )
                .await;
            return Ok(Json(failed_apply_output(
                crate_name,
                previous_version,
                release.latest_version,
                error.to_string(),
            )));
        }
    };

    if apply_result.applied {
        let _ = session_manager()
            .set_apply_commit(&session_id, apply_result.git_commit.clone())
            .await;
        let _ = session_manager()
            .record_event(
                &session_id,
                "apply_upgrade",
                &crate_name,
                "success",
                Some(SessionState::Applied),
            )
            .await;
        info!(
            tool = "deps.apply_upgrade",
            crate_name = %crate_name,
            previous_version = %apply_result.previous_version,
            new_version = %apply_result.new_version,
            git_commit = ?apply_result.git_commit,
            "upgrade applied"
        );
    } else {
        if apply_result
            .error
            .as_deref()
            .is_some_and(|message| message.contains("Compilation failed after update"))
        {
            let _ = session_manager().mark_inconsistent(&session_id).await;
        }
        let _ = session_manager()
            .record_event(
                &session_id,
                "apply_upgrade",
                &crate_name,
                "failure",
                Some(SessionState::Idle),
            )
            .await;
    }

    Ok(Json(DepsApplyUpgradeOutput {
        crate_name,
        previous_version: apply_result.previous_version,
        new_version: apply_result.new_version,
        applied: apply_result.applied,
        git_commit: apply_result.git_commit,
        error: apply_result.error,
        slowdown_percent: apply_result.slowdown_percent,
    }))
}

fn validate_ping_input(input: &PingInput) -> Result<(), AppError> {
    if input.message.trim().is_empty() {
        return Err(AppError::InvalidInput(
            "message must not be empty".to_string(),
        ));
    }
    Ok(())
}

fn classify_update_type(current_version: &str, latest_version: &str) -> String {
    let Ok(current) = Version::parse(current_version) else {
        return "unknown".to_string();
    };
    let Ok(latest) = Version::parse(latest_version) else {
        return "unknown".to_string();
    };

    if latest <= current {
        return "none".to_string();
    }
    if latest.major > current.major {
        return "major".to_string();
    }
    if latest.minor > current.minor {
        return "minor".to_string();
    }
    "patch".to_string()
}

fn failed_upgrade_output(
    crate_name: String,
    error: String,
    stderr: String,
) -> DepsTryUpgradeOutput {
    DepsTryUpgradeOutput {
        crate_name,
        compiles: false,
        tests_pass: false,
        check_exit_code: 1,
        test_exit_code: 1,
        errors: vec![error.clone()],
        warnings: Vec::new(),
        error,
        stderr,
        slowdown_percent: None,
    }
}

fn failed_api_diff_output(
    crate_name: String,
    current_version: String,
    latest_version: String,
    error: String,
) -> DepsApiDiffOutput {
    DepsApiDiffOutput {
        crate_name,
        current_version,
        latest_version,
        breaking: false,
        summary: DepsApiDiffSummary {
            removed_items: 0,
            changed_signatures: 0,
            trait_bound_changes: 0,
        },
        details: Vec::new(),
        error: Some(error),
    }
}

fn resolve_current_version(crates: &[metadata::ExternalCrate], crate_name: &str) -> Option<String> {
    crates
        .iter()
        .filter(|entry| entry.name == crate_name)
        .max_by(|left, right| compare_versions(&left.version, &right.version))
        .map(|entry| entry.version.clone())
}

fn compare_versions(left: &str, right: &str) -> std::cmp::Ordering {
    match (Version::parse(left), Version::parse(right)) {
        (Ok(a), Ok(b)) => a.cmp(&b),
        _ => left.cmp(right),
    }
}

fn failed_apply_output(
    crate_name: String,
    previous_version: String,
    new_version: String,
    error: String,
) -> DepsApplyUpgradeOutput {
    DepsApplyUpgradeOutput {
        crate_name,
        previous_version,
        new_version,
        applied: false,
        git_commit: None,
        error: Some(error),
        slowdown_percent: None,
    }
}

fn remember_try_upgrade(
    workspace_root: &str,
    crate_name: &str,
    latest_version: &str,
    success: bool,
) {
    let key = format!("{workspace_root}::{crate_name}");
    let cache = TRY_UPGRADE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut map) = cache.lock() {
        map.insert(
            key,
            TryUpgradeRecord {
                latest_version: latest_version.to_string(),
                success,
            },
        );
    }
}

fn has_successful_try_upgrade(
    workspace_root: &str,
    crate_name: &str,
    latest_version: &str,
) -> bool {
    let key = format!("{workspace_root}::{crate_name}");
    let cache = TRY_UPGRADE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(map) = cache.lock() {
        return map
            .get(&key)
            .is_some_and(|record| record.success && record.latest_version == latest_version);
    }
    false
}

fn workflow_key(session_id: &str, workspace_root: &str, crate_name: &str) -> String {
    format!("{session_id}::{workspace_root}::{crate_name}")
}

fn cache_api_diff(
    session_id: &str,
    workspace_root: &str,
    crate_name: &str,
    breaking: bool,
    details: Vec<(String, String)>,
) {
    let key = workflow_key(session_id, workspace_root, crate_name);
    let cache = API_DIFF_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut map) = cache.lock() {
        map.insert(key.clone(), ApiDiffRecord { breaking, details });
    }
    let gates = REFACTOR_GATE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut map) = gates.lock() {
        let entry = map.entry(key).or_default();
        entry.api_diff_done = true;
        entry.plan_done = false;
        entry.validate_done = false;
    }
}

fn get_api_diff_cache(key: &str) -> Option<ApiDiffRecord> {
    let cache = API_DIFF_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(map) = cache.lock() {
        return map.get(key).cloned();
    }
    None
}

fn mark_refactor_plan_done(key: &str) {
    let gates = REFACTOR_GATE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut map) = gates.lock() {
        let entry = map.entry(key.to_string()).or_default();
        entry.plan_done = true;
    }
}

fn refactor_plan_completed(key: &str) -> bool {
    let gates = REFACTOR_GATE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(map) = gates.lock() {
        return map
            .get(key)
            .is_some_and(|entry| entry.api_diff_done && entry.plan_done);
    }
    false
}

fn mark_refactor_validate_done(key: &str) {
    let gates = REFACTOR_GATE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut map) = gates.lock() {
        let entry = map.entry(key.to_string()).or_default();
        entry.validate_done = true;
    }
}

fn refactor_gates_satisfied(key: &str) -> bool {
    let gates = REFACTOR_GATE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(map) = gates.lock() {
        return map
            .get(key)
            .is_some_and(|entry| entry.api_diff_done && entry.plan_done && entry.validate_done);
    }
    false
}
