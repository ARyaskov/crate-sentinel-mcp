use std::path::Path;
use std::process::Command;

use anyhow::Error as AnyError;

use crate::cargo::metadata;
use crate::error::{AppError, AppResult};
use crate::guards::performance;
use crate::policy::config::PerformancePolicy;

#[derive(Debug, Clone)]
pub struct ApplyUpgradeResult {
    pub previous_version: String,
    pub new_version: String,
    pub applied: bool,
    pub git_commit: Option<String>,
    pub error: Option<String>,
    pub slowdown_percent: Option<f64>,
}

pub fn apply_upgrade(
    workspace_root: &Path,
    crate_name: &str,
    target_version: &str,
    performance_policy: &PerformancePolicy,
) -> AppResult<ApplyUpgradeResult> {
    let previous_version = resolve_version(workspace_root, crate_name)?;
    let lock_backup = backup_lockfile(workspace_root)?;
    let is_git_repo = workspace_root.join(".git").exists();

    if is_git_repo && has_uncommitted_changes(workspace_root)? {
        return Ok(ApplyUpgradeResult {
            previous_version,
            new_version: String::new(),
            applied: false,
            git_commit: None,
            error: Some("repository has uncommitted changes".to_string()),
            slowdown_percent: None,
        });
    }

    let mut update_failed: Option<String> = None;
    let mut check_failed = false;
    let perf = performance::maybe_check_performance(
        workspace_root,
        performance_policy,
        || Ok(()),
        || {
            let update_result =
                run_command(workspace_root, "cargo", &["update", "-p", crate_name])?;
            if update_result.exit_code != 0 {
                update_failed = Some(update_result.stderr);
                return Ok(());
            }
            let check_result = run_command(workspace_root, "cargo", &["check"])?;
            check_failed = check_result.exit_code != 0;
            Ok(())
        },
    )?;
    if let Some(stderr) = update_failed {
        return Ok(ApplyUpgradeResult {
            previous_version,
            new_version: String::new(),
            applied: false,
            git_commit: None,
            error: Some(format!("cargo update failed: {stderr}")),
            slowdown_percent: None,
        });
    }
    if check_failed {
        restore_lockfile(workspace_root, lock_backup.clone())?;
        return Ok(ApplyUpgradeResult {
            previous_version,
            new_version: String::new(),
            applied: false,
            git_commit: None,
            error: Some("Compilation failed after update".to_string()),
            slowdown_percent: None,
        });
    }
    if !perf.ok {
        restore_lockfile(workspace_root, lock_backup.clone())?;
        return Ok(ApplyUpgradeResult {
            previous_version,
            new_version: String::new(),
            applied: false,
            git_commit: None,
            error: perf.reason,
            slowdown_percent: Some(perf.slowdown_percent),
        });
    }

    let new_version = resolve_version(workspace_root, crate_name)?;
    if new_version == previous_version {
        return Ok(ApplyUpgradeResult {
            previous_version,
            new_version,
            applied: false,
            git_commit: None,
            error: Some("no version change was applied".to_string()),
            slowdown_percent: Some(perf.slowdown_percent),
        });
    }

    if new_version != target_version {
        return Ok(ApplyUpgradeResult {
            previous_version,
            new_version,
            applied: false,
            git_commit: None,
            error: Some("updated version does not match expected latest version".to_string()),
            slowdown_percent: Some(perf.slowdown_percent),
        });
    }

    let git_commit = if is_git_repo {
        let add_result = run_command(workspace_root, "git", &["add", "-A"])?;
        if add_result.exit_code != 0 {
            return Ok(ApplyUpgradeResult {
                previous_version,
                new_version,
                applied: false,
                git_commit: None,
                error: Some(format!("git add failed: {}", add_result.stderr)),
                slowdown_percent: Some(perf.slowdown_percent),
            });
        }

        let message = format!("chore(deps): upgrade {crate_name} to {target_version}");
        let commit_result = run_command(workspace_root, "git", &["commit", "-m", &message])?;
        if commit_result.exit_code != 0 {
            return Ok(ApplyUpgradeResult {
                previous_version,
                new_version,
                applied: false,
                git_commit: None,
                error: Some(format!("git commit failed: {}", commit_result.stderr)),
                slowdown_percent: Some(perf.slowdown_percent),
            });
        }

        let rev_result = run_command(workspace_root, "git", &["rev-parse", "--short", "HEAD"])?;
        if rev_result.exit_code != 0 {
            return Ok(ApplyUpgradeResult {
                previous_version,
                new_version,
                applied: false,
                git_commit: None,
                error: Some(format!(
                    "failed to read git commit hash: {}",
                    rev_result.stderr
                )),
                slowdown_percent: Some(perf.slowdown_percent),
            });
        }

        Some(rev_result.stdout.trim().to_string())
    } else {
        None
    };

    Ok(ApplyUpgradeResult {
        previous_version,
        new_version,
        applied: true,
        git_commit,
        error: None,
        slowdown_percent: Some(perf.slowdown_percent),
    })
}

#[derive(Debug)]
struct CommandResult {
    exit_code: i32,
    stdout: String,
    stderr: String,
}

fn run_command(workdir: &Path, program: &str, args: &[&str]) -> AppResult<CommandResult> {
    let output = Command::new(program)
        .args(args)
        .current_dir(workdir)
        .output()?;
    Ok(CommandResult {
        exit_code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

fn has_uncommitted_changes(workspace_root: &Path) -> AppResult<bool> {
    let status = run_command(workspace_root, "git", &["status", "--porcelain"])?;
    if status.exit_code != 0 {
        return Err(AppError::Other(AnyError::msg(format!(
            "failed to inspect git status: {}",
            status.stderr
        ))));
    }
    Ok(!status.stdout.trim().is_empty())
}

fn resolve_version(workspace_root: &Path, crate_name: &str) -> AppResult<String> {
    let result = metadata::scan_workspace(workspace_root)?;
    result
        .crates
        .into_iter()
        .filter(|entry| entry.name == crate_name)
        .max_by(|left, right| {
            match (
                semver::Version::parse(&left.version),
                semver::Version::parse(&right.version),
            ) {
                (Ok(a), Ok(b)) => a.cmp(&b),
                _ => left.version.cmp(&right.version),
            }
        })
        .map(|entry| entry.version)
        .ok_or_else(|| {
            AppError::InvalidInput(format!(
                "crate '{crate_name}' not found in workspace dependencies"
            ))
        })
}

fn backup_lockfile(workspace_root: &Path) -> AppResult<Option<Vec<u8>>> {
    let lockfile = workspace_root.join("Cargo.lock");
    if !lockfile.exists() {
        return Ok(None);
    }
    Ok(Some(std::fs::read(lockfile)?))
}

fn restore_lockfile(workspace_root: &Path, backup: Option<Vec<u8>>) -> AppResult<()> {
    let lockfile = workspace_root.join("Cargo.lock");
    match backup {
        Some(content) => std::fs::write(lockfile, content)?,
        None if lockfile.exists() => std::fs::remove_file(lockfile)?,
        None => {}
    }
    Ok(())
}
