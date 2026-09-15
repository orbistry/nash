//! Serialized editor state. Every build uses an immutable overlay snapshot.
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

use nash_driver::{
    Database, DriverError, FileSystemSource, InMemorySource, ModuleCatalog, ModuleResult,
    OverlaySource, Project, SourceSpec, build, build_graph,
};
use nash_report::Source;
use tokio::sync::Mutex;
use tower_lsp_server::ls_types::{
    Diagnostic, DiagnosticSeverity, NumberOrString, PublishDiagnosticsParams, Uri,
};
use url::Url;

use crate::diagnostics::to_lsp;

struct Buffer {
    text: String,
    version: i32,
}

#[derive(Default)]
pub(crate) struct Workspace {
    pub roots: Vec<PathBuf>,
    buffers: BTreeMap<Uri, Buffer>,
    uris: BTreeMap<Url, Uri>,
    published: BTreeMap<PathBuf, BTreeSet<Uri>>,
    closed: BTreeSet<Uri>,
}

impl Workspace {
    pub fn open(&mut self, uri: Uri, text: String, version: i32) -> bool {
        if self
            .buffers
            .get(&uri)
            .is_some_and(|buffer| version <= buffer.version)
        {
            return false;
        }
        if let Some(url) = file_url(&uri) {
            self.uris.insert(url, uri.clone());
        }
        self.closed.remove(&uri);
        self.buffers.insert(uri, Buffer { text, version });
        true
    }

    pub fn change(&mut self, uri: &Uri, text: String, version: i32) -> bool {
        if !self.buffers.contains_key(uri) {
            return false;
        }
        self.open(uri.clone(), text, version)
    }

    pub fn close(&mut self, uri: &Uri) {
        self.buffers.remove(uri);
        self.closed.insert(uri.clone());
    }

    fn client_uri(&self, url: &Url) -> Option<Uri> {
        self.uris
            .get(url)
            .cloned()
            .or_else(|| url.as_str().parse().ok())
    }

    fn project_uri(&self, url: &Url) -> Option<Uri> {
        self.uris
            .get(url)
            .cloned()
            .or_else(|| {
                let target = url.to_file_path().ok()?;
                // Project diagnostics use the client's project-root spelling.
                // Source diagnostics keep canonical identities until opened.
                self.uris
                    .values()
                    .filter_map(|uri| {
                        let source = Url::parse(uri.as_str()).ok()?.to_file_path().ok()?;
                        source.ancestors().skip(1).find_map(|ancestor| {
                            let canonical = ancestor.canonicalize().ok()?;
                            let relative = target.strip_prefix(&canonical).ok()?;
                            Some((canonical.components().count(), ancestor.join(relative)))
                        })
                    })
                    .max_by_key(|(depth, _)| *depth)
                    .and_then(|(_, path)| Url::from_file_path(path).ok()?.as_str().parse().ok())
            })
            .or_else(|| url.as_str().parse().ok())
    }

    async fn project_diagnostics(
        &self,
        problems: &[nash_driver::ProjectDiagnostic],
        severity: DiagnosticSeverity,
        fallback: &Uri,
        diagnostics: &mut BTreeMap<Uri, Vec<Diagnostic>>,
    ) {
        for problem in problems {
            let uri = Url::from_file_path(&problem.path)
                .ok()
                .and_then(|url| self.project_uri(&url));
            let (uri, message) = match uri {
                Some(uri) => (uri, problem.message.clone()),
                None => (fallback.clone(), problem.to_string()),
            };
            let mut diagnostic = Diagnostic {
                severity: Some(severity),
                code: Some(NumberOrString::String(problem.code.into())),
                message,
                source: Some("nash".into()),
                ..Diagnostic::default()
            };
            if let Some(region) = problem.region {
                match tokio::fs::read_to_string(&problem.path).await {
                    Ok(text) => {
                        if let Some(range) =
                            crate::diagnostics::to_range(region, &Source::new(&text))
                        {
                            diagnostic.range = range;
                        } else {
                            diagnostic
                                .message
                                .push_str("\nThe source position exceeds LSP's coordinate limits.");
                        }
                    }
                    Err(error) => diagnostic
                        .message
                        .push_str(&format!("\nCannot read diagnostic source: {error}")),
                }
            }
            diagnostics.entry(uri).or_default().push(diagnostic);
        }
    }

