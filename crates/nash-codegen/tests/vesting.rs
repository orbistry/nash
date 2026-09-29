//! Source-to-CEK validator baselines. The context helpers and Lift methods are
//! compiled from Nash alongside the validator; only ledger arguments are built
//! directly as UPLC constants.
#[path = "support/vesting.rs"]
mod vesting_input;

use std::collections::BTreeMap;

use nash_ast::{PackageName, QualifiedName, primitives};
use nash_can::{CanResult, Interface};
use nash_codegen::optimizer;
use nash_codegen::{
    build::{Build, Input, TraceConfig},
    lower, program, recursion,
};
use nash_plutus::{arena::Arena, data::PlutusData, pretty, term::Term};
use nash_solve::SolvedTypes;

#[path = "support/optimizer.rs"]
mod snapshot_optimizer;

struct SourceModule<'a> {
    canonical: CanResult<'a>,
    solved: SolvedTypes<'a>,
}

fn solve<'a>(
    arena: &'a Arena,
    source: &str,
    package: Option<PackageName<'a>>,
    interfaces: &mut BTreeMap<&'a str, Interface<'a>>,
) -> SourceModule<'a> {
    let bump = arena.as_bump();
    let source = bump.alloc_str(source);
    let parsed = nash_parse::Parser::new(bump, source)
        .module()
        .expect("vesting fixture parses");
    let canonical = nash_can::canonicalize(
        bump,
        nash_can::Context {
            package,
            interfaces: Some(interfaces),
        },
        &parsed,
    )
    .expect("vesting fixture canonicalizes");
    let (annotations, solved) = nash_solve::run(
        bump,
        &mut nash_constrain::UnionFind::new(),
        &canonical.module,
        &canonical.tables,
    )
    .expect("vesting fixture typechecks");
    nash_nitpick::check(bump, &canonical.module).expect("vesting matches are exhaustive");
    interfaces.insert(
        canonical.module.name.name,
        nash_can::from_module(bump, &canonical.module, &annotations),
    );
    SourceModule { canonical, solved }
}

fn fixture_modules<'a>(arena: &'a Arena, source: &str) -> Vec<SourceModule<'a>> {
    let mut interfaces = BTreeMap::from([(
        "Builtin",
        nash_can::kinds::builtin_interface(arena.as_bump()),
    )]);
    let mut modules = Vec::new();
    for (source, package) in [
        (
            include_str!("fixtures/VestingLiteral.nash"),
            Some(primitives::BASE),
        ),
        (
            include_str!("fixtures/VestingLift.nash"),
            Some(primitives::BASE),
        ),
        (
            include_str!("../../nash-driver/base/src/Logic.nash"),
            Some(primitives::BASE),
        ),
        (
            include_str!("../../nash-driver/base/src/Eq.nash"),
            Some(primitives::BASE),
        ),
        (include_str!("fixtures/VestingTx.nash"), None),
        (source, None),
    ] {
        modules.push(solve(arena, source, package, &mut interfaces));
    }
    modules
}

struct Snapshot {
    text: String,
    outcomes: Vec<(String, Vec<String>)>,
    boundaries: Vec<(i128, bool, bool)>,
}

impl Snapshot {
    fn assert_equivalent(&self) {
        for pair in self.outcomes.as_chunks::<2>().0 {
            assert_eq!(pair[0], pair[1], "optimizer preserves outcome and logs");
        }
        for (minimum, actual, expected) in &self.boundaries {
            assert_eq!(
                actual, expected,
                "minimum lock {minimum} must affect the claim boundary"
            );
        }
    }
}

