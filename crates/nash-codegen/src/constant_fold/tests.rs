use super::*;
use nash_ir::pretty::pretty;

#[test]
fn exhausted_limits_keep_runtime_work() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let core = b.builtin(F::AddInteger, &[b.int(20), b.int(22)], Ty::Erased);
    let fixture = crate::harness::prepare_fixture(&arena, core);
    for (name, limits) in [
        (
            "cpu_exhausted",
            Limits {
                budget: ExBudget {
                    cpu: 0,
                    mem: 10_000,
                },
                ..Limits::default()
            },
        ),
        (
            "memory_exhausted",
            Limits {
                budget: ExBudget {
                    cpu: 1_000_000,
                    mem: 0,
                },
                ..Limits::default()
            },
        ),
        (
            "attempts_exhausted",
            Limits {
                calls: 0,
                ..Limits::default()
            },
        ),
        (
            "input_size_exhausted",
            Limits {
                bytes: 0,
                ..Limits::default()
            },
        ),
    ] {
        let after = simplify_with(&b, core, limits);
        let evaluated = crate::harness::eval_core_raw(&arena, after);
        insta::assert_snapshot!(
            name,
            format!(
                "{}\n--- limited pass Core\n{}\n--- limited pass UPLC\n{}\n--- result\n{}\n--- logs\n{:?}",
                fixture.code_snapshot(),
                pretty(after),
                evaluated.uplc,
                evaluated.result,
                evaluated.logs
            )
        );
        assert_eq!(fixture.evaluated.observable, evaluated.observable);
        assert_eq!(fixture.evaluated.logs, evaluated.logs);
    }
}

#[test]
fn malformed_constants_and_deep_metadata_are_rejected_before_evaluation() {
    let arena = Arena::new();
    let wrong = C::bool(&arena, false);
    let pair = C::proto_pair(&arena, &Type::Data, &Type::Data, wrong, wrong);
    let malformed = [
        C::proto_list(&arena, &Type::Data, arena.alloc([wrong])),
        C::proto_list(
            &arena,
            arena.alloc(Type::Pair(&Type::Data, &Type::Data)),
            arena.alloc([pair]),
        ),
    ];
    for c in malformed {
        assert!(!valid(&arena, c, 64, &mut 1024, &mut 4096));
    }
    let mut typ = &Type::Integer;
    for _ in 0..100 {
        typ = arena.alloc(Type::List(typ));
    }
    let empty = C::proto_list(&arena, typ, &[]);
    assert!(!valid(&arena, empty, 64, &mut 1024, &mut 4096));
    let unsafe_data = C::data(&arena, D::integer_from(&arena, -(1_i128 << 64) - 1));
    assert!(!valid(&arena, unsafe_data, 64, &mut 1024, &mut 4096));
}

#[test]
fn failures_and_cleanup_share_attempt_allowance() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let sum = b.builtin(F::AddInteger, &[b.int(20), b.int(22)], b.int(0).ty);
    let core = b.constr(
        0,
        &[
            b.builtin(F::DivideInteger, &[b.int(1), b.int(0)], sum.ty),
            b.builtin(F::MultiplyInteger, &[sum, b.int(2)], sum.ty),
        ],
        Ty::Erased,
    );
    let fixture = crate::harness::prepare_fixture(&arena, core);
    let anf = nash_ir::anf::normalize(&b, core);
    let after = simplify_with(
        &b,
        anf,
        Limits {
            calls: 2,
            ..Limits::default()
        },
    );
    let evaluated = crate::harness::eval_core_raw(&arena, after);
    assert!(evaluated.result.starts_with("error:"));
    insta::assert_snapshot!(format!(
        "{}\n--- limited pass Core\n{}\n--- limited pass UPLC\n{}\n--- result\n{}\n--- logs\n{:?}",
        fixture.code_snapshot(),
        pretty(after),
        evaluated.uplc,
        evaluated.result,
        evaluated.logs
    ));
    fixture.assert_equivalent(&arena);
    assert_eq!(fixture.evaluated.observable, evaluated.observable);
    assert_eq!(fixture.evaluated.logs, evaluated.logs);
    nash_ir::anf::validate(after).unwrap();
    nash_ir::hygiene::validate(after, &[]).unwrap();
}

#[test]
fn unsafe_serialized_result_keeps_runtime_call() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let core = b.builtin(F::IData, &[b.int(-(1_i128 << 64) - 1)], Ty::Erased);
    let fixture = crate::harness::prepare_fixture(&arena, core);
    insta::assert_snapshot!(fixture.snapshot());
    fixture.assert_equivalent(&arena);
    assert!(std::ptr::eq(core, simplify(&b, core)));
}
