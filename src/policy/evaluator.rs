use crate::policy::config::UpgradePolicy;

pub fn should_include_dependency(policy: &UpgradePolicy, kind: &str) -> bool {
    if kind == "dev" {
        return policy.general.include_dev_dependencies;
    }
    true
}

pub fn is_update_allowed(policy: &UpgradePolicy, crate_name: &str, update_type: &str) -> bool {
    if update_type == "none" || update_type == "unknown" {
        return false;
    }

    let crate_policy = policy.crates.get(crate_name);
    match update_type {
        "patch" => crate_policy
            .and_then(|value| value.allow_patch)
            .unwrap_or(policy.general.allow_patch),
        "minor" => crate_policy
            .and_then(|value| value.allow_minor)
            .unwrap_or(policy.general.allow_minor),
        "major" => crate_policy
            .and_then(|value| value.allow_major)
            .unwrap_or(policy.general.allow_major),
        _ => false,
    }
}
