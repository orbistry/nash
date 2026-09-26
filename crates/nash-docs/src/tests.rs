use super::*;
use nash_driver::{Database, InMemorySource, build_graph_production, build_with, bundled_base};
use std::sync::Arc;
use tokio::sync::Mutex;
use url::Url;

async fn documented(source: &str) -> Extraction {
    let uri = Url::parse("file:///docs/src/Example.nash").unwrap();
    let memory = InMemorySource::with_files([(uri.clone(), source.into())]);
    let mut origins = bundled_base::modules();
    origins.insert(uri.clone(), None);
    let db = Arc::new(Mutex::new(Database::new(memory)));
    let graph = build_graph_production(db.clone(), &origins.keys().cloned().collect::<Vec<_>>())
        .await
        .unwrap();
    let (result, docs) = build_with(db, &graph, &origins, move |solved| {
        let module = solved.modules.iter().find(|m| m.uri == uri).unwrap();
        let source = nash_parse::Parser::new(solved.store, module.source)
            .module()
            .unwrap();
        extract(
            &source,
            &nash_can::from_module(solved.store, module.module, &module.annotations),
        )
    })
    .await;
    assert!(result.is_success(), "{:?}", result.ordered_reports());
    docs.unwrap()
}

macro_rules! assert_docs_snapshot {
    ($source:expr) => {{
        let source = indoc::indoc!($source);
        let output = documented(source).await;
        insta::with_settings!({description => source, omit_expression => true}, { insta::assert_yaml_snapshot!(output); });
    }};
}

#[tokio::test]
async fn public_declarations() {
    assert_docs_snapshot!(
        r#"
        module Example exposing (id, type box(..), type secret, type record, Keep, (%%))
        {-| Example API.

        @docs Keep, id

        Types below.

        @docs box, record, secret, (%%)
        -}

        infix left 5 (%%) = combine

        {-| Keep a value unchanged. -}
        trait Keep 'a where
            keep : 'a -> 'a

        {-| Identity implementation. -}
        impl Keep int where
            keep x = x

        {-| Preserve the argument. -}
        id x = x

        {-| One little payload. -}
        type box 'a = Box 'a

        {-| A hidden constructor. -}
        type secret = Secret int

        {-| A labeled record. -}
        type alias record = { value : int }

        combine x y = x
        private x = x
    "#
    );
}

#[tokio::test]
async fn warnings_keep_output() {
    assert_docs_snapshot!(
        r#"
        module Example exposing (id)
        {-| Still rendered.

        @docs id, missing, id

        ```nash
        @docs notADirective
        ```
        -}
        id x = x
    "#
    );
}

#[tokio::test]
async fn constrained_types() {
    assert_docs_snapshot!(
        r#"
        module Example exposing (render)
        {-| Render a value with its Show instance. -}
        render : Show 'a => 'a -> string
        render value = show value
    "#
    );
}

#[test]
fn compiler_owned_modules() {
    insta::with_settings!({omit_expression => true}, { insta::assert_yaml_snapshot!(primitives()); });
}

#[tokio::test]
async fn base_documentation() {
    let docs = project::build(project::Input::Base).await.unwrap();
    assert!(
        docs.compiler.is_success(),
        "{:?}",
        docs.compiler.ordered_reports()
    );
    assert!(docs.warnings.is_empty(), "{:?}", docs.warnings);
    assert_eq!(docs.modules.len(), bundled_base::modules().len() + 2);
    let files = render(&docs.modules, Format::Html);
    let index = &files["index.html"];
    for module in &docs.modules {
        let path = format!("{}.html", module.name.replace('.', "/"));
        assert!(index.contains(&format!("href=\"{path}\"")));
        assert!(files.contains_key(&path));
    }
    let search: serde_json::Value = serde_json::from_str(&files["search.json"]).unwrap();
    for entry in search.as_array().unwrap() {
        let (path, anchor) = entry["url"].as_str().unwrap().split_once('#').unwrap();
        assert!(files[path].contains(&format!("id=\"{anchor}\"")));
    }
}

#[tokio::test]
async fn same_trait_method_constraint() {
    assert_docs_snapshot!(
        r#"
        module Example exposing (Keep)
        {-| A method may require the same trait at another type. -}
        trait Keep 'a where
            other : Keep 'b => 'b -> 'a
    "#
    );
}

