//! Module compilation orchestration.
//!
//! Each module runs Elm's full pipeline: parse -> canonicalize ->
//! direct inference -> nitpick -> `Interface::from_module` with the solver's
//! annotations. Modules compile in dependency order, and each solved
//! module and solved evidence remain in the build scope. Canonical nodes
//! live in a shared arena, so interfaces borrow them without moving the
//! nodes addressed by solved evidence.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use bumpalo::Bump;
use nash_can::Interface;
use tokio::sync::Mutex;
use url::Url;

use crate::database::Database;
use crate::error::DriverError;
use crate::graph::DepGraph;

#[cfg(test)]
mod collection_tests;
#[cfg(test)]
mod finish_tests;
#[cfg(test)]
mod nitpick_source_tests;
#[cfg(test)]
mod nitpick_tests;

/// Result of compiling a single module.
#[derive(Debug)]
pub enum ModuleResult {
    /// Module compiled successfully.
    Success {
        /// Number of declarations in the module.
        decl_count: usize,
    },
    /// Compilation was skipped because these original root dependencies failed.
    Blocked { dependencies: Vec<Url> },
    /// Structured errors produced while the module arena was alive.
    Failed(nash_report::ModuleReports),
    /// Source I/O failed before a compiler phase could run.
    SourceUnavailable { message: String },
}

/// Result of a full build.
#[derive(Debug)]
pub struct BuildResult {
    /// Results for each module.
    pub modules: HashMap<Url, ModuleResult>,

    /// Public interface fingerprints and kind contracts from successful modules.
    pub interfaces: HashMap<Url, crate::interface::Interface>,

    /// Total number of modules processed.
    pub total: usize,

    /// Number of successful compilations.
    pub success: usize,

    /// Number of failed compilations.
    pub failed: usize,

    /// Warnings collected during canonicalization.
    pub warnings: Vec<nash_report::ModuleReports>,
}

impl BuildResult {
    /// Compiler errors and warnings in a common order for every frontend.
    pub fn ordered_reports(&self) -> Vec<&nash_report::ModuleReports> {
        let mut reports: Vec<_> = self
            .modules
            .values()
            .filter_map(|result| match result {
                ModuleResult::Failed(reports) => Some(reports),
                _ => None,
            })
            .chain(self.warnings.iter())
            .collect();
        reports.sort_by(|a, b| a.path.cmp(&b.path).then(a.name.cmp(&b.name)));
        reports
    }

    /// Check if the build was completely successful.
    pub fn is_success(&self) -> bool {
        self.failed == 0
    }
}

/// Canonical nodes and their solved schemes and use-site evidence.
/// The maps are owned so their heap allocations are dropped with the build;
/// canonical nodes and map values borrow the build arena.
#[derive(Debug)]
pub struct SolvedModule<'a> {
    pub uri: Url,
    pub tables: nash_can::environment::Tables<'a>,
    pub module: &'a nash_ast::Module<'a>,
    pub annotations: nash_can::Annotations<'a>,
    pub types: nash_solve::SolvedTypes<'a>,
}

/// Borrowed canonical build state. A finish callback must produce an owned
/// result before this arena is dropped; node-addressed solved metadata stays valid.
pub struct Solved<'a> {
    pub store: &'a Bump,
    pub modules: Vec<SolvedModule<'a>>,
}

/// Holds the output of compiling a single module.
struct CompileOutput {
    uri: Url,
    result: ModuleResult,
    warnings: Vec<nash_report::ModuleReports>,
}

/// Compile all modules through the full pipeline, in dependency order.
///
/// The async part only fetches sources; the CPU-bound compilation runs on
/// tokio's blocking pool (`spawn_blocking`) so no executor worker is ever
/// stalled.
/// `origins` must contain every module in the graph, including applications.
pub async fn build(
    db: Arc<Mutex<Database>>,
    graph: &DepGraph,
    origins: &crate::ModuleOrigins,
) -> BuildResult {
    build_with(db, graph, origins, |_| ()).await.0
}

/// Finish a successful frontend build while its original canonical arena and
/// owned metadata maps remain alive. Failed builds never call the backend.
pub async fn build_with<R, F>(
    db: Arc<Mutex<Database>>,
    graph: &DepGraph,
    origins: &crate::ModuleOrigins,
    finish: F,
) -> (BuildResult, Option<R>)
where
    R: Send + 'static,
    F: for<'a> FnOnce(Solved<'a>) -> R + Send + 'static,
{
    let modules: Vec<&Url> = graph.levels().into_iter().flatten().collect();
    let sources = fetch_sources(&db, &modules)
        .await
        .into_iter()
        .map(|(uri, source)| {
            let package = origins[&uri].clone();
            (uri, package, source)
        })
        .collect();

    let edges = graph.edges.clone();
    tokio::task::spawn_blocking(move || build_sync_with_edges_and(sources, &edges, finish))
        .await
        .expect("compile task panicked")
}

