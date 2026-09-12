//! Source-to-CEK validator baselines. The context helpers and Lift methods are
//! compiled from Nash alongside the validator; only ledger arguments are built
//! directly as UPLC constants.
use std::collections::BTreeMap;

use nash_ast::{PackageName, QualifiedName, primitives};
use nash_can::{CanResult, Interface};
use nash_codegen::{
    build::{Build, Input, TraceConfig},
    program::assemble_core,
};
use nash_plutus::{arena::Arena, data::PlutusData, pretty, term::Term};
use nash_solve::SolvedTypes;

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
            Some(primitives::CORE),
        ),
        (
            include_str!("fixtures/VestingLift.nash"),
            Some(primitives::CORE),
        ),
        (include_str!("fixtures/VestingTx.nash"), None),
        (source, None),
    ] {
        modules.push(solve(arena, source, package, &mut interfaces));
    }
    modules
}

fn baseline(source: &str, parameter: bool) -> (String, String, String) {
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
    let core = nash_ir::pretty::pretty(compiled.core);
    let assembled =
        assemble_core(&arena, compiled.core).expect("vesting fixture lowers to closed UPLC");
    let uplc = pretty::program(assembled.program);
    let mut outcomes = String::new();
    for (name, deadline, redeemer, signer, expected) in [
        ("claim after deadline", 10, 0, &b""[..], true),
        ("claim before deadline", 30, 0, &b""[..], false),
        ("cancel signed by owner", 10, 1, &[0xaa][..], true),
        ("cancel unsigned", 10, 1, &b""[..], false),
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
        let context = PlutusData::constr(
            &arena,
            0,
            arena.alloc_slice_copy(&[
                PlutusData::integer_from(&arena, 20),
                PlutusData::byte_string(&arena, signer),
            ]),
        );
        let program = if parameter {
            assembled
                .program
                .apply(&arena, Term::integer_from(&arena, 5))
        } else {
            assembled.program
        };
        let program = program
            .apply(&arena, Term::data(&arena, datum))
            .apply(&arena, Term::data(&arena, action))
            .apply(&arena, Term::data(&arena, context));
        let evaluation = program.eval(&arena);
        assert_eq!(
            evaluation.term.is_ok(),
            expected,
            "{name}: {:?}",
            evaluation.term
        );
        if expected {
            assert_eq!(evaluation.term.unwrap(), Term::unit(&arena));
            assert!(evaluation.info.logs.is_empty(), "{name}");
        } else {
            assert!(
                !evaluation.info.logs.is_empty(),
                "{name}: assert must log before failing"
            );
        }
        assert!(evaluation.info.consumed_budget.cpu > 0);
        assert!(evaluation.info.consumed_budget.mem > 0);
        use std::fmt::Write;
        writeln!(
            outcomes,
            "{name}\nresult: {}\nlogs: {:?}\ncpu: {}\nmem: {}",
            if expected { "unit" } else { "error" },
            evaluation.info.logs,
            evaluation.info.consumed_budget.cpu,
            evaluation.info.consumed_budget.mem
        )
        .unwrap();
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
        let context = PlutusData::constr(
            &arena,
            0,
            arena.alloc_slice_copy(&[
                PlutusData::integer_from(&arena, 20),
                PlutusData::byte_string(&arena, &[]),
            ]),
        );
        for (minimum, expected) in [(0, true), (5, false)] {
            let result = assembled
                .program
                .apply(&arena, Term::integer_from(&arena, minimum))
                .apply(&arena, Term::data(&arena, datum))
                .apply(&arena, Term::data(&arena, action))
                .apply(&arena, Term::data(&arena, context))
                .eval(&arena);
            assert_eq!(
                result.term.is_ok(),
                expected,
                "minimum lock {minimum} must affect the claim boundary"
            );
        }
    }
    (core, uplc, outcomes)
}

#[test]
fn vesting_four_ledger_outcomes() {
    let source = include_str!("fixtures/Vesting.nash");
    let _settings = snapshot_settings(source).bind_to_scope();
    let (core, uplc, outcomes) = baseline(source, false);
    insta::assert_snapshot!("vesting_core", core);
    insta::assert_snapshot!("vesting_uplc", uplc);
    insta::assert_snapshot!("vesting_outcomes", outcomes);
}

#[test]
fn vesting_const_parameter_four_ledger_outcomes() {
    let source = include_str!("fixtures/VestingParam.nash");
    let _settings = snapshot_settings(source).bind_to_scope();
    let (core, uplc, outcomes) = baseline(source, true);
    insta::assert_snapshot!("vesting_parameter_core", core);
    insta::assert_snapshot!("vesting_parameter_uplc", uplc);
    insta::assert_snapshot!("vesting_parameter_outcomes", outcomes);
}

fn snapshot_settings(source: &str) -> insta::Settings {
    let mut settings = insta::Settings::clone_current();
    settings.set_description(
        [
            include_str!("fixtures/VestingLiteral.nash"),
            include_str!("fixtures/VestingLift.nash"),
            include_str!("fixtures/VestingTx.nash"),
            source,
        ]
        .join("\n"),
    );
    settings.set_omit_expression(true);
    settings
}
