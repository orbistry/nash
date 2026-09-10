use super::*;
use crate::InMemorySource;

fn url(name: &str) -> Url {
    Url::parse(&format!("file:///project/{name}.nash")).unwrap()
}

#[tokio::test]
async fn independent_failures_block_only_their_dependents() {
    let files = [
        ("Broken", "module Broken exposing (..)\nf = missing\n"),
        ("Other", "module Other exposing (..)\ng = absent\n"),
        (
            "Middle",
            "module Middle exposing (..)\nimport Broken\nx = Broken.f\n",
        ),
        (
            "Main",
            "module Main exposing (..)\nimport Middle\nx = Middle.x\n",
        ),
        ("Good", "module Good exposing (..)\nx = ()\n"),
    ];
    let memory = InMemorySource::new();
    for (name, source) in files {
        memory.insert(url(name), source.into());
    }
    let db = Arc::new(Mutex::new(Database::new(memory)));
    let uris: Vec<_> = files.iter().map(|(name, _)| url(name)).collect();
    let graph = build_graph(db.clone(), &uris).await.unwrap();
    let origins = uris.iter().cloned().map(|uri| (uri, None)).collect();
    let result = build(db, &graph, &origins).await;
    assert_eq!(result.success, 1);
    assert_eq!(result.failed, 4);
    assert_eq!(result.interfaces.len(), 1);
    assert!(result.interfaces.contains_key(&url("Good")));
    for name in ["Middle", "Main"] {
        assert!(
            matches!(&result.modules[&url(name)], ModuleResult::Blocked { dependencies } if dependencies == &[url("Broken")])
        );
    }
}

#[tokio::test]
async fn unreadable_source_does_not_hide_independent_errors() {
    let memory = InMemorySource::new();
    memory.insert(
        url("Main"),
        "module Main exposing (..)\nx = unknown\n".into(),
    );
    memory.insert(
        url("Dependent"),
        "module Dependent exposing (..)\nimport Missing\nx = Missing.x\n".into(),
    );
    let uris = [url("Missing"), url("Main"), url("Dependent")];
    let db = Arc::new(Mutex::new(Database::new(memory)));
    let graph = build_graph(db.clone(), &uris).await.unwrap();
    let result = build(
        db,
        &graph,
        &uris.iter().cloned().map(|uri| (uri, None)).collect(),
    )
    .await;
    assert!(matches!(
        result.modules[&url("Missing")],
        ModuleResult::SourceUnavailable { .. }
    ));
    assert!(
        matches!(&result.modules[&url("Main")], ModuleResult::Failed(reports) if reports.reports.iter().any(|report| report.title == "NAMING ERROR"))
    );
    assert!(
        matches!(&result.modules[&url("Dependent")], ModuleResult::Blocked { dependencies } if dependencies == &[url("Missing")])
    );
    assert!(result.interfaces.is_empty());
}
