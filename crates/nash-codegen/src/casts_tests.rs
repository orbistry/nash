use super::*;
use nash_ast::{
    Ctor, CtorOpts, Kind, ModuleName, QualifiedName, Type as CanType, Union, primitives,
};
use nash_plutus::{
    arena::Arena,
    constant::Constant,
    data::PlutusData,
    program::{Program, Version},
};
use nash_region::Located;
use std::collections::{BTreeMap, HashMap};

fn data() -> Ty<'static> {
    Ty::Big(&BigTy::Data)
}
fn int() -> Ty<'static> {
    Ty::Big(&BigTy::Int)
}
fn bytes() -> Ty<'static> {
    Ty::Big(&BigTy::Bytes)
}
fn value<'a>(b: &Builder<'a>, d: &'a PlutusData<'a>) -> &'a Core<'a> {
    b.lit(Constant::data(b.arena, d))
}
fn run<'a>(
    b: &Builder<'a>,
    env: &mut TypeEnv<'a, '_>,
    core: &'a Core<'a>,
) -> Result<String, String> {
    let core = expand(b, env, core).unwrap();
    let core = crate::recursion::rewrite(b, core).unwrap();
    let term = crate::lower::lower(b.arena, core).unwrap();
    let db = nash_plutus::debruijn::to_debruijn(b.arena, term).unwrap();
    Program::new(b.arena, Version::plutus_v3(b.arena), db)
        .eval(b.arena)
        .term
        .map(nash_plutus::pretty::term)
        .map_err(|err| format!("{err:?}"))
}
fn name(text: &str) -> QualifiedName<'_> {
    QualifiedName {
        home: ModuleName {
            package: None,
            name: "Test",
        },
        name: text,
    }
}
fn named<'a>(
    a: &'a Arena,
    n: QualifiedName<'a>,
    args: &[&'a Located<CanType<'a>>],
) -> &'a Located<CanType<'a>> {
    a.alloc(Located::at_zero(CanType::Named {
        reference: n,
        args: a.alloc_slice_copy(args),
    }))
}
fn primitive<'a>(
    a: &'a Arena,
    n: &'a str,
    args: &[&'a Located<CanType<'a>>],
) -> &'a Located<CanType<'a>> {
    named(
        a,
        QualifiedName {
            home: primitives::builtin_home(),
            name: n,
        },
        args,
    )
}
fn union<'a>(
    a: &'a Arena,
    n: &'a str,
    params: &'a [&'a str],
    fields: &[&[&'a Located<CanType<'a>>]],
) -> &'a Union<'a> {
    let ctors = fields
        .iter()
        .enumerate()
        .map(|(index, args)| {
            &*a.alloc(Ctor {
                labels: None,
                name: n,
                index: index as u16,
                arity: args.len() as u16,
                arguments: a.alloc_slice_copy(args),
            })
        })
        .collect::<Vec<_>>();
    a.alloc(Union {
        kind: &Kind::Type,
        context: &[],
        name: a.alloc(Located::at_zero(n)),
        parameters: params,
        ctors: a.alloc_slice_copy(&ctors),
        alternatives: ctors.len() as u16,
        options: CtorOpts::Normal,
    })
}
#[test]
fn scalar_lifts_and_reflexive_data() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let unions = HashMap::new();
    let mut env = TypeEnv::new(&a, &unions);
    let lifted = b.cast(CastKind::Lift, Ty::Const(&ConstTy::Int), int(), b.int(41));
    insta::assert_snapshot!(run(&b,&mut env,b.cast(CastKind::Lower,int(),Ty::Const(&ConstTy::Int),lifted)).unwrap(),@"(con integer 41)");
    let lit = b.lit(Constant::byte_string(&a, &[0xaa]));
    let lifted = b.cast(CastKind::Lift, Ty::Const(&ConstTy::Bytes), bytes(), lit);
    assert_eq!(
        run(
            &b,
            &mut env,
            b.cast(CastKind::Lower, bytes(), Ty::Const(&ConstTy::Bytes), lifted)
        )
        .unwrap(),
        "(con bytestring #aa)"
    );
    let raw = value(&b, PlutusData::integer_from(&a, 9));
    assert_eq!(
        run(
            &b,
            &mut env,
            b.cast(
                CastKind::ToData,
                int(),
                data(),
                b.cast(CastKind::Lift, int(), int(), raw)
            )
        )
        .unwrap(),
        "(con data (I 9))"
    );
}
#[test]
fn scalar_checkers_reject_wrong_shape() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let unions = HashMap::new();
    let mut env = TypeEnv::new(&a, &unions);
    let i = value(&b, PlutusData::integer_from(&a, 1));
    let bs = value(&b, PlutusData::byte_string(&a, &[0xaa]));
    for kind in [CastKind::FromDataShallow, CastKind::ValidateData] {
        assert!(run(&b, &mut env, b.cast(kind, data(), int(), i)).is_ok());
        assert!(run(&b, &mut env, b.cast(kind, data(), bytes(), bs)).is_ok());
        assert!(run(&b, &mut env, b.cast(kind, data(), int(), bs)).is_err());
        assert!(run(&b, &mut env, b.cast(kind, data(), bytes(), i)).is_err());
    }
}
#[test]
fn nested_list_full_checks_each_element_and_shallow_does_not() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let unions = HashMap::new();
    let mut env = TypeEnv::new(&a, &unions);
    let ints = Ty::Big(&BigTy::List(int()));
    let nested = Ty::Big(a.alloc(BigTy::List(ints)));
    let good = PlutusData::list(&a, a.alloc_slice_copy(&[PlutusData::integer_from(&a, 7)]));
    let bad = PlutusData::list(&a, a.alloc_slice_copy(&[PlutusData::byte_string(&a, &[])]));
    let wrap = |d| value(&b, PlutusData::list(&a, a.alloc_slice_copy(&[d])));
    assert!(
        run(
            &b,
            &mut env,
            b.cast(CastKind::ValidateData, data(), nested, wrap(good))
        )
        .is_ok()
    );
    assert!(
        run(
            &b,
            &mut env,
            b.cast(CastKind::ValidateData, data(), nested, wrap(bad))
        )
        .is_err()
    );
    assert!(
        run(
            &b,
            &mut env,
            b.cast(CastKind::FromDataShallow, data(), nested, wrap(bad))
        )
        .is_ok()
    );
}
#[test]
fn record_checks_exact_length_and_order() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let unions = HashMap::new();
    let mut env = TypeEnv::new(&a, &unions);
    let record = Ty::Big(&BigTy::Record(&[bytes(), int()]));
    let i = PlutusData::integer_from(&a, 1);
    let bs = PlutusData::byte_string(&a, &[]);
    for (fields, ok) in [
        (vec![bs, i], true),
        (vec![i, bs], false),
        (vec![bs], false),
        (vec![bs, i, i], false),
    ] {
        let raw = value(&b, PlutusData::list(&a, a.alloc_slice_copy(&fields)));
        assert_eq!(
            run(
                &b,
                &mut env,
                b.cast(CastKind::ValidateData, data(), record, raw)
            )
            .is_ok(),
            ok
        );
    }
}
#[test]
fn adt_checks_tag_arity_and_full_fields() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let datum = union(
        &a,
        "Datum",
        &[],
        &[
            &[primitive(&a, "Bytes", &[]), primitive(&a, "Int", &[])],
            &[],
        ],
    );
    let unions = HashMap::from([(name("Datum"), datum)]);
    let mut env = TypeEnv::new(&a, &unions);
    let ty = env
        .ty(named(&a, name("Datum"), &[]), &BTreeMap::new())
        .unwrap();
    let i = PlutusData::integer_from(&a, 1);
    let bs = PlutusData::byte_string(&a, &[0xaa]);
    for (tag, fields, shallow, full) in [
        (0, vec![bs, i], true, true),
        (0, vec![i, i], true, false),
        (0, vec![bs], false, false),
        (0, vec![bs, i, i], false, false),
        (1, vec![], true, true),
        (2, vec![], false, false),
        (u64::MAX, vec![], false, false),
    ] {
        let raw = value(&b, PlutusData::constr(&a, tag, a.alloc_slice_copy(&fields)));
        assert_eq!(
            run(
                &b,
                &mut env,
                b.cast(CastKind::FromDataShallow, data(), ty, raw)
            )
            .is_ok(),
            shallow
        );
        assert_eq!(
            run(
                &b,
                &mut env,
                b.cast(CastKind::ValidateData, data(), ty, raw)
            )
            .is_ok(),
            full
        );
    }
}
#[test]
fn recursive_adt_validation_terminates_and_checks_tail() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let tree = named(&a, name("Tree"), &[]);
    let definition = union(&a, "Tree", &[], &[&[], &[primitive(&a, "Int", &[]), tree]]);
    let unions = HashMap::from([(name("Tree"), definition)]);
    let mut env = TypeEnv::new(&a, &unions);
    let ty = env.ty(tree, &BTreeMap::new()).unwrap();
    let end = PlutusData::constr(&a, 0, &[]);
    let i = PlutusData::integer_from(&a, 3);
    let good = PlutusData::constr(&a, 1, a.alloc_slice_copy(&[i, end]));
    let bad = PlutusData::constr(&a, 1, a.alloc_slice_copy(&[i, i]));
    assert!(
        run(
            &b,
            &mut env,
            b.cast(CastKind::ValidateData, data(), ty, value(&b, good))
        )
        .is_ok()
    );
    assert!(
        run(
            &b,
            &mut env,
            b.cast(CastKind::ValidateData, data(), ty, value(&b, bad))
        )
        .is_err()
    );
}
#[test]
fn rejects_non_intrinsic_and_unknown_representations() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let unions = HashMap::new();
    let mut env = TypeEnv::new(&a, &unions);
    for (kind, from, to) in [
        (CastKind::Lift, Ty::Const(&ConstTy::String), bytes()),
        (CastKind::ValidateData, data(), Ty::Erased),
        (CastKind::ToData, Ty::Const(&ConstTy::Int), data()),
        (CastKind::Lower, int(), Ty::Const(&ConstTy::Bytes)),
    ] {
        assert!(expand(&b, &mut env, b.cast(kind, from, to, b.int(1))).is_err());
    }
    let unknown = Ty::Big(&BigTy::List(Ty::Erased));
    assert!(
        expand(
            &b,
            &mut env,
            b.cast(CastKind::ValidateData, data(), unknown, b.int(1))
        )
        .is_err()
    );
}

