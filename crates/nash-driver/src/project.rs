//! Project loading and discovery.
//!
//! Handles loading `nash.jsonc` configuration files and discovering
//! source files within projects.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use url::Url;

use nash_config::{Config, Workspace};
use nash_frontend::{ModuleName, ModuleRole, SourceInput};

use crate::database::Database;
use crate::error::DriverError;
use crate::source::path_to_uri;

/// Discovered sources with the identity and ownership used by every compiler phase.
pub type ModuleCatalog = BTreeMap<Url, SourceSpec>;

#[derive(Clone, Debug)]
pub struct SourceSpec {
    pub source_root: PathBuf,
    pub module: ModuleName,
    pub role: Option<ModuleRole>,
    pub package: Option<nash_config::PackageName>,
    pub frontend: Option<String>,
}

impl SourceSpec {
    /// Derive an extension-neutral identity relative to the configured source root.
    pub fn new(
        uri: &Url,
        source_root: &Path,
        package: Option<nash_config::PackageName>,
    ) -> Result<Self, DriverError> {
        let path = uri
            .to_file_path()
            .map_err(|()| DriverError::InvalidFileUri { uri: uri.clone() })?;
        let source_root = normalize_path(source_root);
        let path = normalize_path(&path);
        let relative = path
            .strip_prefix(&source_root)
            .map_err(|_| DriverError::InvalidModulePath { path: path.clone() })?;
        let module_path = relative.with_extension("");
        let parts = module_path
            .components()
            .map(|component| component.as_os_str().to_str())
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| DriverError::InvalidModulePath { path: path.clone() })?;
        if parts.is_empty()
            || parts
                .iter()
                .any(|part| part.is_empty() || part.contains('.'))
        {
            return Err(DriverError::InvalidModulePath { path });
        }
        Ok(Self {
            source_root,
            module: ModuleName::new(parts.join(".")),
            role: None,
            package,
            frontend: None,
        })
    }

    /// An editor buffer outside a project uses its containing directory as its root.
    pub fn standalone(uri: &Url) -> Result<Self, DriverError> {
        let path = uri
            .to_file_path()
            .map_err(|()| DriverError::InvalidFileUri { uri: uri.clone() })?;
        let root = path
            .parent()
            .ok_or_else(|| DriverError::InvalidModulePath { path: path.clone() })?;
        Self::new(uri, root, None)
    }

    pub(crate) fn input<'source, 'context>(
        &'context self,
        uri: &'context Url,
        source: &'source str,
    ) -> SourceInput<'source, 'context> {
        SourceInput {
            source,
            uri,
            expected_module: &self.module,
            role: self.role,
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

/// A loaded Nash project.
#[derive(Debug)]
pub struct Project {
    /// Root directory of the project.
    pub root: PathBuf,

    /// Parsed configuration.
    pub config: Config,

    /// Workspace members (for workspace configs).
    pub members: Vec<ProjectMember>,
}

/// A member of a workspace, or a standalone project.
#[derive(Debug)]
pub struct ProjectMember {
    /// Root directory of the member.
    pub root: PathBuf,

    /// Parsed configuration.
    pub config: Config,

    /// Resolved source directories.
    pub source_dirs: Vec<PathBuf>,
}

impl Project {
    /// Load a project from a directory.
    ///
    /// Searches for `nash.jsonc` in the given directory and parent directories.
    pub async fn load(path: impl AsRef<Path>) -> Result<Self, DriverError> {
        let path = path.as_ref();

        // Find project root (directory containing nash.jsonc)
        let root = find_project_root(path)?;
        let root = root
            .canonicalize()
            .map_err(|source| DriverError::ReadError {
                path: root.clone(),
                source,
            })?;
        let config_path = root.join("nash.jsonc");

        // Parse the config
        let config = nash_config::parse_file(&config_path)?;

        // Load members if this is a workspace
        let members = match &config {
            Config::Workspace(ws) => load_workspace_members(&root, ws).await?,
            Config::Application(app) => vec![make_member(&root, Config::Application(app.clone()))],
            Config::Package(pkg) => vec![make_member(&root, Config::Package(pkg.clone()))],
        };

        Ok(Project {
            root,
            config,
            members,
        })
    }

    /// Discover all registered source languages while retaining package ownership.
    pub async fn discover_modules(&self, db: &Database) -> Result<ModuleCatalog, DriverError> {
        let mut modules = ModuleCatalog::new();

        for member in &self.members {
            let package = match &member.config {
                Config::Package(package) => Some(package.name.clone()),
                _ => None,
            };
            for source_dir in &member.source_dirs {
                let source_dir = normalize_path(source_dir);
                let base_uri = path_to_uri(&source_dir)?;
                for extension in crate::FRONTENDS.extensions() {
                    for uri in db.glob(&base_uri, &format!("**/*.{extension}")).await? {
                        let spec = SourceSpec::new(&uri, &source_dir, package.clone())?;
                        if let Some(previous) = modules.get(&uri) {
                            if previous.package != package {
                                return Err(DriverError::ConflictingModuleOwners {
                                    uri: Box::new(uri),
                                    first: previous.package.as_ref().map_or_else(
                                        || "application".to_owned(),
                                        ToString::to_string,
                                    ),
                                    second: member.name(),
                                });
                            }
                            if previous.module != spec.module {
                                return Err(DriverError::ConflictingModuleRoots {
                                    uri: Box::new(uri),
                                    first: previous.source_root.clone(),
                                    second: source_dir.clone(),
                                });
                            }
                        } else {
                            modules.insert(uri, spec);
                        }
                    }
                }
            }
        }

        Ok(modules)
    }

    /// Get source directories from config.
    pub fn source_directories(&self) -> Vec<PathBuf> {
        self.members
            .iter()
            .flat_map(|m| m.source_dirs.clone())
            .collect()
    }
}

/// Find the project root by searching for nash.jsonc.
fn find_project_root(start: &Path) -> Result<PathBuf, DriverError> {
    let start = if start.is_file() {
        start.parent().unwrap_or(start)
    } else {
        start
    };

    let mut current = start.to_path_buf();

    loop {
        let config_path = current.join("nash.jsonc");
        if config_path.exists() {
            return Ok(current);
        }

        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => {
                return Err(DriverError::ProjectNotFound {
                    path: start.to_path_buf(),
                });
            }
        }
    }
}

/// Load all workspace members.
async fn load_workspace_members(
    workspace_root: &Path,
    workspace: &Workspace,
) -> Result<Vec<ProjectMember>, DriverError> {
    let mut members = Vec::new();

    for pattern in &workspace.members {
        let full_pattern = workspace_root.join(pattern);
        let pattern_str = full_pattern.to_string_lossy();

        let matches: Vec<_> = glob::glob(&pattern_str)
            .map_err(|e| DriverError::InvalidModulePath {
                path: PathBuf::from(e.msg),
            })?
            .filter_map(|r| r.ok())
            .collect();

        if matches.is_empty() {
            return Err(DriverError::MemberNotFound {
                pattern: pattern.clone(),
            });
        }

        for member_path in matches {
            // member_path is the glob match - we need to find nash.jsonc
            let member_root = if member_path.is_file() {
                member_path.parent().unwrap().to_path_buf()
            } else {
                member_path
            };

            let config_path = member_root.join("nash.jsonc");
            if !config_path.exists() {
                continue;
            }

            let config = nash_config::parse_file(&config_path)?;
            members.push(make_member(&member_root, config));
        }
    }

    Ok(members)
}

/// Create a ProjectMember from config.
fn make_member(root: &Path, config: Config) -> ProjectMember {
    let source_dirs = match &config {
        Config::Application(app) => resolve_source_dirs(root, &app.source_directories),
        Config::Package(_) => vec![root.join("src")],
        Config::Workspace(_) => vec![], // Workspaces don't have source dirs directly
    };

    ProjectMember {
        root: root.to_path_buf(),
        config,
        source_dirs,
    }
}

/// Resolve source directory paths relative to project root.
fn resolve_source_dirs(root: &Path, dirs: &[String]) -> Vec<PathBuf> {
    dirs.iter().map(|d| root.join(d)).collect()
}

impl ProjectMember {
    /// Get the project name (for packages) or a generated name (for applications).
    pub fn name(&self) -> String {
        match &self.config {
            Config::Package(pkg) => pkg.name.to_string(),
            Config::Application(_) => "application".to_string(),
            Config::Workspace(_) => "workspace".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InMemorySource, ModuleResult, build, build_graph};
    use std::sync::Arc;
    use tokio::sync::Mutex;

    #[tokio::test]
    async fn workspace_literal_defaults_use_discovered_package_ownership() {
        let core = nash_config::parse(
            r#"{
            "type": "package", "name": "nash/core", "version": "1.0.0",
            "summary": "Core", "license": "MIT", "exposedModules": ["Literal"]
        }"#,
            "/work/core/nash.jsonc",
        )
        .unwrap();
        let app = nash_config::parse(r#"{"type":"application"}"#, "/work/app/nash.jsonc").unwrap();
        let mut project = Project {
            root: PathBuf::from("/work"),
            config: nash_config::parse(
                r#"{"type":"workspace","members":["core","app"]}"#,
                "/work/nash.jsonc",
            )
            .unwrap(),
            members: vec![
                make_member(Path::new("/work/core"), core.clone()),
                make_member(Path::new("/work/app"), app),
            ],
        };
        let literal = Url::parse("file:///work/core/src/Literal.nash").unwrap();
        let main = Url::parse("file:///work/app/src/Main.nash").unwrap();
        let mem = InMemorySource::new();
        mem.insert(
            literal.clone(),
            indoc::indoc!(
                r#"
            module Literal exposing (..)
            import Builtin exposing (..)
            trait FromInt 'a where
                fromInt : int -> 'a
            impl FromInt int where
                fromInt x = x
        "#
            )
            .to_owned(),
        );
        mem.insert(
            main.clone(),
            indoc::indoc!(
                r#"
            module Main exposing (..)
            import Builtin exposing (..)
            import Literal exposing (fromInt)
            trait Drop 'a where
                drop : 'a -> ()
            impl Drop int where
                drop x = ()
            value n = drop (fromInt n)
        "#
            )
            .to_owned(),
        );
        let db = Arc::new(Mutex::new(Database::new(mem)));
        // Repeated workspace membership must not duplicate modules.
        project
            .members
            .push(make_member(Path::new("/work/core"), core));
        let mut modules = project.discover_modules(&*db.lock().await).await.unwrap();
        assert_eq!(modules.len(), 2);
        assert_eq!(
            modules[&literal].package.as_ref().unwrap().to_string(),
            "nash/core"
        );
        assert_eq!(modules[&main].package, None);
        let graph = build_graph(db.clone(), &modules).await.unwrap();
        assert_eq!(graph.order, [literal.clone(), main.clone()]);
        let result = build(db.clone(), &graph, &modules).await;
        assert!(result.is_success(), "{result:?}");
        modules.get_mut(&literal).unwrap().package = Some("example/literals".parse().unwrap());
        let result = build(db.clone(), &graph, &modules).await;
        assert!(
            matches!(&result.modules[&main], ModuleResult::Failed(reports) if reports.reports.iter().any(|report| report.title == "AMBIGUOUS TYPE"))
        );
        // An application can name a source directory outside its own root.
        let overlap = nash_config::parse(
            r#"{"type":"application","sourceDirectories":["/work/core/src"]}"#,
            "/work/app/nash.jsonc",
        )
        .unwrap();
        project
            .members
            .push(make_member(Path::new("/work/app"), overlap));
        assert!(matches!(project.discover_modules(&*db.lock().await).await,
            Err(DriverError::ConflictingModuleOwners { uri, .. }) if *uri == literal));
    }
}