#[tokio::test]
async fn overview_markdown_boundaries() {
    assert_docs_snapshot!(
        r#"
        module Example exposing (first, second)
        {-| Order and prose.

        @docs
            second,
        ## Keep this heading

        ````nash
        ```
        @docs first
        ````

            @docs alsoCode

        @docs  first
        -}
        {-| First value. -}
        first x = x
        {-| Second value. -}
        second x = x
    "#
    );
}

macro_rules! assert_render_snapshot {
    ($format:expr, $source:expr) => {{
        let source = indoc::indoc!($source);
        let docs = documented(source).await;
        let files = render(&[docs.module], $format);
        insta::with_settings!({description => source, omit_expression => true}, {
            let output = files.into_iter()
                .filter(|(path, _)| !path.ends_with(".css") && !path.ends_with(".js"))
                .map(|(path, content)| format!("--- {path}\n{content}"))
                .collect::<Vec<_>>().join("\n");
            insta::assert_snapshot!(output);
        });
    }};
}

#[tokio::test]
async fn markdown_output() {
    assert_render_snapshot!(
        Format::Markdown,
        r#"
        module Example exposing (type box(..), identity, (%%))
        {-| # A small API

        @docs box, identity, (%%)
        -}

        infix left 5 (%%) = combine

        {-| A little container. -}
        type box 'a = Box 'a

        {-| Preserve a value.

        ```nash
        identity 42
        ```
        -}
        identity x = x

        combine x y = x
    "#
    );
}

#[tokio::test]
async fn html_output() {
    assert_render_snapshot!(
        Format::Html,
        r#"
        module Example exposing (identity)
        {-| **Public API** with [a link][guide].

        @docs identity

        [guide]: https://example.com
        -}

        {-| Preserve a value.

        ```nash
        identity "hello"
        ```

        <script>alert("raw HTML")</script>

        [Unsafe link](javascript:alert%281%29)
        -}
        identity x = x
    "#
    );
}

#[test]
fn highlight_fragments() {
    let source = indoc::indoc!(
        r##"
        -- A comment
        {- Nested {- comment -} λ -}
        run : int -> int
        run value = if value < 0 then -value else value
        bytes = #"00ab"
        text = "<script>\"quoted\" & λ"
        multiline = """a
        b"""
        unfinished = "λ
    "##
    );
    insta::with_settings!({description => source, omit_expression => true}, {
        insta::assert_snapshot!(highlight::highlight(source));
    });
}

async fn project_fixture(config: &str, sources: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    tokio::fs::write(dir.path().join("nash.jsonc"), config)
        .await
        .unwrap();
    for (name, source) in sources {
        let file = dir
            .path()
            .join(format!("src/{}.nash", name.replace('.', "/")));
        tokio::fs::create_dir_all(file.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(file, source).await.unwrap();
    }
    dir
}

#[tokio::test]
async fn project_exports() {
    let source = "module Public.Api exposing (answer)\n{-| The answer. -}\nanswer = 42\n";
    let sources = [
        ("Public.Api", source),
        ("Private", "module Private exposing (secret)\nsecret = 0\n"),
    ];
    let dir = project_fixture(r#"{"type":"package","name":"example/api","version":"1.0.0","summary":"API","license":"MIT","exposedModules":["Public.Api"]}"#, &sources).await;
    let docs = project::build(project::Input::Project(dir.path()))
        .await
        .unwrap();
    assert!(
        docs.compiler.is_success(),
        "{:?}",
        docs.compiler.ordered_reports()
    );
    assert_eq!(
        docs.modules
            .iter()
            .map(|m| m.name.as_str())
            .collect::<Vec<_>>(),
        ["Public.Api"]
    );
    let files = render(&docs.modules, Format::Html);
    insta::with_settings!({description => source, omit_expression => true}, {
        insta::assert_snapshot!(files["Public/Api.html"]);
    });
    tokio::fs::write(dir.path().join("nash.jsonc"), r#"{"type":"application"}"#)
        .await
        .unwrap();
    let docs = project::build(project::Input::Project(dir.path()))
        .await
        .unwrap();
    assert!(
        docs.compiler.is_success(),
        "{:?}",
        docs.compiler.ordered_reports()
    );
    assert_eq!(
        docs.modules
            .iter()
            .map(|m| m.name.as_str())
            .collect::<Vec<_>>(),
        ["Private", "Public.Api"]
    );
}

#[tokio::test]
async fn invalid_project_has_no_documentation() {
    let dir = project_fixture(
        r#"{"type":"application"}"#,
        &[("Main", "module Main exposing (answer)\nanswer = unknown\n")],
    )
    .await;
    let docs = project::build(project::Input::Project(dir.path()))
        .await
        .unwrap();
    assert!(!docs.compiler.is_success());
    assert!(docs.modules.is_empty());
}

#[tokio::test]
async fn duplicate_workspace_modules_are_reported() {
    let dir = tempfile::tempdir().unwrap();
    tokio::fs::write(
        dir.path().join("nash.jsonc"),
        r#"{"type":"workspace","members":["one","two"]}"#,
    )
    .await
    .unwrap();
    for member in ["one", "two"] {
        let path = dir.path().join(member);
        tokio::fs::create_dir_all(path.join("src")).await.unwrap();
        tokio::fs::write(path.join("nash.jsonc"), r#"{"type":"application"}"#)
            .await
            .unwrap();
        tokio::fs::write(
            path.join("src/Main.nash"),
            "module Main exposing (answer)\nanswer = 42\n",
        )
        .await
        .unwrap();
    }
    let result = project::build(project::Input::Project(dir.path())).await;
    assert!(matches!(result, Err(project::Error::DuplicateModule(name)) if name == "Main"));
}
