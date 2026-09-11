use super::*;
use nash_ir::ty::{ConstTy, TermTy};
use nash_plutus::{
    arena::Arena,
    builtin::DefaultFunction as F,
    program::{Program, Version},
};

fn binding<'a>(b: &Builder<'a>, text: &'a str) -> Binder<'a> {
    Binder {
        name: b.fresh(text),
        ty: Ty::Const(&ConstTy::Int),
    }
}
fn function<'a>(b: &Builder<'a>, text: &'a str) -> Binder<'a> {
    Binder {
        name: b.fresh(text),
        ty: Ty::Term(&TermTy::Fun(
            &[Ty::Const(&ConstTy::Int)],
            Ty::Const(&ConstTy::Int),
        )),
    }
}
fn op<'a>(b: &Builder<'a>, f: F, x: &'a Core<'a>, y: &'a Core<'a>) -> &'a Core<'a> {
    b.builtin(f, &[x, y])
}
fn decrement<'a>(b: &Builder<'a>, n: Binder<'a>) -> &'a Core<'a> {
    op(b, F::SubtractInteger, b.var(n.name), b.int(1))
}
fn eval<'a>(b: &Builder<'a>, core: &'a Core<'a>) -> String {
    let rewritten = rewrite(b, core).unwrap();
    assert!(!nash_ir::pretty::pretty(rewritten).contains("letrec"));
    let term = crate::lower::lower(b.arena, rewritten).unwrap();
    let db = nash_plutus::debruijn::to_debruijn(b.arena, term).unwrap();
    let result = Program::new(b.arena, Version::plutus_v3(b.arena), db).eval(b.arena);
    nash_plutus::pretty::term(result.term.unwrap())
}
fn rec<'a>(
    b: &Builder<'a>,
    f: Binder<'a>,
    params: &[Binder<'a>],
    body: &'a Core<'a>,
) -> RecBinder<'a> {
    RecBinder {
        binder: Binder {
            ty: Ty::Term(
                b.arena.alloc(TermTy::Fun(
                    b.arena
                        .alloc_slice_copy(&params.iter().map(|p| p.ty).collect::<Vec<_>>()),
                    Ty::Const(&ConstTy::Int),
                )),
            ),
            ..f
        },
        params: b.arena.alloc_slice_copy(params),
        static_params: b
            .arena
            .alloc_slice_copy(&static_params(f.name, params, body)),
        body,
    }
}
#[test]
fn tail_recursion() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let f = function(&b, "sum");
    let acc = binding(&b, "acc");
    let n = binding(&b, "n");
    let body = b.if_(
        op(&b, F::EqualsInteger, b.var(n.name), b.int(0)),
        b.var(acc.name),
        b.app(
            b.var(f.name),
            &[
                op(&b, F::AddInteger, b.var(acc.name), b.var(n.name)),
                decrement(&b, n),
            ],
        ),
    );
    let program = b.let_rec(
        &[rec(&b, f, &[acc, n], body)],
        b.app(b.var(f.name), &[b.int(0), b.int(10)]),
    );
    insta::assert_snapshot!(eval(&b,program),@"(con integer 55)");
}
#[test]
fn non_tail_factorial() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let f = function(&b, "factorial");
    let n = binding(&b, "n");
    let body = b.if_(
        op(&b, F::EqualsInteger, b.var(n.name), b.int(0)),
        b.int(1),
        op(
            &b,
            F::MultiplyInteger,
            b.var(n.name),
            b.app(b.var(f.name), &[decrement(&b, n)]),
        ),
    );
    insta::assert_snapshot!(eval(&b,b.let_rec(&[rec(&b,f,&[n],body)],b.app(b.var(f.name),&[b.int(6)]))),@"(con integer 720)");
}
#[test]
fn static_parameter_keeps_original_position() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let f = function(&b, "count");
    let n = binding(&b, "n");
    let step = binding(&b, "step");
    let body = b.if_(
        op(&b, F::EqualsInteger, b.var(n.name), b.int(0)),
        b.int(0),
        op(
            &b,
            F::AddInteger,
            b.var(step.name),
            b.app(b.var(f.name), &[decrement(&b, n), b.var(step.name)]),
        ),
    );
    assert_eq!(static_params(f.name, &[n, step], body), vec![1]);
    let program = b.let_rec(
        &[rec(&b, f, &[n, step], body)],
        b.app(b.var(f.name), &[b.int(3), b.int(7)]),
    );
    insta::assert_snapshot!(eval(&b,program),@"(con integer 21)");
    insta::assert_snapshot!(
        "static_parameter_core",
        nash_ir::pretty::pretty(rewrite(&b, program).unwrap())
    );
}
#[test]
fn mutual_even_odd_and_three_cycle() {
    for size in [2, 3] {
        let a = Arena::new();
        let b = Builder::new(&a);
        let functions = (0..size).map(|_| function(&b, "cycle")).collect::<Vec<_>>();
        let binders = functions
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let n = binding(&b, "n");
                let body = b.if_(
                    op(&b, F::EqualsInteger, b.var(n.name), b.int(0)),
                    b.int(i as i128),
                    b.app(b.var(functions[(i + 1) % size].name), &[decrement(&b, n)]),
                );
                rec(&b, *f, &[n], body)
            })
            .collect::<Vec<_>>();
        assert_eq!(
            eval(
                &b,
                b.let_rec(&binders, b.app(b.var(functions[0].name), &[b.int(7)]))
            ),
            format!("(con integer {})", 7 % size)
        );
    }
}
#[test]
fn captures_lexical_binding() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let f = function(&b, "go");
    let n = binding(&b, "n");
    let captured = binding(&b, "captured");
    let body = b.if_(
        op(&b, F::EqualsInteger, b.var(n.name), b.int(0)),
        b.var(captured.name),
        b.app(b.var(f.name), &[decrement(&b, n)]),
    );
    insta::assert_snapshot!(eval(&b,b.let_(captured,b.int(42),b.let_rec(&[rec(&b,f,&[n],body)],b.app(b.var(f.name),&[b.int(3)])))),@"(con integer 42)");
}
#[test]
fn first_class_recursive_function() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let f = function(&b, "go");
    let n = binding(&b, "n");
    let alias = function(&b, "alias");
    let body = b.if_(
        op(&b, F::EqualsInteger, b.var(n.name), b.int(0)),
        b.int(12),
        b.let_(
            alias,
            b.var(f.name),
            b.app(b.var(alias.name), &[decrement(&b, n)]),
        ),
    );
    assert!(static_params(f.name, &[n], body).is_empty());
    insta::assert_snapshot!(eval(&b,b.let_rec(&[rec(&b,f,&[n],body)],b.app(b.var(f.name),&[b.int(3)]))),@"(con integer 12)");
}
#[test]
fn rejects_recursive_value() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let f = binding(&b, "value");
    let program = b.let_rec(&[rec(&b, f, &[], b.var(f.name))], b.int(0));
    assert!(matches!(rewrite(&b, program), Err(Error::RecursiveValue)));
}
#[test]
fn static_analysis_distinguishes_shadowed_names() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let f = function(&b, "f");
    let x = binding(&b, "x");
    let shadow = binding(&b, "x");
    assert_eq!(
        static_params(f.name, &[x], b.app(b.var(f.name), &[b.var(x.name)])),
        vec![0]
    );
    assert!(
        static_params(
            f.name,
            &[x],
            b.lam(&[shadow], b.app(b.var(f.name), &[b.var(shadow.name)]))
        )
        .is_empty()
    );
    assert!(static_params(f.name, &[x], b.app(b.var(f.name), &[b.int(1)])).is_empty());
}

