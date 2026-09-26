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
    let origins = bundled_base::modules();
    let db = Arc::new(Mutex::new(Database::new(InMemorySource::new())));
    let graph = build_graph_production(db.clone(), &origins.keys().cloned().collect::<Vec<_>>())
        .await
        .unwrap();
    let (result, warnings) = build_with(db, &graph, &origins, |solved| {
        solved
            .modules
            .iter()
            .flat_map(|module| {
                let source = nash_parse::Parser::new(solved.store, module.source)
                    .module()
                    .unwrap();
                extract(
                    &source,
                    &nash_can::from_module(solved.store, module.module, &module.annotations),
                )
                .warnings
            })
            .collect::<Vec<_>>()
    })
    .await;
    assert!(result.is_success(), "{:?}", result.ordered_reports());
    assert!(
        warnings.as_ref().unwrap().is_empty(),
        "{}",
        warnings
            .unwrap()
            .iter()
            .map(|w| format!("{}.{}: {}", w.module, w.name, w.message))
            .collect::<Vec<_>>()
            .join("\n")
    );
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
