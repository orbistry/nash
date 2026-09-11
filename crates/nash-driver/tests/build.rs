use nash_codegen::build::TraceConfig;
use nash_driver::{Database, InMemorySource, build::build_validators, build_graph, build_with};
use nash_plutus::{arena::Arena, flat, syn};
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::Mutex;
use url::Url;

async fn outputs(files: &[(&str, &str)]) -> Vec<nash_driver::build::ValidatorOutput> {
    let source = InMemorySource::new();
    let modules: BTreeMap<_, _> = files
        .iter()
        .map(|(name, text)| {
            let uri = Url::parse(&format!("file:///project/src/{name}.nash")).unwrap();
            source.insert(uri.clone(), (*text).to_owned());
            (uri, None)
        })
        .collect();
    let db = Arc::new(Mutex::new(Database::new(source)));
    let graph = build_graph(db.clone(), &modules.keys().cloned().collect::<Vec<_>>())
        .await
        .unwrap();
    let (report, result) = build_with(db, &graph, &modules, |solved| {
        build_validators(solved, TraceConfig::default())
    })
    .await;
    assert!(report.is_success(), "{report:?}");
    result.unwrap().unwrap()
}

#[tokio::test]
async fn source_dependency_compiles_to_roundtrippable_script() {
    let artifacts = outputs(&[
        ("Helper", "module Helper exposing (identity)\nidentity x = x\n"),
        ("Main", "validator module Main exposing (main)\nimport Helper\nmain : Data -> unit\nmain _ = Helper.identity ()\n"),
    ]).await;
    assert_eq!(artifacts.len(), 1);
    let output = &artifacts[0];
    assert_eq!(output.module, "Main");
    insta::assert_snapshot!(output.uplc);
    let arena = Arena::new();
    let parsed = syn::parse_program(&arena, &output.uplc).unwrap();
    assert_eq!(flat::encode(parsed).unwrap(), output.flat);
    assert_eq!(flat::to_cbor(parsed).unwrap(), output.cbor);
}

#[tokio::test]
async fn ordinary_modules_produce_no_scripts() {
    assert!(
        outputs(&[("Lib", "module Lib exposing (id)\nid x = x\n")])
            .await
            .is_empty()
    );
}

#[tokio::test]
async fn unconstrained_validator_input_uses_data() {
    let outputs = outputs(&[(
        "Main",
        "validator module Main exposing (main)\nmain _ = ()\n",
    )])
    .await;
    assert_eq!(outputs.len(), 1);
    assert!(outputs[0].uplc.contains("lam"));
}