#[test]
fn list_and_map_intrinsic_roundtrips() {
    use nash_plutus::typ::Type;
    let a = Arena::new();
    let b = Builder::new(&a);
    let unions = HashMap::new();
    let mut env = TypeEnv::new(&a, &unions);
    let one = Constant::data(&a, PlutusData::integer_from(&a, 1));
    let xs = b.lit(Constant::proto_list(
        &a,
        Type::data(&a),
        a.alloc_slice_copy(&[one]),
    ));
    let small = Ty::Const(&ConstTy::List(int()));
    let big = Ty::Big(&BigTy::List(int()));
    let round = b.cast(
        CastKind::Lower,
        big,
        small,
        b.cast(CastKind::Lift, small, big, xs),
    );
    assert_eq!(run(&b, &mut env, round), run(&b, &mut env, xs));
    let key = Constant::data(&a, PlutusData::byte_string(&a, &[0xab]));
    let pair = Constant::proto_pair(&a, Type::data(&a), Type::data(&a), key, one);
    let pairs = b.lit(Constant::proto_list(
        &a,
        Type::pair(&a, Type::data(&a), Type::data(&a)),
        a.alloc_slice_copy(&[pair]),
    ));
    let small = Ty::Const(&ConstTy::List(Ty::Const(&ConstTy::Pair(bytes(), int()))));
    let big = Ty::Big(&BigTy::Map(bytes(), int()));
    let round = b.cast(
        CastKind::Lower,
        big,
        small,
        b.cast(CastKind::Lift, small, big, pairs),
    );
    assert_eq!(run(&b, &mut env, round), run(&b, &mut env, pairs));
}

