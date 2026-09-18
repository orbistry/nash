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
        "check must still diagnose unsupported tests"
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
