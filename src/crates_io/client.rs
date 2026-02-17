use reqwest::Client;
use semver::Version;
use serde::Deserialize;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use tempfile::tempdir;

use crate::error::AppResult;

const CRATES_IO_BASE: &str = "https://crates.io/api/v1/crates";
static MOCK_REGISTRY: AtomicBool = AtomicBool::new(false);

#[allow(dead_code)]
pub fn set_mock_registry(enabled: bool) {
    MOCK_REGISTRY.store(enabled, Ordering::Relaxed);
}

#[derive(Debug, Clone)]
pub struct CrateReleaseInfo {
    pub latest_version: String,
    pub yanked: bool,
    pub release_date: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CratesIoClient {
    http: Client,
}

impl CratesIoClient {
    pub fn new() -> AppResult<Self> {
        let http = Client::builder()
            .user_agent("crate-sentinel-mcp/0.1.0")
            .build()?;
        Ok(Self { http })
    }

    pub async fn fetch_release_info(
        &self,
        crate_name: &str,
        current_is_prerelease: bool,
    ) -> AppResult<CrateReleaseInfo> {
        if MOCK_REGISTRY.load(Ordering::Relaxed) {
            return Ok(mock_release_info(crate_name, current_is_prerelease));
        }
        let url = format!("{CRATES_IO_BASE}/{crate_name}");
        let payload = self.http.get(url).send().await?.error_for_status()?;
        let response: CrateResponse = payload.json().await?;

        let latest_version = select_latest_version(&response, current_is_prerelease);
        let release_meta = response
            .versions
            .iter()
            .find(|entry| entry.num == latest_version);

        Ok(CrateReleaseInfo {
            latest_version,
            yanked: release_meta.is_some_and(|entry| entry.yanked),
            release_date: release_meta.map(|entry| entry.created_at.clone()),
        })
    }

    pub async fn fetch_rust_version(
        &self,
        crate_name: &str,
        version: &str,
    ) -> AppResult<Option<String>> {
        if MOCK_REGISTRY.load(Ordering::Relaxed) {
            let _ = crate_name;
            let _ = version;
            return Ok(Some("1.60".to_string()));
        }
        let url = format!("{CRATES_IO_BASE}/{crate_name}");
        let payload = self.http.get(url).send().await?.error_for_status()?;
        let response: CrateResponse = payload.json().await?;
        if let Some(found) = response
            .versions
            .iter()
            .find(|entry| entry.num == version)
            .and_then(|entry| entry.rust_version.clone())
        {
            return Ok(Some(found));
        }
        fetch_rust_version_from_crate_toml(crate_name, version)
    }
}

fn mock_release_info(crate_name: &str, current_is_prerelease: bool) -> CrateReleaseInfo {
    let latest_version = if current_is_prerelease {
        "9.9.9-alpha.1"
    } else if crate_name == "serde" {
        "1.0.200"
    } else {
        "0.0.2"
    };
    CrateReleaseInfo {
        latest_version: latest_version.to_string(),
        yanked: false,
        release_date: Some("2024-01-01T00:00:00Z".to_string()),
    }
}

fn select_latest_version(response: &CrateResponse, current_is_prerelease: bool) -> String {
    if !current_is_prerelease {
        return response.krate.max_stable_version.clone();
    }

    response
        .versions
        .iter()
        .filter_map(|entry| {
            Version::parse(&entry.num)
                .ok()
                .map(|version| (version, &entry.num))
        })
        .max_by(|left, right| left.0.cmp(&right.0))
        .map(|(_, version)| version.clone())
        .unwrap_or_else(|| response.krate.max_stable_version.clone())
}

#[derive(Debug, Deserialize)]
struct CrateResponse {
    #[serde(rename = "crate")]
    krate: CrateDetails,
    versions: Vec<VersionDetails>,
}

#[derive(Debug, Deserialize)]
struct CrateDetails {
    max_stable_version: String,
}

#[derive(Debug, Deserialize)]
struct VersionDetails {
    num: String,
    yanked: bool,
    created_at: String,
    rust_version: Option<String>,
}

fn fetch_rust_version_from_crate_toml(
    crate_name: &str,
    version: &str,
) -> AppResult<Option<String>> {
    let temp = tempdir()?;
    let out_dir = temp.path().join("crate");
    fs::create_dir_all(&out_dir)?;
    let output = Command::new("cargo")
        .args([
            "download",
            crate_name,
            "--version",
            version,
            "--output",
            out_dir.to_string_lossy().as_ref(),
            "--extract",
        ])
        .output()?;
    if !output.status.success() {
        return Ok(None);
    }
    let manifest = find_manifest(&out_dir)?;
    let raw = fs::read_to_string(manifest)?;
    let parsed: toml::Value = toml::from_str(&raw)?;
    let rust_version = parsed
        .get("package")
        .and_then(|value| value.get("rust-version"))
        .and_then(|value| value.as_str())
        .map(ToString::to_string);
    Ok(rust_version)
}

fn find_manifest(root: &Path) -> AppResult<String> {
    let direct = root.join("Cargo.toml");
    if direct.exists() {
        return Ok(direct.to_string_lossy().to_string());
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let candidate = entry.path().join("Cargo.toml");
            if candidate.exists() {
                return Ok(candidate.to_string_lossy().to_string());
            }
        }
    }
    Ok(direct.to_string_lossy().to_string())
}
