use std::path::Path;
use std::sync::Arc;

use nash_driver::{
    BuildResult, Database, Export, FRONTENDS, FileSystemSource, InMemorySource, ModuleCatalog,
    ModuleResult, Project, SourceSpec, build, build_graph, build_with,
};
use tokio::sync::Mutex;
use url::Url;

const ADD_ONE: &str = include_str!("fixtures/aiken/supported/src/add_one.ak");

fn memory_sources(files: &[(&str, &str)]) -> (Arc<Mutex<Database>>, ModuleCatalog) {
    let memory = InMemorySource::new();
    let mut catalog = ModuleCatalog::new();
    for (path, source) in files {
        let uri = Url::parse(&format!("file:///project/src/{path}")).unwrap();
        memory.insert(uri.clone(), (*source).to_owned());
        let spec = SourceSpec::new(&uri, Path::new("/project/src"), None).unwrap();
        catalog.insert(uri, spec);
    }
    (Arc::new(Mutex::new(Database::new(memory))), catalog)
}

fn uri(path: &str) -> Url {
    Url::parse(&format!("file:///project/src/{path}")).unwrap()
}

async fn compile(files: &[(&str, &str)]) -> BuildResult {
    let (db, catalog) = memory_sources(files);
    let graph = build_graph(db.clone(), &catalog).await.unwrap();
    build(db, &graph, &catalog).await
}

#[tokio::test]
async fn native_nested_module_and_validator_headers_keep_their_identity() {
    let result = compile(&[
        ("Nested/Identity.nash", "module Nested.Identity exposing (identity)\nidentity x = x\n"),
        ("Main.nash", "validator module Main exposing (main)\nimport Nested.Identity exposing (identity)\nmain = identity ()\n"),
    ]).await;
    assert!(result.is_success(), "{result:?}");
    assert_eq!(
        result.interfaces[&uri("Nested/Identity.nash")].module_name,
        "Nested.Identity"
    );
    assert_eq!(
        result.interfaces[&uri("Main.nash")].exports,
        [Export::Value {
            name: "main".into()
        }]
    );
}

#[tokio::test]
async fn explicit_selection_overrides_extension_without_bypassing_native_validation() {
    let (db, mut catalog) = memory_sources(&[(
        "Identity.ak",
        "module Identity exposing (identity)\nidentity x = x\n",
    )]);
    assert_eq!(
        FRONTENDS
            .select(&uri("Identity.ak"), None)
            .unwrap()
            .descriptor()
            .id,
        "aiken"
    );
    assert_eq!(
        FRONTENDS
            .select(&uri("Identity.nash"), None)
            .unwrap()
            .descriptor()
            .id,
        "nash"
    );
    assert!(FRONTENDS.select(&uri("Identity.txt"), None).is_err());
    catalog.get_mut(&uri("Identity.ak")).unwrap().frontend = Some("nash".into());
    let graph = build_graph(db.clone(), &catalog).await.unwrap();
    let result = build(db, &graph, &catalog).await;
    assert!(result.is_success(), "{result:?}");
    assert_eq!(
        result.interfaces[&uri("Identity.ak")].exports,
        [Export::Value {
            name: "identity".into()
        }]
    );

    let missing_header = compile(&[("Missing.nash", "identity x = x\n")]).await;
    let ModuleResult::Failed(reports) = &missing_header.modules[&uri("Missing.nash")] else {
        panic!("missing native header must fail: {missing_header:?}")
    };
    assert!(
        reports
            .reports
            .iter()
            .any(|report| report.code == "nash::syntax::missing_module_name")
    );
    assert!(missing_header.interfaces.is_empty());
}