/// Compile modules one at a time in dependency order, threading each
/// solved module's interface to its dependents through a build-wide arena.
///
/// Type checking is inherently dependency-ordered, so within-build
/// compilation is sequential within a build.
#[cfg(test)]
fn build_sync_with_edges(
    sources: Vec<(
        Url,
        Option<nash_config::PackageName>,
        Result<String, String>,
    )>,
    edges: &HashMap<Url, Vec<Url>>,
) -> BuildResult {
    build_sync_with_edges_and(sources, edges, |_| ()).0
}

fn build_sync_with_edges_and<R>(
    sources: Vec<(
        Url,
        Option<nash_config::PackageName>,
        Result<String, String>,
    )>,
    edges: &HashMap<Url, Vec<Url>>,
    finish: impl for<'a> FnOnce(Solved<'a>) -> R,
) -> (BuildResult, Option<R>) {
    let store = Bump::new();
    let mut interfaces = BTreeMap::from([("Builtin", nash_can::kinds::builtin_interface(&store))]);
    let mut public_interfaces = HashMap::new();
    let mut solved = BTreeMap::new();

    let mut results: HashMap<Url, ModuleResult> = HashMap::new();
    let mut all_warnings: Vec<nash_report::ModuleReports> = Vec::new();

    for (uri, package, source) in &sources {
        let mut dependencies = std::collections::BTreeSet::new();
        for dependency in edges.get(uri).into_iter().flatten() {
            match results.get(dependency) {
                Some(ModuleResult::Success { .. }) => {}
                Some(ModuleResult::Blocked {
                    dependencies: roots,
                }) => dependencies.extend(roots.iter().cloned()),
                _ => {
                    dependencies.insert(dependency.clone());
                }
            }
        }
        if !dependencies.is_empty() {
            results.insert(
                uri.clone(),
                ModuleResult::Blocked {
                    dependencies: dependencies.into_iter().collect(),
                },
            );
            continue;
        }
        let (output, compiled) = compile_module(uri, package.as_ref(), source, &store, &interfaces);
        if let Some((interface, module)) = compiled {
            public_interfaces.insert(
                uri.clone(),
                crate::interface::Interface::from_canonical(&interface),
            );
            solved.insert(interface.home.name, module);
            interfaces.insert(interface.home.name, interface);
        }
        all_warnings.extend(output.warnings);
        results.insert(output.uri, output.result);
    }

    let total = results.len();
    let success = results
        .values()
        .filter(|r| matches!(r, ModuleResult::Success { .. }))
        .count();

    let output = if total == success {
        Some(finish(Solved {
            store: &store,
            modules: solved.into_values().collect(),
        }))
    } else {
        None
    };
    (
        BuildResult {
            modules: results,
            interfaces: public_interfaces,
            total,
            success,
            failed: total - success,
            warnings: all_warnings,
        },
        output,
    )
}

#[cfg(test)]
fn build_sync(
    sources: Vec<(
        Url,
        Option<nash_config::PackageName>,
        Result<String, String>,
    )>,
) -> BuildResult {
    let known: Vec<_> = sources.iter().map(|(uri, _, _)| uri.clone()).collect();
    let edges = sources
        .iter()
        .map(|(uri, _, source)| {
            let dependencies = source
                .as_ref()
                .map_or_else(|_| vec![], |source| extract_imports(source, uri, &known));
            (uri.clone(), dependencies)
        })
        .collect();
    build_sync_with_edges(sources, &edges)
}

/// Fetch source content in dependency order, retaining failed reads in place.
async fn fetch_sources(
    db: &Arc<Mutex<Database>>,
    uris: &[&Url],
) -> Vec<(Url, Result<String, String>)> {
    // Database::source needs exclusive access across the read, so spawning
    // tasks cannot parallelize these reads and would lose dependency order.
    let mut db = db.lock().await;
    let mut results = Vec::with_capacity(uris.len());
    for &uri in uris {
        let source = db.source(uri).await.map(str::to_owned);
        results.push((uri.clone(), source.map_err(|e| e.to_string())));
    }
    results
}

