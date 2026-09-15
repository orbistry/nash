use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use aiken_project::config::{Platform, PlutusVersion, ProjectConfig};
use nash_frontend::{
    PackageId, PackageSourceId, ProjectDiagnostic, ProjectMetadata, RepositoryMetadata,
};
use nash_region::{Position, Region};
use serde::{Deserialize, de::DeserializeOwned};

pub(crate) fn root(location: &Path) -> Result<PathBuf, ProjectDiagnostic> {
    let location = fs::canonicalize(location).map_err(|e| io(location, e))?;
    let explicit = location
        .file_name()
        .is_some_and(|name| name == "aiken.toml");
    let start = if location.is_file() {
        location.parent().ok_or_else(|| {
            ProjectDiagnostic::new("NAP4101", &location, "manifest has no parent directory")
        })?
    } else {
        &location
    };
    for dir in start.ancestors() {
        if dir.join("aiken.toml").is_file() {
            if !explicit && dir.join("nash.jsonc").is_file() {
                return Err(ProjectDiagnostic::new(
                    "NAP4101",
                    dir,
                    "both aiken.toml and nash.jsonc exist; select a manifest explicitly",
                ));
            }
            return Ok(dir.to_path_buf());
        }
    }
    Err(ProjectDiagnostic::new(
        "NAP4101",
        location,
        "no owning aiken.toml manifest was found",
    ))
}

#[derive(Deserialize)]
struct WorkspaceConfig {
    members: Vec<String>,
}

pub(crate) fn members(root: &Path) -> Result<Vec<PathBuf>, ProjectDiagnostic> {
    let path = root.join("aiken.toml");
    let value: toml::Value = read_toml(&path)?;
    if value.get("members").is_none() {
        return Ok(vec![root.to_path_buf()]);
    }
    let workspace: WorkspaceConfig = read_toml(&path)?;
    let mut expanded = Vec::new();
    for member in workspace.members {
        let pattern = root.join(member);
        let matches = match pattern
            .to_str()
            .and_then(|pattern| glob::glob(pattern).ok())
        {
            Some(entries) => entries.collect::<Result<Vec<_>, _>>().map_err(|error| {
                ProjectDiagnostic::new(
                    "NAP4102",
                    error.path(),
                    format!("cannot expand workspace member: {error}"),
                )
            })?,
            None => Vec::new(),
        };
        // Pinned expand_members treats invalid or unmatched globs as literal paths.
        if matches.is_empty() {
            expanded.push(pattern);
        } else {
            expanded.extend(matches);
        }
    }
    let mut members = BTreeSet::new();
    for member in expanded {
        let member = fs::canonicalize(&member).map_err(|error| {
            ProjectDiagnostic::new(
                "NAP4102",
                &member,
                format!("cannot read workspace member: {error}"),
            )
        })?;
        if !member.is_dir() || !member.join("aiken.toml").is_file() {
            return Err(ProjectDiagnostic::new(
                "NAP4102",
                member,
                "workspace member must be a directory containing aiken.toml",
            ));
        }
        if !members.insert(member.clone()) {
            return Err(ProjectDiagnostic::new(
                "NAP4102",
                member,
                "duplicate workspace member",
            ));
        }
        let config: toml::Value = read_toml(&member.join("aiken.toml"))?;
        if config.get("members").is_some() {
            return Err(ProjectDiagnostic::new(
                "NAP4102",
                member,
                "a workspace member cannot itself be a workspace",
            ));
        }
    }
    let members: Vec<PathBuf> = members.into_iter().collect();
    for (index, member) in members.iter().enumerate() {
        if let Some(nested) = members[index + 1..]
            .iter()
            .find(|other| other.starts_with(member))
        {
            return Err(ProjectDiagnostic::new(
                "NAP4102",
                nested,
                format!("workspace member is nested inside {}", member.display()),
            ));
        }
    }
    Ok(members)
}

pub(crate) fn config(root: &Path) -> Result<ProjectConfig, ProjectDiagnostic> {
    let path = root.join("aiken.toml");
    let config: ProjectConfig = read_toml(&path)?;
    validate_name(&config.name.to_string(), &path)?;
    for dependency in &config.dependencies {
        validate_name(&dependency.name.to_string(), &path)?;
    }
    Ok(config)
}

pub(crate) fn validate_name(name: &str, path: &Path) -> Result<(), ProjectDiagnostic> {
    let parts: Vec<_> = name.split('/').collect();
    if parts.len() != 2
        || parts.iter().any(|part| {
            part.is_empty()
                || !part.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'_' | b'-')
                })
        })
    {
        return Err(ProjectDiagnostic::new(
            "NAP4103",
            path,
            format!("invalid package name {name:?}; expected lowercase owner/repository"),
        ));
    }
    Ok(())
}

pub(crate) fn metadata(config: &ProjectConfig) -> ProjectMetadata {
    ProjectMetadata {
        name: Some(config.name.to_string()),
        version: config.version.clone(),
        compiler: format!("v{}", config.compiler),
        plutus: match config.plutus {
            PlutusVersion::V1 => "v1",
            PlutusVersion::V2 => "v2",
            PlutusVersion::V3 => "v3",
        }
        .to_string(),
        license: config.license.clone(),
        description: config.description.clone(),
        repository: config
            .repository
            .as_ref()
            .map(|repository| RepositoryMetadata {
                user: repository.user.clone(),
                project: repository.project.clone(),
                platform: repository.platform.to_string(),
            }),
    }
}

pub(crate) fn requirements(config: &ProjectConfig) -> Vec<PackageId> {
    config
        .dependencies
        .iter()
        .map(|dependency| PackageId {
            name: Some(dependency.name.to_string()),
            version: dependency.version.clone(),
            source: source(dependency.source),
        })
        .collect()
}

pub(crate) fn source(platform: Platform) -> PackageSourceId {
    match platform {
        Platform::Github => PackageSourceId::Github,
        Platform::Gitlab => PackageSourceId::Gitlab,
        Platform::Bitbucket => PackageSourceId::Bitbucket,
    }
}

pub(crate) fn read_toml<T: DeserializeOwned>(path: &Path) -> Result<T, ProjectDiagnostic> {
    let source = fs::read_to_string(path).map_err(|e| io(path, e))?;
    toml::from_str(&source).map_err(|error| {
        let mut diagnostic = ProjectDiagnostic::new("NAP4103", path, error.message());
        diagnostic.region = error
            .span()
            .map(|span| region(&source, span.start, span.end));
        diagnostic
    })
}

pub(crate) fn io(path: &Path, error: std::io::Error) -> ProjectDiagnostic {
    ProjectDiagnostic::new("NAP4104", path, error.to_string())
}

fn region(source: &str, start: usize, end: usize) -> Region {
    fn position(source: &str, offset: usize) -> Position {
        let bytes = &source.as_bytes()[..offset.min(source.len())];
        let line = 1 + bytes.iter().filter(|byte| **byte == b'\n').count();
        let column = bytes
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(bytes.len() + 1, |newline| bytes.len() - newline);
        Position::new(line, column)
    }
    Region::new(position(source, start), position(source, end))
}
