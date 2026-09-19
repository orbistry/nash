use nash_driver::{Database, InMemorySource, build, build_graph, bundled_base};
use std::sync::Arc;
use tokio::sync::Mutex;
use url::Url;

#[tokio::test]
async fn all_bundled_modules_and_implicit_prelude_compile() {
    let memory = InMemorySource::new();
    let main = Url::parse("file:///fresh-project/src/Main.nash").unwrap();
    memory.insert(main.clone(), "module Main exposing (..)\ntype Token = Token Int\nroundTrip : Big 'a => 'a -> 'a\nroundTrip item = fromData (toData item)\nvalue : Token\nvalue = roundTrip (Token 42)\nchecked : Int\nchecked = validate (I 42)\nnumber : int\nnumber = 1 + 2\ncast : Int\ncast = coerce (I 42)\ntests\n    test \"labels\" = do\n        label \"compiled\"\n        assert True\n".into());
    let db = Arc::new(Mutex::new(Database::new(memory)));
    let project = nash_driver::Project {
        root: "/fresh-project".into(),
        config: nash_config::parse(r#"{"type":"application"}"#, "/fresh-project/nash.jsonc")
            .unwrap(),
        members: vec![nash_driver::ProjectMember {
            root: "/fresh-project".into(),
            config: nash_config::parse(r#"{"type":"application"}"#, "/fresh-project/nash.jsonc")
                .unwrap(),
            source_dirs: vec!["/fresh-project/src".into()],
        }],
    };
    let modules = project.discover_modules(&*db.lock().await).await.unwrap();
    let graph = build_graph(db.clone(), &modules.keys().cloned().collect::<Vec<_>>())
        .await
        .unwrap();
    let result = build(db.clone(), &graph, &modules).await;
    assert!(result.is_success(), "{:#?}", result.ordered_reports());
    assert_eq!(result.success, bundled_base::SOURCES.len() + 1);
    let mut db = db.lock().await;
    let uri = bundled_base::uri("Prelude");
    db.invalidate(&uri);
    assert_eq!(
        db.source(&uri).await.unwrap(),
        bundled_base::source(&uri).unwrap()
    );
}

#[tokio::test]
async fn embedded_sources_are_immutable_and_reject_unknown_virtual_paths() {
    let memory = InMemorySource::new();
    let prelude = bundled_base::uri("Prelude");
    let unknown = bundled_base::uri("Unknown");
    memory.insert(prelude.clone(), "replacement".into());
    memory.insert(unknown.clone(), "replacement".into());
    let mut db = Database::new(memory);
    assert_eq!(
        db.source(&prelude).await.unwrap(),
        bundled_base::source(&prelude).unwrap()
    );
    assert!(db.write(&prelude, "replacement").await.is_err());
    db.invalidate(&prelude);
    assert_eq!(
        db.source(&prelude).await.unwrap(),
        bundled_base::source(&prelude).unwrap()
    );
    assert!(!db.exists(&unknown).await.unwrap());
    assert!(db.source(&unknown).await.is_err());
}

#[tokio::test]
async fn application_headers_cannot_replace_bundled_modules() {
    for name in ["Prelude", "Eq", "Primitive", "Builtin"] {
        let memory = InMemorySource::new();
        let main = Url::parse("file:///app/src/DifferentFilename.nash").unwrap();
        memory.insert(
            main.clone(),
            format!("module {name} exposing (..)\nidentity x = x\n"),
        );
        let db = Arc::new(Mutex::new(Database::new(memory)));
        let mut modules = bundled_base::modules();
        modules.insert(main, None);
        let error = build_graph(db, &modules.keys().cloned().collect::<Vec<_>>())
            .await
            .unwrap_err();
        assert!(
            matches!(error, nash_driver::DriverError::ReservedModule { name: reserved, .. } if reserved == name)
        );
    }
}

#[tokio::test]
async fn builtin_rejects_compiler_intrinsics() {
    let memory = InMemorySource::new();
    let main = Url::parse("file:///app/src/Main.nash").unwrap();
    memory.insert(
        main.clone(),
        "module Main exposing (..)\nvalue = Builtin.coerce ()\n".into(),
    );
    let db = Arc::new(Mutex::new(Database::new(memory)));
    let mut modules = bundled_base::modules();
    modules.insert(main.clone(), None);
    let graph = build_graph(db.clone(), &modules.keys().cloned().collect::<Vec<_>>())
        .await
        .unwrap();
    let result = build(db, &graph, &modules).await;
    assert!(
        matches!(&result.modules[&main], nash_driver::ModuleResult::Failed(reports) if !reports.reports.is_empty())
    );
}

#[tokio::test]
async fn base_application_fixture_compiles_without_imports() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/base");
    let project = nash_driver::Project::load(root).await.unwrap();
    let db = Arc::new(Mutex::new(Database::new(
        nash_driver::FileSystemSource::new(),
    )));
    let origins = project.discover_modules(&*db.lock().await).await.unwrap();
    let graph = build_graph(db.clone(), &origins.keys().cloned().collect::<Vec<_>>())
        .await
        .unwrap();
    let result = build(db, &graph, &origins).await;
    assert!(result.is_success(), "{:#?}", result.ordered_reports());
}

#[tokio::test]
async fn projects_cannot_declare_or_replace_the_bundled_package() {
    for source in [
        r#"{"type":"application","dependencies":{"nash/base":{"path":"../replacement"}}}"#,
        r#"{"type":"package","name":"nash/base","version":"1.0.0","summary":"Replacement","license":"MIT","exposedModules":[]}"#,
    ] {
        let config = nash_config::parse(source, "/app/nash.jsonc").unwrap();
        let project = nash_driver::Project {
            root: "/app".into(),
            config: config.clone(),
            members: vec![nash_driver::ProjectMember {
                root: "/app".into(),
                config,
                source_dirs: vec!["/app/src".into()],
            }],
        };
        let db = Database::new(InMemorySource::new());
        let error = project.discover_modules(&db).await.unwrap_err();
        assert!(
            matches!(error, nash_driver::DriverError::Dependency { package, .. } if package == "nash/base")
        );
    }
}