#[tokio::test]
async fn real_aiken_project_publishes_an_interface_and_consumers_check_its_type() {
    let project =
        Project::load(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/aiken/supported"))
            .await
            .unwrap();
    let db = Arc::new(Mutex::new(Database::new(FileSystemSource::new())));
    let catalog = project.discover_modules(&*db.lock().await).await.unwrap();
    let graph = build_graph(db.clone(), &catalog).await.unwrap();
    let result = build(db, &graph, &catalog).await;
    assert!(result.is_success(), "{result:?}");
    let source = catalog.keys().next().unwrap();
    assert_eq!(result.interfaces[source].module_name, "add_one");
    assert_eq!(
        result.interfaces[source].exports,
        [Export::Value {
            name: "add_one".into()
        }]
    );

    let good = compile(&[
        ("folder/math.ak", ADD_ONE),
        (
            "consumer.ak",
            "use folder/math\npub fn increment(value: Int) -> Int { math.add_one(value) }\n",
        ),
    ])
    .await;
    assert!(good.is_success(), "{good:?}");
    assert_eq!(
        good.interfaces[&uri("folder/math.ak")].module_name,
        "folder.math"
    );
    let wrong = compile(&[
        ("folder/math.ak", ADD_ONE),
        (
            "consumer.ak",
            "use folder/math\npub fn increment(value: ByteArray) -> Int { math.add_one(value) }\n",
        ),
    ])
    .await;
    assert!(matches!(
        wrong.modules[&uri("folder/math.ak")],
        ModuleResult::Success { .. }
    ));
    let ModuleResult::Failed(reports) = &wrong.modules[&uri("consumer.ak")] else {
        panic!("imported Int argument must be checked: {wrong:?}")
    };
    assert!(
        reports
            .reports
            .iter()
            .any(|report| report.code.starts_with("nash::type::")),
        "{reports:?}"
    );
}

#[tokio::test]
async fn invalid_aiken_tool_declaration_keeps_a_located_diagnostic() {
    let result = compile(&[("invalid.ak", "test rejected() { 42 }\n")]).await;
    let ModuleResult::Failed(reports) = &result.modules[&uri("invalid.ak")] else {
        panic!("invalid tool declarations must not publish an interface: {result:?}")
    };
    assert!(result.interfaces.is_empty());
    let diagnostic = reports
        .reports
        .iter()
        .find(|report| report.code.starts_with("nash::type::"))
        .unwrap();
    assert_eq!(diagnostic.region.start.line, 1);
    assert!(!diagnostic.region.is_empty());
    assert!(diagnostic.primary_label.is_some());
}

#[tokio::test]
async fn native_imports_aiken_through_the_same_semantic_interface() {
    let result = compile(&[
        ("Math.ak", ADD_ONE),
        ("Main.nash", "module Main exposing (increment)\nimport Builtin exposing (..)\nimport Math\nincrement : int -> int\nincrement value = Math.add_one value\n"),
    ]).await;
    assert!(result.is_success(), "{result:?}");
    assert_eq!(
        result.interfaces[&uri("Main.nash")].exports,
        [Export::Value {
            name: "increment".into()
        }]
    );
}

#[tokio::test]
async fn unknown_imports_are_exact_and_inspection_failures_stay_module_local() {
    let (db, catalog) = memory_sources(&[
        (
            "Prefix/Math.nash",
            "module Prefix.Math exposing (identity)\nidentity x = x\n",
        ),
        (
            "Unknown.nash",
            "module Unknown exposing (..)\nimport Math\nvalue = ()\n",
        ),
        ("Broken.nash", "module Broken exposing (..)\nvalue =\n"),
        (
            "Dependent.nash",
            "module Dependent exposing (..)\nimport Broken\nvalue = Broken.value\n",
        ),
    ]);
    let graph = build_graph(db.clone(), &catalog).await.unwrap();
    assert!(!graph.diagnostics[&uri("Broken.nash")].is_empty());
    let result = build(db, &graph, &catalog).await;
    assert!(matches!(
        result.modules[&uri("Prefix/Math.nash")],
        ModuleResult::Success { .. }
    ));
    assert!(matches!(
        result.modules[&uri("Broken.nash")],
        ModuleResult::Failed(_)
    ));
    assert!(
        matches!(&result.modules[&uri("Dependent.nash")], ModuleResult::Blocked { dependencies } if dependencies == &[uri("Broken.nash")])
    );
    let ModuleResult::Failed(reports) = &result.modules[&uri("Unknown.nash")] else {
        panic!("suffix matches must not resolve an unknown import: {result:?}")
    };
    let diagnostic = reports
        .reports
        .iter()
        .find(|report| report.code == "NAF1002")
        .unwrap();
    assert_eq!(diagnostic.region.start.line, 2);
}

#[tokio::test]
async fn duplicate_names_across_packages_never_overwrite_semantic_interfaces() {
    let (db, mut catalog) = memory_sources(&[
        (
            "left/Math.nash",
            "module Math exposing (identity)\nidentity x = x\n",
        ),
        ("right/Math.ak", ADD_ONE),
        (
            "Main.nash",
            "module Main exposing (..)\nimport Math\nvalue = ()\n",
        ),
        (
            "Good.nash",
            "module Good exposing (identity)\nidentity x = x\n",
        ),
    ]);
    for (path, package, root) in [
        ("left/Math.nash", "example/left", "/project/src/left"),
        ("right/Math.ak", "example/right", "/project/src/right"),
    ] {
        let uri = uri(path);
        catalog.insert(
            uri.clone(),
            SourceSpec::new(&uri, Path::new(root), Some(package.parse().unwrap())).unwrap(),
        );
    }
    let graph = build_graph(db.clone(), &catalog).await.unwrap();
    let (result, output) = build_with(db, &graph, &catalog, |_| {
        panic!("ambiguous graph must not reach the backend")
    })
    .await;
    assert!(output.is_none());
    assert!(matches!(
        result.modules[&uri("Good.nash")],
        ModuleResult::Success { .. }
    ));
    assert_eq!(result.interfaces.len(), 1);
    for path in ["left/Math.nash", "right/Math.ak", "Main.nash"] {
        let ModuleResult::Failed(reports) = &result.modules[&uri(path)] else {
            panic!("ambiguous provider and importer must each fail: {result:?}")
        };
        assert!(
            reports
                .reports
                .iter()
                .any(|report| report.code == "NAF1003"),
            "{reports:?}"
        );
    }
}

#[tokio::test]
async fn resolved_package_releases_preserve_nominal_types_through_facades() {
    use nash_driver::{PackageId, PackageSourceId};
    let model = "module Model exposing (Box(..))\ntype Box = Box\n";
    let (db, mut catalog) = memory_sources(&[
        ("first/Model.nash", model),
        ("second/Model.nash", model),
        (
            "Left.nash",
            "module Left exposing (make)\nimport Model exposing (Box(..))\nmake = Box\n",
        ),
        (
            "Right.nash",
            "module Right exposing (accept)\nimport Model exposing (Box(..))\naccept box = case box of\n    Box -> ()\n",
        ),
        (
            "Main.nash",
            "module Main exposing (..)\nimport Left\nimport Right\nresult = Right.accept Left.make\n",
        ),
    ]);
    let first = PackageId {
        name: Some("example/model".into()),
        version: "1.0.0".into(),
        source: PackageSourceId::Github,
    };
    let application = catalog[&uri("Main.nash")].key.package.clone();
    for second in [
        PackageId {
            version: "2.0.0".into(),
            ..first.clone()
        },
        PackageId {
            source: PackageSourceId::Gitlab,
            ..first.clone()
        },
    ] {
        for (path, package, root) in [
            ("first/Model.nash", first.clone(), "/project/src/first"),
            ("second/Model.nash", second.clone(), "/project/src/second"),
        ] {
            let mut spec = SourceSpec::new(&uri(path), Path::new(root), None).unwrap();
            spec.key.package = package.clone();
            spec.visible_packages = Some(vec![package]);
            catalog.insert(uri(path), spec);
        }
        catalog.get_mut(&uri("Left.nash")).unwrap().visible_packages =
            Some(vec![application.clone(), first.clone()]);
        catalog
            .get_mut(&uri("Right.nash"))
            .unwrap()
            .visible_packages = Some(vec![application.clone(), second.clone()]);
        catalog.get_mut(&uri("Main.nash")).unwrap().visible_packages =
            Some(vec![application.clone(), first.clone(), second.clone()]);
        let graph = build_graph(db.clone(), &catalog).await.unwrap();
        let result = build(db.clone(), &graph, &catalog).await;
        for path in [
            "first/Model.nash",
            "second/Model.nash",
            "Left.nash",
            "Right.nash",
        ] {
            assert!(
                matches!(result.modules[&uri(path)], ModuleResult::Success { .. }),
                "{result:?}"
            );
        }
        assert!(
            matches!(&result.modules[&uri("Main.nash")], ModuleResult::Failed(reports)
            if reports.reports.iter().any(|report| report.code == "nash::type::mismatch")),
            "{result:?}"
        );
        catalog
            .get_mut(&uri("Right.nash"))
            .unwrap()
            .visible_packages = Some(vec![application.clone(), first.clone()]);
        let graph = build_graph(db.clone(), &catalog).await.unwrap();
        let result = build(db.clone(), &graph, &catalog).await;
        assert!(result.is_success(), "{result:?}");
    }
}

#[tokio::test]
async fn visible_aiken_provider_ambiguity_is_located_at_the_import() {
    use nash_driver::{PackageId, PackageSourceId};
    let (db, mut catalog) = memory_sources(&[
        ("left/math.ak", "pub fn value() { 1 }"),
        ("right/math.ak", "pub fn value() { 2 }"),
        ("main.ak", "use math\npub fn value() { math.value() }"),
    ]);
    let packages: Vec<_> = ["left", "right"]
        .map(|name| PackageId {
            name: Some(format!("example/{name}")),
            version: "1.0.0".into(),
            source: PackageSourceId::Github,
        })
        .into_iter()
        .collect();
    for (name, package) in ["left", "right"].into_iter().zip(&packages) {
        let source = uri(&format!("{name}/math.ak"));
        let mut spec =
            SourceSpec::new(&source, Path::new(&format!("/project/src/{name}")), None).unwrap();
        spec.key.package = package.clone();
        spec.visible_packages = Some(vec![package.clone()]);
        catalog.insert(source, spec);
    }
    catalog.get_mut(&uri("main.ak")).unwrap().visible_packages = Some(packages);
    let graph = build_graph(db.clone(), &catalog).await.unwrap();
    let diagnostic = graph.diagnostics[&uri("main.ak")]
        .iter()
        .find(|diagnostic| diagnostic.code == "NAF1003")
        .unwrap();
    assert_eq!(diagnostic.region.unwrap().start.line, 1);
    let result = build(db, &graph, &catalog).await;
    for path in ["left/math.ak", "right/math.ak"] {
        assert!(
            matches!(result.modules[&uri(path)], ModuleResult::Success { .. }),
            "{result:?}"
        );
    }
    assert!(matches!(
        result.modules[&uri("main.ak")],
        ModuleResult::Failed(_)
    ));
}