#[test]
fn fully_static_dead_call_stays_lazy() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let f = function(&b, "loop");
    let x = binding(&b, "x");
    let body = b.if_(
        b.lit(nash_plutus::constant::Constant::bool(&a, true)),
        b.var(x.name),
        b.app(b.var(f.name), &[b.var(x.name)]),
    );
    assert_eq!(static_params(f.name, &[x], body), vec![0]);
    assert_eq!(
        eval(
            &b,
            b.let_rec(&[rec(&b, f, &[x], body)], b.app(b.var(f.name), &[b.int(8)]))
        ),
        "(con integer 8)"
    );
}

#[test]
fn stale_static_metadata_does_not_drop_changing_arguments() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let f = function(&b, "loop");
    let n = binding(&b, "n");
    let body = b.if_(
        op(&b, F::EqualsInteger, b.var(n.name), b.int(0)),
        b.int(9),
        b.app(b.var(f.name), &[decrement(&b, n)]),
    );
    let mut rb = rec(&b, f, &[n], body);
    rb.static_params = &[0];
    assert_eq!(
        eval(&b, b.let_rec(&[rb], b.app(b.var(f.name), &[b.int(3)]))),
        "(con integer 9)"
    );
}

#[test]
fn separate_builder_name_supply_does_not_capture_inputs() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let f = function(&b, "go");
    let n = binding(&b, "n");
    let captured = binding(&b, "self");
    let body = b.if_(
        op(&b, F::EqualsInteger, b.var(n.name), b.int(0)),
        b.var(captured.name),
        b.app(b.var(f.name), &[decrement(&b, n)]),
    );
    let program = b.let_(
        captured,
        b.int(42),
        b.let_rec(&[rec(&b, f, &[n], body)], b.app(b.var(f.name), &[b.int(3)])),
    );
    assert_eq!(eval(&Builder::new(&a), program), "(con integer 42)");
}
