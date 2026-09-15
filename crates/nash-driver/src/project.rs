//! Manifest selection and frontend-neutral project loading.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use nash_config::{Config, Workspace};
use nash_frontend::{LoadedPackage, ProjectDiagnostic, ProjectLoadRequest, ProjectMetadata};
pub use nash_frontend::{
    LoadedProject, ModuleCatalog, PackageId, PackageSourceId, ProjectFormat, ProjectMode,
    SourceSpec,
};

use crate::database::Database;
use crate::error::DriverError;
use crate::source::path_to_uri;

#[derive(Debug)]
pub struct Project {
    pub root: PathBuf,
    pub format: ProjectFormat,
    pub loaded: LoadedProject,
    /// Native source roots are retained for editor overlay discovery.
    pub members: Vec<ProjectMember>,
    environment: Option<String>,
    mode: ProjectMode,
}

#[derive(Debug)]
pub struct ProjectMember {
    pub root: PathBuf,
    pub config: Config,
    pub source_dirs: Vec<PathBuf>,
}

impl Project {
    pub async fn load(path: impl AsRef<Path>) -> Result<Self, DriverError> {
        Self::load_with_options(path, None, ProjectMode::Check).await
    }

    pub async fn load_with_options(
        path: impl AsRef<Path>,
        environment: Option<&str>,
        mode: ProjectMode,
    ) -> Result<Self, DriverError> {
        Self::load_with_sources(path, environment, mode, &[]).await
    }

    /// Include editor buffers before applying project discovery rules.
    pub async fn load_with_sources(
        path: impl AsRef<Path>,
        environment: Option<&str>,
        mode: ProjectMode,
        additional_sources: &[url::Url],
    ) -> Result<Self, DriverError> {
        let (manifest, format) = select_manifest(path.as_ref())?;
        let manifest = manifest
            .canonicalize()
            .map_err(|source| DriverError::ReadError {
                path: manifest.clone(),
                source,
            })?;
        let root = manifest
            .parent()
            .expect("manifest has parent")
            .to_path_buf();
        if format == ProjectFormat::Aiken {
            let loaded =
                load_aiken(&manifest, environment, mode, additional_sources.to_vec()).await?;
            return Ok(Self {
                root: loaded.root.clone(),
                format,
                loaded,
                members: vec![],
                environment: environment.map(str::to_owned),
                mode,
            });
        }
        let config = nash_config::parse_file(&manifest)?;
        let members = match config {
            Config::Workspace(workspace) => load_workspace_members(&root, &workspace)?,
            config => vec![make_member(&root, config)],
        };
        let mut project = Self {
            root: root.clone(),
            format,
            loaded: LoadedProject {
                root,
                packages: vec![],
                catalog: ModuleCatalog::new(),
                warnings: vec![],
            },
            members,
            environment: environment.map(str::to_owned),
            mode,
        };
        project.loaded.packages = project.native_packages();
        let db = Database::new(crate::FileSystemSource::new());
        project.loaded.catalog = project.discover_native(&db).await?;
        Ok(project)
    }

    /// Native discovery includes unsaved overlay files; Aiken discovery is owned by its loader.
    pub async fn discover_modules(&self, db: &Database) -> Result<ModuleCatalog, DriverError> {
        match self.format {
            ProjectFormat::Nash => self.discover_native(db).await,
            ProjectFormat::Aiken => {
                let mut additional = std::collections::BTreeSet::new();
                for root in self.source_directories() {
                    for uri in db.glob(&path_to_uri(&root)?, "**/*.ak").await? {
                        additional.insert(uri);
                    }
                }
                if additional
                    .iter()
                    .all(|uri| self.loaded.catalog.contains_key(uri))
                {
                    return Ok(self.loaded.catalog.clone());
                }
                let loaded = load_aiken(
                    &self.root.join("aiken.toml"),
                    self.environment.as_deref(),
                    self.mode,
                    additional.into_iter().collect(),
                )
                .await?;
                Ok(loaded.catalog)
            }
        }
    }