/// Compile into the build arena, preserving the original canonical addresses.
/// Owned solved maps return alongside the borrowing interface.
fn compile_module<'s>(
    uri: &Url,
    package: Option<&nash_config::PackageName>,
    source: &Result<String, String>,
    store: &'s Bump,
    interfaces: &BTreeMap<&'s str, Interface<'s>>,
) -> (CompileOutput, Option<(Interface<'s>, SolvedModule<'s>)>) {
    let source = match source {
        Ok(source) => source,
        Err(message) => {
            return (
                CompileOutput {
                    uri: uri.clone(),
                    result: ModuleResult::SourceUnavailable {
                        message: message.clone(),
                    },
                    warnings: vec![],
                },
                None,
            );
        }
    };
    let path = uri.to_file_path().map_or_else(
        |_| uri.path().to_owned(),
        |path| path.to_string_lossy().into_owned(),
    );
    let expected_name = std::path::Path::new(&path)
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("Main");
    let source_view = nash_report::Source::new(source);
    let owned = |name: &str, reports: Vec<nash_report::Report>| {
        let mut reports = nash_report::ModuleReports {
            name: name.to_owned(),
            path: path.clone(),
            source: source.clone(),
            reports,
        };
        reports.sort();
        reports
    };
    let failed = |name: &str,
                  error: nash_report::ModuleError<'_>,
                  warnings: Vec<nash_report::ModuleReports>| {
        let mut reports = nash_report::to_reports(&source_view, expected_name, &error);
        reports.extend(warnings.into_iter().flat_map(|module| module.reports));
        (
            CompileOutput {
                uri: uri.clone(),
                result: ModuleResult::Failed(owned(name, reports)),
                warnings: vec![],
            },
            None,
        )
    };

    let bump = store;
    let src: &str = bump.alloc_str(source);
    let mut parser = nash_parse::Parser::new(bump, src);
    let module = match parser.module() {
        Ok(module) => module,
        Err(error) => {
            return failed(
                expected_name,
                nash_report::ModuleError::Syntax(nash_parse::error::Error::ParseError(
                    bump.alloc(error),
                )),
                vec![],
            );
        }
    };
    let name = module.name.map_or(expected_name, |name| name.value);
    // Default imports belong to Plan 12; localize exactly the imports in use.
    let localizer =
        nash_report::Localizer::from_module(&module, &[]).with_package(package.map(|package| {
            nash_ast::PackageName {
                author: bump.alloc_str(package.author()),
                project: bump.alloc_str(package.project()),
            }
        }));
    let context = nash_can::Context {
        package: package.map(|package| nash_ast::PackageName {
            author: bump.alloc_str(package.author()),
            project: bump.alloc_str(package.project()),
        }),
        interfaces: Some(interfaces),
    };
    let can_result = match nash_can::canonicalize(bump, context, &module) {
        Ok(can_result) => can_result,
        Err(errors) => return failed(name, nash_report::ModuleError::Names(errors), vec![]),
    };
    let warnings = if can_result.warnings.is_empty() {
        vec![]
    } else {
        vec![owned(
            name,
            can_result
                .warnings
                .iter()
                .map(nash_report::warning::to_report)
                .collect(),
        )]
    };
    let mut uf = nash_constrain::UnionFind::new();
    let module = &can_result.module;
    let (annotations, types) = match nash_solve::run(bump, &mut uf, module, &can_result.tables) {
        Ok(solved) => solved,
        Err(errors) => {
            return failed(
                name,
                nash_report::ModuleError::Types(localizer, errors),
                warnings,
            );
        }
    };
    if let Err(errors) = nash_nitpick::check(bump, &can_result.module) {
        return failed(name, nash_report::ModuleError::Patterns(errors), warnings);
    }
    let module = bump.alloc(can_result.module);
    let interface = nash_can::from_module(bump, module, &annotations);
    let solved = SolvedModule {
        uri: uri.clone(),
        tables: can_result.tables,
        module,
        annotations,
        types,
    };
    let output = CompileOutput {
        uri: uri.clone(),
        result: ModuleResult::Success {
            decl_count: count_decls(module.decls),
        },
        warnings,
    };
    (output, Some((interface, solved)))
}

fn count_decls(decls: &nash_ast::Decls<'_>) -> usize {
    match decls {
        nash_ast::Decls::Declare { next, .. } => 1 + count_decls(next),
        nash_ast::Decls::DeclareRec {
            following, next, ..
        } => 1 + following.len() + count_decls(next),
        nash_ast::Decls::Empty => 0,
    }
}

/// Build a dependency graph from parsed modules.
///
/// This is a simplified implementation that parses modules to extract imports.
/// For a full implementation, we would parse just the header/imports.
pub async fn build_graph(
    db: Arc<Mutex<Database>>,
    modules: &[Url],
) -> Result<DepGraph, DriverError> {
    let mut graph = DepGraph::new();

    for uri in modules {
        // Parse module to get imports
        let source = {
            let mut db = db.lock().await;
            db.source(uri).await.map(str::to_owned)
        };

        // Retain unreadable nodes: the build reports their I/O failure and
        // blocks dependents while continuing independent modules.
        let imports = source
            .as_ref()
            .map_or_else(|_| vec![], |source| extract_imports(source, uri, modules));
        graph.add_module(uri.clone(), imports);
    }

    graph.compute_order()?;
    Ok(graph)
}

/// Extract import URIs from source code.
///
/// This is a simplified implementation - in production we'd use the parser.
fn extract_imports(source: &str, current: &Url, known_modules: &[Url]) -> Vec<Url> {
    let mut imports = Vec::new();

    // Parse to get imports
    let bump = Bump::new();
    let src = bump.alloc_str(source);
    let mut parser = nash_parse::Parser::new(&bump, src);

    if let Ok(module) = parser.module() {
        for import in module.imports {
            let import_name = import.import.value;

            // Try to resolve import to a known module
            if let Some(uri) = resolve_import(import_name, current, known_modules) {
                imports.push(uri);
            }
        }
    }

    imports
}

