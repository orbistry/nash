use nash_codegen::build::TraceConfig;
use nash_driver::{
    Database, InMemorySource, ModuleCatalog, SourceSpec, build::build_validators, build_graph,
    build_with,
};
use nash_plutus::{arena::Arena, data::PlutusData, flat, syn, term::Term};
use std::{path::Path, sync::Arc};
use tokio::sync::Mutex;
use url::Url;

async fn outputs(files: &[(&str, &str)]) -> Vec<nash_driver::build::ValidatorOutput> {
    let source = InMemorySource::new();
    let modules: ModuleCatalog = files
        .iter()
        .map(|(name, text)| {
            let uri = Url::parse(&format!("file:///project/src/{name}")).unwrap();
            source.insert(uri.clone(), (*text).to_owned());
            let spec = SourceSpec::new(&uri, Path::new("/project/src"), None).unwrap();
            (uri, spec)
        })
        .collect();
    let db = Arc::new(Mutex::new(Database::new(source)));
    let graph = build_graph(db.clone(), &modules).await.unwrap();
    let (report, result) = build_with(db, &graph, &modules, |solved| {
        build_validators(
            solved,
            TraceConfig {
                compiler: false,
                ..TraceConfig::default()
            },
        )
    })
    .await;
    assert!(report.is_success(), "{report:?}");
    result.unwrap().unwrap()
}

#[tokio::test]
async fn source_dependency_compiles_to_roundtrippable_script() {
    let files = [
        (
            "Helper.nash",
            "module Helper exposing (identity)\nidentity x = x\n",
        ),
        (
            "Main.nash",
            "validator module Main exposing (main)\nimport Helper\nmain : Data -> unit\nmain _ = Helper.identity ()\n",
        ),
    ];
    let artifacts = outputs(&files).await;
    assert_eq!(artifacts.len(), 1);
    let output = &artifacts[0];
    assert_eq!(output.module, "Main");
    insta::with_settings!({ description => files.iter().map(|(_, source)| *source).collect::<Vec<_>>().join("\n"), omit_expression => true }, { insta::assert_snapshot!(output.uplc); });
    let arena = Arena::new();
    let parsed = syn::parse_program(&arena, &output.uplc).unwrap();
    assert_eq!(flat::encode(parsed).unwrap(), output.flat);
    assert_eq!(flat::to_cbor(parsed).unwrap(), output.cbor);
}

#[tokio::test]
async fn ordinary_modules_produce_no_scripts() {
    assert!(
        outputs(&[("Lib.nash", "module Lib exposing (id)\nid x = x\n")])
            .await
            .is_empty()
    );
}

#[tokio::test]
async fn unconstrained_validator_input_uses_data() {
    let outputs = outputs(&[(
        "Main.nash",
        "validator module Main exposing (main)\nmain _ = ()\n",
    )])
    .await;
    assert_eq!(outputs.len(), 1);
    let arena = Arena::new();
    let program = syn::parse_program(&arena, &outputs[0].uplc)
        .into_result()
        .expect("generated validator parses");
    let evaluation = program
        .apply(
            &arena,
            Term::data(&arena, PlutusData::integer_from(&arena, 7)),
        )
        .eval(&arena);
    assert_eq!(evaluation.term.unwrap(), Term::unit(&arena));
}

#[tokio::test]
async fn aiken_library_add_one_and_fixed_match_execute_in_native_validator() {
    let math = format!(
        "{}\npub fn is_answer(value: Int) -> Bool {{ when value is {{ 42 -> True _ -> False }} }}\n",
        include_str!("fixtures/aiken/supported/src/add_one.ak"),
    );
    let artifacts = outputs(&[
        ("Math.ak", &math),
        (
            "Main.nash",
            "validator module Main exposing (main)\nimport Builtin exposing (..)\nimport Math\nmain : Data -> unit\nmain value = assert (Math.is_answer (Math.add_one (Builtin.unIData value)))\n",
        ),
    ])
    .await;
    assert_eq!(artifacts.len(), 1);
    let arena = Arena::new();
    let program = syn::parse_program(&arena, &artifacts[0].uplc)
        .into_result()
        .expect("generated validator parses");
    assert_eq!(flat::encode(program).unwrap(), artifacts[0].flat);
    for (input, succeeds) in [(41, true), (40, false)] {
        let evaluation = program
            .apply(
                &arena,
                Term::data(&arena, PlutusData::integer_from(&arena, input)),
            )
            .eval(&arena);
        if succeeds {
            assert_eq!(evaluation.term.unwrap(), Term::unit(&arena));
        } else {
            assert!(
                matches!(
                    evaluation.term,
                    Err(nash_plutus::machine::MachineError::ExplicitErrorTerm)
                ),
                "unexpected validator outcome for {input}: {:?}",
                evaluation.term
            );
        }
    }
}