    pub async fn rebuild(
        &mut self,
        trigger: &Uri,
    ) -> (Vec<PublishDiagnosticsParams>, Option<String>) {
        let Some(url) = file_url(trigger) else {
            return (vec![], Some("Invalid document URI".into()));
        };
        let Ok(path) = url.to_file_path() else {
            return (vec![], Some("Only file documents can be compiled".into()));
        };
        let parent = path.parent().unwrap_or(&path).to_path_buf();
        let root = self
            .roots
            .iter()
            .filter(|root| path.starts_with(root))
            .max_by_key(|root| root.components().count());
        let additional_sources: Vec<_> = self.buffers.keys().filter_map(file_url).collect();
        let project = match root {
            Some(root) => match Project::load_with_sources(
                root,
                None,
                nash_driver::ProjectMode::Editor,
                &additional_sources,
            )
            .await
            {
                Ok(project)
                    if project
                        .source_directories()
                        .iter()
                        .any(|dir| path.starts_with(dir)) =>
                {
                    Ok(project)
                }
                Ok(_) | Err(DriverError::ProjectNotFound { .. }) => {
                    Project::load_with_sources(
                        &parent,
                        None,
                        nash_driver::ProjectMode::Editor,
                        &additional_sources,
                    )
                    .await
                }
                Err(error) => Err(error),
            },
            None => {
                Project::load_with_sources(
                    &parent,
                    None,
                    nash_driver::ProjectMode::Editor,
                    &additional_sources,
                )
                .await
            }
        };
        let scope = project.as_ref().map_or_else(
            |_| {
                self.published
                    .keys()
                    .filter(|root| path.starts_with(root))
                    .max_by_key(|root| root.components().count())
                    .cloned()
                    .unwrap_or_else(|| {
                        parent
                            .ancestors()
                            .find(|dir| {
                                dir.join("aiken.toml").is_file() || dir.join("nash.jsonc").is_file()
                            })
                            .unwrap_or(&parent)
                            .to_path_buf()
                    })
            },
            |project| project.root.clone(),
        );
        let source_dirs = project.as_ref().ok().map(Project::source_directories);
        let warnings = project
            .as_ref()
            .map(|project| project.loaded.warnings.clone())
            .unwrap_or_default();
        let overlay =
            InMemorySource::with_files(self.buffers.iter().filter_map(|(uri, buffer)| {
                let url = file_url(uri)?;
                let path = url.to_file_path().ok()?;
                if source_dirs
                    .as_ref()
                    .is_some_and(|dirs| !dirs.iter().any(|dir| path.starts_with(dir)))
                {
                    return None;
                }
                Some((url, buffer.text.clone()))
            }));
        let db = Arc::new(Mutex::new(Database::new(OverlaySource::new(
            overlay,
            FileSystemSource::new(),
        ))));
        let modules = match project {
            Ok(project) => project.discover_modules(&*db.lock().await).await,
            // Standalone files still get parser, name, and type diagnostics.
            Err(DriverError::ProjectNotFound { .. }) => self
                .buffers
                .keys()
                .filter_map(|uri| {
                    let uri = file_url(uri)?;
                    let candidate = uri.to_file_path().ok()?;
                    (candidate.parent() == Some(parent.as_path())).then_some(uri)
                })
                .map(|uri| {
                    SourceSpec::standalone(&uri)
                        .map(|spec| (uri, spec))
                        .map_err(DriverError::from)
                })
                .collect::<Result<ModuleCatalog, DriverError>>(),
            Err(error) => Err(error),
        };
        let result = async {
            let modules = modules?;
            let graph = build_graph(db.clone(), &modules).await?;
            Ok::<_, DriverError>(build(db, &graph, &modules).await)
        }
        .await;
        let mut diagnostics: BTreeMap<Uri, Vec<Diagnostic>> = BTreeMap::new();
        let error = match result {
            Ok(result) => {
                for (uri, module) in &result.modules {
                    if let Some(uri) = self.client_uri(uri) {
                        let problems = match module {
                            ModuleResult::SourceUnavailable { message } => vec![Diagnostic {
                                severity: Some(DiagnosticSeverity::ERROR),
                                code: Some(NumberOrString::String("SOURCE UNAVAILABLE".into())),
                                source: Some("nash".into()),
                                message: message.clone(),
                                ..Diagnostic::default()
                            }],
                            _ => vec![],
                        };
                        diagnostics.insert(uri, problems);
                    }
                }
                for module in result.ordered_reports() {
                    if let Ok(url) = Url::from_file_path(&module.path)
                        && let Some(uri) = self.client_uri(&url)
                    {
                        let source = Source::new(&module.source);
                        diagnostics.entry(uri.clone()).or_default().extend(
                            module
                                .reports
                                .iter()
                                .map(|report| to_lsp(report, &source, &uri)),
                        );
                    }
                }
                None
            }
            Err(error) => {
                match &error {
                    DriverError::ProjectLoad {
                        diagnostics: problems,
                    } => {
                        self.project_diagnostics(
                            problems,
                            DiagnosticSeverity::ERROR,
                            trigger,
                            &mut diagnostics,
                        )
                        .await;
                    }
                    DriverError::ProjectDiagnostic(problem) => {
                        self.project_diagnostics(
                            std::slice::from_ref(problem),
                            DiagnosticSeverity::ERROR,
                            trigger,
                            &mut diagnostics,
                        )
                        .await;
                    }
                    _ => {}
                }
                Some(error.to_string())
            }
        };
        self.project_diagnostics(
            &warnings,
            DiagnosticSeverity::WARNING,
            trigger,
            &mut diagnostics,
        )
        .await;
        diagnostics.retain(|uri, _| !self.closed.contains(uri));
        let current: BTreeSet<_> = diagnostics.keys().cloned().collect();
        let previous = self.published.insert(scope, current).unwrap_or_default();
        for uri in previous {
            diagnostics.entry(uri).or_default();
        }
        // Always acknowledge this document, including clean buffers and close.
        diagnostics.entry(trigger.clone()).or_default();
        let notifications = diagnostics
            .into_iter()
            .map(|(uri, diagnostics)| {
                let version = self.buffers.get(&uri).map(|buffer| buffer.version);
                PublishDiagnosticsParams {
                    uri,
                    diagnostics,
                    version,
                }
            })
            .collect();
        (notifications, error)
    }
}

