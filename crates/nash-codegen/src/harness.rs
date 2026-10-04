//! Shared executable Core fixture harness.
use std::collections::BTreeMap;

use nash_ir::core::Core;
use nash_plutus::{
    arena::Arena,
    debruijn,
    machine::ExBudget,
    pretty,
    program::{Program, Version},
};

pub fn dependency_order<'a>(
    sources: impl IntoIterator<Item = (&'a str, Option<nash_ast::PackageName<'a>>)>,
) -> Vec<(&'a str, Option<nash_ast::PackageName<'a>>)> {
    let bump = bumpalo::Bump::new();
    let sources: BTreeMap<_, _> = sources
        .into_iter()
        .map(|(source, package)| {
            let parsed = nash_parse::Parser::new(&bump, source).module().unwrap();
            (parsed.name.unwrap().value, (source, package, parsed))
        })
        .collect();
    let uri = |name| url::Url::parse(&format!("file:///fixtures/{name}.nash")).unwrap();
    let mut graph = nash_driver::DepGraph::new();
    let mut modules = BTreeMap::new();
    for (name, (source, package, parsed)) in &sources {
        let mut imports: Vec<_> = parsed
            .imports
            .iter()
            .chain(
                parsed
                    .tests
                    .into_iter()
                    .flat_map(|tests| tests.imports.iter()),
            )
            .map(|import| uri(import.import.value))
            .collect();
        if *package != Some(nash_ast::primitives::BASE) {
            // Fixture applications receive all supplied Base interfaces.
            imports.extend(
                sources
                    .iter()
                    .filter(|(_, (_, package, _))| *package == Some(nash_ast::primitives::BASE))
                    .map(|(name, _)| uri(name)),
            );
        }
        imports.sort();
        imports.dedup();
        let module_uri = uri(name);
        graph.add_module(module_uri.clone(), imports);
        modules.insert(module_uri, (*source, *package));
    }
    graph.compute_order().expect("acyclic fixture imports");
    graph
        .order
        .iter()
        .filter_map(|uri| modules.remove(uri))
        .collect()
}

pub struct Evaluated {
    pub uplc: String,
    pub result: String,
    pub logs: Vec<String>,
    pub budget: ExBudget,
    // Function values need application tests, not syntax equality.
    pub(crate) observable: Option<String>,
}

impl std::fmt::Display for Evaluated {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "--- uplc\n{}\n--- result\n{}\n--- logs\n{:?}\n--- budget\ncpu: {}, memory: {}",
            self.uplc, self.result, self.logs, self.budget.cpu, self.budget.mem
        )
    }
}

pub fn eval_core<'a>(arena: &'a Arena, core: &'a Core<'a>) -> Evaluated {
    let fixture = prepare_fixture(arena, core);
    fixture.assert_equivalent(arena);
    fixture.evaluated
}

/// Source compilation happens before this helper. Prepare each pipeline once.
pub(crate) struct Fixture<'a> {
    prepared: crate::snapshot_optimizer::Prepared<'a>,
    pub evaluated: Evaluated,
}

pub(crate) fn prepare_fixture<'a>(arena: &'a Arena, core: &'a Core<'a>) -> Fixture<'a> {
    let prepared = crate::snapshot_optimizer::prepare(arena, core);
    let evaluated = eval_compiled(arena, &prepared.before);
    Fixture {
        prepared,
        evaluated,
    }
}

impl<'a> Fixture<'a> {
    pub fn code_snapshot(&self) -> String {
        self.prepared.snapshot()
    }

    pub fn snapshot(&self) -> String {
        let evaluated = &self.evaluated;
        format!(
            "{}\n--- result\n{}\n--- logs\n{:?}\n--- budget\ncpu: {}, memory: {}",
            self.prepared.snapshot(),
            evaluated.result,
            evaluated.logs,
            evaluated.budget.cpu,
            evaluated.budget.mem
        )
    }

    /// Call after the snapshot so output differences are presented first.
    pub fn assert_equivalent(&self, arena: &'a Arena) {
        let optimized = eval_compiled(arena, &self.prepared.after);
        assert_eq!(
            self.evaluated.observable, optimized.observable,
            "candidate passes preserve ground results and error category"
        );
        assert_eq!(
            self.evaluated.logs, optimized.logs,
            "candidate passes preserve trace order"
        );
        let twice = crate::snapshot_optimizer::optimize(arena, self.prepared.optimized);
        let once = crate::program::assemble_core(arena, self.prepared.optimized).unwrap();
        let twice = crate::program::assemble_core(arena, twice).unwrap();
        assert_eq!(
            nash_plutus::flat::encode(once.program).unwrap(),
            nash_plutus::flat::encode(twice.program).unwrap(),
            "a second O1 invocation must not change the optimized program"
        );
    }
}

pub(crate) fn eval_core_raw<'a>(arena: &'a Arena, core: &'a Core<'a>) -> Evaluated {
    let named = crate::lower::lower(arena, core).expect("valid lowered Core");
    eval_named(arena, named)
}

pub(crate) fn eval_named<'a>(
    arena: &'a Arena,
    named: &'a nash_plutus::term::Term<'a, nash_plutus::binder::Name<'a>>,
) -> Evaluated {
    let term = debruijn::to_debruijn(arena, named).expect("closed term");
    let program = Program::new(arena, Version::plutus_v3(arena), term);
    eval_compiled(arena, &crate::program::Compiled { named, program })
}

fn eval_compiled<'a>(arena: &'a Arena, compiled: &crate::program::Compiled<'a>) -> Evaluated {
    let evaluation = compiled.program.eval(arena);
    fn ground(term: &nash_plutus::term::Term<'_, nash_plutus::binder::DeBruijn>) -> bool {
        match term {
            nash_plutus::term::Term::Constant(_) => true,
            nash_plutus::term::Term::Constr { fields, .. } => {
                fields.iter().all(|field| ground(field))
            }
            _ => false,
        }
    }
    let observable = match &evaluation.term {
        Ok(term) if ground(term) => Some(pretty::term(term)),
        Ok(_) => None,
        Err(error) => Some(format!(
            "error: {:?}: {error}",
            std::mem::discriminant(error)
        )),
    };
    Evaluated {
        observable,
        uplc: pretty::program(&Program {
            version: compiled.program.version,
            term: compiled.named,
        }),
        result: match evaluation.term {
            Ok(term) => pretty::term(term),
            Err(error) => format!("error: {error:?}"),
        },
        logs: evaluation.info.logs,
        budget: evaluation.info.consumed_budget,
    }
}

/// Keep isolated-pass evidence while every executable fixture tracks the accepted pipeline.
pub(crate) fn pass_snapshot<'a>(arena: &'a Arena, core: &'a Core<'a>, isolated: String) -> String {
    format!(
        "{}\n--- isolated pass\n{isolated}",
        code_snapshot(arena, core)
    )
}

pub(crate) fn candidate<'a>(arena: &'a Arena, core: &'a Core<'a>) -> &'a Core<'a> {
    let b = nash_ir::build::Builder::new(arena);
    let optimized = crate::snapshot_optimizer::optimize_with(&b, core);
    let rewritten = crate::recursion::rewrite(&b, optimized).unwrap();
    // Recursion rewriting reuses self-application lambda subtrees.
    let result = nash_ir::hygiene::freshen(&b, rewritten);
    nash_ir::hygiene::validate(result, &[]).unwrap();
    assert_eq!(core.ty, result.ty);
    result
}

pub(crate) fn code_snapshot<'a>(arena: &'a Arena, core: &'a Core<'a>) -> String {
    crate::snapshot_optimizer::prepare(arena, core).snapshot()
}
