use std::{collections::BTreeMap, sync::Arc};

use nash_codegen::build::{Build, Input, TraceConfig};
use nash_driver::{Database, InMemorySource, build_graph, build_with};
use nash_plutus::{
    arena::Arena, binder::DeBruijn, flat, machine::PlutusVersion, pretty, term::Term,
};
use tokio::sync::Mutex;
use url::Url;

const SOURCE: &str = include_str!("fixtures/NestedTrace.nash");

use nash_test::prng::Trace;

async fn compile() -> BTreeMap<String, Vec<u8>> {
    let memory = InMemorySource::new();
    let uri = Url::parse("file:///project/src/NestedTrace.nash").unwrap();
    memory.insert(uri.clone(), SOURCE.into());
    let mut origins = nash_driver::bundled_base::modules();
    origins.insert(uri.clone(), None);
    let db = Arc::new(Mutex::new(Database::new(memory)));
    let graph = build_graph(db.clone(), &origins.keys().cloned().collect::<Vec<_>>())
        .await
        .unwrap();
    let (report, programs) = build_with(db, &graph, &origins, move |solved| {
        let arena = Arena::new();
        let build = Build::new(solved.modules.iter().map(|module| Input {
            module: module.module,
            types: &module.types,
            tables: &module.tables,
        }));
        let home = solved
            .modules
            .iter()
            .find(|m| m.uri == uri)
            .unwrap()
            .module
            .name;
        ["generate", "replay", "dependent", "siblings", "repartition"]
            .into_iter()
            .map(|name| {
                let core = build
                    .compile(
                        &arena,
                        nash_ast::QualifiedName { home, name },
                        None,
                        TraceConfig::default(),
                    )
                    .unwrap();
                let program = nash_codegen::program::assemble_core(&arena, core.core).unwrap();
                (name.to_owned(), flat::encode(program.program).unwrap())
            })
            .collect()
    })
    .await;
    assert!(
        report.is_success(),
        "{}",
        report
            .ordered_reports()
            .iter()
            .map(|reports| {
                let view = nash_report::Source::new(&reports.source);
                reports
                    .reports
                    .iter()
                    .map(|r| nash_report::render_plain(r, &view, &reports.path))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .collect::<Vec<_>>()
            .join("\n")
    );
    programs.unwrap()
}

fn run<'a>(
    arena: &'a Arena,
    program: &[u8],
    input: &'a Term<'a, DeBruijn>,
) -> Option<(&'a Term<'a, DeBruijn>, Trace)> {
    let ev = nash_test::eval::evaluate(arena, PlutusVersion::V3, program, Some(input));
    match ev.term.expect("generator evaluation") {
        Term::Constr { tag: 1, fields: [] } => None,
        Term::Constr {
            tag: 0,
            fields:
                [
                    Term::Constr {
                        tag: 0,
                        fields: [value, trace],
                    },
                ],
        } => Some((value, Trace::from_term(trace).unwrap())),
        term => panic!("unexpected generator output: {term:?}"),
    }
}

fn replay(
    arena: &Arena,
    programs: &BTreeMap<String, Vec<u8>>,
    name: &str,
    trace: &Trace,
) -> String {
    match run(arena, &programs[name], trace.to_term(arena)) {
        None => "rejected".into(),
        Some((value, used)) => format!("value: {}\nconsumed: {used:?}", pretty::term(value)),
    }
}

#[tokio::test]
async fn nested_trace_generate_edit_replay() {
    let programs = compile().await;
    let arena = Arena::new();
    use Trace::{Choice as C, Group as G};
    let mut output = String::new();
    let (value, trace) = run(
        &arena,
        &programs["generate"],
        Term::byte_string(&arena, arena.alloc([9; 32])),
    )
    .unwrap();
    output.push_str(&format!(
        "seed: 9\ngenerated: {}\ntrace: {trace:?}\n",
        pretty::term(value)
    ));
    let (replayed, consumed) = run(&arena, &programs["replay"], trace.to_term(&arena)).unwrap();
    assert_eq!(pretty::term(value), pretty::term(replayed));
    assert_eq!(trace, consumed);
    output.push_str(&format!("replayed: {}\n", pretty::term(replayed)));
    let G(mut children) = trace else {
        unreachable!()
    };
    children.remove(0);
    let deleted = G(children);
    output.push_str(&format!(
        "delete first element:\n{}\n",
        replay(&arena, &programs, "replay", &deleted)
    ));
    let cases = [
        (
            "dependent original",
            "dependent",
            G(vec![G(vec![C(7)]), G(vec![C(6)])]),
        ),
        (
            "dependent bound reduced",
            "dependent",
            G(vec![G(vec![C(2)]), G(vec![C(6)])]),
        ),
        (
            "dependent both reduced",
            "dependent",
            G(vec![G(vec![C(2)]), G(vec![C(0)])]),
        ),
        (
            "missing child cannot borrow sibling",
            "dependent",
            G(vec![G(vec![]), G(vec![C(6)])]),
        ),
        (
            "unused children normalized",
            "dependent",
            G(vec![G(vec![C(7), C(99)]), G(vec![C(6)])]),
        ),
        (
            "list and sibling",
            "siblings",
            G(vec![
                G(vec![G(vec![C(1), G(vec![C(8)])]), G(vec![C(0)])]),
                G(vec![C(42)]),
            ]),
        ),
        (
            "shortened branch preserves sibling",
            "siblings",
            G(vec![
                G(vec![G(vec![C(0), G(vec![C(8)])]), G(vec![C(0)])]),
                G(vec![C(42)]),
            ]),
        ),
        (
            "wrong node kind rejected",
            "dependent",
            G(vec![C(7), G(vec![C(6)])]),
        ),
        (
            "missing stop cannot borrow sibling",
            "siblings",
            G(vec![G(vec![G(vec![C(1), G(vec![C(8)])])]), G(vec![C(42)])]),
        ),
    ];
    for (label, name, trace) in cases {
        output.push_str(&format!(
            "{label}:\n{}\n",
            replay(&arena, &programs, name, &trace)
        ));
    }
    insta::with_settings!({description => SOURCE, omit_expression => true}, {
        insta::assert_snapshot!(output);
    });
}

#[tokio::test]
async fn nested_trace_reduction_repartitions_strict_groups() {
    use Trace::{Choice as C, Group as G};
    use nash_test::shrink::{Cache, Counterexample, Status};
    let programs = compile().await;
    let original = vec![G(vec![C(1), C(8)]), G(vec![C(42), C(9)])];
    let oracle = |nodes: &[Trace]| {
        let arena = Arena::new();
        match run(
            &arena,
            &programs["repartition"],
            G(nodes.to_vec()).to_term(&arena),
        ) {
            Some((value, G(used))) => Status::Keep(pretty::term(value), used),
            None => Status::Invalid,
            _ => panic!("expected root group"),
        }
    };
    let mut reduced = Counterexample {
        value: "(con integer 1)".to_owned(),
        choices: original.clone(),
        cache: Cache::new(oracle),
        steps: 0,
    };
    reduced.simplify();
    assert_eq!(
        reduced.choices,
        vec![G(vec![C(0), C(0), C(0)]), G(vec![C(0)])]
    );
    insta::with_settings!({description => SOURCE, omit_expression => true}, {
        insta::assert_snapshot!(format!("original: {original:?}\nreduced: {:?}\nvalue: {}", reduced.choices, reduced.value));
    });
}