fn baseline(source: &str, parameter: bool) -> Snapshot {
    let arena = Arena::new();
    let modules = fixture_modules(&arena, source);
    let validator = modules.last().unwrap();
    let build = Build::new(modules.iter().map(|module| Input {
        module: &module.canonical.module,
        types: &module.solved,
        tables: &module.canonical.tables,
    }));
    let root = QualifiedName {
        home: validator.canonical.module.name,
        name: "main",
    };
    let compiled = build
        .compile(&arena, root, None, TraceConfig::default())
        .expect("vesting fixture compiles");
    let prepared = snapshot_optimizer::prepare(&arena, compiled.core);
    let code = prepared.snapshot();
    let baseline_program = prepared.before.program;
    let optimized_program = prepared.after.program;
    let mut outcomes = String::new();
    let mut observations = Vec::new();
    let mut boundaries = Vec::new();
    for (name, deadline, redeemer, signer) in [
        ("claim after deadline", 10, 0, &b""[..]),
        ("claim before deadline", 30, 0, &b""[..]),
        ("cancel signed by owner", 10, 1, &[0xaa][..]),
        ("cancel unsigned", 10, 1, &b""[..]),
    ] {
        let datum = PlutusData::constr(
            &arena,
            0,
            arena.alloc_slice_copy(&[
                PlutusData::byte_string(&arena, &[0xaa]),
                PlutusData::integer_from(&arena, deadline),
            ]),
        );
        let action = PlutusData::constr(&arena, redeemer, &[]);
        let context = vesting_input::context(&arena, datum, action, 20, signer);
        for (phase, validator) in [
            ("unoptimized", baseline_program),
            ("optimized", optimized_program),
        ] {
            let program = if parameter {
                validator.apply(&arena, Term::integer_from(&arena, 5))
            } else {
                validator
            };
            let evaluation = program
                .apply(&arena, Term::data(&arena, context))
                .eval(&arena);
            let result = match &evaluation.term {
                Ok(term) => pretty::term(term),
                Err(error) => format!("error: {error:?}"),
            };
            observations.push((result.clone(), evaluation.info.logs.clone()));
            if phase == "unoptimized" {
                use std::fmt::Write;
                writeln!(outcomes,
                    "--- scenario\n{name}\n--- result\n{result}\n--- logs\n{:?}\n--- budget\ncpu: {}, memory: {}",
                    evaluation.info.logs, evaluation.info.consumed_budget.cpu, evaluation.info.consumed_budget.mem,
                ).unwrap();
            }
        }
    }
    // The four baseline rows also pass if the extra parameter is ignored.
    // Probe its boundary separately so the test proves it affects execution.
    if parameter {
        let datum = PlutusData::constr(
            &arena,
            0,
            arena.alloc_slice_copy(&[
                PlutusData::byte_string(&arena, &[0xaa]),
                PlutusData::integer_from(&arena, 18),
            ]),
        );
        let action = PlutusData::constr(&arena, 0, &[]);
        let context = vesting_input::context(&arena, datum, action, 20, &[]);
        for (minimum, expected) in [(0, true), (5, false)] {
            for validator in [baseline_program, optimized_program] {
                let result = validator
                    .apply(&arena, Term::integer_from(&arena, minimum))
                    .apply(&arena, Term::data(&arena, context))
                    .eval(&arena);
                boundaries.push((minimum, result.term.is_ok(), expected));
            }
        }
    }
    Snapshot {
        text: format!("{code}\n{outcomes}"),
        outcomes: observations,
        boundaries,
    }
}

#[test]
fn vesting_four_ledger_outcomes() {
    let source = include_str!("fixtures/Vesting.nash");
    let _settings = snapshot_settings(source).bind_to_scope();
    let snapshot = baseline(source, false);
    insta::assert_snapshot!("vesting", snapshot.text);
    snapshot.assert_equivalent();
}

#[test]
fn vesting_const_parameter_four_ledger_outcomes() {
    let source = include_str!("fixtures/VestingParam.nash");
    let _settings = snapshot_settings(source).bind_to_scope();
    let snapshot = baseline(source, true);
    insta::assert_snapshot!("vesting_parameter", snapshot.text);
    snapshot.assert_equivalent();
}

fn snapshot_settings(source: &str) -> insta::Settings {
    let mut settings = insta::Settings::clone_current();
    settings.set_description([include_str!("fixtures/VestingTx.nash"), source].join("\n"));
    settings.set_omit_expression(true);
    settings
}
