use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use cargo_metadata::{DependencyKind, MetadataCommand, PackageId};

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct DependencyScanResult {
    pub workspace_root: String,
    pub crates: Vec<ExternalCrate>,
}

#[derive(Debug, Clone)]
pub struct ExternalCrate {
    pub name: String,
    pub version: String,
    pub kind: String,
    pub source: String,
}

pub fn scan_workspace(workspace_path: &Path) -> AppResult<DependencyScanResult> {
    let manifest_path = resolve_manifest_path(workspace_path)?;
    let metadata = MetadataCommand::new()
        .manifest_path(&manifest_path)
        .exec()?;

    let kind_by_package = dependency_kind_by_package(&metadata);
    let mut unique = BTreeMap::<(String, String), (u8, ExternalCrate)>::new();

    for package in &metadata.packages {
        if !is_registry_source(package.source.as_ref().map(|source| source.repr.as_str())) {
            continue;
        }

        let kind_rank = *kind_by_package.get(&package.id).unwrap_or(&3_u8);
        let key = (package.name.to_string(), package.version.to_string());
        let entry = ExternalCrate {
            name: package.name.to_string(),
            version: package.version.to_string(),
            kind: dependency_kind_label(kind_rank).to_string(),
            source: "registry".to_string(),
        };

        match unique.get_mut(&key) {
            Some((existing_rank, existing_entry)) if kind_rank > *existing_rank => {
                *existing_rank = kind_rank;
                *existing_entry = entry;
            }
            Some(_) => {}
            None => {
                unique.insert(key, (kind_rank, entry));
            }
        }
    }

    let crates = unique.into_values().map(|(_, entry)| entry).collect();
    Ok(DependencyScanResult {
        workspace_root: metadata.workspace_root.to_string(),
        crates,
    })
}

fn resolve_manifest_path(workspace_path: &Path) -> AppResult<PathBuf> {
    if !workspace_path.exists() {
        return Err(AppError::InvalidInput(format!(
            "workspace path does not exist: {}",
            workspace_path.display()
        )));
    }

    let manifest_path = if workspace_path.is_dir() {
        workspace_path.join("Cargo.toml")
    } else if workspace_path.is_file()
        && workspace_path
            .file_name()
            .is_some_and(|name| name == "Cargo.toml")
    {
        workspace_path.to_path_buf()
    } else {
        return Err(AppError::InvalidInput(format!(
            "workspace path must be a directory or Cargo.toml file: {}",
            workspace_path.display()
        )));
    };

    if !manifest_path.exists() {
        return Err(AppError::InvalidInput(format!(
            "Cargo.toml not found at: {}",
            manifest_path.display()
        )));
    }

    Ok(manifest_path)
}

fn dependency_kind_by_package(metadata: &cargo_metadata::Metadata) -> BTreeMap<PackageId, u8> {
    let mut kind_map = BTreeMap::new();

    if let Some(resolve) = &metadata.resolve {
        for node in &resolve.nodes {
            for dep in &node.deps {
                for dep_kind in &dep.dep_kinds {
                    let rank = dependency_kind_rank(dep_kind.kind);
                    let existing = kind_map.entry(dep.pkg.clone()).or_insert(rank);
                    if rank > *existing {
                        *existing = rank;
                    }
                }
            }
        }
    }

    kind_map
}

fn is_registry_source(source: Option<&str>) -> bool {
    source.is_some_and(|value| value.starts_with("registry+"))
}

fn dependency_kind_rank(kind: DependencyKind) -> u8 {
    match kind {
        DependencyKind::Normal => 3,
        DependencyKind::Build => 2,
        DependencyKind::Development => 1,
        DependencyKind::Unknown => 3,
    }
}

fn dependency_kind_label(rank: u8) -> &'static str {
    match rank {
        2 => "build",
        1 => "dev",
        _ => "normal",
    }
}
