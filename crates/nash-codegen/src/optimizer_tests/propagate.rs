//! Rule 1 in isolation: paired Core/UPLC semantics, without performance assertions.
use nash_ir::{
    anf,
    build::Builder,
    core::*,
    hygiene,
    pretty::pretty,
    propagate::propagate,
    ty::{ConstTy, Ty},
};
use nash_plutus::{arena::Arena, builtin::DefaultFunction as F, constant::Constant};
const INT: Ty<'static> = Ty::Const(&ConstTy::Int);
fn binder<'a>(b: &Builder<'a>, name: &'a str, ty: Ty<'a>) -> Binder<'a> {
    Binder {
        name: b.fresh(name),
        ty,
    }
}
fn traced<'a>(b: &Builder<'a>, message: &'a str, value: &'a Core<'a>) -> &'a Core<'a> {
    b.trace(b.lit(Constant::string(b.arena, message)), value)
}
fn check<'a>(name: &str, b: &Builder<'a>, core: &'a Core<'a>, fails: bool) {
    let before = anf::normalize(b, core);
    let after = propagate(b, before);

    let baseline = crate::harness::eval_core_raw(b.arena, before);
    let candidate = crate::harness::eval_core_raw(b.arena, after);

    assert_eq!(candidate.result.starts_with("error:"), fails);
    fn output(v: &crate::harness::Evaluated) -> String {
        format!(
            "--- uplc\n{}\n--- result\n{}\n--- logs\n{:?}",
            v.uplc, v.result, v.logs
        )
    }
    insta::assert_snapshot!(
        name,
        crate::harness::pass_snapshot(
            b.arena,
            core,
            format!(
                "--- core before\n{}\n--- baseline\n{}\n--- core after\n{}\n--- candidate\n{}",
                pretty(before),
                output(&baseline),
                pretty(after),
                output(&candidate)
            )
        )
    );
    for phase in [before, after] {
        anf::validate(phase).unwrap();
        hygiene::validate(phase, &[]).unwrap();
    }
    // Properties independent of the expected snapshot.
    assert_eq!(before.ty, after.ty);
    assert_eq!(pretty(after), pretty(propagate(b, after)));
    assert_eq!(baseline.observable, candidate.observable);
    assert_eq!(baseline.logs, candidate.logs);
}
#[test]
fn aliases_in_unselected_branch_do_not_hide_strict_failure() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let x = binder(&b, "strict", INT);
    let y = binder(&b, "alias", INT);
    let core = b.let_(
        x,
        traced(&b, "strict failure", b.error(INT)),
        b.let_(
            y,
            b.var(x.name, INT),
            b.if_(
                b.lit(Constant::bool(&arena, true)),
                b.int(42),
                b.var(y.name, INT),
            ),
        ),
    );
    check("unselected_alias_strict_failure", &b, core, true);
}
#[test]
fn alias_inside_delay_does_not_delay_original_computation() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let x = binder(&b, "strict", INT);
    let y = binder(&b, "alias", INT);
    let delayed = b.delay(traced(&b, "delayed", b.var(y.name, INT)));
    let d = binder(&b, "suspended", delayed.ty);
    let core = b.let_(
        x,
        traced(&b, "strict", b.int(42)),
        b.let_(
            y,
            b.var(x.name, INT),
            b.let_(
                d,
                delayed,
                traced(&b, "before force", b.force(b.var(d.name, d.ty), INT)),
            ),
        ),
    );
    check("alias_capture_delay", &b, core, false);
}
#[test]
fn alias_captured_by_function_keeps_outer_binding() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let x = binder(&b, "x", INT);
    let y = binder(&b, "alias", INT);
    let inner = binder(&b, "x", INT);
    let core = b.let_(
        x,
        traced(&b, "outer", b.int(42)),
        b.let_(
            y,
            b.var(x.name, INT),
            b.app(
                b.lam(&[inner], b.var(y.name, INT)),
                &[traced(&b, "argument", b.int(0))],
                INT,
            ),
        ),
    );
    check("alias_capture_function", &b, core, false);
}
#[test]
fn alias_to_branch_field_preserves_scope() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let field = binder(&b, "field", INT);
    let alias = binder(&b, "alias", INT);
    let subject = b.constr(
        0,
        &[b.int(42)],
        Ty::Runtime(arena.alloc(nash_ir::ty::RuntimeTy::Constr {
            tag: 0,
            fields: arena.alloc_slice_copy(&[INT]),
        })),
    );
    let core = b.case(
        CaseKind::Tag,
        subject,
        &[Branch {
            test: Test::Tag(0),
            binders: arena.alloc_slice_copy(&[field]),
            body: b.let_(alias, b.var(field.name, INT), b.var(alias.name, INT)),
        }],
        None,
        INT,
    );
    check("alias_branch_field", &b, core, false);
}
#[test]
fn shared_string_alias_retains_one_literal_binding() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let string = Ty::Const(&ConstTy::String);
    let x = binder(&b, "text", string);
    let y = binder(&b, "alias", string);
    let core = b.let_(
        x,
        b.lit(Constant::string(&arena, "shared payload")),
        b.let_(
            y,
            b.var(x.name, string),
            b.builtin(
                F::AppendString,
                &[b.var(y.name, string), b.var(y.name, string)],
                string,
            ),
        ),
    );
    check("shared_string_alias", &b, core, false);
}
#[test]
fn literal_alias_chain_collapses() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let x = binder(&b, "x", INT);
    let y = binder(&b, "y", INT);
    check(
        "literal_alias_chain",
        &b,
        b.let_(
            x,
            b.int(42),
            b.let_(y, b.var(x.name, INT), b.var(y.name, INT)),
        ),
        false,
    );
}

#[test]
fn repeated_bytes_at_64_byte_limit_evaluate() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let value = b.lit(Constant::byte_string(&a, &[42; 64]));
    let x = binder(&b, "bytes", value.ty);
    check(
        "repeated_64_byte_literal",
        &b,
        b.let_(
            x,
            value,
            b.builtin(
                F::AppendByteString,
                &[b.var(x.name, x.ty), b.var(x.name, x.ty)],
                x.ty,
            ),
        ),
        false,
    );
}