// Project discovery canonicalizes roots. Keep the client's original URI for
// publications while matching disk and overlay files by canonical path.
fn file_url(uri: &Uri) -> Option<Url> {
    let url = Url::parse(uri.as_str()).ok()?;
    let path = url.to_file_path().ok()?;
    let canonical = path
        .ancestors()
        .find_map(|ancestor| {
            ancestor.canonicalize().ok().map(|root| {
                root.join(
                    path.strip_prefix(ancestor)
                        .expect("ancestor is a path prefix"),
                )
            })
        })
        .unwrap_or(path);
    Url::from_file_path(canonical).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nash_report::json::module_to_json;

    const CLEAN: &str = "module Main exposing (..)\nvalue = ()\n";
    const BROKEN: &str = "module Main exposing (..)\nvalue = unknown\n";

    fn fixture() -> (tempfile::TempDir, Uri) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(
            dir.path().join("nash.jsonc"),
            r#"{"type":"application","sourceDirectories":["src"]}"#,
        )
        .unwrap();
        let path = dir.path().join("src/Main.nash");
        std::fs::write(&path, CLEAN).unwrap();
        let uri = Url::from_file_path(path).unwrap().as_str().parse().unwrap();
        (dir, uri)
    }

    fn for_uri<'a>(
        notifications: &'a [PublishDiagnosticsParams],
        uri: &Uri,
    ) -> &'a PublishDiagnosticsParams {
        notifications
            .iter()
            .find(|notification| &notification.uri == uri)
            .unwrap()
    }

    #[tokio::test]
    async fn unsaved_versions_override_disk_and_clear_fixed_errors() {
        let (_dir, uri) = fixture();
        let mut workspace = Workspace::default();
        assert!(workspace.open(uri.clone(), BROKEN.into(), 1));
        let (notifications, error) = workspace.rebuild(&uri).await;
        assert!(error.is_none(), "{error:?}");
        let notification = for_uri(&notifications, &uri);
        assert_eq!(notification.version, Some(1));
        assert!(!notification.diagnostics.is_empty());
        assert!(workspace.change(&uri, CLEAN.into(), 3));
        assert!(!workspace.change(&uri, BROKEN.into(), 2));
        let (notifications, error) = workspace.rebuild(&uri).await;
        assert!(error.is_none(), "{error:?}");
        let notification = for_uri(&notifications, &uri);
        assert_eq!(notification.version, Some(3));
        assert!(
            notification.diagnostics.is_empty(),
            "{:?}",
            notification.diagnostics
        );
        assert_eq!(
            std::fs::read_to_string(Url::parse(uri.as_str()).unwrap().to_file_path().unwrap())
                .unwrap(),
            CLEAN
        );
    }

    #[tokio::test]
    async fn close_discards_overlay_and_clears_diagnostics() {
        let (_dir, uri) = fixture();
        let mut workspace = Workspace::default();
        workspace.open(uri.clone(), BROKEN.into(), 8);
        assert!(
            !for_uri(&workspace.rebuild(&uri).await.0, &uri)
                .diagnostics
                .is_empty()
        );
        workspace.close(&uri);
        let (notifications, error) = workspace.rebuild(&uri).await;
        assert!(error.is_none(), "{error:?}");
        let notification = for_uri(&notifications, &uri);
        assert_eq!(notification.version, None);
        assert!(notification.diagnostics.is_empty());
        assert!(!workspace.change(&uri, BROKEN.into(), 9));
    }

    #[tokio::test]
    async fn lsp_and_json_contain_the_same_problem_set_and_spans() {
        let (dir, uri) = fixture();
        let mut workspace = Workspace::default();
        workspace.open(uri.clone(), BROKEN.into(), 1);
        let (notifications, error) = workspace.rebuild(&uri).await;
        assert!(error.is_none(), "{error:?}");
        let lsp = &for_uri(&notifications, &uri).diagnostics;
        let overlay = InMemorySource::with_files([(file_url(&uri).unwrap(), BROKEN.into())]);
        let db = Arc::new(Mutex::new(Database::new(OverlaySource::new(
            overlay,
            FileSystemSource::new(),
        ))));
        let project = Project::load(dir.path()).await.unwrap();
        let modules = project.discover_modules(&*db.lock().await).await.unwrap();
        let graph = build_graph(db.clone(), &modules).await.unwrap();
        let result = build(db, &graph, &modules).await;
        let modules = result.ordered_reports();
        let problems: Vec<_> = modules
            .iter()
            .flat_map(|module| {
                module_to_json(module)["problems"]
                    .as_array()
                    .unwrap()
                    .clone()
            })
            .collect();
        assert_eq!(lsp.len(), problems.len());
        for (diagnostic, problem) in lsp.iter().zip(problems) {
            assert_eq!(
                serde_json::to_value(&diagnostic.code).unwrap(),
                problem["code"]
            );
            assert_eq!(
                diagnostic.range.start.line + 1,
                problem["region"]["start"]["line"].as_u64().unwrap() as u32
            );
            assert_eq!(
                diagnostic.range.start.character + 1,
                problem["region"]["start"]["column"].as_u64().unwrap() as u32
            );
            assert_eq!(
                diagnostic.range.end.line + 1,
                problem["region"]["end"]["line"].as_u64().unwrap() as u32
            );
            assert_eq!(
                diagnostic.range.end.character + 1,
                problem["region"]["end"]["column"].as_u64().unwrap() as u32
            );
        }
    }

    #[tokio::test]
    async fn independent_disk_module_errors_are_published_and_cleared() {
        let (dir, uri) = fixture();
        let other_path = dir.path().join("src/Other.nash");
        let other: Uri = Url::from_file_path(
            other_path
                .parent()
                .unwrap()
                .canonicalize()
                .unwrap()
                .join("Other.nash"),
        )
        .unwrap()
        .as_str()
        .parse()
        .unwrap();
        std::fs::write(&other_path, "module Other exposing (..)\nvalue = missing\n").unwrap();
        let mut workspace = Workspace::default();
        workspace.open(uri.clone(), BROKEN.into(), 1);
        let (notifications, error) = workspace.rebuild(&uri).await;
        assert!(error.is_none(), "{error:?}");
        assert!(!for_uri(&notifications, &uri).diagnostics.is_empty());
        assert!(!for_uri(&notifications, &other).diagnostics.is_empty());
        std::fs::remove_file(other_path).unwrap();
        let (notifications, _) = workspace.rebuild(&uri).await;
        assert!(for_uri(&notifications, &other).diagnostics.is_empty());
    }
    #[tokio::test]
    async fn close_rebuilds_dependents_from_disk_without_cascade_errors() {
        let (dir, uri) = fixture();
        let other_path = dir.path().join("src/Other.nash");
        std::fs::write(&other_path, "module Other exposing (..)\nvalue = ()\n").unwrap();
        let other: Uri = Url::from_file_path(&other_path)
            .unwrap()
            .as_str()
            .parse()
            .unwrap();
        let mut workspace = Workspace::default();
        workspace.open(
            uri.clone(),
            "module Main exposing (..)\nimport Other\nvalue = Other.value\n".into(),
            1,
        );
        workspace.open(
            other.clone(),
            "module Other exposing (..)\nvalue = missing\n".into(),
            1,
        );
        let (notifications, error) = workspace.rebuild(&other).await;
        assert!(error.is_none(), "{error:?}");
        assert!(!for_uri(&notifications, &other).diagnostics.is_empty());
        assert!(for_uri(&notifications, &uri).diagnostics.is_empty());
        workspace.close(&other);
        let (notifications, error) = workspace.rebuild(&other).await;
        assert!(error.is_none(), "{error:?}");
        assert!(for_uri(&notifications, &other).diagnostics.is_empty());
        assert!(for_uri(&notifications, &uri).diagnostics.is_empty());
    }

    #[tokio::test]
    async fn unsaved_new_file_is_discovered() {
        let (dir, _) = fixture();
        let uri: Uri = Url::from_file_path(dir.path().join("src/New.nash"))
            .unwrap()
            .as_str()
            .parse()
            .unwrap();
        let mut workspace = Workspace::default();
        workspace.open(
            uri.clone(),
            "module New exposing (..)\nvalue = missing\n".into(),
            1,
        );
        let (notifications, error) = workspace.rebuild(&uri).await;
        assert!(error.is_none(), "{error:?}");
        assert!(!for_uri(&notifications, &uri).diagnostics.is_empty());
    }
    #[tokio::test]
    async fn nested_projects_use_the_most_specific_owning_root() {
        let (dir, _) = fixture();
        let nested = dir.path().join("nested");
        std::fs::create_dir_all(nested.join("src")).unwrap();
        std::fs::write(
            nested.join("nash.jsonc"),
            r#"{"type":"application","sourceDirectories":["src"]}"#,
        )
        .unwrap();
        let path = nested.join("src/Main.nash");
        std::fs::write(&path, CLEAN).unwrap();
        let uri: Uri = Url::from_file_path(path).unwrap().as_str().parse().unwrap();
        // Also cover a nested project not separately registered by the client:
        // the parent application does not own its source directory.
        for roots in [
            vec![
                dir.path().canonicalize().unwrap(),
                nested.canonicalize().unwrap(),
            ],
            vec![dir.path().canonicalize().unwrap()],
        ] {
            let mut workspace = Workspace {
                roots,
                ..Workspace::default()
            };
            workspace.open(uri.clone(), BROKEN.into(), 1);
            let (notifications, error) = workspace.rebuild(&uri).await;
            assert!(error.is_none(), "{error:?}");
            assert!(!for_uri(&notifications, &uri).diagnostics.is_empty());
        }
    }

    #[tokio::test]
    async fn unreadable_source_has_a_diagnostic_that_clears_after_repair() {
        let (dir, uri) = fixture();
        let unavailable = dir.path().join("src/Unavailable.nash");
        // Reading a directory is a deterministic I/O failure even as root.
        std::fs::create_dir(&unavailable).unwrap();
        let unavailable_uri: Uri = Url::from_file_path(unavailable.canonicalize().unwrap())
            .unwrap()
            .as_str()
            .parse()
            .unwrap();
        let mut workspace = Workspace::default();
        workspace.open(uri.clone(), BROKEN.into(), 1);
        let (notifications, _) = workspace.rebuild(&uri).await;
        let diagnostic = &for_uri(&notifications, &unavailable_uri).diagnostics[0];
        assert_eq!(
            diagnostic.code,
            Some(NumberOrString::String("SOURCE UNAVAILABLE".into()))
        );
        assert_eq!(diagnostic.severity, Some(DiagnosticSeverity::ERROR));
        assert!(diagnostic.message.contains("read"));
        assert!(!for_uri(&notifications, &uri).diagnostics.is_empty());
        std::fs::remove_dir(&unavailable).unwrap();
        std::fs::write(
            &unavailable,
            "module Unavailable exposing (..)\nvalue = ()\n",
        )
        .unwrap();
        let (notifications, error) = workspace.rebuild(&uri).await;
        assert!(error.is_none(), "{error:?}");
        assert!(
            for_uri(&notifications, &unavailable_uri)
                .diagnostics
                .is_empty()
        );
    }

    #[tokio::test]
    async fn aiken_unsaved_project_and_standalone_buffers_share_selection_and_diagnostics() {
        const VALID: &str = "test addition() { 1 + 1 == 2 }\n";
        const INVALID: &str = "test addition() { 1 + True == 2 }\n";
        for project in ["nash", "aiken", "standalone"] {
            let dir = tempfile::tempdir().unwrap();
            let source_dir = match project {
                "nash" => {
                    std::fs::write(
                        dir.path().join("nash.jsonc"),
                        r#"{"type":"application","sourceDirectories":["src"]}"#,
                    )
                    .unwrap();
                    let source_dir = dir.path().join("src");
                    std::fs::create_dir(&source_dir).unwrap();
                    source_dir
                }
                "aiken" => {
                    std::fs::write(dir.path().join("aiken.toml"), "name = \"test/editor\"\nversion = \"1.0.0\"\ncompiler = \"v1.1.23\"\nplutus = \"v3\"\n").unwrap();
                    // Neither the source file nor its directory exists on disk.
                    dir.path().join("lib")
                }
                _ => dir.path().to_path_buf(),
            };
            let uri: Uri = Url::from_file_path(source_dir.join("add_one.ak"))
                .unwrap()
                .as_str()
                .parse()
                .unwrap();
            let mut workspace = Workspace::default();
            workspace.open(uri.clone(), VALID.into(), 1);
            let (notifications, error) = workspace.rebuild(&uri).await;
            assert!(error.is_none(), "{error:?}");
            assert!(
                for_uri(&notifications, &uri).diagnostics.is_empty(),
                "{notifications:?}"
            );
            workspace.change(&uri, INVALID.into(), 2);
            let (notifications, error) = workspace.rebuild(&uri).await;
            assert!(error.is_none(), "{error:?}");
            let notification = for_uri(&notifications, &uri);
            let diagnostic = notification
                .diagnostics
                .iter()
                .find(|diagnostic| diagnostic.severity == Some(DiagnosticSeverity::ERROR))
                .unwrap();
            assert_eq!(notification.version, Some(2));
            assert_eq!(diagnostic.range.start.line, 0);
            assert_ne!(diagnostic.range.start, diagnostic.range.end);

            workspace.change(&uri, VALID.into(), 3);
            let (notifications, error) = workspace.rebuild(&uri).await;
            assert!(error.is_none(), "{error:?}");
            let notification = for_uri(&notifications, &uri);
            assert_eq!(notification.version, Some(3));
            assert!(
                notification.diagnostics.is_empty(),
                "{:?}",
                notification.diagnostics
            );
        }
    }

    #[tokio::test]
    async fn unsaved_default_environment_is_available_before_project_validation() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        std::fs::write(root.join("aiken.toml"), "name = \"test/editor\"\nversion = \"1.0.0\"\ncompiler = \"v1.1.23\"\nplutus = \"v3\"\n[config.default]\noffset = 1\n").unwrap();
        std::fs::create_dir(root.join("env")).unwrap();
        std::fs::write(root.join("env/preview.ak"), "pub const value = 0\n").unwrap();
        let uri = |relative| {
            Url::from_file_path(root.join(relative))
                .unwrap()
                .as_str()
                .parse::<Uri>()
                .unwrap()
        };
        let main = uri("lib/main.ak");
        let default = uri("env/default.ak");
        let mut workspace = Workspace::default();
        workspace.open(
            main.clone(),
            "use env\nuse config\npub fn selected() -> Int { env.value + config.offset }\n".into(),
            1,
        );
        workspace.open(default.clone(), "pub const value = 41\n".into(), 1);
        let (notifications, error) = workspace.rebuild(&main).await;
        assert!(error.is_none(), "{error:?}");
        assert!(
            for_uri(&notifications, &main).diagnostics.is_empty(),
            "{notifications:?}"
        );
        workspace.change(&default, "pub const value = False\n".into(), 2);
        let (notifications, error) = workspace.rebuild(&default).await;
        assert!(error.is_none(), "{error:?}");
        assert!(
            for_uri(&notifications, &main)
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity == Some(DiagnosticSeverity::ERROR)),
            "{notifications:?}"
        );
    }

    #[tokio::test]
    async fn aiken_manifest_errors_are_located_and_clear_after_repair() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let manifest = "name = \"test/editor\"\nversion = \"1.0.0\"\ncompiler = \"v1.1.23\"\nplutus = \"v2\"\n";
        std::fs::write(root.join("aiken.toml"), manifest).unwrap();
        let uri = |relative| {
            Url::from_file_path(root.join(relative))
                .unwrap()
                .as_str()
                .parse::<Uri>()
                .unwrap()
        };
        let main = uri("lib/main.ak");
        let config = uri("aiken.toml");
        let mut workspace = Workspace::default();
        workspace.open(main.clone(), "pub const value = 42\n".into(), 1);
        let (notifications, error) = workspace.rebuild(&main).await;
        assert!(error.is_some());
        let diagnostic = &for_uri(&notifications, &config).diagnostics[0];
        assert_eq!(
            diagnostic.code,
            Some(NumberOrString::String("NAP4103".into()))
        );
        assert_eq!(diagnostic.range.start.line, 3);
        assert_ne!(diagnostic.range.start, diagnostic.range.end);
        std::fs::write(root.join("aiken.toml"), manifest.replace("v2", "v3")).unwrap();
        let (notifications, error) = workspace.rebuild(&main).await;
        assert!(error.is_none(), "{error:?}");
        assert!(for_uri(&notifications, &config).diagnostics.is_empty());
        assert!(
            for_uri(&notifications, &main).diagnostics.is_empty(),
            "{notifications:?}"
        );
    }
}
