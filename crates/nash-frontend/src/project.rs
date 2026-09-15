//! Owned project contracts shared by loaders, the driver and editor integrations.

use crate::{ModuleName, ModuleRole, SourceInput};
use nash_config::PackageName;
use nash_region::Region;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use url::Url;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PackageSourceId {
    Local(PathBuf),
    Github,
    Gitlab,
    Bitbucket,
    Compiler,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackageId {
    pub name: Option<String>,
    pub version: String,
    pub source: PackageSourceId,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModuleKey {
    pub package: PackageId,
    pub module: ModuleName,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceOrigin {
    Root,
    Dependency,
    Compiler,
    Synthetic,
}

#[derive(Clone, Debug)]
pub struct SourceSpec {
    pub key: ModuleKey,
    pub source_root: PathBuf,
    pub role: Option<ModuleRole>,
    pub frontend: Option<String>,
    pub origin: SourceOrigin,
    pub visible_packages: Option<Vec<PackageId>>,
    /// Independently checked workspace member; restricts provider lookup, not package ownership.
    pub resolution_root: Option<PathBuf>,
    pub import_aliases: BTreeMap<ModuleName, ModuleName>,
    pub synthetic_source: Option<String>,
    pub project: Option<std::sync::Arc<ProjectMetadata>>,
}

pub type ModuleCatalog = BTreeMap<Url, SourceSpec>;

impl SourceSpec {
    pub fn new(
        uri: &Url,
        source_root: &Path,
        package: Option<PackageName>,
    ) -> Result<Self, ProjectDiagnostic> {
        let path = uri.to_file_path().map_err(|()| {
            ProjectDiagnostic::new("NAP4001", source_root, format!("invalid file URI: {uri}"))
        })?;
        let source_root = normalize_path(source_root);
        let path = normalize_path(&path);
        let relative = path.strip_prefix(&source_root).map_err(|_| {
            ProjectDiagnostic::new("NAP4001", &path, "source is outside its declared root")
        })?;
        let module_path = relative.with_extension("");
        let parts = module_path
            .components()
            .map(|component| component.as_os_str().to_str())
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| ProjectDiagnostic::new("NAP4001", &path, "module path is not UTF-8"))?;
        if parts.is_empty()
            || parts
                .iter()
                .any(|part| part.is_empty() || part.contains('.'))
        {
            return Err(ProjectDiagnostic::new(
                "NAP4001",
                &path,
                "invalid module path",
            ));
        }
        Ok(Self {
            key: ModuleKey {
                package: PackageId {
                    name: package.map(|name| name.to_string()),
                    version: String::new(),
                    source: PackageSourceId::Local(source_root.clone()),
                },
                module: ModuleName::new(parts.join(".")),
            },
            source_root,
            role: None,
            frontend: None,
            origin: SourceOrigin::Root,
            visible_packages: None,
            resolution_root: None,
            import_aliases: BTreeMap::new(),
            synthetic_source: None,
            project: None,
        })
    }

    pub fn standalone(uri: &Url) -> Result<Self, ProjectDiagnostic> {
        let path = uri.to_file_path().map_err(|()| {
            ProjectDiagnostic::new(
                "NAP4001",
                Path::new("."),
                format!("invalid file URI: {uri}"),
            )
        })?;
        let root = path.parent().ok_or_else(|| {
            ProjectDiagnostic::new("NAP4001", &path, "source has no parent directory")
        })?;
        Self::new(uri, root, None)
    }

    pub fn input<'source, 'context>(
        &'context self,
        uri: &'context Url,
        source: &'source str,
    ) -> SourceInput<'source, 'context> {
        SourceInput {
            source,
            uri,
            expected_module: &self.key.module,
            role: self.role,
            origin: self.origin,
        }
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            component => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

#[derive(Clone, Debug)]
pub struct ResolvedDependency {
    pub requested: ModuleName,
    pub provider: ModuleKey,
    pub uri: Url,
    pub region: Region,
}

#[derive(Clone, Debug)]
pub struct RepositoryMetadata {
    pub user: String,
    pub project: String,
    pub platform: String,
}

#[derive(Clone, Debug)]
pub struct ProjectMetadata {
    pub name: Option<String>,
    pub version: String,
    pub license: Option<String>,
    pub description: String,
    pub repository: Option<RepositoryMetadata>,
    pub compiler: String,
    pub plutus: String,
}

#[derive(Clone, Debug)]
pub struct LoadedPackage {
    pub id: PackageId,
    pub root: PathBuf,
    pub metadata: ProjectMetadata,
    pub dependencies: Vec<PackageId>,
}

#[derive(Clone, Debug)]
pub struct LoadedProject {
    pub root: PathBuf,
    pub packages: Vec<LoadedPackage>,
    pub catalog: ModuleCatalog,
    pub warnings: Vec<ProjectDiagnostic>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectMode {
    Check,
    Build,
    Editor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectFormat {
    Nash,
    Aiken,
}

pub struct ProjectLoadRequest<'a> {
    pub location: &'a Path,
    pub environment: Option<&'a str>,
    pub mode: ProjectMode,
}

#[derive(Clone, Debug, thiserror::Error)]
#[error("{code}: {message} ({path})")]
pub struct ProjectDiagnostic {
    pub code: &'static str,
    pub path: PathBuf,
    pub message: String,
    pub region: Option<Region>,
}

impl ProjectDiagnostic {
    pub fn new(code: &'static str, path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self {
            code,
            path: path.into(),
            message: message.into(),
            region: None,
        }
    }
}
