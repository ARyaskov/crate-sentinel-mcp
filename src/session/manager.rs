use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

use crate::error::AppError;
use crate::persistence::{recovery, store};
use crate::session::model::{AuditEvent, Session, SessionState};

#[derive(Debug, Clone)]
pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, Session>>>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn start_session(&self, workspace_root: &str) -> Result<Session, AppError> {
        let mut sessions = self.sessions.write().await;
        if sessions
            .values()
            .any(|session| session.workspace_root == workspace_root)
        {
            return Err(AppError::InvalidInput(
                "workspace locked by another active session".to_string(),
            ));
        }

        let mut session = Session {
            session_id: Uuid::new_v4().to_string(),
            workspace_root: workspace_root.to_string(),
            current_crate: None,
            state: SessionState::Idle,
            locked: true,
            audit: Vec::new(),
            last_updated: String::new(),
            last_apply_commit: None,
        };
        session.touch();
        store::persist_session(&session)?;
        sessions.insert(session.session_id.clone(), session.clone());
        info!(
            session_id = %session.session_id,
            workspace_root,
            "session started and workspace locked"
        );
        Ok(session)
    }

    pub async fn get_session(&self, session_id: &str) -> Result<Session, AppError> {
        let sessions = self.sessions.read().await;
        sessions
            .get(session_id)
            .cloned()
            .ok_or_else(|| AppError::InvalidInput("invalid session_id".to_string()))
    }

    pub async fn end_session(&self, session_id: &str) -> Result<bool, AppError> {
        let mut sessions = self.sessions.write().await;
        let removed = sessions.remove(session_id);
        if let Some(session) = removed {
            store::delete_session(&session.workspace_root, session_id)?;
            info!(session_id, "session ended and workspace lock released");
            return Ok(true);
        }
        Ok(false)
    }

    pub async fn recover_workspace(
        &self,
        workspace_root: &str,
    ) -> Result<recovery::RecoveryReport, AppError> {
        let (sessions_to_recover, report) = recovery::recover_workspace(Path::new(workspace_root))?;
        let mut sessions = self.sessions.write().await;
        for session in sessions_to_recover {
            if sessions
                .values()
                .any(|existing| existing.workspace_root == session.workspace_root)
            {
                continue;
            }
            sessions.insert(session.session_id.clone(), session);
        }
        Ok(report)
    }

    pub async fn recover_from_root(
        &self,
        search_root: &Path,
    ) -> Result<recovery::RecoveryReport, AppError> {
        let (sessions_to_recover, report) = recovery::recover_from_search_root(search_root)?;
        let mut sessions = self.sessions.write().await;
        for session in sessions_to_recover {
            if sessions
                .values()
                .any(|existing| existing.workspace_root == session.workspace_root)
            {
                continue;
            }
            sessions.insert(session.session_id.clone(), session);
        }
        Ok(report)
    }

    pub async fn verify_workspace(
        &self,
        session_id: &str,
        workspace_root: &str,
    ) -> Result<Session, AppError> {
        let session = self.get_session(session_id).await?;
        if session.workspace_root != workspace_root {
            return Err(AppError::InvalidInput(
                "session workspace does not match requested workspace".to_string(),
            ));
        }
        Ok(session)
    }

    pub async fn assert_mutation_allowed(
        &self,
        session_id: &str,
        workspace_root: &str,
    ) -> Result<Session, AppError> {
        let session = self.verify_workspace(session_id, workspace_root).await?;
        if session.state == SessionState::Inconsistent {
            return Err(AppError::InvalidInput(
                "session is inconsistent; resolve workspace state before mutation".to_string(),
            ));
        }
        Ok(session)
    }

    pub async fn set_apply_commit(
        &self,
        session_id: &str,
        commit: Option<String>,
    ) -> Result<(), AppError> {
        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| AppError::InvalidInput("invalid session_id".to_string()))?;
        session.last_apply_commit = commit;
        session.touch();
        store::persist_session(session)?;
        Ok(())
    }

    pub async fn mark_inconsistent(&self, session_id: &str) -> Result<(), AppError> {
        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| AppError::InvalidInput("invalid session_id".to_string()))?;
        session.state = SessionState::Inconsistent;
        session.touch();
        store::persist_session(session)?;
        Ok(())
    }

    pub async fn record_event(
        &self,
        session_id: &str,
        event: &str,
        crate_name: &str,
        result: &str,
        next_state: Option<SessionState>,
    ) -> Result<(), AppError> {
        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| AppError::InvalidInput("invalid session_id".to_string()))?;
        let audit = AuditEvent::new(event, crate_name, result);
        info!(
            session_id,
            event = %audit.event,
            crate_name = %audit.crate_name,
            result = %audit.result,
            timestamp = %audit.timestamp,
            "session audit event"
        );
        session.audit.push(audit);
        if let Some(state) = next_state {
            info!(
                session_id,
                from_state = session.state.as_str(),
                to_state = state.as_str(),
                "session state transition"
            );
            session.state = state;
        }
        if !crate_name.is_empty() {
            session.current_crate = Some(crate_name.to_string());
        }
        session.touch();
        store::persist_session(session)?;
        Ok(())
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

static SESSION_MANAGER: OnceLock<SessionManager> = OnceLock::new();

pub fn session_manager() -> &'static SessionManager {
    SESSION_MANAGER.get_or_init(SessionManager::new)
}

pub async fn recover_startup_sessions(search_root: &Path) -> Result<(), AppError> {
    let report = session_manager().recover_from_root(search_root).await?;
    info!(
        recovered_sessions = report.recovered_sessions,
        inconsistent_sessions = report.inconsistent_sessions,
        "startup session recovery completed"
    );
    Ok(())
}
