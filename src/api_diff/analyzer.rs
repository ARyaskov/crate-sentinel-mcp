use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Error as AnyError;
use tempfile::tempdir;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct ApiDiffSummary {
    pub removed_items: usize,
    pub changed_signatures: usize,
    pub trait_bound_changes: usize,
}

#[derive(Debug, Clone)]
pub struct ApiDiffDetail {
    pub kind: String,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct ApiDiffAnalysis {
    pub breaking: bool,
    pub summary: ApiDiffSummary,
    pub details: Vec<ApiDiffDetail>,
}

pub fn analyze(
    crate_name: &str,
    current_version: &str,
    latest_version: &str,
) -> AppResult<ApiDiffAnalysis> {
    ensure_tool_available(
        &["semver-checks", "--version"],
        "missing required tool: cargo-semver-checks",
    )?;
    ensure_tool_available(
        &["public-api", "--version"],
        "missing required tool: cargo-public-api",
    )?;
    ensure_tool_available(
        &["download", "--version"],
        "missing required tool: cargo-download",
    )?;

    let temp_dir = tempdir()?;
    let old_root = temp_dir.path().join("old");
    let new_root = temp_dir.path().join("new");
    std::fs::create_dir_all(&old_root)?;
    std::fs::create_dir_all(&new_root)?;

    download_crate(crate_name, current_version, &old_root)?;
    download_crate(crate_name, latest_version, &new_root)?;

    let old_manifest = find_manifest(&old_root)?;
    let new_manifest = find_manifest(&new_root)?;

    let semver_output = run_cargo(
        &[
            "semver-checks",
            "check-release",
            "--manifest-path",
            new_manifest.to_string_lossy().as_ref(),
            "--baseline-root",
            old_manifest
                .parent()
                .unwrap_or(Path::new("."))
                .to_string_lossy()
                .as_ref(),
        ],
        None,
    )?;

    let old_api = run_public_api(&old_manifest)?;
    let new_api = run_public_api(&new_manifest)?;
    let mut details = parse_semver_details(&semver_output.stdout, &semver_output.stderr);
    details.extend(extract_added_items(&old_api, &new_api));
    details.sort_by(|left, right| left.kind.cmp(&right.kind).then(left.path.cmp(&right.path)));

    let removed_items = details
        .iter()
        .filter(|item| item.kind == "removed_item")
        .count();
    let changed_signatures = details
        .iter()
        .filter(|item| item.kind == "changed_signature")
        .count();
    let trait_bound_changes = details
        .iter()
        .filter(|item| item.kind == "trait_bound_change")
        .count();
    let breaking = removed_items > 0
        || changed_signatures > 0
        || trait_bound_changes > 0
        || semver_output.exit_code != 0;

    Ok(ApiDiffAnalysis {
        breaking,
        summary: ApiDiffSummary {
            removed_items,
            changed_signatures,
            trait_bound_changes,
        },
        details,
    })
}

#[derive(Debug)]
struct CommandResult {
    exit_code: i32,
    stdout: String,
    stderr: String,
}

fn ensure_tool_available(args: &[&str], message: &str) -> AppResult<()> {
    let result = run_cargo(args, None)?;
    if result.exit_code != 0 {
        return Err(AppError::Other(AnyError::msg(message.to_string())));
    }
    Ok(())
}

fn run_cargo(args: &[&str], workdir: Option<&Path>) -> AppResult<CommandResult> {
    let mut command = Command::new("cargo");
    command.args(args);
    if let Some(dir) = workdir {
        command.current_dir(dir);
    }
    let output = command.output()?;
    Ok(CommandResult {
        exit_code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

fn download_crate(crate_name: &str, version: &str, output_dir: &Path) -> AppResult<()> {
    let result = run_cargo(
        &[
            "download",
            crate_name,
            "--version",
            version,
            "--output",
            output_dir.to_string_lossy().as_ref(),
            "--extract",
        ],
        None,
    )?;
    if result.exit_code != 0 {
        return Err(AppError::Other(AnyError::msg(format!(
            "failed to download crate {crate_name}@{version}: {}",
            result.stderr
        ))));
    }
    Ok(())
}

fn find_manifest(root: &Path) -> AppResult<PathBuf> {
    let direct = root.join("Cargo.toml");
    if direct.exists() {
        return Ok(direct);
    }
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let candidate = entry.path().join("Cargo.toml");
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }
    Err(AppError::Other(AnyError::msg(format!(
        "Cargo.toml not found in extracted crate at {}",
        root.display()
    ))))
}

fn run_public_api(manifest_path: &Path) -> AppResult<BTreeSet<String>> {
    let result = run_cargo(
        &[
            "public-api",
            "--manifest-path",
            manifest_path.to_string_lossy().as_ref(),
        ],
        None,
    )?;
    if result.exit_code != 0 {
        return Err(AppError::Other(AnyError::msg(format!(
            "cargo public-api failed: {}",
            result.stderr
        ))));
    }
    Ok(parse_public_api_items(&result.stdout))
}

fn parse_public_api_items(output: &str) -> BTreeSet<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with("warning:"))
        .filter(|line| !line.starts_with("note:"))
        .filter(|line| line.contains("::"))
        .map(|line| line.to_string())
        .collect()
}

fn parse_semver_details(stdout: &str, stderr: &str) -> Vec<ApiDiffDetail> {
    let mut details = BTreeSet::<(String, String)>::new();
    for line in stdout.lines().chain(stderr.lines()) {
        let lower = line.to_lowercase();
        let detail_type = if lower.contains("removed") {
            Some("removed_item")
        } else if lower.contains("signature") && lower.contains("change") {
            Some("changed_signature")
        } else if lower.contains("trait bound") {
            Some("trait_bound_change")
        } else if lower.contains("visibility") {
            Some("visibility_change")
        } else {
            None
        };

        if let Some(kind) = detail_type {
            let path = extract_path(line);
            details.insert((kind.to_string(), path));
        }
    }
    details
        .into_iter()
        .map(|(kind, path)| ApiDiffDetail { kind, path })
        .collect()
}

fn extract_added_items(
    old_api: &BTreeSet<String>,
    new_api: &BTreeSet<String>,
) -> Vec<ApiDiffDetail> {
    new_api
        .difference(old_api)
        .map(|path| ApiDiffDetail {
            kind: "added_item".to_string(),
            path: path.to_string(),
        })
        .collect()
}

fn extract_path(line: &str) -> String {
    line.split_whitespace()
        .map(|token| {
            token.trim_matches(|c: char| c == '`' || c == '\'' || c == '"' || c == ',' || c == '.')
        })
        .find(|token| token.contains("::"))
        .map(ToString::to_string)
        .unwrap_or_else(|| line.trim().to_string())
}
