pub mod report;

use std::sync::atomic::{AtomicBool, Ordering};

static CI_CLI_MODE: AtomicBool = AtomicBool::new(false);

pub fn set_ci_cli_mode(enabled: bool) {
    CI_CLI_MODE.store(enabled, Ordering::Relaxed);
}

pub fn ci_cli_mode() -> bool {
    CI_CLI_MODE.load(Ordering::Relaxed)
}

pub fn error_exit_code(error: &str) -> i32 {
    if error.contains("policy") || error.contains("disallowed") || error.contains("msrv") {
        return 2;
    }
    if error.contains("invalid input") || error.contains("failed") || error.contains("validation") {
        return 1;
    }
    3
}
