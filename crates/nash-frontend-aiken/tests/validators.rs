//! Compare the supported boundary with the exact-pinned official compiler, not
//! a locally installed CLI. No official semantic pipeline is used in production.
use aiken_lang::{
    IdGenerator,
    ast::{Definition, ModuleKind, TraceLevel, Tracing},
    builtins,
    gen_uplc::CodeGenerator,
    line_numbers::LineNumbers,
    parser,
    plutus_version::PlutusVersion,
};
use nash_driver::{Database, InMemorySource, ModuleCatalog, SourceSpec, build_graph, build_with};
use nash_plutus::{arena::Arena, data::PlutusData, syn, term::Term};
use std::{collections::HashMap, path::Path, sync::Arc};
use tokio::sync::Mutex;
use uplc::{
    ast::{Data, NamedDeBruijn, Program},
    machine::cost_model::ExBudget,
};
use url::Url;

fn official(source: &str) -> Program<NamedDeBruijn> {
    let (mut module, _) = parser::module(source, ModuleKind::Validator).unwrap();
    module.name = "fixture".into();
    let ids = IdGenerator::new();
    let mut modules = HashMap::from([
        (builtins::PRELUDE.to_owned(), builtins::prelude(&ids)),
        (builtins::BUILTIN.to_owned(), builtins::plutus(&ids)),
    ]);
    let tracing = Tracing::UserDefined(TraceLevel::Verbose);
    let typed = module
        .infer(
            &ids,
            ModuleKind::Validator,
            "test/project",
            &modules,
            tracing,
            &mut vec![],
            None,
        )
        .unwrap();
    modules.insert(typed.name.clone(), typed.type_info.clone());
    let functions = builtins::prelude_functions(&ids, &modules);
    let data_types = builtins::prelude_data_types(&ids);
    let source_info = (source.to_owned(), LineNumbers::new(source));
    let mut generator = CodeGenerator::new(
        PlutusVersion::V3,
        functions.iter().collect(),
        Default::default(),
        data_types.iter().collect(),
        modules
            .iter()
            .map(|(name, info)| (name.as_str(), info))
            .collect(),
        [("fixture", &source_info)].into_iter().collect(),
        tracing,
    );
    let validator = typed
        .definitions
        .iter()
        .find_map(|def| match def {
            Definition::Validator(validator) => Some(validator),
            _ => None,
        })
        .unwrap();
    generator
        .generate(validator, "fixture")
        .to_named_debruijn()
        .unwrap()
}

async fn nash(source: &str) -> String {
    let memory = InMemorySource::new();
    let uri = Url::parse("file:///project/fixture.ak").unwrap();
    memory.insert(uri.clone(), source.to_owned());
    let spec = SourceSpec::new(&uri, Path::new("/project"), None).unwrap();
    let catalog = ModuleCatalog::from([(uri, spec)]);
    let db = Arc::new(Mutex::new(Database::new(memory)));
    let graph = build_graph(db.clone(), &catalog).await.unwrap();
    let (report, artifacts) = build_with(db, &graph, &catalog, |solved| {
        nash_driver::build::build_validators(
            solved,
            nash_codegen::build::TraceConfig {
                user: nash_codegen::build::TraceLevel::Verbose,
                compiler: false,
            },
        )
    })
    .await;
    assert!(report.is_success(), "{report:?}");
    let mut outputs = artifacts.unwrap().unwrap();
    assert_eq!(outputs.len(), 1);
    outputs.remove(0).uplc
}

