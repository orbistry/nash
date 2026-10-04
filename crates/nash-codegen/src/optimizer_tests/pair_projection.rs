use nash_ir::{build::Builder, hygiene, pretty::pretty};
use nash_plutus::arena::Arena;
#[path = "../../tests/support/pair_projection.rs"]
mod inputs;

#[test]
fn pair_projection() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    for (name, before, fails) in inputs::cases(&b) {
        let fixture = crate::harness::prepare_fixture(&arena, before);
        assert_eq!(fixture.evaluated.result.starts_with("error:"), fails);
        let anf = nash_ir::anf::normalize(&b, before);
        let isolated = nash_ir::pair_projection::reduce(&b, anf);
        insta::assert_snapshot!(
            name,
            format!(
                "{}\n--- isolated Core before\n{}\n--- isolated Core after\n{}",
                fixture.snapshot(),
                pretty(anf),
                pretty(isolated)
            )
        );
        assert_eq!(anf.ty, isolated.ty);
        hygiene::validate(isolated, &[]).unwrap();
        nash_ir::anf::validate(isolated).unwrap();
        fixture.assert_equivalent(&arena);
    }
}

#[test]
fn pair_projection_guards() {
    use nash_ir::core::{Binder, Branch, CaseKind, Test};
    use nash_plutus::{constant::Constant as C, typ::Type};
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let pair = b.lit(C::proto_pair(
        &arena,
        &Type::Integer,
        &Type::Integer,
        C::integer_from(&arena, 7),
        C::integer_from(&arena, 8),
    ));
    for mode in [
        "default",
        "wrong_test",
        "one_binder",
        "no_branches",
        "field_type",
        "unknown_parameter",
    ] {
        let x = Binder {
            name: b.fresh("x"),
            ty: if mode == "field_type" {
                nash_ir::ty::Ty::Erased
            } else {
                b.int(0).ty
            },
        };
        let y = Binder {
            name: b.fresh("y"),
            ty: b.int(0).ty,
        };
        let p = Binder {
            name: b.fresh("p"),
            ty: pair.ty,
        };
        let binders = if mode == "one_binder" {
            arena.alloc_slice_copy(&[x])
        } else {
            arena.alloc_slice_copy(&[x, y])
        };
        let branch = Branch {
            test: if mode == "wrong_test" {
                Test::Tag(0)
            } else {
                Test::Pair
            },
            binders,
            body: b.var(x.name, x.ty),
        };
        let branches = if mode == "no_branches" {
            &[][..]
        } else {
            arena.alloc_slice_copy(&[branch])
        };
        let subject = if mode == "unknown_parameter" {
            b.var(p.name, p.ty)
        } else {
            pair
        };
        let before = b.case(
            CaseKind::Pair,
            subject,
            branches,
            (mode == "default").then(|| b.int(0)),
            x.ty,
        );
        let before = if mode == "unknown_parameter" {
            b.lam(&[p], before)
        } else {
            before
        };
        let after = nash_ir::pair_projection::reduce(&b, before);
        let lowered = |core| match crate::lower::lower(&arena, core) {
            Ok(term) => nash_plutus::pretty::term(term),
            Err(error) => format!("error: {error}"),
        };
        let pipeline = if matches!(mode, "field_type" | "unknown_parameter") {
            crate::snapshot_optimizer::prepare(&arena, before).snapshot()
        } else {
            String::new()
        };
        insta::assert_snapshot!(
            mode,
            format!(
                "{}--- isolated Core before\n{}\n--- UPLC before\n{}\n--- isolated Core after\n{}\n--- UPLC after\n{}",
                if pipeline.is_empty() {
                    pipeline
                } else {
                    format!("{pipeline}\n")
                },
                pretty(before),
                lowered(before),
                pretty(after),
                lowered(after)
            )
        );
        assert!(std::ptr::eq(before, after));
    }
}
