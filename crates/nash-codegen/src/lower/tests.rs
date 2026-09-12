use super::*;
use nash_ir::{
    build::Builder,
    core::{Binder, Branch, CaseKind, Core, Test},
    ty::{ConstTy, Ty},
};
use nash_plutus::{
    constant::Constant,
    program::{Program, Version},
};

fn evaluate<'a>(
    arena: &'a Arena,
    core: &'a Core<'a>,
) -> nash_plutus::machine::EvalResult<'a, nash_plutus::binder::DeBruijn> {
    let named = lower(arena, core).unwrap();
    let debruijn = nash_plutus::debruijn::to_debruijn(arena, named).unwrap();
    Program::new(arena, Version::plutus_v3(arena), debruijn).eval(arena)
}

#[test]
fn let_application_evaluates() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let int = Ty::Const(arena.alloc(ConstTy::Int));
    let x = Binder {
        name: b.fresh("x"),
        ty: int,
    };
    let y = Binder {
        name: b.fresh("y"),
        ty: int,
    };
    let add = b.builtin(DefaultFunction::AddInteger, &[b.var(x.name), b.var(y.name)]);
    let core = b.let_(x, b.int(1), b.app(b.lam(&[y], add), &[b.int(2)]));
    let result = evaluate(&arena, core);
    assert_eq!(result.term.unwrap(), Term::integer_from(&arena, 3));
    assert!(result.info.consumed_budget.cpu > 0);
}

#[test]
fn boolean_case_does_not_evaluate_unselected_failure() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let core = b.if_(b.lit(Constant::bool(&arena, true)), b.int(42), b.error());
    assert_eq!(
        evaluate(&arena, core).term.unwrap(),
        Term::integer_from(&arena, 42)
    );
}

#[test]
fn trace_precedes_failure() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let core = b.trace(b.lit(Constant::string(&arena, "before failure")), b.error());
    let result = evaluate(&arena, core);
    assert!(result.term.is_err());
    assert_eq!(result.info.logs, ["before failure"]);
}

#[test]
fn field_projection_and_tag_order_are_semantic() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let x = Binder {
        name: b.fresh("x"),
        ty: Ty::Erased,
    };
    let core = b.case(
        CaseKind::Tag,
        b.constr(1, &[b.int(17)]),
        &[
            Branch {
                test: Test::Tag(1),
                binders: arena.alloc_slice_copy(&[x]),
                body: b.var(x.name),
            },
            Branch {
                test: Test::Tag(0),
                binders: &[],
                body: b.error(),
            },
        ],
        None,
    );
    assert_eq!(
        evaluate(&arena, core).term.unwrap(),
        Term::integer_from(&arena, 17)
    );
    let field = b.field(b.constr(0, &[b.int(1), b.int(2)]), 1, 2);
    assert_eq!(
        evaluate(&arena, field).term.unwrap(),
        Term::integer_from(&arena, 2)
    );
}

#[test]
fn malformed_core_cases_are_rejected() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let malformed = arena.alloc(Core::Field {
        record: b.constr(0, &[]),
        index: 1,
        arity: 1,
    });
    assert!(matches!(
        lower(&arena, malformed),
        Err(Error::InvalidField { .. })
    ));
    let sparse = b.case(
        CaseKind::Tag,
        b.constr(2, &[]),
        &[Branch {
            test: Test::Tag(2),
            binders: &[],
            body: b.int(1),
        }],
        None,
    );
    assert!(matches!(lower(&arena, sparse), Err(Error::InvalidCase(_))));
}

#[test]
fn list_case_only_unpacks_a_nonempty_list() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let head = Binder {
        name: b.fresh("head"),
        ty: Ty::Erased,
    };
    let tail = Binder {
        name: b.fresh("tail"),
        ty: Ty::Erased,
    };
    let branches = arena.alloc_slice_copy(&[
        Branch {
            test: Test::Nil,
            binders: &[],
            body: b.int(7),
        },
        Branch {
            test: Test::Cons,
            binders: arena.alloc_slice_copy(&[head, tail]),
            body: b.var(head.name),
        },
    ]);
    let nil = b.lit(Constant::proto_list(
        &arena,
        nash_plutus::typ::Type::integer(&arena),
        &[],
    ));
    let list = b.builtin(DefaultFunction::MkCons, &[b.int(42), nil]);
    for (value, expected) in [(nil, 7), (list, 42)] {
        let core = b.case(CaseKind::List, value, branches, None);
        assert_eq!(
            evaluate(&arena, core).term.unwrap(),
            Term::integer_from(&arena, expected)
        );
    }
}

#[test]
fn data_case_default_does_not_unpack_wrong_shape() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let value = Binder {
        name: b.fresh("value"),
        ty: Ty::Erased,
    };
    let core = b.case(
        CaseKind::Data,
        b.lit(Constant::data(
            &arena,
            nash_plutus::data::PlutusData::integer_from(&arena, 12),
        )),
        &[Branch {
            test: Test::DataB,
            binders: arena.alloc_slice_copy(&[value]),
            body: b.error(),
        }],
        Some(b.int(3)),
    );
    assert_eq!(
        evaluate(&arena, core).term.unwrap(),
        Term::integer_from(&arena, 3)
    );
}
