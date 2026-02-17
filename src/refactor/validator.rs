use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::tempdir;

use crate::error::AppResult;

#[derive(Debug, Clone)]
pub struct RefactorValidationResult {
    pub compiles: bool,
    pub tests_pass: bool,
    pub errors: Vec<String>,
    pub error: Option<String>,
}

pub fn validate_patch(workspace_root: &Path, patch: &str) -> AppResult<RefactorValidationResult> {
    validate_patch_paths(patch)?;
    let temp_dir = tempdir()?;
    let temp_workspace = temp_dir.path().join("workspace");
    fs::create_dir_all(&temp_workspace)?;
    copy_workspace(workspace_root, &temp_workspace)?;

    let patch_path = temp_workspace.join("refactor.patch");
    fs::write(&patch_path, patch)?;

    let check_apply = run_command(
        &temp_workspace,
        "git",
        &["apply", "--check", patch_path.to_string_lossy().as_ref()],
    )?;
    if check_apply.exit_code != 0 {
        return Ok(RefactorValidationResult {
            compiles: false,
            tests_pass: false,
            errors: vec!["patch apply failed".to_string()],
            error: Some(check_apply.stderr),
        });
    }

    let apply = run_command(
        &temp_workspace,
        "git",
        &["apply", patch_path.to_string_lossy().as_ref()],
    )?;
    if apply.exit_code != 0 {
        return Ok(RefactorValidationResult {
            compiles: false,
            tests_pass: false,
            errors: vec!["patch apply failed".to_string()],
            error: Some(apply.stderr),
        });
    }

    let check = run_command(&temp_workspace, "cargo", &["check"])?;
    let compiles = check.exit_code == 0;
    let mut errors = parse_errors(&check.stderr);
    let mut tests_pass = true;

    if workspace_has_tests(&temp_workspace)? {
        let test = run_command(&temp_workspace, "cargo", &["test"])?;
        tests_pass = test.exit_code == 0;
        errors.extend(parse_errors(&test.stderr));
    }

    Ok(RefactorValidationResult {
        compiles,
        tests_pass,
        errors,
        error: None,
    })
}

#[derive(Debug)]
struct CommandResult {
    exit_code: i32,
    stderr: String,
}

fn run_command(workdir: &Path, program: &str, args: &[&str]) -> AppResult<CommandResult> {
    let output = Command::new(program)
        .args(args)
        .current_dir(workdir)
        .output()?;
    Ok(CommandResult {
        exit_code: output.status.code().unwrap_or(-1),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

fn parse_errors(stderr: &str) -> Vec<String> {
    stderr
        .lines()
        .filter(|line| line.starts_with("error:"))
        .map(ToString::to_string)
        .collect()
}

fn validate_patch_paths(patch: &str) -> AppResult<()> {
    for line in patch.lines() {
        if !line.starts_with("+++ ") && !line.starts_with("--- ") {
            continue;
        }
        let raw = line[4..].trim();
        if raw == "/dev/null" {
            continue;
        }
        let cleaned = raw
            .strip_prefix("a/")
            .or_else(|| raw.strip_prefix("b/"))
            .unwrap_or(raw);
        let path = PathBuf::from(cleaned);
        if path.is_absolute() {
            return Err(crate::error::AppError::InvalidInput(
                "patch contains absolute path".to_string(),
            ));
        }
        if path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err(crate::error::AppError::InvalidInput(
                "patch contains parent directory traversal".to_string(),
            ));
        }
    }
    Ok(())
}

fn copy_workspace(source: &Path, destination: &Path) -> AppResult<()> {
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let name = entry.file_name();
        let name = name.to_string_lossy();

        if file_type.is_dir() {
            if name == "target" || name == ".git" {
                continue;
            }
            let next_destination = destination.join(name.as_ref());
            fs::create_dir_all(&next_destination)?;
            copy_workspace(&entry.path(), &next_destination)?;
            continue;
        }

        if file_type.is_file() {
            fs::copy(entry.path(), destination.join(name.as_ref()))?;
        }
    }
    Ok(())
}

fn workspace_has_tests(path: &Path) -> AppResult<bool> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if file_type.is_dir() {
            if name == "target" || name == ".git" {
                continue;
            }
            if name == "tests" || workspace_has_tests(&entry.path())? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