async fn compare(source: &str, cases: Vec<(Vec<uplc::PlutusData>, bool, Vec<&str>)>) {
    let reference = official(source);
    let output = nash(source).await;
    let arena = Arena::new();
    let compiled = syn::parse_program(&arena, &output).into_result().unwrap();
    for (inputs, succeeds, logs) in cases {
        let mut expected = reference.clone();
        let mut actual = compiled;
        for input in inputs {
            let bytes = uplc::plutus_data_to_bytes(&input);
            actual = actual.apply(
                &arena,
                Term::data(&arena, PlutusData::from_cbor(&arena, &bytes).unwrap()),
            );
            expected = expected.apply_data(input);
        }
        let expected = expected.eval(ExBudget::max());
        let actual = actual.eval(&arena);
        assert_eq!(expected.result.is_ok(), succeeds, "official: {expected:?}");
        assert_eq!(actual.term.is_ok(), succeeds, "Nash: {actual:?}");
        assert_eq!(actual.info.logs, expected.logs());
        assert_eq!(actual.info.logs, logs);
        if succeeds {
            assert_eq!(actual.term.unwrap(), Term::unit(&arena));
            assert_eq!(
                expected.unwrap_constant().unwrap(),
                uplc::ast::Constant::Unit
            );
        }
    }
}

fn context(purpose: u64, redeemer: uplc::PlutusData) -> uplc::PlutusData {
    Data::constr(
        0,
        vec![
            Data::integer(7.into()),
            redeemer,
            Data::constr(purpose, vec![Data::bytestring(b"policy".to_vec())]),
        ],
    )
}

#[tokio::test]
async fn pinned_aiken_and_nash_agree_on_dispatch_boundaries_results_and_traces() {
    compare(
        include_str!("../../nash-driver/tests/fixtures/aiken/validator/src/mint.ak"),
        vec![
            (
                vec![context(0, Data::integer(1.into()))],
                true,
                vec!["mint"],
            ),
            (
                vec![context(0, Data::integer(0.into()))],
                false,
                vec!["mint"],
            ),
            (vec![context(1, Data::integer(1.into()))], false, vec![]),
            (vec![context(0, Data::bytestring(vec![]))], false, vec![]),
            (vec![Data::integer(0.into())], false, vec![]),
            (vec![Data::constr(0, vec![])], false, vec![]),
        ],
    )
    .await;
    let bytes = r#"
use aiken/builtin.{equals_data, length_of_bytearray}
validator bytes(expected: Data) {
  mint(redeemer: ByteArray, policy: ByteArray, transaction: Data) {
    trace @"bytes"
    equals_data(expected, transaction) && length_of_bytearray(policy) > 0 && length_of_bytearray(redeemer) > 0
  }
}
"#;
    compare(
        bytes,
        vec![
            (
                vec![
                    Data::integer(7.into()),
                    context(0, Data::bytestring(vec![42])),
                ],
                true,
                vec!["bytes"],
            ),
            (
                vec![
                    Data::integer(8.into()),
                    context(0, Data::bytestring(vec![42])),
                ],
                false,
                vec!["bytes"],
            ),
            (
                vec![
                    Data::integer(7.into()),
                    context(0, Data::integer(42.into())),
                ],
                false,
                vec![],
            ),
            (
                vec![
                    Data::integer(7.into()),
                    context(1, Data::integer(42.into())),
                ],
                false,
                vec![],
            ),
        ],
    )
    .await;
    compare(
        "use aiken/builtin.{equals_data}\nvalidator raw(expected: Data) { else(context: Data) { equals_data(expected, context) } }",
        vec![
            (vec![Data::integer(1.into()), Data::integer(1.into())], true, vec![]),
            (vec![Data::integer(1.into()), Data::integer(2.into())], false, vec![]),
        ],
    ).await;
}

#[tokio::test]
async fn named_redeemer_decode_is_checked_even_when_body_does_not_use_it() {
    compare(
        "validator checked { mint(r: Int, _policy: ByteArray, _tx: Data) { True } }",
        vec![
            (vec![context(0, Data::integer(1.into()))], true, vec![]),
            (vec![context(0, Data::bytestring(vec![]))], false, vec![]),
            (
                vec![Data::constr(
                    0,
                    vec![
                        Data::integer(7.into()),
                        Data::integer(1.into()),
                        Data::constr(0, vec![Data::integer(42.into())]),
                    ],
                )],
                false,
                vec![],
            ),
            (
                vec![Data::constr(
                    0,
                    vec![
                        Data::integer(7.into()),
                        Data::integer(1.into()),
                        Data::constr(0, vec![]),
                    ],
                )],
                false,
                vec![],
            ),
        ],
    )
    .await;
}