#[test]
fn map_validation_checks_keys_and_values() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let unions = HashMap::new();
    let mut env = TypeEnv::new(&a, &unions);
    let ty = Ty::Big(&BigTy::Map(bytes(), Ty::Big(&BigTy::List(int()))));
    let bs = PlutusData::byte_string(&a, &[]);
    let i = PlutusData::integer_from(&a, 1);
    let ints = PlutusData::list(&a, a.alloc_slice_copy(&[i]));
    for (key, val, full) in [(bs, ints, true), (i, ints, false), (bs, i, false)] {
        let map = value(&b, PlutusData::map(&a, a.alloc_slice_copy(&[(key, val)])));
        assert_eq!(
            run(
                &b,
                &mut env,
                b.cast(CastKind::ValidateData, data(), ty, map)
            )
            .is_ok(),
            full
        );
        assert!(
            run(
                &b,
                &mut env,
                b.cast(CastKind::FromDataShallow, data(), ty, map)
            )
            .is_ok()
        );
    }
    let empty = value(&b, PlutusData::map(&a, &[]));
    assert!(
        run(
            &b,
            &mut env,
            b.cast(CastKind::ValidateData, data(), ty, empty)
        )
        .is_ok()
    );
}

#[test]
fn repeated_casts_share_one_checker() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let unions = HashMap::new();
    let mut env = TypeEnv::new(&a, &unions);
    let first = Binder {
        name: b.fresh("first"),
        ty: int(),
    };
    let check = |n| {
        b.cast(
            CastKind::ValidateData,
            data(),
            int(),
            value(&b, PlutusData::integer_from(&a, n)),
        )
    };
    let expanded = expand(&b, &mut env, b.let_(first, check(1), check(2))).unwrap();
    let Core::LetRec { binders, .. } = expanded else {
        panic!("expected checker bindings")
    };
    assert_eq!(binders.len(), 1);
    insta::assert_snapshot!(nash_ir::pretty::pretty(expanded));
}

