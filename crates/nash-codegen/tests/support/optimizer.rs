//! Shared accepted optimizer pipeline for unit and integration snapshots.
//! Rendering helpers reuse the production O1 Core pipeline.
use crate::{lower, program, recursion};
use nash_ir::{anf, build::Builder, core::Core, hygiene, pretty::pretty};
use nash_plutus::{
    arena::Arena,
    pretty as uplc,
    program::{Program, Version},
};

/// Normalize once before optimization, then rewrite recursion and lower nested
/// Core directly. Reuses the production O1 optimizer.
pub fn optimize<'a>(arena: &'a Arena, core: &'a Core<'a>) -> &'a Core<'a> {
    optimize_with(&Builder::new(arena), core)
}

pub(crate) fn optimize_with<'a>(b: &Builder<'a>, core: &'a Core<'a>) -> &'a Core<'a> {
    let optimized = crate::optimizer::optimize_with(b, core);
    hygiene::validate(optimized, &[]).unwrap();
    anf::validate(optimized).unwrap();
    assert_eq!(core.ty, optimized.ty);
    optimized
}

/// Prepared once; rendering and evaluation share these exact programs.
pub struct Prepared<'a> {
    core: &'a Core<'a>,
    pub optimized: &'a Core<'a>,
    pub before: program::Compiled<'a>,
    pub after: program::Compiled<'a>,
}

pub fn prepare<'a>(arena: &'a Arena, core: &'a Core<'a>) -> Prepared<'a> {
    let before = program::assemble_core(arena, core).expect("O0 snapshot program");
    // Keep the snapshot name supply independent of optimization.
    let b = Builder::new(arena);
    let optimized = optimize(arena, core);
    let rewritten = recursion::rewrite(&b, optimized).expect("optimized recursion encoding");
    let rewritten = hygiene::freshen(&b, rewritten);
    hygiene::validate(rewritten, &[]).unwrap();
    assert_eq!(core.ty, rewritten.ty);
    let named = lower::lower_optimized(arena, rewritten).expect("optimized lowering");
    let closed = nash_plutus::debruijn::to_debruijn(arena, named).expect("closed snapshot program");
    let program = Program::new(arena, Version::plutus_v3(arena), closed);
    nash_plutus::script::validate_program(program, nash_plutus::machine::PlutusVersion::V3)
        .expect("valid snapshot target");
    Prepared {
        core,
        optimized,
        before,
        after: program::Compiled { named, program },
    }
}

impl Prepared<'_> {
    pub fn snapshot(&self) -> String {
        // Named and closed programs have the same version; only binders differ.
        let render = |compiled: &program::Compiled<'_>| {
            uplc::program(&Program {
                version: compiled.program.version,
                term: compiled.named,
            })
        };
        format!(
            "--- unoptimized Core\n{}\n--- unoptimized UPLC\n{}\n--- optimized Core\n{}\n--- optimized UPLC\n{}",
            pretty(self.core),
            render(&self.before),
            pretty(self.optimized),
            render(&self.after)
        )
    }
}
