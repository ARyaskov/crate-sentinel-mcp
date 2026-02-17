use semver::Version;

use crate::crates_io::client::CratesIoClient;
use crate::error::AppResult;
use crate::policy::config::MsrvPolicy;

#[derive(Debug, Clone)]
pub struct MsrvDecision {
    pub required: Option<String>,
    pub ok: bool,
    pub disallowed_reason: Option<String>,
}

pub async fn evaluate_msrv(
    client: &CratesIoClient,
    crate_name: &str,
    latest_version: &str,
    policy: &MsrvPolicy,
) -> AppResult<MsrvDecision> {
    if !policy.enforce {
        return Ok(MsrvDecision {
            required: None,
            ok: true,
            disallowed_reason: None,
        });
    }

    let required = client
        .fetch_rust_version(crate_name, latest_version)
        .await?;
    let Some(required_value) = required else {
        return Ok(MsrvDecision {
            required: None,
            ok: false,
            disallowed_reason: Some("msrv_unknown".to_string()),
        });
    };
    let Some(max_allowed) = policy.max_allowed.clone() else {
        return Ok(MsrvDecision {
            required: Some(required_value),
            ok: true,
            disallowed_reason: None,
        });
    };

    let ok = compare_msrv(&required_value, &max_allowed);
    Ok(MsrvDecision {
        required: Some(required_value),
        ok,
        disallowed_reason: if ok {
            None
        } else {
            Some("msrv_exceeds_policy".to_string())
        },
    })
}

fn compare_msrv(required: &str, max_allowed: &str) -> bool {
    let required_version = parse_version(required);
    let max_version = parse_version(max_allowed);
    match (required_version, max_version) {
        (Some(a), Some(b)) => a <= b,
        _ => true,
    }
}

fn parse_version(raw: &str) -> Option<Version> {
    Version::parse(raw.trim_start_matches('v')).ok()
}