#[test]
fn input_evaluated_once_and_compiler_trace_switch() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let unions = HashMap::new();
    let mut env = TypeEnv::new(&a, &unions);
    for compiler_traces in [false, true] {
        let input = b.trace(
            b.lit(Constant::string(&a, "input")),
            value(&b, PlutusData::byte_string(&a, &[])),
        );
        let core = expand_with_traces(
            &b,
            &mut env,
            b.cast(CastKind::ValidateData, data(), int(), input),
            compiler_traces,
        )
        .unwrap();
        let core = crate::recursion::rewrite(&b, core).unwrap();
        let term = crate::lower::lower(&a, core).unwrap();
        let db = nash_plutus::debruijn::to_debruijn(&a, term).unwrap();
        let result = Program::new(&a, Version::plutus_v3(&a), db).eval(&a);
        assert!(result.term.is_err());
        assert_eq!(
            result.info.logs,
            if compiler_traces {
                vec!["input", "validateData: Int: expected I"]
            } else {
                vec!["input"]
            }
        );
    }
}

#[test]
fn growing_validation_layout_hits_resource_limit() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let var = a.alloc(Located::at_zero(CanType::Var("a")));
    let next = named(&a, name("Nest"), &[primitive(&a, "List", &[var])]);
    let definition = union(&a, "Nest", &["a"], &[&[], &[var, next]]);
    let unions = HashMap::from([(name("Nest"), definition)]);
    let mut env = TypeEnv::new(&a, &unions);
    let ty = env
        .ty(
            named(&a, name("Nest"), &[primitive(&a, "Int", &[])]),
            &BTreeMap::new(),
        )
        .unwrap();
    let result = expand(
        &b,
        &mut env,
        b.cast(
            CastKind::ValidateData,
            data(),
            ty,
            value(&b, PlutusData::constr(&a, 0, &[])),
        ),
    );
    assert!(matches!(result, Err(Error::ValidationLayoutLimit)));
}

#[test]
fn finite_parameter_replacement_can_grow_then_stabilize() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let var_a = a.alloc(Located::at_zero(CanType::Var("a")));
    let var_b = a.alloc(Located::at_zero(CanType::Var("b")));
    let list_int = primitive(&a, "List", &[primitive(&a, "Int", &[])]);
    let next = named(&a, name("Finite"), &[var_b, list_int]);
    let definition = union(&a, "Finite", &["a", "b"], &[&[], &[var_a, next]]);
    let unions = HashMap::from([(name("Finite"), definition)]);
    let mut env = TypeEnv::new(&a, &unions);
    let int = primitive(&a, "Int", &[]);
    let ty = env
        .ty(named(&a, name("Finite"), &[int, int]), &BTreeMap::new())
        .unwrap();
    assert!(
        run(
            &b,
            &mut env,
            b.cast(
                CastKind::ValidateData,
                data(),
                ty,
                value(&b, PlutusData::constr(&a, 0, &[]))
            )
        )
        .is_ok()
    );
}
