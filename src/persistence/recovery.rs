use std::path::{Path, PathBuf};
use std::process::Command;

use tracing::info;

use crate::error::AppResult;
use crate::persistence::store;
use crate::session::model::{Session, SessionState};

#[derive(Debug, Clone, Copy, Default)]
pub struct RecoveryReport {
    pub recovered_sessions: usize,
    pub inconsistent_sessions: usize,
}

pub fn recover_workspace(workspace_root: &Path) -> AppResult<(Vec<Session>, RecoveryReport)> {
    let mut sessions = Vec::new();
    let mut report = RecoveryReport::default();
    for path in store::list_session_files(workspace_root)? {
        let mut session = store::load_session(&path)?;
        session.locked = true;
        if session.state != SessionState::Idle && is_inconsistent(&session)? {
            session.state = SessionState::Inconsistent;
            report.inconsistent_sessions += 1;
            info!(
                session_id = %session.session_id,
                workspace_root = %session.workspace_root,
                "inconsistent session detected during recovery"
            );
        } else if session.state != SessionState::Idle {
            info!(
                session_id = %session.session_id,
                workspace_root = %session.workspace_root,
                state = session.state.as_str(),
                "recovering active session"
            );
        }
        sessions.push(session);
        report.recovered_sessions += 1;
    }
    Ok((sessions, report))
}

pub fn recover_from_search_root(search_root: &Path) -> AppResult<(Vec<Session>, RecoveryReport)> {
    let mut sessions = Vec::new();
    let mut report = RecoveryReport::default();
    for workspace in find_workspaces_with_sessions(search_root)? {
        let (mut recovered, partial) = recover_workspace(&workspace)?;
        sessions.append(&mut recovered);
        report.recovered_sessions += partial.recovered_sessions;
        report.inconsistent_sessions += partial.inconsistent_sessions;
    }
    Ok((sessions, report))
}

fn find_workspaces_with_sessions(search_root: &Path) -> AppResult<Vec<PathBuf>> {
    let mut workspaces = Vec::new();
    visit_dirs(search_root, &mut workspaces)?;
    workspaces.sort();
    workspaces.dedup();
    Ok(workspaces)
}

fn visit_dirs(dir: &Path, workspaces: &mut Vec<PathBuf>) -> AppResult<()> {
    if !dir.exists() || !dir.is_dir() {
        return Ok(());
    }

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == ".git" || name == "target" {
            continue;
        }
        if name == ".crate-sentinel" {
            let sessions_dir = path.join("sessions");
            if sessions_dir.exists()
                && sessions_dir.is_dir()
                && let Some(parent) = path.parent()
            {
                workspaces.push(parent.to_path_buf());
            }
            continue;
        }
        visit_dirs(&path, workspaces)?;
    }
    Ok(())
}

fn is_inconsistent(session: &Session) -> AppResult<bool> {
    if session.state != SessionState::Applied {
        return Ok(false);
    }

    let workspace = Path::new(&session.workspace_root);
    let is_git_repo = workspace.join(".git").exists();
    if !is_git_repo {
        return Ok(false);
    }

    if session.last_apply_commit.is_none() {
        return is_cargo_lock_dirty(workspace);
    }

    let commit = session.last_apply_commit.as_deref().unwrap_or_default();
    let output = Command::new("git")
        .args(["cat-file", "-e", &format!("{commit}^{{commit}}")])
        .current_dir(workspace)
        .output()?;
    if !output.status.success() {
        return Ok(true);
    }
    Ok(false)
}

fn is_cargo_lock_dirty(workspace_root: &Path) -> AppResult<bool> {
    let output = Command::new("git")
        .args(["status", "--porcelain", "Cargo.lock"])
        .current_dir(workspace_root)
        .output()?;
    Ok(!String::from_utf8_lossy(&output.stdout).trim().is_empty())
}
