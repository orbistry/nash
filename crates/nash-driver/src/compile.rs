//! Module compilation orchestration.
//!
//! Each module runs Elm's full pipeline: parse -> canonicalize ->
//! constrain -> solve -> `Interface::from_module` with the solver's
//! annotations. Modules compile in dependency order, and each solved
//! module's interface is deep-copied into a build-wide arena so dependents
//! canonicalize their imports against it — interfaces only ever exist for
//! type-solved modules.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use bumpalo::Bump;
use nash_can::Interface;
use tokio::sync::Mutex;
use url::Url;

use crate::database::Database;
use crate::error::DriverError;
use crate::graph::DepGraph;

/// Result of compiling a single module.
#[derive(Debug)]
pub enum ModuleResult {
    /// Module compiled successfully.
    Success {
        /// Number of declarations in the module.
        decl_count: usize,
    },
    /// Module failed to compile.
    Failed {
        /// Parse or other error message.
        message: String,
    },
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
    pub warnings: Vec<String>,
}

impl BuildResult {
    /// Check if the build was completely successful.
    pub fn is_success(&self) -> bool {
        self.failed == 0
    }
}

/// Holds the output of compiling a single module.
struct CompileOutput {
    uri: Url,
    result: ModuleResult,
    warnings: Vec<String>,
}

/// Compile all modules through the full pipeline, in dependency order.
///
/// The async part only fetches sources; the CPU-bound compilation runs on
/// tokio's blocking pool (`spawn_blocking`) so no executor worker is ever
/// stalled.
pub async fn build(db: Arc<Mutex<Database>>, graph: &DepGraph) -> BuildResult {
    let modules: Vec<&Url> = graph.levels().into_iter().flatten().collect();
    let sources = fetch_sources(&db, &modules).await;

    tokio::task::spawn_blocking(move || build_sync(sources))
        .await
        .expect("compile task panicked")
}

/// Compile modules one at a time in dependency order, threading each
/// solved module's interface to its dependents through a build-wide arena.
///
/// Type checking is inherently dependency-ordered, so within-build
/// compilation is sequential within a build.
fn build_sync(sources: Vec<(Url, Result<String, String>)>) -> BuildResult {
    let store = Bump::new();
    let mut interfaces = BTreeMap::from([("Builtin", nash_can::kinds::builtin_interface(&store))]);
    let mut public_interfaces = HashMap::new();

    let mut results: HashMap<Url, ModuleResult> = HashMap::new();
    let mut all_warnings: Vec<String> = Vec::new();

    for (uri, source) in &sources {
        let (output, interface) = compile_module(uri, source, &store, &interfaces);
        if let Some(interface) = interface {
            public_interfaces.insert(
                uri.clone(),
                crate::interface::Interface::from_canonical(&interface),
            );
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

    BuildResult {
        modules: results,
        interfaces: public_interfaces,
        total,
        success,
        failed: total - success,
        warnings: all_warnings,
    }
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

/// Run one module through the full pipeline in its own arena:
/// parse -> canonicalize -> constrain -> solve -> interface.
///
/// On success the module's interface is deep-copied into the build-wide
/// `store` arena so it outlives this module's arena.
fn compile_module<'s>(
    uri: &Url,
    source: &Result<String, String>,
    store: &'s Bump,
    interfaces: &BTreeMap<&'s str, Interface<'s>>,
) -> (CompileOutput, Option<Interface<'s>>) {
    let failed = |message: String| {
        (
            CompileOutput {
                uri: uri.clone(),
                result: ModuleResult::Failed { message },
                warnings: vec![],
            },
            None,
        )
    };

    let source = match source {
        Ok(s) => s,
        Err(e) => return failed(e.clone()),
    };

    let bump = Bump::new();
    let src: &str = bump.alloc_str(source);
    let mut parser = nash_parse::Parser::new(&bump, src.as_bytes());

    let module = match parser.module() {
        Ok(module) => module,
        Err(e) => return failed(format!("{:?}", e)),
    };

    let context = nash_can::Context {
        package: None,
        interfaces: Some(interfaces),
    };
    let can_result = match nash_can::canonicalize(&bump, context, &module) {
        Ok(can_result) => can_result,
        Err(errors) => return failed(format!("{:?}", errors)),
    };
    let warnings: Vec<String> = can_result
        .warnings
        .iter()
        .map(|w| format!("{:?}", w))
        .collect();

    let mut uf = nash_constrain::UnionFind::new();
    let constraint = nash_constrain::constrain(&bump, &mut uf, &can_result.module);
    let annotations = match nash_solve::run(&bump, &mut uf, &constraint) {
        Ok(annotations) => annotations,
        Err(errors) => return failed(format!("{:?}", errors)),
    };

    let interface = nash_can::from_module(&bump, &can_result.module, &annotations);
    let stored = nash_can::deep_copy_interface(store, &interface);

    (
        CompileOutput {
            uri: uri.clone(),
            result: ModuleResult::Success {
                decl_count: count_decls(can_result.module.decls),
            },
            warnings,
        },
        Some(stored),
    )
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
            db.source(uri).await?.to_string()
        };

        let imports = extract_imports(&source, uri, modules);
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
    let mut parser = nash_parse::Parser::new(&bump, src.as_bytes());

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
mod tests {
    use super::*;
    use crate::source::InMemorySource;

    fn url(path: &str) -> Url {
        Url::parse(&format!("file:///{}", path)).unwrap()
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
        let result = build(db, &graph).await;
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
        let result = build(db, &graph).await;

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
        let result = build(db, &graph).await;

        // Utils.helper is a number, not a function: Main gets a type
        // error against the imported annotation.
        assert_eq!(result.total, 2);
        assert_eq!(result.success, 1);
        assert_eq!(result.failed, 1);
        assert!(matches!(
            result.modules[&url("Main.nash")],
            ModuleResult::Failed { .. }
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

main = Utils.pong 1
"#
            .to_string(),
        );

        let db = Arc::new(Mutex::new(Database::new(mem)));
        let modules = vec![url("Utils.nash"), url("Main.nash")];

        let graph = build_graph(db.clone(), &modules).await.unwrap();
        let result = build(db, &graph).await;

        assert_eq!(result.total, 2);
        assert_eq!(result.success, 2);
        assert!(result.is_success());
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
        build(db, &graph).await
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
        let ModuleResult::Failed { message } =
            &result.modules[&Url::parse("file:///Main.nash").unwrap()]
        else {
            panic!("invalid consumer compiled");
        };
        assert!(message.contains("KindMismatch"), "{message}");
    }

    #[tokio::test]
    async fn real_export_kind_changes_the_build_interface_fingerprint() {
        let big = compile_pair("module Types exposing (type item)\n\nimport Builtin exposing (..)\n\ntype alias item = int\n", "module Main exposing (..)\n\nimport Types exposing (type item)\n\nf : item -> item\nf x = x\n").await;
        let term = compile_pair("module Types exposing (type item)\n\nimport Builtin exposing (..)\n\ntype alias item = unit -> unit\n", "module Main exposing (..)\n\nimport Types exposing (type item)\n\nf : item -> item\nf x = x\n").await;
        assert_eq!(big.success, 2, "{big:?}");
        assert_eq!(term.success, 2, "{term:?}");
        let uri = Url::parse("file:///Types.nash").unwrap();
        assert!(big.interfaces[&uri].differs_from(&term.interfaces[&uri]));
    }
    #[tokio::test]
    async fn real_export_bound_changes_the_build_interface_fingerprint() {
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
        assert_eq!(kind, "forall k0:{Big,Const}. k0 -> Term");
    }
}
