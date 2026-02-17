use std::path::Path;
use std::process::Command;

use anyhow::Error as AnyError;

use crate::error::{AppError, AppResult};
use crate::policy::config::PerformancePolicy;

#[derive(Debug, Clone)]
pub struct PerformanceResult {
    pub slowdown_percent: f64,
    pub ok: bool,
    pub reason: Option<String>,
}

pub fn maybe_check_performance(
    workspace_root: &Path,
    policy: &PerformancePolicy,
    baseline_phase: impl FnOnce() -> AppResult<()>,
    current_phase: impl FnOnce() -> AppResult<()>,
) -> AppResult<PerformanceResult> {
    if !policy.enforce {
        baseline_phase()?;
        current_phase()?;
        return Ok(PerformanceResult {
            slowdown_percent: 0.0,
            ok: true,
            reason: None,
        });
    }

    let command = policy.command.clone().ok_or_else(|| {
        AppError::InvalidInput(
            "performance.command is required when performance.enforce=true".to_string(),
        )
    })?;
    let max_slowdown = policy.max_slowdown_percent.unwrap_or(0.0);

    let baseline = run_perf_command(workspace_root, &command)?;
    baseline_phase()?;
    current_phase()?;
    let current = run_perf_command(workspace_root, &command)?;

    let slowdown_percent = if baseline == 0.0 {
        0.0
    } else {
        ((current - baseline) / baseline) * 100.0
    };
    let ok = slowdown_percent <= max_slowdown;
    Ok(PerformanceResult {
        slowdown_percent,
        ok,
        reason: if ok {
            None
        } else {
            Some("performance_regression_exceeds_policy".to_string())
        },
    })
}

pub fn run_perf_command(workspace_root: &Path, command: &str) -> AppResult<f64> {
    let output = if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(["/C", command])
            .current_dir(workspace_root)
            .output()?
    } else {
        Command::new("sh")
            .args(["-lc", command])
            .current_dir(workspace_root)
            .output()?
    };

    if !output.status.success() {
        return Err(AppError::Other(AnyError::msg(format!(
            "performance command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    parse_perf_metric(&stdout).ok_or_else(|| {
        AppError::Other(AnyError::msg(
            "performance command must print PERF=<number> or PERF_CURRENT=<number>".to_string(),
        ))
    })
}

fn parse_perf_metric(output: &str) -> Option<f64> {
    for line in output.lines() {
        if let Some(value) = line.strip_prefix("PERF_CURRENT=")
            && let Ok(parsed) = value.trim().parse::<f64>()
        {
            return Some(parsed);
        }
        if let Some(value) = line.strip_prefix("PERF=")
            && let Ok(parsed) = value.trim().parse::<f64>()
        {
            return Some(parsed);
        }
        if let Some(value) = line.strip_prefix("PERF_BASELINE=")
            && let Ok(parsed) = value.trim().parse::<f64>()
        {
            return Some(parsed);
        }
    }
    None
}
