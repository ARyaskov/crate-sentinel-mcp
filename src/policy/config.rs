use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::error::AppResult;

const POLICY_FILE_NAME: &str = "upgrade_policy.toml";

#[derive(Debug, Clone)]
pub struct UpgradePolicy {
    pub general: GeneralPolicy,
    pub crates: HashMap<String, CratePolicyOverride>,
    pub msrv: MsrvPolicy,
    pub performance: PerformancePolicy,
    pub ci: CiPolicy,
}

#[derive(Debug, Clone)]
pub struct GeneralPolicy {
    pub allow_patch: bool,
    pub allow_minor: bool,
    pub allow_major: bool,
    pub include_dev_dependencies: bool,
}

#[derive(Debug, Clone)]
pub struct CratePolicyOverride {
    pub allow_patch: Option<bool>,
    pub allow_minor: Option<bool>,
    pub allow_major: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct MsrvPolicy {
    pub enforce: bool,
    pub max_allowed: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PerformancePolicy {
    pub enforce: bool,
    pub command: Option<String>,
    pub max_slowdown_percent: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct CiPolicy {
    pub mode: bool,
    pub fail_on_any_disallowed_update: bool,
}

impl Default for UpgradePolicy {
    fn default() -> Self {
        Self {
            general: GeneralPolicy {
                allow_patch: true,
                allow_minor: true,
                allow_major: false,
                include_dev_dependencies: false,
            },
            crates: HashMap::new(),
            msrv: MsrvPolicy {
                enforce: false,
                max_allowed: None,
            },
            performance: PerformancePolicy {
                enforce: false,
                command: None,
                max_slowdown_percent: None,
            },
            ci: CiPolicy {
                mode: false,
                fail_on_any_disallowed_update: false,
            },
        }
    }
}

pub fn load_policy(workspace_root: &Path) -> AppResult<(UpgradePolicy, bool)> {
    let policy_path = workspace_root.join(POLICY_FILE_NAME);
    if !policy_path.exists() {
        return Ok((UpgradePolicy::default(), false));
    }

    let raw = fs::read_to_string(policy_path)?;
    let parsed: UpgradePolicyFile = toml::from_str(&raw)?;

    let default = UpgradePolicy::default();
    let general_file = parsed.general.unwrap_or_default();
    let general = GeneralPolicy {
        allow_patch: general_file
            .allow_patch
            .unwrap_or(default.general.allow_patch),
        allow_minor: general_file
            .allow_minor
            .unwrap_or(default.general.allow_minor),
        allow_major: general_file
            .allow_major
            .unwrap_or(default.general.allow_major),
        include_dev_dependencies: general_file
            .include_dev_dependencies
            .unwrap_or(default.general.include_dev_dependencies),
    };

    let crates = parsed
        .crates
        .unwrap_or_default()
        .into_iter()
        .map(|(name, override_file)| {
            (
                name,
                CratePolicyOverride {
                    allow_patch: override_file.allow_patch,
                    allow_minor: override_file.allow_minor,
                    allow_major: override_file.allow_major,
                },
            )
        })
        .collect();

    let msrv_file = parsed.msrv.unwrap_or_default();
    let msrv = MsrvPolicy {
        enforce: msrv_file.enforce.unwrap_or(default.msrv.enforce),
        max_allowed: msrv_file.max_allowed.or(default.msrv.max_allowed),
    };

    let perf_file = parsed.performance.unwrap_or_default();
    let performance = PerformancePolicy {
        enforce: perf_file.enforce.unwrap_or(default.performance.enforce),
        command: perf_file.command.or(default.performance.command),
        max_slowdown_percent: perf_file
            .max_slowdown_percent
            .or(default.performance.max_slowdown_percent),
    };

    let ci_file = parsed.ci.unwrap_or_default();
    let ci = CiPolicy {
        mode: ci_file.mode.unwrap_or(default.ci.mode),
        fail_on_any_disallowed_update: ci_file
            .fail_on_any_disallowed_update
            .unwrap_or(default.ci.fail_on_any_disallowed_update),
    };

    Ok((
        UpgradePolicy {
            general,
            crates,
            msrv,
            performance,
            ci,
        },
        true,
    ))
}

#[derive(Debug, Deserialize)]
struct UpgradePolicyFile {
    general: Option<GeneralPolicyFile>,
    crates: Option<HashMap<String, CratePolicyOverrideFile>>,
    msrv: Option<MsrvPolicyFile>,
    performance: Option<PerformancePolicyFile>,
    ci: Option<CiPolicyFile>,
}

#[derive(Debug, Default, Deserialize)]
struct GeneralPolicyFile {
    allow_patch: Option<bool>,
    allow_minor: Option<bool>,
    allow_major: Option<bool>,
    include_dev_dependencies: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct CratePolicyOverrideFile {
    allow_patch: Option<bool>,
    allow_minor: Option<bool>,
    allow_major: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
struct MsrvPolicyFile {
    enforce: Option<bool>,
    max_allowed: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct PerformancePolicyFile {
    enforce: Option<bool>,
    command: Option<String>,
    max_slowdown_percent: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct CiPolicyFile {
    mode: Option<bool>,
    fail_on_any_disallowed_update: Option<bool>,
}
