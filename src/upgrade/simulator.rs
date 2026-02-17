use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::tempdir;
use tracing::info;

use crate::error::AppResult;
use crate::guards::performance;
use crate::policy::config::PerformancePolicy;

#[derive(Debug, Clone)]
pub struct TryUpgradeResult {
    pub compiles: bool,
    pub tests_pass: bool,
    pub check_exit_code: i32,
    pub test_exit_code: i32,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub error: String,
    pub stderr: String,
    pub slowdown_percent: Option<f64>,
}

pub fn simulate_upgrade(
    workspace_root: &Path,
    crate_name: &str,
    performance_policy: &PerformancePolicy,
) -> AppResult<TryUpgradeResult> {
    let temp_dir = tempdir()?;
    let temp_workspace = temp_dir.path().join("workspace");
    fs::create_dir_all(&temp_workspace)?;
    copy_workspace(workspace_root, &temp_workspace)?;

    info!(
        tool = "deps.try_upgrade",
        crate_name,
        temp_workspace = %temp_workspace.display(),
        "created isolated workspace"
    );

    let mut update_error: Option<String> = None;
    let mut check_result: Option<CommandResult> = None;
    let mut test_result: Option<(i32, bool, Vec<String>)> = None;
    let perf = performance::maybe_check_performance(
        &temp_workspace,
        performance_policy,
        || Ok(()),
        || {
            let update_result = run_cargo(&temp_workspace, &["update", "-p", crate_name])?;
            if update_result.exit_code != 0 {
                update_error = Some(update_result.stderr);
                return Ok(());
            }
            check_result = Some(run_cargo(&temp_workspace, &["check"])?);
            test_result = Some(run_tests_if_present(&temp_workspace)?);
            Ok(())
        },
    )?;
    if let Some(stderr) = update_error {
        return Ok(TryUpgradeResult {
            compiles: false,
            tests_pass: false,
            check_exit_code: 1,
            test_exit_code: 1,
            errors: vec!["cargo update failed".to_string()],
            warnings: Vec::new(),
            error: "cargo update failed".to_string(),
            stderr,
            slowdown_percent: None,
        });
    }
    let check_result = if let Some(value) = check_result {
        value
    } else {
        CommandResult {
            exit_code: 1,
            stdout: String::new(),
            stderr: String::new(),
        }
    };
    let (test_exit_code, tests_pass, test_outputs) = if let Some(value) = test_result {
        value
    } else {
        (0, true, Vec::new())
    };
    let compiles = check_result.exit_code == 0;
    let mut outputs = vec![check_result.stderr, check_result.stdout];
    outputs.extend(test_outputs);
    let output_refs = outputs.iter().map(String::as_str).collect::<Vec<_>>();
    let (mut errors, warnings) = collect_messages(&output_refs);
    if !perf.ok {
        errors.push("performance regression exceeds policy".to_string());
    }

    info!(
        tool = "deps.try_upgrade",
        crate_name, compiles, tests_pass, "simulation completed"
    );

    Ok(TryUpgradeResult {
        compiles,
        tests_pass,
        check_exit_code: check_result.exit_code,
        test_exit_code,
        errors,
        warnings,
        error: if perf.ok {
            String::new()
        } else {
            perf.reason
                .unwrap_or_else(|| "performance guard failed".to_string())
        },
        stderr: String::new(),
        slowdown_percent: Some(perf.slowdown_percent),
    })
}

#[derive(Debug)]
struct CommandResult {
    exit_code: i32,
    stdout: String,
    stderr: String,
}

fn run_cargo(workdir: &Path, args: &[&str]) -> AppResult<CommandResult> {
    let output = Command::new("cargo")
        .args(args)
        .current_dir(workdir)
        .output()?;
    Ok(CommandResult {
        exit_code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

fn run_tests_if_present(workspace_root: &Path) -> AppResult<(i32, bool, Vec<String>)> {
    if !workspace_has_tests(workspace_root)? {
        return Ok((0, true, Vec::new()));
    }

    let test_result = run_cargo(workspace_root, &["test"])?;
    let outputs = vec![test_result.stderr.clone(), test_result.stdout.clone()];
    Ok((test_result.exit_code, test_result.exit_code == 0, outputs))
}

fn workspace_has_tests(path: &Path) -> AppResult<bool> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if file_type.is_dir() {
            if should_skip_dir(&name) {
                continue;
            }
            if name == "tests" || workspace_has_tests(&entry.path())? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn copy_workspace(source: &Path, destination: &Path) -> AppResult<()> {
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let name = entry.file_name();
        let name = name.to_string_lossy();

        if file_type.is_dir() {
            if should_skip_dir(&name) {
                continue;
            }
            let next_destination = destination.join(name.as_ref());
            fs::create_dir_all(&next_destination)?;
            copy_workspace(&entry.path(), &next_destination)?;
            continue;
        }

        if file_type.is_file() {
            let target_file = destination.join(name.as_ref());
            fs::copy(entry.path(), target_file)?;
        }
    }
    Ok(())
}

fn should_skip_dir(name: &str) -> bool {
    name == "target" || name == ".git"
}

fn collect_messages(outputs: &[&str]) -> (Vec<String>, Vec<String>) {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    for output in outputs {
        for line in output.lines() {
            if line.starts_with("error:") {
                errors.push(line.to_string());
            } else if line.starts_with("warning:") {
                warnings.push(line.to_string());
            }
        }
    }
    (errors, warnings)
}
