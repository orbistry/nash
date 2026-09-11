//! Shared executable Core fixture harness.
use nash_ir::core::Core;
use nash_plutus::{
    arena::Arena,
    debruijn,
    machine::ExBudget,
    pretty,
    program::{Program, Version},
};

pub struct Evaluated {
    pub uplc: String,
    pub result: String,
    pub logs: Vec<String>,
    pub budget: ExBudget,
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
    let named = crate::lower::lower(arena, core).expect("valid lowered Core");
    let term = debruijn::to_debruijn(arena, named).expect("closed term");
    let program = Program::new(arena, Version::plutus_v3(arena), term);
    let evaluation = program.eval(arena);
    Evaluated {
        uplc: pretty::program(Program::new(arena, Version::plutus_v3(arena), named)),
        result: match evaluation.term {
            Ok(term) => pretty::term(term),
            Err(error) => format!("error: {error:?}"),
        },
        logs: evaluation.info.logs,
        budget: evaluation.info.consumed_budget,
    }
}