#[tokio::test]
async fn aiken_mint_dispatch_checks_redeemers_and_fails_on_false() {
    let artifacts = outputs(&[(
        "mint.ak",
        include_str!("fixtures/aiken/validator/src/mint.ak"),
    )])
    .await;
    assert_eq!(artifacts.len(), 1);
    let arena = Arena::new();
    let program = syn::parse_program(&arena, &artifacts[0].uplc)
        .into_result()
        .unwrap();
    assert_eq!(flat::encode(program).unwrap(), artifacts[0].flat);
    for (purpose, redeemer, succeeds, logs) in [
        (0, PlutusData::integer_from(&arena, 1), true, vec!["mint"]),
        (0, PlutusData::integer_from(&arena, 0), false, vec!["mint"]),
        (1, PlutusData::integer_from(&arena, 1), false, vec![]),
        (0, PlutusData::byte_string(&arena, b"bad"), false, vec![]),
    ] {
        let context = PlutusData::constr(
            &arena,
            0,
            arena.alloc_slice_copy(&[
                PlutusData::constr(&arena, 0, &[]),
                redeemer,
                PlutusData::constr(
                    &arena,
                    purpose,
                    arena.alloc_slice_copy(&[PlutusData::byte_string(&arena, b"policy")]),
                ),
            ]),
        );
        let result = program
            .apply(&arena, Term::data(&arena, context))
            .eval(&arena);
        assert_eq!(result.term.is_ok(), succeeds, "{result:?}");
        if succeeds {
            assert_eq!(result.term.unwrap(), Term::unit(&arena));
        }
        assert_eq!(result.info.logs, logs);
    }
    let malformed = program
        .apply(
            &arena,
            Term::data(&arena, PlutusData::integer_from(&arena, 0)),
        )
        .eval(&arena);
    assert!(malformed.term.is_err());
    assert!(malformed.info.logs.is_empty());
}

#[tokio::test]
async fn imported_external_layout_is_constructed_and_matched_by_native_nash() {
    let artifacts = outputs(&[
        ("External.ak", "pub type Packet { @tag(19) Packet(Int) }\npub fn encoded(packet: Packet) -> Data { let encoded: Data = packet\n encoded }"),
        ("Main.nash", "validator module Main exposing (main)\nimport Builtin exposing (..)\nimport External\nmain : Data -> unit\nmain input = assert (Builtin.equalsInteger (Builtin.fstPair (Builtin.unConstrData (External.encoded (External.Packet (Builtin.unIData input))))) (Builtin.unIData input))\n"),
    ]).await;
    let arena = Arena::new();
    let program = syn::parse_program(&arena, &artifacts[0].uplc)
        .into_result()
        .unwrap();
    for (input, succeeds) in [(19, true), (0, false)] {
        let result = program
            .apply(
                &arena,
                Term::data(&arena, PlutusData::integer_from(&arena, input)),
            )
            .eval(&arena);
        assert_eq!(result.term.is_ok(), succeeds);
    }
}

#[tokio::test]
async fn shared_dependency_is_specialized_in_each_workspace_environment() {
    use nash_driver::{PackageId, PackageSourceId};
    let mut catalog = ModuleCatalog::new();
    let remote = PackageId {
        name: Some("example/shared".into()),
        version: "1.0.0".into(),
        source: PackageSourceId::Github,
    };
    for (member, env, entry) in [
        (
            "A",
            "module Env exposing (value)\nvalue = ()\n",
            "validator module EntryA exposing (main)\nimport Shared\nmain _ = Shared.get ()\n",
        ),
        (
            "B",
            "module Env exposing (value)\nimport Builtin exposing (type bool(..))\nvalue = True\n",
            "validator module EntryB exposing (main)\nimport Builtin exposing (type bool(..))\nimport Shared\nmain _ = case Shared.get () of\n    True -> ()\n    False -> ()\n",
        ),
    ] {
        let root = std::path::PathBuf::from(format!("/workspace/{member}"));
        let package = PackageId {
            name: Some(format!("example/{member}")),
            version: "1.0.0".into(),
            source: PackageSourceId::Local(root.clone()),
        };
        for (module, source, owner) in [
            ("Env".to_owned(), env, package.clone()),
            (format!("Entry{member}"), entry, package.clone()),
            (
                "Shared".to_owned(),
                "module Shared exposing (get)\nimport Env\nget () = Env.value\n",
                remote.clone(),
            ),
        ] {
            let uri = Url::from_file_path(root.join(format!("{module}.nash"))).unwrap();
            let mut spec = SourceSpec::new(&uri, &root, None).unwrap();
            spec.key.package = owner;
            spec.visible_packages = Some(vec![package.clone(), remote.clone()]);
            spec.resolution_root = Some(root.clone());
            spec.origin = nash_frontend::SourceOrigin::Synthetic;
            spec.synthetic_source = Some(source.into());
            catalog.insert(uri, spec);
        }
    }
    // No backing files exist: inspection and compilation must both consume synthetic text.
    let db = Arc::new(Mutex::new(Database::new(InMemorySource::new())));
    let graph = build_graph(db.clone(), &catalog).await.unwrap();
    let (report, artifacts) = build_with(db, &graph, &catalog, |solved| {
        build_validators(solved, TraceConfig::default())
    })
    .await;
    assert!(report.is_success(), "{report:?}");
    let artifacts = artifacts.unwrap().unwrap();
    assert_eq!(
        artifacts
            .iter()
            .map(|artifact| artifact.module.as_str())
            .collect::<Vec<_>>(),
        ["EntryA", "EntryB"]
    );
    for artifact in artifacts {
        let arena = Arena::new();
        let program = syn::parse_program(&arena, &artifact.uplc)
            .into_result()
            .unwrap();
        let result = program
            .apply(
                &arena,
                Term::data(&arena, PlutusData::integer_from(&arena, 0)),
            )
            .eval(&arena);
        assert_eq!(
            result.term.unwrap(),
            Term::unit(&arena),
            "{}",
            artifact.module
        );
    }
}
