//! Project loading and discovery.
//!
//! Handles loading `nash.jsonc` configuration files and discovering
//! source files within projects.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use url::Url;

use nash_config::{Config, Workspace};

use crate::database::Database;
use crate::error::DriverError;
use crate::source::path_to_uri;

/// Source URIs and their owning packages; applications have no package name.
pub type ModuleOrigins = BTreeMap<Url, Option<nash_config::PackageName>>;

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

    /// Discover all Nash source files in the project.
    pub async fn discover_own_modules(&self, db: &Database) -> Result<ModuleOrigins, DriverError> {
        let mut modules = ModuleOrigins::new();

        for member in &self.members {
            let package = match &member.config {
                Config::Package(package) => Some(package.name.clone()),
                _ => None,
            };
            for source_dir in &member.source_dirs {
                let base_uri = path_to_uri(source_dir)?;
                for uri in db.glob(&base_uri, "**/*.nash").await? {
                    if let Some(owner) = modules.get(&uri) {
                        if owner != &package {
                            return Err(DriverError::ConflictingModuleOwners {
                                uri: Box::new(uri),
                                first: owner
                                    .as_ref()
                                    .map_or_else(|| "application".to_owned(), ToString::to_string),
                                second: member.name(),
                            });
                        }
                    } else {
                        modules.insert(uri, package.clone());
                    }
                }
            }
        }

        Ok(modules)
    }

    /// Discover project sources and local dependencies, including root test dependencies.
    pub async fn discover_modules(&self, db: &Database) -> Result<ModuleOrigins, DriverError> {
        self.discover_with_dependencies(db, true).await
    }

    /// Discover production sources without test dependencies.
    pub async fn discover_modules_production(
        &self,
        db: &Database,
    ) -> Result<ModuleOrigins, DriverError> {
        self.discover_with_dependencies(db, false).await
    }

    async fn discover_with_dependencies(
        &self,
        db: &Database,
        tests: bool,
    ) -> Result<ModuleOrigins, DriverError> {
        let mut modules = self.discover_own_modules(db).await?;
        let mut queue: Vec<_> = self
            .members
            .iter()
            .map(|member| (member.root.clone(), member.config.clone(), tests))
            .collect();
        let mut seen = std::collections::BTreeSet::new();
        while let Some((root, config, include_tests)) = queue.pop() {
            if !seen.insert((root.clone(), include_tests)) {
                continue;
            }
            if matches!(&config, Config::Package(pkg) if pkg.name.to_string() == "nash/base") {
                return Err(DriverError::Dependency {
                    package: "nash/base".into(),
                    message: "this package name is reserved for the compiler-bundled Base".into(),
                });
            }
            let (normal, testing) = match &config {
                Config::Application(app) => (&app.dependencies, &app.test_dependencies),
                Config::Package(pkg) => (&pkg.dependencies, &pkg.test_dependencies),
                Config::Workspace(_) => continue,
            };
            for (name, dependency) in normal
                .iter()
                .chain(testing.iter().filter(|_| include_tests))
            {
                if name.to_string() == "nash/base" {
                    return Err(DriverError::Dependency {
                        package: name.to_string(),
                        message: "Base ships with the compiler; remove this explicit dependency"
                            .into(),
                    });
                }
                let (dependency, base) = match dependency {
                    nash_config::Dependency::Source(nash_config::DependencySource::Workspace(
                        _,
                    )) => {
                        let Config::Workspace(workspace) = &self.config else {
                            return Err(DriverError::Dependency {
                                package: name.to_string(),
                                message: "workspace dependency used outside a workspace".into(),
                            });
                        };
                        (
                            workspace.dependencies.get(name).ok_or_else(|| {
                                DriverError::Dependency {
                                    package: name.to_string(),
                                    message: "missing workspace dependency declaration".into(),
                                }
                            })?,
                            &self.root,
                        )
                    }
                    _ => (dependency, &root),
                };
                let member = if let nash_config::Dependency::Source(nash_config::DependencySource::Path(path)) = dependency {
                    let dep_root = base.join(&path.path).canonicalize().map_err(|source| DriverError::ReadError { path: base.join(&path.path), source })?;
                    let config = nash_config::parse_file(dep_root.join("nash.jsonc"))?;
                    make_member(&dep_root, config)
                } else if let Some(member) = self.members.iter().find(|member| matches!(&member.config, Config::Package(package) if &package.name == name)) {
                    make_member(&member.root, member.config.clone())
                } else {
                    return Err(DriverError::Dependency { package: name.to_string(), message: "registry and git resolution are not implemented; use a local path dependency or a workspace package".into() });
                };
                if !matches!(&member.config, Config::Package(package) if &package.name == name) {
                    return Err(DriverError::Dependency {
                        package: name.to_string(),
                        message: format!(
                            "{} must declare the requested package name",
                            member.root.display()
                        ),
                    });
                }
                for source_dir in &member.source_dirs {
                    for uri in db.glob(&path_to_uri(source_dir)?, "**/*.nash").await? {
                        let owner = Some(name.clone());
                        if let Some(previous) = modules.insert(uri.clone(), owner.clone())
                            && previous != owner
                        {
                            return Err(DriverError::ConflictingModuleOwners {
                                uri: Box::new(uri),
                                first: previous
                                    .map_or_else(|| "application".into(), |name| name.to_string()),
                                second: name.to_string(),
                            });
                        }
                    }
                }
                // A dependency contributes its production dependencies, never its test dependencies.
                queue.push((member.root, member.config, false));
            }
        }
        if tests {
            // Test packages are available only to block-local imports. Checking this
            // before canonicalization prevents the global interface map from making
            // a test dependency visible to ordinary production imports.
            let production = Box::pin(self.discover_with_dependencies(db, false)).await?;
            for uri in production.keys() {
                let Ok(source) = db.file_source().read(uri).await else {
                    // The compiler reports source I/O failures alongside independent modules.
                    continue;
                };
                let arena = bumpalo::Bump::new();
                let source = arena.alloc_str(&source);
                if let Ok(module) = nash_parse::Parser::new(&arena, source).module() {
                    for import in module.imports {
                        let suffix = format!("/{}.nash", import.import.value.replace('.', "/"));
                        let ordinary = production.keys().any(|uri| uri.path().ends_with(&suffix));
                        if !ordinary && modules.keys().any(|uri| uri.path().ends_with(&suffix)) {
                            return Err(DriverError::Dependency {
                                package: import.import.value.to_owned(),
                                message: format!(
                                    "{} imports a test-only dependency outside its tests block; move the import into tests or declare a production dependency",
                                    uri.path()
                                ),
                            });
                        }
                    }
                }
            }
        }
        modules.extend(crate::bundled_base::modules());
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
            "type": "package", "name": "nash/base", "version": "1.0.0",
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
            import Primitive exposing (..)
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
            import Primitive exposing (..)
            import Builtin exposing (..)
            import Literal exposing (fromInt)
            trait Drop 'a where
                drop : 'a -> ()
            impl Drop int where
                drop _ = assert Primitive.True
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
        let mut modules = project
            .discover_own_modules(&*db.lock().await)
            .await
            .unwrap();
        assert_eq!(modules.len(), 2);
        assert_eq!(modules[&literal].as_ref().unwrap().to_string(), "nash/base");
        assert_eq!(modules[&main], None);
        let graph = build_graph(db.clone(), &modules.keys().cloned().collect::<Vec<_>>())
            .await
            .unwrap();
        assert_eq!(graph.order, [literal.clone(), main.clone()]);
        let result = build(db.clone(), &graph, &modules).await;
        assert!(result.is_success(), "{result:?}");
        modules.insert(literal.clone(), Some("example/literals".parse().unwrap()));
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
        assert!(
            matches!(project.discover_own_modules(&*db.lock().await).await,
            Err(DriverError::ConflictingModuleOwners { uri, .. }) if *uri == literal)
        );
    }
}
