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
use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
    sync::Arc,
};
use tokio::sync::Mutex;
use uplc::{
    ast::{Data, NamedDeBruijn, Program},
    machine::cost_model::ExBudget,
};
use url::Url;

fn official(source: &str) -> Program<NamedDeBruijn> {
    official_modules(&[("fixture", source)])
}

fn official_modules(sources: &[(&str, &str)]) -> Program<NamedDeBruijn> {
    let mut validators = official_validators(sources);
    assert_eq!(validators.len(), 1);
    validators.pop_first().unwrap().1
}

fn official_validators(sources: &[(&str, &str)]) -> BTreeMap<String, Program<NamedDeBruijn>> {
    let ids = IdGenerator::new();
    let mut modules = HashMap::from([
        (builtins::PRELUDE.to_owned(), builtins::prelude(&ids)),
        (builtins::BUILTIN.to_owned(), builtins::plutus(&ids)),
    ]);
    let tracing = Tracing::UserDefined(TraceLevel::Verbose);
    let mut functions = builtins::prelude_functions(&ids, &modules);
    let mut data_types = builtins::prelude_data_types(&ids);
    let mut constants = Default::default();
    let mut entry = None;
    let mut source_info = HashMap::new();
    for (index, (name, source)) in sources.iter().enumerate() {
        let kind = if index + 1 == sources.len() {
            ModuleKind::Validator
        } else {
            ModuleKind::Lib
        };
        let (mut module, _) = parser::module(source, kind).unwrap();
        module.name = (*name).into();
        let typed = module
            .infer(
                &ids,
                kind,
                "test/project",
                &modules,
                tracing,
                &mut vec![],
                None,
            )
            .unwrap();
        modules.insert(typed.name.clone(), typed.type_info.clone());
        typed.register_definitions(&mut functions, &mut constants, &mut data_types);
        source_info.insert(*name, ((*source).to_owned(), LineNumbers::new(source)));
        entry = Some(typed);
    }
    let mut generator = CodeGenerator::new(
        PlutusVersion::V3,
        functions.iter().collect(),
        constants.iter().collect(),
        data_types.iter().collect(),
        modules
            .iter()
            .map(|(name, info)| (name.as_str(), info))
            .collect(),
        source_info
            .iter()
            .map(|(name, info)| (*name, info))
            .collect(),
        tracing,
    );
    let entry = entry.unwrap();
    entry
        .definitions
        .iter()
        .filter_map(|definition| {
            let Definition::Validator(validator) = definition else {
                return None;
            };
            Some((
                validator.name.clone(),
                generator
                    .generate(validator, &entry.name)
                    .to_named_debruijn()
                    .unwrap(),
            ))
        })
        .collect()
}

async fn nash(source: &str) -> String {
    nash_modules(&[("fixture", source)]).await
}

async fn nash_modules(sources: &[(&str, &str)]) -> String {
    let mut outputs = nash_validators(sources).await;
    assert_eq!(outputs.len(), 1);
    outputs.remove(0).uplc
}

async fn nash_validators(sources: &[(&str, &str)]) -> Vec<nash_driver::build::ValidatorOutput> {
    let memory = InMemorySource::new();
    let mut catalog = ModuleCatalog::new();
    for (name, source) in sources {
        let uri = Url::parse(&format!("file:///project/{name}.ak")).unwrap();
        memory.insert(uri.clone(), (*source).to_owned());
        let spec = SourceSpec::new(&uri, Path::new("/project"), None).unwrap();
        catalog.insert(uri, spec);
    }
    nash_catalog(memory, catalog).await
}

async fn nash_catalog(
    memory: InMemorySource,
    catalog: ModuleCatalog,
) -> Vec<nash_driver::build::ValidatorOutput> {
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
    artifacts.unwrap().unwrap()
}

async fn compare(source: &str, cases: Vec<(Vec<uplc::PlutusData>, bool, Vec<&str>)>) {
    let reference = official(source);
    let output = nash(source).await;
    compare_programs(reference, &output, cases);
}

