use super::*;
use nash_ir::pretty::pretty;
use nash_ir::ty::Ty;
use nash_plutus::builtin::DefaultFunction as F;

#[test]
fn pure_builtins_fold_without_size_or_cost_limits() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let bytes = b.lit(C::byte_string(&arena, arena.alloc_slice_copy(&[42; 5000])));
    let calls = [
        ("large_input", b.builtin(F::Sha2_256, &[bytes], Ty::Erased)),
        (
            "growing_output",
            b.builtin(F::ReplicateByte, &[b.int(5000), b.int(42)], Ty::Erased),
        ),
        (
            "pure_conditional",
            b.builtin(
                F::IfThenElse,
                &[b.lit(C::bool(&arena, true)), b.int(20), b.int(22)],
                Ty::Erased,
            ),
        ),
        (
            "construct_data",
            b.builtin(
                F::ConstrData,
                &[b.int(0), b.lit(C::proto_list(&arena, &Type::Data, &[]))],
                Ty::Erased,
            ),
        ),
    ];
    for (name, core) in calls {
        let fixture = crate::harness::prepare_fixture(&arena, core);
        let after = simplify(&b, core);
        let evaluated = crate::harness::eval_core_raw(&arena, after);
        assert!(!evaluated.result.starts_with("error:"));
        insta::assert_snapshot!(
            name,
            format!(
                "{}\n--- isolated Core\n{}\n--- result\n{}",
                fixture.code_snapshot(),
                pretty(after),
                evaluated.result
            )
        );
        fixture.assert_equivalent(&arena);
        assert_eq!(fixture.evaluated.observable, evaluated.observable);
        assert_eq!(fixture.evaluated.logs, evaluated.logs);
        assert!(std::ptr::eq(after, simplify(&b, after)));
    }
}

#[test]
fn newly_enabled_builtins_preserve_results_and_errors() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let bytes = b.lit(C::byte_string(&arena, &[1, 2, 3]));
    let huge = b.lit(C::integer(
        &arena,
        arena.alloc_integer(nash_plutus::constant::Integer::from(1) << 200),
    ));
    let array = b.lit(C::proto_array(
        &arena,
        &Type::Integer,
        arena.alloc([C::integer_from(&arena, 1)]),
    ));
    let cases: &[(&str, F, &[&Core<'_>])] = &[
        ("whole_byte_shift", F::ShiftByteString, &[bytes, b.int(8)]),
        ("huge_shift", F::ShiftByteString, &[bytes, huge]),
        ("huge_rotation", F::RotateByteString, &[bytes, huge]),
        ("huge_byte_index", F::IndexByteString, &[bytes, huge]),
        ("huge_array_index", F::IndexArray, &[array, huge]),
        (
            "negative_constr_tag",
            F::ConstrData,
            &[b.int(-1), b.lit(C::proto_list(&arena, &Type::Data, &[]))],
        ),
        (
            "huge_constr_tag",
            F::ConstrData,
            &[huge, b.lit(C::proto_list(&arena, &Type::Data, &[]))],
        ),
        ("array_index", F::IndexArray, &[array, b.int(0)]),
        ("cons_byte", F::ConsByteString, &[b.int(42), bytes]),
    ];
    for (name, func, args) in cases {
        let core = b.builtin(*func, args, Ty::Erased);
        let fixture = crate::harness::prepare_fixture(&arena, core);
        insta::assert_snapshot!(
            *name,
            format!(
                "{}\n--- result\n{}\n--- logs\n{:?}",
                fixture.code_snapshot(),
                fixture.evaluated.result,
                fixture.evaluated.logs
            )
        );
        fixture.assert_equivalent(&arena);
    }
}

#[test]
fn validity_checks_structure_without_depth_or_node_limits() {
    let arena = Arena::new();
    let wrong = C::bool(&arena, false);
    let pair = C::proto_pair(&arena, &Type::Data, &Type::Data, wrong, wrong);
    for c in [
        C::proto_list(&arena, &Type::Data, arena.alloc([wrong])),
        C::proto_list(
            &arena,
            arena.alloc(Type::Pair(&Type::Data, &Type::Data)),
            arena.alloc([pair]),
        ),
    ] {
        assert!(!valid(&arena, c));
    }
    let mut typ = &Type::Integer;
    for _ in 0..100 {
        typ = arena.alloc(Type::List(typ));
    }
    assert!(valid(&arena, C::proto_list(&arena, typ, &[])));
    let many = arena.alloc_slice_copy(&vec![wrong; 1100]);
    assert!(valid(&arena, C::proto_list(&arena, &Type::Bool, many)));
    let unsafe_data = C::data(&arena, D::integer_from(&arena, -(1_i128 << 64) - 1));
    assert!(!valid(&arena, unsafe_data));
}

#[test]
fn failed_calls_do_not_stop_folding_or_cleanup() {
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
    let after = simplify(&b, anf);
    let evaluated = crate::harness::eval_core_raw(&arena, after);
    assert!(evaluated.result.starts_with("error:"));
    insta::assert_snapshot!(format!(
        "{}\n--- isolated pass Core\n{}\n--- isolated pass UPLC\n{}\n--- result\n{}\n--- logs\n{:?}",
        fixture.code_snapshot(),
        pretty(after),
        evaluated.uplc,
        evaluated.result,
        evaluated.logs
    ));
    fixture.assert_equivalent(&arena);
    assert!(std::ptr::eq(after, simplify(&b, after)));
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

#[test]
fn cleanup_exposes_folding_even_when_first_fold_pass_does_not_change() {
    use nash_ir::core::Binder;
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let int = b.int(0).ty;
    let x = Binder {
        name: b.fresh("x"),
        ty: int,
    };
    let value = b.lam(
        &[x],
        b.builtin(F::AddInteger, &[b.var(x.name, int), b.int(22)], int),
    );
    let f = Binder {
        name: b.fresh("f"),
        ty: value.ty,
    };
    let core = b.let_(f, value, b.app(b.var(f.name, f.ty), &[b.int(20)], int));
    let fixture = crate::harness::prepare_fixture(&arena, core);
    let after = simplify(&b, core);
    let evaluated = crate::harness::eval_core_raw(&arena, after);
    assert!(!evaluated.result.starts_with("error:"));
    insta::assert_snapshot!(format!(
        "{}\n--- isolated Core before\n{}\n--- isolated fixed point\n{}\n--- isolated UPLC\n{}\n--- result\n{}\n--- logs\n{:?}",
        fixture.code_snapshot(),
        pretty(core),
        pretty(after),
        evaluated.uplc,
        evaluated.result,
        evaluated.logs
    ));
    fixture.assert_equivalent(&arena);
    assert_eq!(fixture.evaluated.observable, evaluated.observable);
    assert_eq!(fixture.evaluated.logs, evaluated.logs);
    assert!(std::ptr::eq(after, simplify(&b, after)));
    assert!(std::ptr::eq(
        after,
        nash_ir::known_case::simplify_constr_data(&b, after)
    ));
    nash_ir::anf::validate(after).unwrap();
    nash_ir::hygiene::validate(after, &[]).unwrap();
}
