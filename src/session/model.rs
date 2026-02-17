use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Idle,
    Analyzed,
    Simulated,
    Refactored,
    Applied,
    Inconsistent,
}

impl SessionState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Analyzed => "analyzed",
            Self::Simulated => "simulated",
            Self::Refactored => "refactored",
            Self::Applied => "applied",
            Self::Inconsistent => "inconsistent",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub timestamp: String,
    pub event: String,
    #[serde(rename = "crate")]
    pub crate_name: String,
    pub result: String,
}

impl AuditEvent {
    pub fn new(event: &str, crate_name: &str, result: &str) -> Self {
        Self {
            timestamp: Utc::now().to_rfc3339(),
            event: event.to_string(),
            crate_name: crate_name.to_string(),
            result: result.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub session_id: String,
    pub workspace_root: String,
    pub current_crate: Option<String>,
    pub state: SessionState,
    pub locked: bool,
    pub audit: Vec<AuditEvent>,
    pub last_updated: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_apply_commit: Option<String>,
}

impl Session {
    pub fn touch(&mut self) {
        self.last_updated = Utc::now().to_rfc3339();
    }
}