fn compare_programs(
    reference: Program<NamedDeBruijn>,
    output: &str,
    cases: Vec<(Vec<uplc::PlutusData>, bool, Vec<&str>)>,
) {
    let arena = Arena::new();
    let compiled = syn::parse_program(&arena, output).into_result().unwrap();
    for (case, (inputs, succeeds, logs)) in cases.into_iter().enumerate() {
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
        assert_eq!(
            expected.result.is_ok(),
            succeeds,
            "case {case}, official: {expected:?}"
        );
        assert_eq!(
            actual.term.is_ok(),
            succeeds,
            "case {case}, Nash: {actual:?}\n{output}"
        );
        assert_eq!(actual.info.logs, expected.logs(), "case {case}");
        assert_eq!(actual.info.logs, logs, "case {case}");
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

#[tokio::test]
async fn data_layouts_and_nested_expect_match_pinned_aiken() {
    let source = r#"
use aiken/builtin.{equals_data}
type Choice { Empty Pick(Int, ByteArray) }
type Tagged { @tag(9) Tagged(Int) }
@list
type Packed { choices: List<Choice>, pair: Pair<Int, Tagged>, tuple: (Int, ByteArray) }
fn encoded() -> Data {
  let result: Data = Packed([Empty, Pick(42, #"ab")], Pair(7, Tagged(8)), (9, #"cd"))
  result
}
validator layout {
  else(context: Data) {
    expect decoded: Packed = context
    let roundtrip: Data = decoded
    let Packed(choices, Pair(number, Tagged(inner)), (last, _)) = decoded
    when choices is {
      [Empty, Pick(value, _)] -> value > 41 && number > 6 && inner > 7 && last > 8 && equals_data(roundtrip, encoded())
      _ -> False
    }
  }
}
"#;
    let valid = Data::list(vec![
        Data::list(vec![
            Data::constr(0, vec![]),
            Data::constr(
                1,
                vec![Data::integer(42.into()), Data::bytestring(vec![0xab])],
            ),
        ]),
        Data::list(vec![
            Data::integer(7.into()),
            Data::constr(9, vec![Data::integer(8.into())]),
        ]),
        Data::list(vec![Data::integer(9.into()), Data::bytestring(vec![0xcd])]),
    ]);
    let malformed = Data::list(vec![
        Data::list(vec![Data::constr(
            1,
            vec![Data::bytestring(vec![]), Data::bytestring(vec![0xab])],
        )]),
        Data::list(vec![
            Data::integer(7.into()),
            Data::constr(9, vec![Data::integer(8.into())]),
        ]),
        Data::list(vec![Data::integer(9.into()), Data::bytestring(vec![0xcd])]),
    ]);
    compare(
        source,
        vec![
            (vec![valid], true, vec![]),
            (vec![malformed], false, vec![]),
            (vec![Data::constr(0, vec![])], false, vec![]),
        ],
    )
    .await;
}

#[tokio::test]
async fn discarded_expect_validates_every_nested_field() {
    let source = r#"
type Box<a> { Box(a) }
validator checked {
  else(context: Data) {
    expect _: List<Box<(Int, ByteArray)>> = context
    True
  }
}
"#;
    compare(
        source,
        vec![
            (
                vec![Data::list(vec![Data::constr(
                    0,
                    vec![Data::list(vec![
                        Data::integer(1.into()),
                        Data::bytestring(vec![]),
                    ])],
                )])],
                true,
                vec![],
            ),
            (
                vec![Data::list(vec![Data::constr(
                    0,
                    vec![Data::list(vec![
                        Data::integer(1.into()),
                        Data::integer(2.into()),
                    ])],
                )])],
                false,
                vec![],
            ),
            (
                vec![Data::list(vec![Data::constr(
                    1,
                    vec![Data::list(vec![
                        Data::integer(1.into()),
                        Data::bytestring(vec![]),
                    ])],
                )])],
                false,
                vec![],
            ),
        ],
    )
    .await;
}

#[tokio::test]
async fn qualified_generic_decoders_do_not_collide() {
    let sources = [
        ("left", "pub type Box<a> { @tag(3) Box(a) }"),
        ("right", "pub type Box<a> { @tag(7) Box(a) }"),
        (
            "fixture",
            r#"
use left
use right
validator identity {
  else(context: Data) {
    expect _: (left.Box<Int>, right.Box<ByteArray>, left.Box<ByteArray>) = context
    True
  }
}
"#,
        ),
    ];
    let reference = official_modules(&sources);
    let output = nash_modules(&sources).await;
    compare_programs(
        reference,
        &output,
        vec![
            (
                vec![Data::list(vec![
                    Data::constr(3, vec![Data::integer(1.into())]),
                    Data::constr(7, vec![Data::bytestring(vec![])]),
                    Data::constr(3, vec![Data::bytestring(vec![])]),
                ])],
                true,
                vec![],
            ),
            (
                vec![Data::list(vec![
                    Data::constr(3, vec![Data::integer(1.into())]),
                    Data::constr(3, vec![Data::bytestring(vec![])]),
                    Data::constr(3, vec![Data::bytestring(vec![])]),
                ])],
                false,
                vec![],
            ),
            (
                vec![Data::list(vec![
                    Data::constr(3, vec![Data::integer(1.into())]),
                    Data::constr(7, vec![Data::bytestring(vec![])]),
                    Data::constr(3, vec![Data::integer(1.into())]),
                ])],
                false,
                vec![],
            ),
        ],
    );
}

#[tokio::test]
async fn all_validator_purposes_preserve_positions_and_boolean_failure() {
    let source = r#"
use aiken/builtin.{equals_integer, equals_data, length_of_bytearray}
validator purposes(expected: Int) {
  mint(r: Int, policy: ByteArray, tx: Data) {
    trace @"mint"
    r > expected && length_of_bytearray(policy) > 0
  }
  spend(datum: Option<Int>, r: Int, output: Data, tx: Data) {
    trace @"spend"
    when datum is { Some(value) -> equals_integer(value, 42) && r > expected None -> r > expected }
  }
  withdraw(r: Int, credential: Data, tx: Data) { trace @"withdraw" r > expected && equals_data(credential, tx) }
  publish(r: Int, certificate: Data, tx: Data) { trace @"publish" r > expected && equals_data(certificate, tx) }
  vote(r: Int, voter: Data, tx: Data) { trace @"vote" r > expected && equals_data(voter, tx) }
  propose(r: Int, proposal: Data, tx: Data) { trace @"propose" r > expected && equals_data(proposal, tx) }
}
"#;
    let mut cases = Vec::new();
    for (tag, name) in ["mint", "spend", "withdraw", "publish", "vote", "propose"]
        .into_iter()
        .enumerate()
    {
        let purpose_fields = match tag {
            0 => vec![Data::bytestring(vec![1])],
            1 => vec![
                Data::integer(7.into()),
                Data::constr(0, vec![Data::integer(42.into())]),
            ],
            3 | 5 => vec![Data::integer(0.into()), Data::integer(7.into())],
            _ => vec![Data::integer(7.into())],
        };
        for (redeemer, succeeds) in [(2, true), (0, false)] {
            cases.push((
                vec![
                    Data::integer(1.into()),
                    Data::constr(
                        0,
                        vec![
                            Data::integer(7.into()),
                            Data::integer(redeemer.into()),
                            Data::constr(tag as u64, purpose_fields.clone()),
                        ],
                    ),
                ],
                succeeds,
                vec![name],
            ));
        }
    }
    for tag in [3, 5] {
        cases.push((
            vec![
                Data::integer(1.into()),
                Data::constr(
                    0,
                    vec![
                        Data::integer(7.into()),
                        Data::integer(2.into()),
                        Data::constr(tag, vec![Data::bytestring(vec![]), Data::integer(7.into())]),
                    ],
                ),
            ],
            false,
            vec![],
        ));
    }
    for (datum, succeeds, logs) in [
        (Data::constr(1, vec![]), true, vec!["spend"]),
        (
            Data::constr(0, vec![Data::bytestring(vec![])]),
            false,
            vec!["spend"],
        ),
        (Data::constr(2, vec![]), true, vec!["spend"]),
    ] {
        cases.push((
            vec![
                Data::integer(1.into()),
                Data::constr(
                    0,
                    vec![
                        Data::integer(7.into()),
                        Data::integer(2.into()),
                        Data::constr(1, vec![Data::integer(7.into()), datum]),
                    ],
                ),
            ],
            succeeds,
            logs,
        ));
    }
    cases.push((
        vec![Data::integer(1.into()), context(6, Data::integer(2.into()))],
        false,
        vec![],
    ));
    compare(source, cases).await;
}

#[tokio::test]
async fn arbitrary_precision_literals_and_patterns_survive_codegen() {
    let source = r#"
validator integers {
  else(context: Data) {
    expect value: Int = context
    when value is {
      340282366920938463463374607431768211456 -> value < 340282366920938463463374607431768211457
      -340282366920938463463374607431768211456 -> value > -340282366920938463463374607431768211457
      _ -> False
    }
  }
}
"#;
    compare(
        source,
        vec![
            (
                vec![Data::integer(
                    "340282366920938463463374607431768211456".parse().unwrap(),
                )],
                true,
                vec![],
            ),
            (
                vec![Data::integer(
                    "-340282366920938463463374607431768211456".parse().unwrap(),
                )],
                true,
                vec![],
            ),
            (vec![Data::integer(0.into())], false, vec![]),
        ],
    )
    .await;
}

#[tokio::test]
async fn custom_layout_validator_cli_fixture_matches_official_boundary() {
    compare(
        include_str!("../../nash-driver/tests/fixtures/aiken/runtime/src/custom.ak"),
        vec![
            (
                vec![context(0, Data::constr(9, vec![Data::integer(1.into())]))],
                true,
                vec!["custom"],
            ),
            (
                vec![context(0, Data::constr(9, vec![Data::integer(0.into())]))],
                false,
                vec!["custom"],
            ),
            (
                vec![context(0, Data::constr(0, vec![Data::integer(1.into())]))],
                false,
                vec![],
            ),
            (
                vec![context(0, Data::constr(9, vec![Data::bytestring(vec![])]))],
                false,
                vec![],
            ),
        ],
    )
    .await;
}

#[tokio::test]
async fn validator_parameters_use_shallow_extraction_not_expect_validation() {
    compare(
        "validator shallow(value: (Int, ByteArray)) { else(_) { True } }",
        vec![
            (
                vec![
                    Data::list(vec![Data::integer(1.into()), Data::integer(2.into())]),
                    Data::integer(0.into()),
                ],
                true,
                vec![],
            ),
            (
                vec![Data::integer(1.into()), Data::integer(0.into())],
                false,
                vec![],
            ),
        ],
    )
    .await;
    compare(
        "validator shallow(value: Bool) { else(_) { True } }",
        vec![
            (
                vec![
                    Data::constr(9, vec![Data::integer(0.into())]),
                    Data::integer(0.into()),
                ],
                true,
                vec![],
            ),
            (
                vec![Data::integer(1.into()), Data::integer(0.into())],
                false,
                vec![],
            ),
        ],
    )
    .await;
    compare(
        "validator shallow(value: Void) { else(_) { True } }",
        vec![(
            vec![Data::integer(1.into()), Data::integer(0.into())],
            true,
            vec![],
        )],
    )
    .await;
    compare(
        "validator shallow(value: Pair<Int, ByteArray>) { else(_) { True } }",
        vec![
            (
                vec![
                    Data::list(vec![
                        Data::integer(1.into()),
                        Data::integer(2.into()),
                        Data::integer(3.into()),
                    ]),
                    Data::integer(0.into()),
                ],
                true,
                vec![],
            ),
            (
                vec![
                    Data::list(vec![Data::integer(1.into())]),
                    Data::integer(0.into()),
                ],
                false,
                vec![],
            ),
        ],
    )
    .await;
    compare(
        "validator boundary { withdraw(r: Int, credential: Data, tx: Data) { True } }",
        vec![(
            vec![Data::constr(
                0,
                vec![
                    Data::integer(7.into()),
                    Data::integer(1.into()),
                    Data::constr(2, vec![]),
                ],
            )],
            false,
            vec![],
        )],
    )
    .await;
    compare(
        "validator boundary { withdraw(r: Int, _credential: Data, _tx: Data) { True } }",
        vec![(
            vec![Data::constr(
                0,
                vec![
                    Data::integer(7.into()),
                    Data::integer(1.into()),
                    Data::constr(2, vec![]),
                ],
            )],
            false,
            vec![],
        )],
    )
    .await;
    compare("pub type Packet { Packet(Int) }\nvalidator boundary { withdraw(r: Int, credential: Data, tx: Packet) { let Packet(_) = tx True } }", vec![
        (vec![Data::constr(0, vec![Data::integer(7.into()), Data::integer(1.into()), Data::constr(2, vec![Data::integer(7.into())])])], true, vec![]),
    ]).await;
}

#[tokio::test]
async fn opaque_single_field_data_preserves_official_transparency() {
    compare(
        r#"
use aiken/builtin.{equals_data}
pub opaque type Secret { Secret(Int) }
validator secrecy {
  else(context: Data) {
    let value: Data = Secret(42)
    equals_data(context, value)
  }
}
"#,
        vec![
            (vec![Data::integer(42.into())], true, vec![]),
            (
                vec![Data::constr(0, vec![Data::integer(42.into())])],
                false,
                vec![],
            ),
        ],
    )
    .await;
}

#[tokio::test]
async fn encoded_maps_and_primitive_fields_validate_recursively() {
    let source = r#"
use aiken/builtin.{equals_data, equals_string}
pub type Record { entries: List<Pair<Int, ByteArray>>, flag: Bool, text: String, done: Void }
fn encoded() -> Data {
  let value: Data = Record([Pair(1, #"ab")], True, @"ok", Void)
  value
}
validator primitives {
  else(context: Data) {
    expect record: Record = context
    let Record(entries, flag, text, _) = record
    when entries is {
      [Pair(key, _)] -> flag && key > 0 && equals_string(text, @"ok") && equals_data(context, encoded())
      _ -> False
    }
  }
}
"#;
    let record = |value, text| {
        Data::constr(
            0,
            vec![
                Data::map(vec![(Data::integer(1.into()), value)]),
                Data::constr(1, vec![]),
                Data::bytestring(text),
                Data::constr(0, vec![]),
            ],
        )
    };
    compare(
        source,
        vec![
            (
                vec![record(Data::bytestring(vec![0xab]), b"ok".to_vec())],
                true,
                vec![],
            ),
            (
                vec![record(Data::integer(2.into()), b"ok".to_vec())],
                false,
                vec![],
            ),
            (
                vec![record(Data::bytestring(vec![0xab]), vec![0xff])],
                false,
                vec![],
            ),
        ],
    )
    .await;
    let fallback = "use aiken/builtin.{equals_data}\nvalidator fallback(expected: Data) { mint(r: Int, p: ByteArray, tx: Data) { False } else(raw: Data) { trace @\"fallback\" equals_data(expected, raw) } }";
    let raw = context(4, Data::integer(1.into()));
    compare(
        fallback,
        vec![
            (vec![raw.clone(), raw.clone()], true, vec!["fallback"]),
            (vec![Data::integer(0.into()), raw], false, vec!["fallback"]),
        ],
    )
    .await;
}

async fn compare_assignment_encoding(declaration: &str, binding: &str, encoded: uplc::PlutusData) {
    let source = format!(
        "use aiken/builtin.{{equals_data}}\n{declaration}\n\
         validator encoding {{ else(raw: Data) {{ {binding}\n equals_data(encoded, raw) }} }}"
    );
    compare(
        &source,
        vec![
            (vec![encoded], true, vec![]),
            (vec![Data::bytestring(vec![0xff])], false, vec![]),
        ],
    )
    .await;
}

#[tokio::test]
async fn assignment_encoding_empty_list() {
    compare_assignment_encoding("", "let encoded: Data = []", Data::list(vec![])).await;
}

#[tokio::test]
async fn assignment_encoding_none() {
    compare_assignment_encoding("", "let encoded: Data = None", Data::constr(1, vec![])).await;
}

#[tokio::test]
async fn assignment_encoding_module_constant() {
    compare_assignment_encoding("pub const encoded: Data = 1", "", Data::integer(1.into())).await;
}

#[tokio::test]
async fn assignment_encoding_concrete_list() {
    compare_assignment_encoding(
        "",
        "let values: List<Int> = [1, 2]\nlet encoded: Data = values",
        Data::list(vec![Data::integer(1.into()), Data::integer(2.into())]),
    )
    .await;
}

#[tokio::test]
async fn assignment_encoding_rejects_function_result() {
    let source = "pub fn encoded() -> Data { 1 }";
    let ids = IdGenerator::new();
    let modules = HashMap::from([
        (builtins::PRELUDE.to_owned(), builtins::prelude(&ids)),
        (builtins::BUILTIN.to_owned(), builtins::plutus(&ids)),
    ]);
    let (mut module, _) = parser::module(source, ModuleKind::Lib).unwrap();
    module.name = "fixture".into();
    assert!(
        module
            .infer(
                &ids,
                ModuleKind::Lib,
                "test/project",
                &modules,
                Tracing::UserDefined(TraceLevel::Verbose),
                &mut vec![],
                None,
            )
            .is_err()
    );
    let memory = InMemorySource::new();
    let uri = Url::parse("file:///project/fixture.ak").unwrap();
    memory.insert(uri.clone(), source.to_owned());
    let mut catalog = ModuleCatalog::new();
    catalog.insert(
        uri.clone(),
        SourceSpec::new(&uri, Path::new("/project"), None).unwrap(),
    );
    let db = Arc::new(Mutex::new(Database::new(memory)));
    let graph = build_graph(db.clone(), &catalog).await.unwrap();
    let (report, artifacts) = build_with(db, &graph, &catalog, |_| ()).await;
    assert!(!report.is_success(), "{report:?}");
    assert!(artifacts.is_none());
}

#[tokio::test]
async fn calls_shadowing_and_comparisons_preserve_results_and_trace_order() {
    let source = r#"
use aiken/builtin.{equals_integer}
use ops.{add_one as increment}
fn one() -> Int {
  let value = fn() -> Data { 1 }
  value()
}
fn left(value: Int) -> Int { trace @"left" value }
fn right() -> Int { trace @"right" 0 }
validator calls {
  else(raw: Data) {
    expect x: Int = raw
    let x = increment(x)
    x > 0 && left(x) > right() && ops.equal(#"ab", #"ab") && equals_integer(one(), 1)
  }
}
"#;
    let sources = [
        ("ops", include_str!("fixtures/builtin_calls.ak")),
        ("fixture", source),
    ];
    let reference = official_modules(&sources);
    let output = nash_modules(&sources).await;
    compare_programs(
        reference,
        &output,
        vec![
            (vec![Data::integer(0.into())], true, vec!["right", "left"]),
            (vec![Data::integer((-1).into())], false, vec![]),
        ],
    );
}

#[tokio::test]
async fn encoded_builtin_lists_pairs_and_nullary_nil_match_pinned_runtime() {
    let source = r#"
use aiken/builtin

fn first(xs: List<a>) -> a { builtin.head_list(xs) }
fn prepend(x: a, xs: List<a>) -> List<a> { builtin.cons_list(x, xs) }
fn left(pair: Pair<a, b>) -> a { builtin.fst_pair(pair) }

validator primitives {
  else(raw: Data) {
    expect n: Int = raw
    let ints = prepend(n, [n + 1])
    let pairs = prepend(Pair(n, n + 1), [])
    let encoded = builtin.i_data(n)
    let fields = builtin.cons_list(encoded, builtin.new_list())
    let constr = builtin.constr_data(3, fields)
    let unpacked = builtin.un_constr_data(constr)
    let map = builtin.map_data(
      builtin.cons_list(builtin.new_pair(encoded, encoded), builtin.new_pairs()),
    )
    and {
      first(ints) == n,
      first(builtin.tail_list(ints)) == n + 1,
      left(first(pairs)) == n,
      builtin.snd_pair(first(pairs)) == n + 1,
      builtin.fst_pair(unpacked) == 3,
      builtin.unconstr_index(constr) == 3,
      builtin.equals_data(first(builtin.snd_pair(unpacked)), encoded),
      builtin.equals_data(first(builtin.unconstr_fields(constr)), encoded),
      builtin.equals_data(first(builtin.un_list_data(builtin.list_data(fields))), encoded),
      builtin.equals_data(builtin.fst_pair(first(builtin.un_map_data(map))), encoded),
      builtin.null_list(builtin.new_list()),
      builtin.null_list(builtin.new_pairs()),
      builtin.choose_void(encoded, True),
      builtin.write_bits(#"00", [if n < 0 { -n } else { n }], True) == #"08",
    }
  }
}
"#;
    compare(
        source,
        vec![
            (vec![Data::integer(3.into())], true, vec![]),
            (vec![Data::integer((-3).into())], true, vec![]),
        ],
    )
    .await;
}

#[tokio::test]
async fn compiler_prelude_values_layouts_and_diagnostic_suffixes_match_pinned_runtime() {
    let source = r#"
use aiken/builtin

validator prelude {
  else(raw: Data) {
    expect n: Int = raw
    let decimal = if n < 0 { #"2d3321" } else { #"3321" }
    let rendered = if n < 0 { @"124([_ -3])!" } else { @"124([_ 3])!" }
    let encoded: Data = n
    let ordering: Data = Greater
    let never: Data = Never
    let seeded: Data = Seeded { seed: #"01", choices: #"02" }
    let constr = builtin.constr_data(3, [encoded])
    and {
      identity(n) == n,
      always(n, n + 1) == n,
      not(False),
      flip(fn(a: Int, b: Int) { a - b })(n, n + 2) == 2,
      builtin.equals_data(as_data(encoded), encoded),
      enumerate([n, n + 1], 0, fn(x, acc) { x + acc }, fn(x, acc) { 2 * x + acc }) == 3 * n + 2,
      from_int(n, #"21") == decimal,
      do_from_int(0, #"21") == #"21",
      encode_base16(#"0aef", 1, #"21") == #"3041454621",
      builtin.decode_utf8(diagnostic(constr, #"21")) == rendered,
      builtin.decode_utf8(diagnostic(builtin.map_data([Pair(encoded, encoded)]), #"")) ==
        if n < 0 { @"{_ -3: -3 }" } else { @"{_ 3: 3 }" },
      builtin.decode_utf8(diagnostic(builtin.b_data(#"0aef"), #"21")) == @"h'0AEF'!",
      builtin.equals_data(ordering, builtin.constr_data(2, [])),
      builtin.equals_data(never, builtin.constr_data(1, [])),
      builtin.equals_data(seeded, builtin.constr_data(0, [builtin.b_data(#"01"), builtin.b_data(#"02")])),
    }
  }
}
"#;
    compare(
        source,
        vec![
            (vec![Data::integer(3.into())], true, vec![]),
            (vec![Data::integer((-3).into())], true, vec![]),
        ],
    )
    .await;
}

#[tokio::test]
async fn solved_calls_projections_and_updates_preserve_order_and_untouched_data() {
    compare(
        r#"
use aiken/builtin
pub type Packet { count: Int, owner: Int }
fn difference(left a: Int, right b: Int) -> Int { a - b }
fn mark(value: Int) -> Int { trace @"input" value }
validator surface {
  withdraw(_redeemer: Data, _credential: Data, original: Packet) {
    let unused_trace = { trace @"unused" 99 }
    let unused_failure: Int = { fail }
    let _ = { trace @"discarded" fail }
    let delta = mark(10) |> difference(right: 4)
    let updated = Packet { ..original, count: delta }
    let coordinates = (updated.count, 2, 3)
    let pair = Pair(coordinates.1st, coordinates.3rd)
    trace @"values": pair.1st, pair.2nd
    let encoded: Data = updated
    and {
      pair.1st == 6,
      pair.2nd == 3,
      builtin.equals_data(encoded, builtin.constr_data(0, [
        builtin.i_data(6), builtin.b_data(#"ff"),
      ])),
    }
  }
}
"#,
        vec![(
            vec![Data::constr(
                0,
                vec![
                    Data::constr(
                        0,
                        vec![Data::integer(5.into()), Data::bytestring(vec![255])],
                    ),
                    Data::integer(0.into()),
                    Data::constr(2, vec![Data::integer(1.into())]),
                ],
            )],
            true,
            vec!["input", "values: 6, 3"],
        )],
    )
    .await;
    compare(
        r#"
validator shared_binding {
  else(raw: Data) {
    expect flag: Bool = raw
    let value = { trace @"shared" 5 }
    if flag { value == 6 && value > 0 } else { False }
  }
}
"#,
        vec![
            (vec![Data::constr(0, vec![])], false, vec!["shared"]),
            (vec![Data::constr(1, vec![])], false, vec!["shared"]),
        ],
    )
    .await;
}

#[tokio::test]
async fn if_is_validates_nested_data_and_evaluates_the_subject_once() {
    compare(
        r#"
fn observed(value: Data) -> Data { trace @"subject" value }
validator refutation {
  else(context: Data) {
    if observed(context) is values: List<Int> {
      trace @"decoded"
      values == [4]
    } else {
      trace @"fallback"
      True
    }
  }
}
"#,
        vec![
            (
                vec![Data::list(vec![Data::integer(4.into())])],
                true,
                vec!["subject", "decoded"],
            ),
            (
                vec![Data::list(vec![Data::integer(5.into())])],
                false,
                vec!["subject", "decoded"],
            ),
            (
                vec![Data::list(vec![Data::bytestring(vec![255])])],
                true,
                vec!["subject", "fallback"],
            ),
            (
                vec![Data::integer(4.into())],
                true,
                vec!["subject", "fallback"],
            ),
        ],
    )
    .await;
}

#[tokio::test]
async fn declaration_holes_share_constraints_across_imports_but_not_declarations() {
    let cases: &[(&[(&str, &str)], bool)] = &[
        (
            &[(
                "local",
                "pub type Unknown = _\npub fn first(value: Unknown) -> Int { value }\npub fn second(value: Unknown) -> Bool { value }",
            )],
            false,
        ),
        (
            &[
                ("holes", "pub type Unknown = _"),
                (
                    "use_int",
                    "use holes.{Unknown}\npub fn read_int(value: Unknown) -> Int { value }",
                ),
                (
                    "use_bool",
                    "use holes.{Unknown}\npub fn read_bool(value: Unknown) -> Bool { value }",
                ),
            ],
            false,
        ),
        (
            &[
                ("int_holes", "pub type Unknown = _"),
                ("bool_holes", "pub type Unknown = _"),
                (
                    "use_int",
                    "use int_holes.{Unknown}\npub fn read_int(value: Unknown) -> Int { value }",
                ),
                (
                    "use_bool",
                    "use bool_holes.{Unknown}\npub fn read_bool(value: Unknown) -> Bool { value }",
                ),
            ],
            true,
        ),
    ];
    for (sources, accepted) in cases {
        let ids = IdGenerator::new();
        let mut modules = HashMap::from([
            (builtins::PRELUDE.to_owned(), builtins::prelude(&ids)),
            (builtins::BUILTIN.to_owned(), builtins::plutus(&ids)),
        ]);
        let mut official_accepted = true;
        for (name, source) in *sources {
            let (mut module, _) = parser::module(source, ModuleKind::Lib).unwrap();
            module.name = (*name).into();
            match module.infer(
                &ids,
                ModuleKind::Lib,
                "test/project",
                &modules,
                Tracing::UserDefined(TraceLevel::Verbose),
                &mut vec![],
                None,
            ) {
                Ok(module) => {
                    modules.insert(module.name.clone(), module.type_info);
                }
                Err(_) => {
                    official_accepted = false;
                    break;
                }
            }
        }
        assert_eq!(
            official_accepted, *accepted,
            "official source acceptance: {sources:?}"
        );

        let memory = InMemorySource::new();
        let mut catalog = ModuleCatalog::new();
        for (name, source) in *sources {
            let uri = Url::parse(&format!("file:///project/{name}.ak")).unwrap();
            memory.insert(uri.clone(), (*source).to_owned());
            catalog.insert(
                uri.clone(),
                SourceSpec::new(&uri, Path::new("/project"), None).unwrap(),
            );
        }
        let db = Arc::new(Mutex::new(Database::new(memory)));
        let graph = build_graph(db.clone(), &catalog).await.unwrap();
        let (report, output) = build_with(db, &graph, &catalog, |_| ()).await;
        assert_eq!(report.is_success(), *accepted, "{sources:?}: {report:?}");
        assert_eq!(output.is_some(), *accepted);
        if !accepted {
            assert!(
                report.modules.values().any(|module| {
                    let nash_driver::ModuleResult::Failed(module) = module else {
                        return false;
                    };
                    module.reports.iter().any(|error| {
                        error.code.starts_with("nash::type::")
                            && error.region.start.line > 0
                            && error.region.start.column > 0
                    })
                }),
                "rejected hole constraints must have located type errors: {report:?}"
            );
        }
    }
}

#[tokio::test]
async fn imported_labeled_pipeline_updates_and_qualified_patterns_match_pinned_runtime() {
    let sources = [
        (
            "records",
            r#"
pub type Packet { count: Int, owner: ByteArray, untouched: Int }
pub fn difference(left a: Int, right b: Int) -> Int { a - b }
"#,
        ),
        (
            "fixture",
            r#"
use records.{Packet}
fn observed(value: Int) -> Int { trace @"input" value }
fn apply(value: a, with: fn(a) -> b) -> b { with(value) }
fn owner(packet: Packet) -> ByteArray {
  let records.Packet { owner, .. } = packet
  owner
}
validator record_updates {
  else(raw: Data) {
    expect packet: Packet = raw
    let changed = Packet {
      ..packet,
      count: observed(packet.count) |> records.difference(right: 4),
    }
    let Packet.Packet { count: renamed, .. } = changed
    let nested = renamed |> apply(fn(number) {
      #"ab" |> apply(fn(bytes) { (number, bytes) })
    })
    nested == (6, #"ab") && owner(changed) == #"ff" && changed.untouched == packet.untouched
  }
}
"#,
        ),
    ];
    let reference = official_modules(&sources);
    let output = nash_modules(&sources).await;
    compare_programs(
        reference,
        &output,
        vec![
            (
                vec![Data::constr(
                    0,
                    vec![
                        Data::integer(10.into()),
                        Data::bytestring(vec![255]),
                        Data::integer(9.into()),
                    ],
                )],
                true,
                vec!["input"],
            ),
            (
                vec![Data::constr(
                    0,
                    vec![
                        Data::integer(11.into()),
                        Data::bytestring(vec![255]),
                        Data::integer(9.into()),
                    ],
                )],
                false,
                vec!["input"],
            ),
        ],
    );
}

#[tokio::test]
async fn polymorphic_equality_specializes_without_native_eq_evidence() {
    compare(r#"
use aiken/builtin
type Box<a> { Box(a) }
fn same(left: a, right: a) -> Bool { left == right }
fn different(left: a, right: a) -> Bool { left != right }
validator equality {
  else(raw: Data) {
    expect n: Int = raw
    let g1 = #<Bls12_381, G1>"97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb"
    let g2 = #<Bls12_381, G2>"93e02b6052719f607dacd3a088274f65596bd0d09920b61ab5da61bbdc7f5049334cf11213945d57e5ac7d055d042b7e024aa2b2f08f0a91260805272dc51051c6e47ad4fa403b02b4510b647ae3d1770bac0326a805bbefd48056c8c121bdb8"
    and {
      same(n, n),
      different(n, n + 1),
      same(n > 0, n > 0),
      same(#"0102", #"0102"),
      different(@"left", @"right"),
      same(raw, builtin.i_data(n)),
      same([n, n + 1], [n, n + 1]),
      different(Pair(n, True), Pair(n + 1, True)),
      same((n, #"ff", False), (n, #"ff", False)),
      same(Box(Some(n)), Box(Some(n))),
      different(Box(Some(n)), Box(None)),
      same(builtin.bls12_381_g1_scalar_mul(n, g1), builtin.bls12_381_g1_scalar_mul(n, g1)),
      same(builtin.bls12_381_g2_scalar_mul(n, g2), builtin.bls12_381_g2_scalar_mul(n, g2)),
    }
  }
}
"#, vec![
        (vec![Data::integer(3.into())], true, vec![]),
        (vec![Data::integer((-2).into())], true, vec![]),
    ]).await;
}

#[tokio::test]
async fn trace_arguments_and_false_suffix_preserve_evaluation_order() {
    compare(
        r#"
fn label() -> String { trace @"label" @"values" }
fn first(n: Int) -> Int { trace @"first" n }
fn second(n: Int) -> Int { trace @"second" n }
fn observe(value: Bool) -> Bool { trace @"condition" value }
fn map(value: Option<a>, apply: fn(a) -> b) -> Option<b> {
  when value is { Some(item) -> Some(apply(item)) None -> None }
}
validator traces {
  else(raw: Data) {
    expect n: Int = raw
    trace label(): first(n), second(n + 1)
    let projected = Some((n, True)) |> map(fn(tuple) { tuple.1st })
    let log_key = fn(key: ByteArray, _) { trace @"key": key projected == Some(n) }
    expect True = log_key(#"ab", n)
    observe(n == 1)?
  }
}
"#,
        vec![
            (
                vec![Data::integer(1.into())],
                true,
                vec![
                    "label",
                    "first",
                    "second",
                    "values: 1, 2",
                    "key: h'AB'",
                    "condition",
                ],
            ),
            (
                vec![Data::integer(2.into())],
                false,
                vec![
                    "label",
                    "first",
                    "second",
                    "values: 2, 3",
                    "key: h'AB'",
                    "condition",
                    "observe(n == 1) ? False",
                ],
            ),
        ],
    )
    .await;
}

#[tokio::test]
async fn local_types_can_shadow_prelude_types_without_changing_primitive_values() {
    compare(
        r#"
type Int { Number(ByteArray) }
type String = ByteArray
fn unwrap(value: Int) -> String {
  let Number(bytes) = value
  bytes
}
validator shadow {
  else(raw: Data) {
    expect bytes: ByteArray = raw
    unwrap(Number(bytes)) == #"01"
  }
}
"#,
        vec![
            (vec![Data::bytestring(vec![1])], true, vec![]),
            (vec![Data::bytestring(vec![2])], false, vec![]),
        ],
    )
    .await;
}

#[tokio::test]
async fn two_validators_keep_distinct_parameters_results_and_traces() {
    let source = r#"
fn ints(prng: PRNG) -> Option<(PRNG, Int)> { Some((prng, 1)) }
fn sized(_size: Int) -> Fuzzer<Int> { ints }
test unit_result() { Void }
test generated(value via ints) { value == 1 }
bench sampled(value via sized) { value + 1 }
validator first(expected: Int) {
  else(raw: Data) {
    expect n: Int = raw
    trace @"first"
    n == expected
  }
}
validator second {
  else(raw: Data) {
    expect n: Int = raw
    trace @"second"
    n == 2
  }
}
"#;
    let mut reference = official_validators(&[("fixture", source)]);
    let outputs = nash_validators(&[("fixture", source)]).await;
    assert_eq!(
        outputs.len(),
        2,
        "tool declarations must not produce artifacts"
    );
    assert_ne!(outputs[0].output_name, outputs[1].output_name);
    for output in outputs {
        let name = &output.metadata.as_ref().unwrap().id.name;
        let cases = match name.as_str() {
            "first" => vec![
                (
                    vec![Data::integer(1.into()), Data::integer(1.into())],
                    true,
                    vec!["first"],
                ),
                (
                    vec![Data::integer(1.into()), Data::integer(2.into())],
                    false,
                    vec!["first"],
                ),
            ],
            "second" => vec![
                (vec![Data::integer(2.into())], true, vec!["second"]),
                (vec![Data::integer(1.into())], false, vec!["second"]),
            ],
            other => panic!("unexpected validator {other}"),
        };
        compare_programs(reference.remove(name).unwrap(), &output.uplc, cases);
    }
    assert!(reference.is_empty());
}

#[tokio::test]
async fn package_owned_layout_decodes_at_validator_boundary() {
    use nash_frontend::{PackageId, PackageSourceId, SourceOrigin};
    let sources = [
        (
            "payload",
            "pub type Packet<a> { @tag(7) Packet { value: a, owner: ByteArray } }",
        ),
        (
            "fixture",
            r#"
use payload.{Packet}
validator packaged {
  mint(packet: Packet<Int>, _policy: ByteArray, _transaction: Data) {
    packet.value == 42 && packet.owner == #"ff"
  }
}
"#,
        ),
    ];
    let reference = official_modules(&sources);
    let package = PackageId {
        name: Some("vendor/layouts".to_owned()),
        version: "1.2.3".to_owned(),
        source: PackageSourceId::Github,
    };
    let memory = InMemorySource::new();
    let mut catalog = ModuleCatalog::new();
    for (index, (name, source)) in sources.iter().enumerate() {
        let root = if index == 0 {
            "/packages/layouts"
        } else {
            "/project"
        };
        let uri = Url::parse(&format!("file://{root}/{name}.ak")).unwrap();
        memory.insert(uri.clone(), (*source).to_owned());
        let mut spec = SourceSpec::new(&uri, Path::new(root), None).unwrap();
        if index == 0 {
            spec.key.package = package.clone();
            spec.origin = SourceOrigin::Dependency;
        } else {
            spec.visible_packages = Some(vec![package.clone()]);
        }
        catalog.insert(uri, spec);
    }
    let mut outputs = nash_catalog(memory, catalog).await;
    assert_eq!(outputs.len(), 1);
    let output = outputs.remove(0);
    compare_programs(
        reference,
        &output.uplc,
        vec![
            (
                vec![context(
                    0,
                    Data::constr(
                        7,
                        vec![Data::integer(42.into()), Data::bytestring(vec![255])],
                    ),
                )],
                true,
                vec![],
            ),
            (
                vec![context(
                    0,
                    Data::constr(
                        7,
                        vec![Data::integer(41.into()), Data::bytestring(vec![255])],
                    ),
                )],
                false,
                vec![],
            ),
            (
                vec![context(
                    0,
                    Data::constr(
                        7,
                        vec![Data::bytestring(vec![42]), Data::bytestring(vec![255])],
                    ),
                )],
                false,
                vec![],
            ),
        ],
    );
}

#[tokio::test]
async fn tool_declarations_check_generators_annotations_and_bodies_without_artifacts() {
    let cases = [
        (
            r#"
fn ints(prng: PRNG) -> Option<(PRNG, Int)> { Some((prng, 1)) }
fn sized(_size: Int) -> Fuzzer<Int> { ints }
test bool_result() { True }
test void_result() { Void }
test generated(value: Int via ints) { value == 1 }
bench sampled(value via sized) { value + 1 }
"#,
            true,
        ),
        ("test wrong_result() { 1 }", false),
        ("test wrong_generator(value via 1) { value == 1 }", false),
        (
            "fn generic(prng: PRNG) { Some((prng, [])) }\ntest unresolved(value: List<Int> via generic) { value == [] }",
            false,
        ),
        (
            "fn ints(prng: PRNG) { Some((prng, 1)) }\ntest mismatch(value: Bool via ints) { value }",
            false,
        ),
        ("bench no_sampler() { 1 }", false),
        (
            "fn ints(prng: PRNG) { Some((prng, 1)) }\nbench unsized(value via ints) { value }",
            false,
        ),
        (
            "pub fn invalid(left: fn(Int) -> Int, right: fn(Int) -> Int) { left == right }",
            false,
        ),
    ];
    for (source, accepted) in cases {
        assert_source_acceptance(source, ModuleKind::Lib, accepted).await;
    }
}

#[tokio::test]
async fn backpassing_patterns_evaluate_once_and_expect_refutes() {
    let sources = [
        (
            "rows",
            include_str!("../../nash-driver/tests/fixtures/aiken/full-language/lib/rows.ak"),
        ),
        (
            "patterns",
            include_str!("../../nash-driver/tests/fixtures/aiken/full-language/lib/patterns.ak"),
        ),
        (
            "fixture",
            r#"
use patterns
validator backpassing {
  else(raw: Data) {
    expect n: Int = raw
    patterns.terminal_expect()
    if n > 0 {
      patterns.patterns_once() == 12
    } else {
      patterns.expect_backpass_refutes() == 1
    }
  }
}
"#,
        ),
    ];
    let reference = official_modules(&sources);
    let output = nash_modules(&sources).await;
    compare_programs(
        reference,
        &output,
        vec![
            (vec![Data::integer(1.into())], true, vec!["supply"]),
            (vec![Data::integer(0.into())], false, vec!["supply"]),
        ],
    );
}

#[tokio::test]
async fn pinned_tautology_advertises_void_but_returns_boolean() {
    compare(
        r#"
validator tautology_abi {
  else(raw: Data) {
    expect n: Int = raw
    tautology(n) == Void
  }
}
"#,
        vec![(vec![Data::integer(3.into())], false, vec![])],
    )
    .await;
}

async fn assert_source_acceptance(source: &str, kind: ModuleKind, accepted: bool) {
    let ids = IdGenerator::new();
    let modules = HashMap::from([
        (builtins::PRELUDE.to_owned(), builtins::prelude(&ids)),
        (builtins::BUILTIN.to_owned(), builtins::plutus(&ids)),
    ]);
    let (mut module, _) = parser::module(source, kind).unwrap();
    module.name = "fixture".into();
    let checked = module.infer(
        &ids,
        kind,
        "test/project",
        &modules,
        Tracing::UserDefined(TraceLevel::Verbose),
        &mut vec![],
        None,
    );
    assert_eq!(
        checked.is_ok(),
        accepted,
        "official source acceptance: {source}\n{checked:?}"
    );
    let memory = InMemorySource::new();
    let uri = Url::parse("file:///project/fixture.ak").unwrap();
    memory.insert(uri.clone(), source.to_owned());
    let mut catalog = ModuleCatalog::new();
    catalog.insert(
        uri.clone(),
        SourceSpec::new(&uri, Path::new("/project"), None).unwrap(),
    );
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
    assert_eq!(report.is_success(), accepted, "{source}\n{report:?}");
    if accepted {
        assert!(artifacts.unwrap().unwrap().is_empty());
    } else {
        assert!(artifacts.is_none());
        assert!(
            report.modules.values().any(|module| {
                let nash_driver::ModuleResult::Failed(module) = module else {
                    return false;
                };
                module
                    .reports
                    .iter()
                    .any(|error| error.region.start.line > 0 && error.region.start.column > 0)
            }),
            "a rejected declaration must have a located diagnostic: {report:?}"
        );
    }
}

#[tokio::test]
async fn validator_result_annotation_obeys_local_bool_alias() {
    assert_source_acceptance(
        "type Bool = ByteArray\nvalidator shadow { else(raw: Data) { raw == raw } }",
        ModuleKind::Validator,
        false,
    )
    .await;
}

#[tokio::test]
async fn record_fields_precede_modules_but_invalid_fields_fall_back() {
    let sources = [
        (
            "ops",
            r#"
pub fn subtract(left a: Int, right b: Int) -> Int { a - b }
pub fn add(left a: Int, right b: Int) -> Int { a + b }
"#,
        ),
        (
            "fixture",
            r#"
use aiken/builtin
use ops
type Fields { subtract: Int, own: Int }
fn ops(value: Int) -> Int { value }
fn global(n: Int) -> Int { ops.subtract(right: 4, left: n) }
fn scalar(ops: Int, n: Int) -> Int { ops.subtract(right: 4, left: n) }
fn field(ops: Fields) -> Int { ops.subtract + ops.own }
fn absent(ops: Fields, n: Int) -> Int { ops.add(right: 2, left: n) }
fn lazy_builtin(builtin: Int) -> Bool { builtin.if_then_else(True, True, fail) }
fn unused_shadow(n: Int) -> Int {
  let ops = { trace @"discarded" 0 }
  ops.add(left: n, right: 2)
}
validator namespaces {
  else(raw: Data) {
    expect n: Int = raw
    let fields = Fields { subtract: n, own: 2 }
    and {
      global(n) == n - 4,
      scalar(0, n) == n - 4,
      field(fields) == n + 2,
      absent(fields, n) == n + 2,
      (n |> ops.subtract(left: 4)) == 4 - n,
      unused_shadow(n) == n + 2,
      lazy_builtin(0),
    }
  }
}
"#,
        ),
    ];
    let reference = official_modules(&sources);
    let output = nash_modules(&sources).await;
    compare_programs(
        reference,
        &output,
        vec![
            (vec![Data::integer(10.into())], true, vec!["discarded"]),
            (vec![Data::integer((-3).into())], true, vec!["discarded"]),
        ],
    );
}

#[tokio::test]
async fn constructor_tags_override_type_tags_without_overriding_list_encoding() {
    compare(
        r#"
use aiken/builtin
@tag(7)
pub type Tagged { @tag(3) Tagged(Int) }
@list
pub type Listed { @tag(4) Listed(Int, Int) }
validator decorators {
  else(raw: Data) {
    expect n: Int = raw
    let tagged: Data = Tagged(n)
    let listed: Data = Listed(n, n + 1)
    builtin.equals_data(tagged, builtin.constr_data(3, [builtin.i_data(n)])) &&
      builtin.equals_data(listed, builtin.list_data([builtin.i_data(n), builtin.i_data(n + 1)]))
  }
}
"#,
        vec![(vec![Data::integer(4.into())], true, vec![])],
    )
    .await;
}

#[tokio::test]
async fn public_signatures_reject_private_nominal_types_but_not_private_aliases() {
    for (source, kind, accepted) in [
        (
            "type Hidden { Hidden(Int) }\npub fn leak() { Hidden(1) }",
            ModuleKind::Lib,
            false,
        ),
        (
            "type Hidden { Hidden(Int) }\npub fn leak(value: Hidden) -> Int { let Hidden(n) = value n }",
            ModuleKind::Lib,
            false,
        ),
        (
            "type Hidden { Hidden(Int) }\npub type Visible { Visible(Hidden) }",
            ModuleKind::Lib,
            false,
        ),
        (
            "type Alias = Int\npub fn allowed(value: Alias) -> Alias { value }",
            ModuleKind::Lib,
            true,
        ),
        (
            "type Hidden { Hidden(Int) }\npub opaque type Wrapped { Wrapped(Hidden) }\npub fn allowed() -> Wrapped { Wrapped(Hidden(1)) }",
            ModuleKind::Lib,
            true,
        ),
        (
            "type Hidden { Hidden(Int) }\ntype Phantom<a> = Int\npub fn allowed() -> Phantom<Hidden> { 1 }",
            ModuleKind::Lib,
            true,
        ),
        (
            "type Hidden { Hidden(Int) }\ntype Unknown = _\npub fn leak() -> Unknown { Hidden(1) }",
            ModuleKind::Lib,
            false,
        ),
        (
            "type Hidden { Hidden(Int) }\nvalidator leak { mint(_value: Hidden, _policy: ByteArray, _transaction: Data) { True } }",
            ModuleKind::Validator,
            false,
        ),
    ] {
        assert_source_acceptance(source, kind, accepted).await;
    }
}
