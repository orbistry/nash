//! Accepted O1 Core pipeline. Trace policy is decided by codegen, not optimization.
use nash_ir::{anf, build::Builder, core::Core, hygiene};
use nash_plutus::arena::Arena;

pub fn optimize<'a>(arena: &'a Arena, core: &'a Core<'a>) -> &'a Core<'a> {
    optimize_with(&Builder::new(arena), core)
}

pub fn optimize_with<'a>(b: &Builder<'a>, core: &'a Core<'a>) -> &'a Core<'a> {
    let original_ty = core.ty;
    let core = hygiene::freshen(b, core);
    debug_assert!(hygiene::validate(core, &[]).is_ok());
    let core = nash_ir::static_lift::lift(b, core);
    debug_assert!(hygiene::validate(core, &[]).is_ok());
    let core = nash_ir::unused_params::reduce(b, core);
    debug_assert!(hygiene::validate(core, &[]).is_ok());
    let core = anf::normalize(b, core);
    debug_assert!(hygiene::validate(core, &[]).is_ok());
    debug_assert!(anf::validate(core).is_ok());
    // Fold representations and constructors through ANF bindings in the cleanup
    // loops, including opportunities exposed by inlining and propagation.
    let core = nash_ir::small_inline::simplify(b, core);
    debug_assert!(hygiene::validate(core, &[]).is_ok());
    let core = nash_ir::known_case::simplify_constr_data(b, core);
    debug_assert!(hygiene::validate(core, &[]).is_ok());
    debug_assert!(anf::validate(core).is_ok());
    debug_assert_eq!(original_ty, core.ty);
    core
}
