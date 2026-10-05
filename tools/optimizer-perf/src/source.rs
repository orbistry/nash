//! Small source fixture compiler using the same public stages as integration tests.
use nash_ast::{PackageName, QualifiedName, primitives};
use nash_can::{CanResult, Interface};
use nash_codegen::build::{Build, Input, TraceConfig};
use nash_ir::core::Core;
use nash_plutus::arena::Arena;
use nash_solve::SolvedTypes;
use std::collections::BTreeMap;

struct Module<'a> {
    canonical: CanResult<'a>,
    solved: SolvedTypes<'a>,
}

pub fn compile<'a>(arena: &'a Arena, source: &str, roots: &[&str]) -> Vec<&'a Core<'a>> {
    compile_with_trace(arena, source, roots, TraceConfig::default())
}

pub fn compile_with_trace<'a>(
    arena: &'a Arena,
    source: &str,
    roots: &[&str],
    trace: TraceConfig,
) -> Vec<&'a Core<'a>> {
    let bump = arena.as_bump();
    let mut interfaces = BTreeMap::from([("Builtin", nash_can::kinds::builtin_interface(bump))]);
    let mut modules = Vec::new();
    for (_, text, package) in SUPPORT
        .iter()
        .copied()
        .chain(std::iter::once(("main", source, None)))
    {
        modules.push(solve(arena, text, package, &mut interfaces));
    }
    let build = Build::new(modules.iter().map(|m| Input {
        module: &m.canonical.module,
        types: &m.solved,
        tables: &m.canonical.tables,
    }));
    let home = modules.last().unwrap().canonical.module.name;
    roots
        .iter()
        .map(|name| {
            build
                .compile(
                    arena,
                    QualifiedName {
                        home,
                        name: bump.alloc_str(name),
                    },
                    None,
                    trace,
                )
                .expect("fixture codegen")
                .core
        })
        .collect()
}

fn solve<'a>(
    arena: &'a Arena,
    source: &str,
    package: Option<PackageName<'a>>,
    interfaces: &mut BTreeMap<&'a str, Interface<'a>>,
) -> Module<'a> {
    let bump = arena.as_bump();
    let parsed = nash_parse::Parser::new(bump, bump.alloc_str(source))
        .module()
        .expect("fixture parse");
    let canonical = nash_can::canonicalize(
        bump,
        nash_can::Context {
            package,
            interfaces: Some(interfaces),
        },
        &parsed,
    )
    .expect("fixture canonicalization");
    let (annotations, solved) = nash_solve::run(
        bump,
        &mut nash_constrain::UnionFind::new(),
        &canonical.module,
        &canonical.tables,
    )
    .expect("fixture types");
    nash_nitpick::check(bump, &canonical.module).expect("fixture coverage");
    interfaces.insert(
        canonical.module.name.name,
        nash_can::from_module(bump, &canonical.module, &annotations),
    );
    Module { canonical, solved }
}

pub const SUPPORT: [(&str, &str, Option<PackageName<'static>>); 5] = [
    (
        "Literal",
        include_str!("../../../crates/nash-codegen/tests/fixtures/VestingLiteral.nash"),
        Some(primitives::BASE),
    ),
    (
        "Lift",
        include_str!("../../../crates/nash-codegen/tests/fixtures/VestingLift.nash"),
        Some(primitives::BASE),
    ),
    (
        "Logic",
        include_str!("../../../crates/nash-driver/base/src/Logic.nash"),
        Some(primitives::BASE),
    ),
    (
        "Eq",
        include_str!("../../../crates/nash-driver/base/src/Eq.nash"),
        Some(primitives::BASE),
    ),
    (
        "Cardano.Tx",
        include_str!("../../../crates/nash-codegen/tests/fixtures/VestingTx.nash"),
        None,
    ),
];