/// Resolve an import name to a module URI.
///
/// This is a simplified implementation. Full resolution would handle:
/// - Package dependencies
/// - Source directory structure
/// - Module naming conventions
fn resolve_import(name: &str, _current: &Url, known_modules: &[Url]) -> Option<Url> {
    // Convert module name to file path pattern
    // e.g., "Json.Decode" -> "Json/Decode.nash"
    let path_pattern = format!("{}.nash", name.replace('.', "/"));

    // Find matching module
    known_modules
        .iter()
        .find(|uri| uri.path().ends_with(&path_pattern))
        .cloned()
}

#[cfg(test)]
fn report_text(reports: &nash_report::ModuleReports) -> String {
    let source = nash_report::Source::new(&reports.source);
    reports
        .reports
        .iter()
        .map(|report| nash_report::render_plain(report, &source, &reports.path))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::InMemorySource;

    fn url(path: &str) -> Url {
        Url::parse(&format!("file:///{}", path)).unwrap()
    }

    #[test]
    fn retained_module_nodes_match_solved_evidence_after_compilation() {
        let store = Bump::new();
        let mut interfaces = BTreeMap::new();
        let (output, compiled) = compile_module(
            &url("Base.nash"), None,
            &Ok("module Base exposing (..)\ntrait Keep 'a where keep : 'a -> 'a\nidentity x = keep x\n".to_owned()),
            &store, &interfaces,
        );
        assert!(matches!(output.result, ModuleResult::Success { .. }));
        let (interface, base) = compiled.unwrap();
        interfaces.insert(interface.home.name, interface);
        let (output, compiled) = compile_module(
            &url("Main.nash"),
            None,
            &Ok("module Main exposing (..)\nimport Base\nforward x = Base.identity x\n".to_owned()),
            &store,
            &interfaces,
        );
        assert!(
            matches!(output.result, ModuleResult::Success { .. }),
            "{:?}",
            output.result
        );
        let (_, main) = compiled.unwrap();
        for solved in [&base, &main] {
            let nash_ast::Decls::Declare { definition, .. } = solved.module.decls else {
                panic!("expected definition")
            };
            let nash_ast::Def::Def { name, body, .. } = definition else {
                panic!("expected inferred definition")
            };
            let nash_ast::Expr::Call { function, .. } = body.value else {
                panic!("expected call")
            };
            let scheme = &solved.types.schemes[&nash_ast::NodeId::def(name)];
            assert!(std::ptr::eq(
                scheme.annotation,
                solved.annotations[name.value]
            ));
            let instance = &solved.types.instances[&nash_ast::NodeId::expr(function)];
            let [nash_ast::Evidence::Given { binder, index }] = instance.evidence else {
                panic!("expected retained trait evidence")
            };
            assert_eq!(*binder, scheme.binder);
            assert_eq!(*index, 0);
            assert_eq!(
                scheme.annotation.context[0].trait_ref().unwrap().home.name,
                "Base"
            );
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn source_fetch_preserves_dependency_order_and_errors() {
        let mem = InMemorySource::new();
        let uris: Vec<_> = (0..128).map(|i| url(&format!("Module{i}.nash"))).collect();
        for (index, uri) in uris.iter().enumerate() {
            if index != 63 {
                mem.insert(uri.clone(), format!("source {index}"));
            }
        }
        let db = Arc::new(Mutex::new(Database::new(mem)));
        let ordered: Vec<_> = uris.iter().collect();
        let sources = fetch_sources(&db, &ordered).await;
        assert_eq!(sources.len(), uris.len());
        for (index, (uri, source)) in sources.iter().enumerate() {
            assert_eq!(uri, &uris[index], "source moved out of dependency order");
            if index == 63 {
                assert!(source.is_err(), "missing source must retain its position");
            } else {
                assert_eq!(source.as_ref().unwrap(), &format!("source {index}"));
            }
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn reverse_input_dependency_chain_builds_all_interfaces() {
        let mem = InMemorySource::new();
        let modules = [url("Main.nash"), url("Middle.nash"), url("Base.nash")];
        let sources = [
            "module Main exposing (value)\nimport Middle exposing (identity)\nvalue = identity ()\n",
            "module Middle exposing (identity)\nimport Base\nidentity x = Base.identity x\n",
            "module Base exposing (identity)\nidentity x = x\n",
        ];
        for (uri, source) in modules.iter().zip(sources) {
            mem.insert(uri.clone(), source.to_owned());
        }
        let db = Arc::new(Mutex::new(Database::new(mem)));
        let graph = build_graph(db.clone(), &modules).await.unwrap();
        let result = build(
            db,
            &graph,
            &graph.order.iter().cloned().map(|uri| (uri, None)).collect(),
        )
        .await;
        assert_eq!(result.total, 3);
        assert!(result.is_success(), "{result:?}");
        for uri in &modules {
            assert!(
                result.interfaces.contains_key(uri),
                "missing interface: {uri}"
            );
        }
    }

    #[tokio::test]
    async fn test_compile_invalid_module() {
        let mem = InMemorySource::new();
        let uri = url("Bad.nash");
        mem.insert(
            uri.clone(),
            "this is not valid nash syntax {{{{".to_string(),
        );

        let db = Arc::new(Mutex::new(Database::new(mem)));
        let modules = vec![uri];
        let graph = build_graph(db.clone(), &modules).await.unwrap();
        let result = build(
            db,
            &graph,
            &graph.order.iter().cloned().map(|uri| (uri, None)).collect(),
        )
        .await;

        assert_eq!(result.total, 1);
        assert_eq!(result.failed, 1);
    }

    #[tokio::test]
    async fn test_cross_module_type_error() {
        let mem = InMemorySource::new();

        mem.insert(
            url("Utils.nash"),
            r#"
module Utils exposing (..)

helper = 1
"#
            .to_string(),
        );

        mem.insert(
            url("Main.nash"),
            r#"
module Main exposing (..)

import Utils

main = Utils.helper "not a function argument"
"#
            .to_string(),
        );

        let db = Arc::new(Mutex::new(Database::new(mem)));
        let modules = vec![url("Utils.nash"), url("Main.nash")];

        let graph = build_graph(db.clone(), &modules).await.unwrap();
        let result = build(
            db,
            &graph,
            &graph.order.iter().cloned().map(|uri| (uri, None)).collect(),
        )
        .await;

        // Utils.helper is a number, not a function: Main gets a type
        // error against the imported annotation.
        assert_eq!(result.total, 2);
        assert_eq!(result.success, 1);
        assert_eq!(result.failed, 1);
        assert!(matches!(
            result.modules[&url("Main.nash")],
            ModuleResult::Failed(_)
        ));
    }

    /// Unannotated mutually recursive exports used from another module:
    /// Elm 0.19.1 crashes on this exact shape ("Map.!: given key is not an
    /// element in the map") because `getVarNames`' visit marks persist
    /// across `toAnnotation` calls, leaving `pong`'s `Forall` empty. Nash
    /// deliberately fixes that (see `nash-solve/src/annotation.rs`).
    #[tokio::test]
    async fn test_cross_module_mutual_recursion() {
        let mem = InMemorySource::new();

        mem.insert(
            url("Utils.nash"),
            r#"
module Utils exposing (..)

ping x = pong x

pong x = ping x
"#
            .to_string(),
        );

        mem.insert(
            url("Main.nash"),
            r#"
module Main exposing (..)

import Utils

main = Utils.pong ()
"#
            .to_string(),
        );

        let db = Arc::new(Mutex::new(Database::new(mem)));
        let modules = vec![url("Utils.nash"), url("Main.nash")];

        let graph = build_graph(db.clone(), &modules).await.unwrap();
        let result = build(
            db,
            &graph,
            &graph.order.iter().cloned().map(|uri| (uri, None)).collect(),
        )
        .await;

        assert_eq!(result.total, 2);
        assert_eq!(result.success, 2, "{result:?}");
        assert!(result.is_success());
    }
}

#[cfg(test)]
mod trait_tests {
    use super::*;
    use crate::source::InMemorySource;

    async fn compile_sources(sources: &[(&str, &str)]) -> BuildResult {
        let mem = InMemorySource::new();
        let mut modules = Vec::new();
        for (name, source) in sources {
            let uri = Url::parse(&format!("file:///{name}.nash")).unwrap();
            mem.insert(uri.clone(), (*source).to_owned());
            modules.push(uri);
        }
        let db = Arc::new(Mutex::new(Database::new(mem)));
        let graph = build_graph(db.clone(), &modules).await.unwrap();
        build(
            db,
            &graph,
            &graph.order.iter().cloned().map(|uri| (uri, None)).collect(),
        )
        .await
    }

    #[tokio::test]
    async fn trait_impls_resolve_in_direct_and_transitive_consumers() {
        let result = compile_sources(&[
            ("Transitive", "module Transitive exposing (..)\nimport Types exposing (Token(..), forward)\nvalue = forward Token\nunit = forward ()\n"),
            ("Main", "module Main exposing (..)\nimport Methods exposing (Keep)\nimport Types exposing (Token(..))\nvalue = keep Token\nunit = keep ()\n"),
            ("Types", "module Types exposing (Token(..), forward)\nimport Methods exposing (Keep)\ntype Token = Token\nimpl Keep Token where\n    keep x = x\nforward x = keep x\n"),
            ("Methods", "module Methods exposing (Keep)\ntrait Keep 'a where\n    keep : 'a -> 'a\nimpl Keep () where\n    keep x = x\n"),
        ]).await;
        assert!(result.is_success(), "{result:?}");
        assert_eq!(result.success, 4);
        assert_eq!(result.interfaces.len(), 4);
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    }

    #[tokio::test]
    async fn driver_reports_orphan_and_overlap_at_the_impl_module() {
        let mut diagnostics = Vec::new();
        for (case, bad, expected) in [
            (
                "orphan",
                "module Bad exposing (..)\nimport Methods exposing (Keep)\nimport Types exposing (Token)\nimpl Keep Token where\n    keep x = x\n",
                "nash::names::orphan_impl",
            ),
            (
                "overlap",
                "module Bad exposing (..)\ntrait Keep 'a where\n    keep : 'a -> 'a\nimpl Keep () where\n    keep x = x\nimpl Keep () where\n    keep x = x\n",
                "nash::names::overlapping_impls",
            ),
        ] {
            let result = compile_sources(&[
                ("Bad", bad),
                (
                    "Types",
                    "module Types exposing (Token(..))\ntype Token = Token\n",
                ),
                (
                    "Methods",
                    "module Methods exposing (Keep)\ntrait Keep 'a where\n    keep : 'a -> 'a\n",
                ),
            ])
            .await;
            assert_eq!(result.success, 2, "{result:?}");
            assert_eq!(result.failed, 1, "{result:?}");
            let ModuleResult::Failed(reports) =
                &result.modules[&Url::parse("file:///Bad.nash").unwrap()]
            else {
                panic!("impl module must fail")
            };
            let message = report_text(reports);
            assert!(message.contains(expected), "{message}");
            diagnostics.push(format!("{case}: {message}"));
        }
        insta::assert_snapshot!(diagnostics.join("\n"));
    }
}

#[cfg(test)]
mod kind_tests {
    use super::*;
    use crate::source::InMemorySource;

    async fn compile_pair(producer: &str, consumer: &str) -> BuildResult {
        let mem = InMemorySource::new();
        let types = Url::parse("file:///Types.nash").unwrap();
        let main = Url::parse("file:///Main.nash").unwrap();
        mem.insert(types.clone(), producer.to_owned());
        mem.insert(main.clone(), consumer.to_owned());
        let db = Arc::new(Mutex::new(Database::new(mem)));
        let graph = build_graph(db.clone(), &[main, types]).await.unwrap();
        build(
            db,
            &graph,
            &graph.order.iter().cloned().map(|uri| (uri, None)).collect(),
        )
        .await
    }

    #[tokio::test]
    async fn labeled_ctor_imports_preserve_sugar_and_projection() {
        let result = compile_pair(
            "module Types exposing (type box(..))\ntype box 'a = Box { z : 'a, a : unit }\n",
            "module Main exposing (..)\nimport Types\nmake x = Types.Box { a = (), z = x }\nget : Types.box 'a -> 'a\nget x = x.z\npattern (Types.Box { z }) = z\n",
        ).await;
        assert_eq!(result.success, 2, "{result:?}");
    }

    #[tokio::test]
    async fn labeled_ctor_closed_exports_hide_projection() {
        let result = compile_pair(
            "module Types exposing (type box)\ntype box 'a = Box { value : 'a }\n",
            "module Main exposing (..)\nimport Types\nget : Types.box 'a -> 'a\nget x = x.value\n",
        )
        .await;
        assert_eq!(result.success, 1, "{result:?}");
        let ModuleResult::Failed(reports) =
            &result.modules[&Url::parse("file:///Main.nash").unwrap()]
        else {
            panic!("consumer must reject hidden label")
        };
        let message = report_text(reports);
        assert!(message.contains("nash::type::not_a_record"), "{message}");
    }

    #[tokio::test]
    async fn labeled_ctor_import_rejects_updates() {
        let result = compile_pair(
            "module Types exposing (type box(..))\ntype box = Box { value : unit }\n",
            "module Main exposing (..)\nimport Types\nchange : Types.box -> Types.box\nchange x = { x | value = () }\n",
        ).await;
        assert_eq!(result.success, 1, "{result:?}");
        let ModuleResult::Failed(reports) =
            &result.modules[&Url::parse("file:///Main.nash").unwrap()]
        else {
            panic!("consumer must reject union update")
        };
        let message = report_text(reports);
        assert!(
            message.contains("nash::type::update_not_record"),
            "{message}"
        );
    }

    #[tokio::test]
    async fn labeled_ctor_private_type_returned_by_export_has_no_projection() {
        let result = compile_pair(
            "module Types exposing (make)\ntype box = Box { value : unit }\nmake = Box ()\n",
            "module Main exposing (..)\nimport Types\nbad = Types.make.value\n",
        )
        .await;
        assert_eq!(result.success, 1, "{result:?}");
        let ModuleResult::Failed(reports) =
            &result.modules[&Url::parse("file:///Main.nash").unwrap()]
        else {
            panic!("private constructor labels must remain hidden")
        };
        let message = report_text(reports);
        assert!(message.contains("nash::type::not_a_record"), "{message}");
    }

    #[tokio::test]
    async fn labeled_ctor_imported_higher_kinded_projection() {
        let producer = "module Types exposing (type holder(..), get)\ntype holder 'f 'a = Holder { value : 'f 'a }\nget : holder 'f 'a -> 'f 'a\nget h = h.value\n";
        let positive = compile_pair(
            producer,
            "module Main exposing (..)\nimport Types\ntype option 'a = Some 'a\nlist = Types.get (Types.Holder [()])\nterm = Types.get (Types.Holder (Some ()))\n",
        ).await;
        assert_eq!(positive.success, 2, "{positive:?}");
        let negative = compile_pair(
            producer,
            "module Main exposing (..)\nimport Types\nmake x = Types.Holder [x]\nbad = Types.get (make (\\x -> x))\n",
        ).await;
        assert_eq!(negative.success, 1, "{negative:?}");
        assert_eq!(negative.failed, 1, "{negative:?}");
    }

    #[tokio::test]
    async fn labeled_ctor_imported_captured_parameter_stays_shared() {
        let producer = "module Types exposing (type box(..))\ntype box 'a = Box { value : 'a }\n";
        let positive = compile_pair(
            producer,
            "module Main exposing (..)\nimport Types\ngood : Types.box 'a -> ('a, 'a)\ngood b =\n    let\n        get ignored = b.value\n    in\n    (get (), get ())\n",
        ).await;
        assert_eq!(positive.success, 2, "{positive:?}");
        let negative = compile_pair(
            producer,
            "module Main exposing (..)\nimport Types\nbad : Types.box 'a -> ('a, unit)\nbad b =\n    let\n        get ignored = b.value\n    in\n    (get (), get ())\n",
        ).await;
        assert_eq!(negative.success, 1, "{negative:?}");
        assert_eq!(negative.failed, 1, "{negative:?}");
    }

    #[tokio::test]
    async fn nominal_record_imports_preserve_identity_and_field_order() {
        let result = compile_pair(
            "module Types exposing (type point)\ntype alias point = { z : unit, a : unit }\n",
            "module Main exposing (..)\nimport Types\nvalue = { a = (), z = () }\nget : Types.point -> unit\nget r = r.z\nconstructed = Types.point () ()\n",
        ).await;
        assert_eq!(result.success, 2, "{result:?}");
        assert!(result.is_success());
    }

    #[tokio::test]
    async fn nominal_record_import_rejects_identical_foreign_fields() {
        let result = compile_pair(
            "module Types exposing (type point)\ntype alias point = { x : unit }\n",
            "module Main exposing (..)\nimport Types\ntype alias local = { x : unit }\nf : Types.point -> local\nf r = r\n",
        ).await;
        assert_eq!(result.success, 1, "{result:?}");
        assert_eq!(result.failed, 1, "{result:?}");
    }

    #[tokio::test]
    async fn nominal_record_literal_deduplicates_exposed_and_qualified_alias() {
        let result = compile_pair(
            "module Types exposing (type point)\ntype alias point = { x : unit }\n",
            "module Main exposing (..)\nimport Types exposing (type point)\nvalue = { x = () }\n",
        )
        .await;
        assert_eq!(result.success, 2, "{result:?}");
    }

    #[tokio::test]
    async fn producer_rejects_ternary_self_application_with_an_infinite_kind() {
        let result = compile_pair(
            "module Types exposing (type s)\ntype s 'f 'g 'a = S ('g ('f 'f 'a))\n",
            "module Main exposing (..)\nimport Types exposing (type s)\ntype w = W (s s s)\n",
        )
        .await;
        assert_eq!(result.success, 0, "{result:?}");
        assert_eq!(result.failed, 2, "{result:?}");
        let ModuleResult::Failed(reports) =
            &result.modules[&Url::parse("file:///Types.nash").unwrap()]
        else {
            panic!("producer must report a kind error");
        };
        let message = report_text(reports);
        assert!(message.contains("nash::names::kind_infinite"), "{message}");
        assert!(message.contains("infinite kind"), "{message}");
    }

    #[tokio::test]
    async fn producer_rejects_self_application_before_consumer_checking() {
        for field in ["a a", "local a"] {
            let result = compile_pair(
                "module Types exposing (type a)\ntype a 'f = A ('f 'f)\n",
                &format!("module Main exposing (..)\nimport Types exposing (type a)\ntype local 'f = Local ('f 'f)\ntype b = B ({field})\n"),
            ).await;
            assert_eq!(result.success, 0, "{result:?}");
            assert_eq!(result.failed, 2, "{result:?}");
            let ModuleResult::Failed(reports) =
                &result.modules[&Url::parse("file:///Types.nash").unwrap()]
            else {
                panic!("producer must fail")
            };
            let message = report_text(reports);
            assert!(message.contains("nash::names::kind_infinite"), "{message}");
        }
    }

    #[tokio::test]
    async fn concrete_consumer_cannot_hide_an_infinite_producer_kind() {
        let result = compile_pair(
            "module Types exposing (type s)\ntype s 'f 'g 'a = S ('g ('f 'f 'a))\n",
            "module Main exposing (..)\nimport Types exposing (type s)\ntype tag 'a = Tag\ntype w = W (s s tag tag)\n",
        ).await;
        assert_eq!(result.success, 0, "{result:?}");
        assert_eq!(result.failed, 2, "{result:?}");
    }

    #[tokio::test]
    async fn imported_partial_heads_enforce_supplied_and_remaining_contexts() {
        let producer = "module Types exposing (type wrap)\nimport Builtin exposing (..)\ntype wrap 'f 'a = Wrap ('f 'a)\n";
        for (field, succeeds) in [
            ("wrap (pair int) bytes", true),
            ("wrap (pair (option int)) bytes", false),
            ("wrap (pair int) (option int)", false),
        ] {
            let result = compile_pair(
                producer,
                &format!("module Main exposing (..)\nimport Builtin exposing (..)\nimport Types exposing (type wrap)\ntype option 'a = None | Some 'a\ntype use = Use ({field})\n"),
            ).await;
            assert_eq!(result.success, if succeeds { 2 } else { 1 }, "{result:?}");
            assert_eq!(result.failed, usize::from(!succeeds), "{result:?}");
            if !succeeds {
                let ModuleResult::Failed(reports) =
                    &result.modules[&Url::parse("file:///Main.nash").unwrap()]
                else {
                    panic!("consumer must reject the supplied Term argument")
                };
                let message = report_text(reports);
                assert!(
                    message.contains("nash::names::representation_mismatch")
                        && message.contains("Storable"),
                    "{message}"
                );
            }
        }
    }

    #[tokio::test]
    async fn imported_big_alias_is_a_valid_list_element() {
        let result = compile_pair(
            "module Types exposing (Item)\n\nimport Builtin exposing (..)\n\ntype alias Item = Int\n",
            "module Main exposing (..)\n\nimport Builtin exposing (..)\nimport Types\n\ntype alias items = list Types.Item\n",
        ).await;
        assert_eq!(result.success, 2, "{result:?}");
    }

    #[tokio::test]
    async fn imported_term_alias_is_rejected_as_a_list_element() {
        let result = compile_pair(
            "module Types exposing (type item)\n\nimport Builtin exposing (..)\n\ntype alias item = unit -> unit\n",
            "module Main exposing (..)\n\nimport Builtin exposing (..)\nimport Types exposing (type item)\n\ntype alias items = list item\n",
        ).await;
        assert_eq!(result.success, 1, "{result:?}");
        let ModuleResult::Failed(reports) =
            &result.modules[&Url::parse("file:///Main.nash").unwrap()]
        else {
            panic!("invalid consumer compiled");
        };
        let message = report_text(reports);
        assert!(
            message.contains("nash::names::representation_mismatch")
                && message.contains("Storable"),
            "{message}"
        );
        assert!(
            message.contains("Big or Const") && message.contains("Term"),
            "{message}"
        );
    }

    #[tokio::test]
    async fn application_context_argument_order_changes_the_interface_fingerprint() {
        let consumer = "module Main exposing (..)\nimport Types exposing (type app)\n";
        let forward = compile_pair(
            "module Types exposing (type app)\ntype app 'f 'a 'b = App ('f 'a 'b)\n",
            consumer,
        )
        .await;
        let reverse = compile_pair(
            "module Types exposing (type app)\ntype app 'f 'a 'b = App ('f 'b 'a)\n",
            consumer,
        )
        .await;
        assert_eq!(forward.success, 2, "{forward:?}");
        assert_eq!(reverse.success, 2, "{reverse:?}");
        let uri = Url::parse("file:///Types.nash").unwrap();
        assert!(forward.interfaces[&uri].differs_from(&reverse.interfaces[&uri]));
    }

    #[tokio::test]
    async fn exported_alias_representation_changes_the_interface_fingerprint() {
        let big = compile_pair("module Types exposing (type item)\n\nimport Builtin exposing (..)\n\ntype alias item = int\n", "module Main exposing (..)\n\nimport Types exposing (type item)\n\nf : item -> item\nf x = x\n").await;
        let term = compile_pair("module Types exposing (type item)\n\nimport Builtin exposing (..)\n\ntype alias item = unit -> unit\n", "module Main exposing (..)\n\nimport Types exposing (type item)\n\nf : item -> item\nf x = x\n").await;
        assert_eq!(big.success, 2, "{big:?}");
        assert_eq!(term.success, 2, "{term:?}");
        let uri = Url::parse("file:///Types.nash").unwrap();
        assert!(big.interfaces[&uri].differs_from(&term.interfaces[&uri]));
    }
    #[tokio::test]
    async fn exported_datatype_context_changes_the_interface_fingerprint() {
        let consumer = "module Main exposing (..)\n\nimport Types exposing (type box)\n";
        let any = compile_pair("module Types exposing (type box)\n\nimport Builtin exposing (..)\n\ntype box 'a = Box 'a\n", consumer).await;
        let storable = compile_pair("module Types exposing (type box)\n\nimport Builtin exposing (..)\n\ntype box 'a = Box (list 'a)\n", consumer).await;
        assert_eq!(any.success, 2, "{any:?}");
        assert_eq!(storable.success, 2, "{storable:?}");
        let uri = Url::parse("file:///Types.nash").unwrap();
        assert!(any.interfaces[&uri].differs_from(&storable.interfaces[&uri]));
        let crate::interface::Export::Type { kind, .. } = &storable.interfaces[&uri].exports[0]
        else {
            panic!("missing type export");
        };
        assert_eq!(kind, "Type -> Type");
        let crate::interface::Export::Type {
            kind: unrestricted_kind,
            ..
        } = &any.interfaces[&uri].exports[0]
        else {
            panic!("missing type export")
        };
        assert_eq!(
            kind, unrestricted_kind,
            "representation context changes do not change H98 kinds"
        );
    }
}