    async fn discover_native(&self, db: &Database) -> Result<ModuleCatalog, DriverError> {
        let mut modules = ModuleCatalog::new();
        for member in &self.members {
            let package = member.package();
            let metadata = Arc::new(member.metadata());
            for source_dir in &member.source_dirs {
                let source_dir = normalize_path(source_dir);
                let base_uri = path_to_uri(&source_dir)?;
                for extension in crate::FRONTENDS.extensions() {
                    for uri in db.glob(&base_uri, &format!("**/*.{extension}")).await? {
                        let mut spec = SourceSpec::new(&uri, &source_dir, None)?;
                        spec.key.package = package.clone();
                        spec.project = Some(metadata.clone());
                        if let Some(previous) = modules.get(&uri) {
                            if previous.key.package != package {
                                return Err(DriverError::ConflictingModuleOwners {
                                    uri: Box::new(uri),
                                    first: previous
                                        .key
                                        .package
                                        .name
                                        .clone()
                                        .unwrap_or_else(|| "application".into()),
                                    second: member.name(),
                                });
                            }
                            if previous.key.module != spec.key.module {
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

    fn native_packages(&self) -> Vec<LoadedPackage> {
        let mut packages = std::collections::BTreeMap::new();
        for member in &self.members {
            let id = member.package();
            packages.entry(id.clone()).or_insert_with(|| LoadedPackage {
                id,
                root: member.root.clone(),
                metadata: member.metadata(),
                dependencies: vec![],
            });
        }
        for member in &self.members {
            let declared = match &member.config {
                Config::Package(package) => &package.dependencies,
                Config::Application(application) => &application.dependencies,
                Config::Workspace(workspace) => &workspace.dependencies,
            };
            let dependencies = packages
                .keys()
                .filter(|id| {
                    id.name.as_deref().is_some_and(|name| {
                        declared.keys().any(|declared| declared.to_string() == name)
                    })
                })
                .cloned()
                .collect();
            packages
                .get_mut(&member.package())
                .expect("member package was collected")
                .dependencies = dependencies;
        }
        packages.into_values().collect()
    }

    pub fn source_directories(&self) -> Vec<PathBuf> {
        if self.format == ProjectFormat::Nash {
            self.members
                .iter()
                .flat_map(|member| member.source_dirs.clone())
                .collect()
        } else {
            let mut roots = std::collections::BTreeSet::new();
            for package in &self.loaded.packages {
                roots.insert(package.root.join("lib"));
                if matches!(&package.id.source, PackageSourceId::Local(root) if root == &package.root)
                {
                    roots.insert(package.root.join("validators"));
                    roots.insert(package.root.join("env"));
                }
            }
            roots.into_iter().collect()
        }
    }
}

async fn load_aiken(
    manifest: &Path,
    environment: Option<&str>,
    mode: ProjectMode,
    additional_sources: Vec<url::Url>,
) -> Result<LoadedProject, DriverError> {
    let location = manifest.to_path_buf();
    let environment = environment.map(str::to_owned);
    tokio::task::spawn_blocking(move || {
        let request = ProjectLoadRequest {
            location: &location,
            environment: environment.as_deref(),
            mode,
        };
        if additional_sources.is_empty() {
            nash_project_aiken::load(request)
        } else {
            nash_project_aiken::load_with_sources(request, &additional_sources)
        }
    })
    .await
    .map_err(|error| DriverError::ProjectLoad {
        diagnostics: vec![ProjectDiagnostic::new(
            "NAP4000",
            manifest,
            format!("project loader failed: {error}"),
        )],
    })?
    .map_err(|diagnostics| DriverError::ProjectLoad { diagnostics })
}

/// The closest manifest wins; an explicit manifest resolves same-directory ambiguity.
fn select_manifest(start: &Path) -> Result<(PathBuf, ProjectFormat), DriverError> {
    let absolute = if start.is_absolute() {
        start.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|source| DriverError::ReadError {
                path: start.to_path_buf(),
                source,
            })?
            .join(start)
    };
    match absolute.file_name().and_then(|name| name.to_str()) {
        Some("nash.jsonc") => return Ok((absolute, ProjectFormat::Nash)),
        Some("aiken.toml") => return Ok((absolute, ProjectFormat::Aiken)),
        _ => {}
    }
    let directory = if absolute.is_file() {
        absolute.parent().unwrap_or(&absolute)
    } else {
        &absolute
    };
    for current in directory.ancestors() {
        let nash = current.join("nash.jsonc");
        let aiken = current.join("aiken.toml");
        match (nash.is_file(), aiken.is_file()) {
            (true, true) => {
                return Err(DriverError::AmbiguousProject {
                    path: current.to_path_buf(),
                });
            }
            (true, false) => return Ok((nash, ProjectFormat::Nash)),
            (false, true) => return Ok((aiken, ProjectFormat::Aiken)),
            (false, false) => {}
        }
    }
    Err(DriverError::ProjectNotFound {
        path: start.to_path_buf(),
    })
}

fn load_workspace_members(
    root: &Path,
    workspace: &Workspace,
) -> Result<Vec<ProjectMember>, DriverError> {
    let mut members = Vec::new();
    for pattern in &workspace.members {
        let full_pattern = root.join(pattern);
        let matches = glob::glob(&full_pattern.to_string_lossy())
            .map_err(|error| DriverError::InvalidModulePath {
                path: error.msg.into(),
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| DriverError::ReadError {
                path: error.path().into(),
                source: std::io::Error::new(error.error().kind(), error.error().to_string()),
            })?;
        if matches.is_empty() {
            return Err(DriverError::MemberNotFound {
                pattern: pattern.clone(),
            });
        }
        for member in matches {
            let member = if member.is_file() {
                member.parent().unwrap().to_path_buf()
            } else {
                member
            };
            let manifest = member.join("nash.jsonc");
            if manifest.is_file() {
                members.push(make_member(&member, nash_config::parse_file(&manifest)?));
            }
        }
    }
    Ok(members)
}

fn make_member(root: &Path, config: Config) -> ProjectMember {
    let source_dirs = match &config {
        Config::Application(app) => app
            .source_directories
            .iter()
            .map(|directory| root.join(directory))
            .collect(),
        Config::Package(_) => vec![root.join("src")],
        Config::Workspace(_) => vec![],
    };
    ProjectMember {
        root: root.to_path_buf(),
        config,
        source_dirs,
    }
}

impl ProjectMember {
    pub fn name(&self) -> String {
        match &self.config {
            Config::Package(package) => package.name.to_string(),
            Config::Application(_) => "application".into(),
            Config::Workspace(_) => "workspace".into(),
        }
    }

    fn package(&self) -> PackageId {
        let (name, version) = match &self.config {
            Config::Package(package) => (Some(package.name.to_string()), package.version.clone()),
            _ => (None, String::new()),
        };
        PackageId {
            name,
            version,
            source: PackageSourceId::Local(normalize_path(&self.root)),
        }
    }

    fn metadata(&self) -> ProjectMetadata {
        let id = self.package();
        let (license, description, compiler) = match &self.config {
            Config::Package(package) => (
                Some(package.license.clone()),
                package.summary.clone(),
                package.compiler.clone(),
            ),
            Config::Application(app) => (None, String::new(), app.compiler.clone()),
            Config::Workspace(workspace) => (None, String::new(), workspace.compiler.clone()),
        };
        ProjectMetadata {
            name: id.name,
            version: id.version,
            license,
            description,
            repository: None,
            compiler: compiler.unwrap_or_default(),
            plutus: "v3".into(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InMemorySource, ModuleResult, build, build_graph};
    use tokio::sync::Mutex;
    use url::Url;

    struct Directory(PathBuf);
    impl Directory {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "nash-project-{name}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[tokio::test]
    async fn workspace_literal_defaults_use_discovered_package_ownership() {
        let core = nash_config::parse(
            r#"{"type":"package","name":"nash/core","version":"1.0.0","summary":"Core","license":"MIT","exposedModules":["Literal"]}"#,
            "/work/core/nash.jsonc",
        ).unwrap();
        let app = nash_config::parse(r#"{"type":"application"}"#, "/work/app/nash.jsonc").unwrap();
        let mut project = Project {
            root: "/work".into(),
            format: ProjectFormat::Nash,
            loaded: LoadedProject {
                root: "/work".into(),
                packages: vec![],
                catalog: ModuleCatalog::new(),
                warnings: vec![],
            },
            members: vec![
                make_member(Path::new("/work/core"), core.clone()),
                make_member(Path::new("/work/app"), app),
            ],
            environment: None,
            mode: ProjectMode::Check,
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
            .into(),
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
            .into(),
        );
        let db = Arc::new(Mutex::new(Database::new(mem)));
        project
            .members
            .push(make_member(Path::new("/work/core"), core));
        let mut modules = project.discover_modules(&*db.lock().await).await.unwrap();
        assert_eq!(modules.len(), 2);
        assert_eq!(
            modules[&literal].key.package.name.as_deref(),
            Some("nash/core")
        );
        assert_eq!(modules[&literal].key.package.version, "1.0.0");
        assert_eq!(modules[&main].key.package.name, None);
        let graph = build_graph(db.clone(), &modules).await.unwrap();
        assert_eq!(graph.order, [literal.clone(), main.clone()]);
        let result = build(db.clone(), &graph, &modules).await;
        assert!(result.is_success(), "{result:?}");
        modules.get_mut(&literal).unwrap().key.package.name = Some("example/literals".into());
        let graph = build_graph(db.clone(), &modules).await.unwrap();
        let result = build(db.clone(), &graph, &modules).await;
        assert!(
            matches!(&result.modules[&main], ModuleResult::Failed(reports)
            if reports.reports.iter().any(|report| report.title == "AMBIGUOUS TYPE"))
        );
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

    #[test]
    fn closest_manifest_and_explicit_selection_are_unambiguous() {
        let directory = Directory::new("manifest-selection");
        let root = &directory.0;
        std::fs::create_dir_all(root.join("child/src")).unwrap();
        std::fs::write(root.join("nash.jsonc"), "{}").unwrap();
        std::fs::write(root.join("child/aiken.toml"), "").unwrap();
        assert_eq!(
            select_manifest(&root.join("child/src")).unwrap(),
            (root.join("child/aiken.toml"), ProjectFormat::Aiken)
        );
        std::fs::write(root.join("child/nash.jsonc"), "{}").unwrap();
        assert!(
            matches!(select_manifest(&root.join("child/src")), Err(DriverError::AmbiguousProject { path }) if path == root.join("child"))
        );
        assert_eq!(
            select_manifest(&root.join("child/nash.jsonc")).unwrap(),
            (root.join("child/nash.jsonc"), ProjectFormat::Nash)
        );
        assert_eq!(
            select_manifest(&root.join("child/aiken.toml")).unwrap(),
            (root.join("child/aiken.toml"), ProjectFormat::Aiken)
        );
    }

    #[tokio::test]
    async fn empty_aiken_project_discovers_new_unsaved_library_and_environment() {
        let directory = Directory::new("editor-overlays");
        std::fs::write(directory.0.join("aiken.toml"),
            "name = \"example/empty\"\nversion = \"1.0.0\"\ncompiler = \"v1.1.23\"\nplutus = \"v3\"\n").unwrap();
        let project = Project::load_with_options(&directory.0, None, ProjectMode::Editor)
            .await
            .unwrap();
        let main = Url::from_file_path(project.root.join("lib/main.ak")).unwrap();
        let env = Url::from_file_path(project.root.join("env/default.ak")).unwrap();
        let db = Arc::new(Mutex::new(Database::new(InMemorySource::with_files([
            (
                main.clone(),
                "use env\npub fn value() { env.value }\n".into(),
            ),
            (env.clone(), "pub const value = 1\n".into()),
        ]))));
        let catalog = project.discover_modules(&*db.lock().await).await.unwrap();
        let graph = build_graph(db.clone(), &catalog).await.unwrap();
        let result = build(db, &graph, &catalog).await;
        assert!(
            matches!(result.modules[&main], ModuleResult::Success { .. }),
            "{result:?}"
        );
        assert!(
            matches!(result.modules[&env], ModuleResult::Success { .. }),
            "{result:?}"
        );
        assert!(result.is_success(), "{result:?}");
    }
}
