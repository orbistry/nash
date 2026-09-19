use nash_driver::{Database, InMemorySource, build, build_graph, build_with};
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::Mutex;
use url::Url;

#[tokio::test]
async fn production_excludes_test_blocks_and_their_imports() {
    let files = [
        (
            "Helper",
            indoc::indoc!(
                r#"
            module Helper exposing (identity)
            identity x = x
            tests
                import TestOnlyMissing
                test "helper" = do
                    assert undefinedTestName
        "#
            ),
        ),
        (
            "Main",
            indoc::indoc!(
                r#"
            validator module Main exposing (main)
            import Helper
            main : Data -> unit
            main _ = Helper.identity ()
            tests
                import MissingFuzzer
                test "main" = do
                    assert anotherUndefinedName
        "#
            ),
        ),
    ];
    let memory = InMemorySource::new();
    let origins: BTreeMap<_, _> = files
        .iter()
        .map(|(name, text)| {
            let uri = Url::parse(&format!("file:///project/src/{name}.nash")).unwrap();
            memory.insert(uri.clone(), text.to_string());
            (uri, None)
        })
        .collect();
    let db = Arc::new(Mutex::new(Database::new(memory)));
    let graph = build_graph(db.clone(), &origins.keys().cloned().collect::<Vec<_>>())
        .await
        .unwrap();
    let (report, output) = build_with(db.clone(), &graph, &origins, |solved| {
        assert_eq!(solved.modules.len(), 2);
        nash_driver::build::build_validators(solved, nash_config::Build::default())
    })
    .await;
    assert!(report.is_success(), "{report:?}");
    let output = output.unwrap().unwrap();
    assert_eq!(output.len(), 1);
    insta::with_settings!({description => files.iter().map(|(_, source)| *source).collect::<Vec<_>>().join("\n"), omit_expression => true}, {
        insta::assert_snapshot!(output[0].uplc);
    });
    let checked = build(db, &graph, &origins).await;
    assert!(
        !checked.is_success(),
        "check must diagnose invalid test imports and bodies"
    );
}

#[tokio::test]
async fn production_keeps_normal_import_errors() {
    let text = "validator module Main exposing (main)\nimport Missing\nmain _ = ()\n";
    let uri = Url::parse("file:///project/src/Main.nash").unwrap();
    let memory = InMemorySource::new();
    memory.insert(uri.clone(), text.to_string());
    let origins = BTreeMap::from([(uri.clone(), None)]);
    let db = Arc::new(Mutex::new(Database::new(memory)));
    let graph = build_graph(db.clone(), &[uri]).await.unwrap();
    let (report, output) = build_with(db, &graph, &origins, |_| {
        panic!("invalid production module reached codegen")
    })
    .await;
    assert!(!report.is_success());
    assert!(output.is_none());
}

#[tokio::test]
async fn check_retains_tests_and_production_has_no_test_dependency_edges() {
    let memory = InMemorySource::new();
    let main = Url::parse("file:///project/src/Main.nash").unwrap();
    let helper = Url::parse("file:///dependency/src/Helper.nash").unwrap();
    let source = indoc::indoc!(
        r#"
        module Main exposing (..)
        private = ()
        tests
            import Helper
            test "private and scoped imports" = do
                Helper.identity private
    "#
    );
    memory.insert(main.clone(), source.into());
    // Dependency tests must not be checked or executed as part of root tests.
    memory.insert(helper.clone(), "module Helper exposing (identity)\nidentity x = x\ntests\n    import Missing\n    test \"dependency\" = do\n        undefined\n".into());
    let origins = BTreeMap::from([(main.clone(), None), (helper.clone(), None)]);
    let db = Arc::new(Mutex::new(Database::new(memory)));
    let modules: Vec<_> = origins.keys().cloned().collect();
    let graph =
        nash_driver::build_graph_with_tests(db.clone(), &modules, std::slice::from_ref(&main))
            .await
            .unwrap();
    assert_eq!(graph.edges[&main], vec![helper]);
    let checked = build(db.clone(), &graph, &origins).await;
    assert!(checked.is_success(), "{checked:#?}");
    let (checked, retained) = nash_driver::test_with(db.clone(), &graph, &origins, move |solved| {
        let main = solved
            .modules
            .iter()
            .find(|module| module.uri == main)
            .unwrap();
        assert_eq!(main.source, source);
        main.module.tests.len()
    })
    .await;
    assert!(checked.is_success());
    assert_eq!(retained, Some(1));
    let production = nash_driver::build_graph_production(db, &modules)
        .await
        .unwrap();
    assert!(production.edges.values().all(Vec::is_empty));
}

#[tokio::test]
async fn check_rejects_non_unit_test_body() {
    let uri = Url::parse("file:///project/src/Main.nash").unwrap();
    let source = indoc::indoc!(
        r#"
        module Main exposing (..)
        import Builtin exposing (type bool(..))
        tests
            test "must return unit" = do
                True
    "#
    );
    let memory = InMemorySource::new();
    memory.insert(uri.clone(), source.into());
    let origins = BTreeMap::from([(uri.clone(), None)]);
    let db = Arc::new(Mutex::new(Database::new(memory)));
    let graph = build_graph(db.clone(), std::slice::from_ref(&uri))
        .await
        .unwrap();
    let checked = build(db, &graph, &origins).await;
    let nash_driver::ModuleResult::Failed(reports) = &checked.modules[&uri] else {
        panic!("invalid test body passed: {checked:?}")
    };
    insta::with_settings!({description => source, omit_expression => true}, {
        let view = nash_report::Source::new(&reports.source);
        let rendered = reports.reports.iter().map(|report| nash_report::render_plain(report, &view, &reports.path)).collect::<Vec<_>>().join("\n");
        insta::assert_snapshot!(rendered);
    });
}

#[tokio::test]
async fn dependency_validators_are_not_emitted_and_selected_roots_keep_their_target() {
    let memory = InMemorySource::new();
    let main = Url::parse("file:///project/src/Main.nash").unwrap();
    let dependency = Url::parse("file:///dependency/src/Dependency.nash").unwrap();
    memory.insert(main.clone(), "validator module Main exposing (main)\nimport Dependency\nmain : Data -> unit\nmain value = Dependency.main value\n".into());
    memory.insert(
        dependency.clone(),
        "validator module Dependency exposing (main)\nmain : Data -> unit\nmain _ = ()\n".into(),
    );
    let origins = BTreeMap::from([
        (main.clone(), None),
        (dependency, Some("sample/dependency".parse().unwrap())),
    ]);
    let db = Arc::new(Mutex::new(Database::new(memory)));
    let graph = nash_driver::build_graph_production(
        db.clone(),
        &origins.keys().cloned().collect::<Vec<_>>(),
    )
    .await
    .unwrap();
    let (report, outputs) = build_with(db, &graph, &origins, move |solved| {
        nash_driver::build::build_validators_matching_with(solved, |uri| {
            (uri == &main).then_some(nash_config::Build {
                plutus_version: nash_config::PlutusVersion::V1,
                ..Default::default()
            })
        })
    })
    .await;
    assert!(report.is_success(), "{report:?}");
    let outputs = outputs.unwrap().unwrap();
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0].module, "Main");
    assert!(outputs[0].uplc.starts_with("(program 1.1.0"));
    assert_eq!(
        outputs[0].hash,
        nash_plutus::script::script_hash(nash_plutus::machine::PlutusVersion::V1, &outputs[0].cbor)
    );
}
