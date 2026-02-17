use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::AppResult;
use crate::session::model::Session;

const SENTINEL_DIR: &str = ".crate-sentinel";
const SESSIONS_DIR: &str = "sessions";

pub fn session_dir(workspace_root: &Path) -> PathBuf {
    workspace_root.join(SENTINEL_DIR).join(SESSIONS_DIR)
}

pub fn session_file_path(workspace_root: &Path, session_id: &str) -> PathBuf {
    session_dir(workspace_root).join(format!("{session_id}.json"))
}

pub fn persist_session(session: &Session) -> AppResult<()> {
    let workspace_root = Path::new(&session.workspace_root);
    let dir = session_dir(workspace_root);
    fs::create_dir_all(&dir)?;
    let target = session_file_path(workspace_root, &session.session_id);
    atomic_write_json(&target, session)
}

pub fn delete_session(workspace_root: &str, session_id: &str) -> AppResult<()> {
    let path = session_file_path(Path::new(workspace_root), session_id);
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

pub fn load_session(path: &Path) -> AppResult<Session> {
    let raw = fs::read(path)?;
    let session = serde_json::from_slice::<Session>(&raw)?;
    Ok(session)
}

pub fn list_session_files(workspace_root: &Path) -> AppResult<Vec<PathBuf>> {
    let dir = session_dir(workspace_root);
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut files = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
                Some(path)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    files.sort();
    Ok(files)
}

fn atomic_write_json(path: &Path, value: &Session) -> AppResult<()> {
    let mut temp = path.to_path_buf();
    temp.set_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(value)?;

    let mut file = File::create(&temp)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    fs::rename(temp, path)?;
    Ok(())
}
