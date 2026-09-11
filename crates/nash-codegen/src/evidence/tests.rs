use super::*;
use nash_ast::primitives::{self, ReprTrait};
use nash_region::Located;

fn named<'a>(
    arena: &'a Arena,
    name: &'a str,
    args: &'a [&'a Located<Type<'a>>],
) -> &'a Located<Type<'a>> {
    arena.alloc(Located::at_zero(Type::Named {
        reference: QualifiedName {
            home: primitives::builtin_home(),
            name,
        },
        args,
    }))
}

fn fixture(arena: &Arena) -> Tables<'_> {
    let bump = arena.as_bump();
    let source = bump.alloc_str(indoc::indoc!(
        r#"
        module Main exposing (..)
        import Builtin exposing (..)
        trait Keep 'a where
            keep : 'a -> 'a
        trait Keep 'a => Twice 'a where
            twice : 'a -> 'a
        impl Keep int where
            keep x = x
        impl Keep bytes where
            keep x = x
        impl Keep 'a => Keep (list 'a) where
            keep x = x
        impl Keep 'a => Twice (list 'a) where
            twice x = x
    "#
    ));
    let parsed = nash_parse::Parser::new(bump, source).module().unwrap();
    let interfaces =
        std::collections::BTreeMap::from([("Builtin", nash_can::kinds::builtin_interface(bump))]);
    nash_can::canonicalize(
        bump,
        nash_can::Context {
            package: None,
            interfaces: Some(&interfaces),
        },
        &parsed,
    )
    .unwrap()
    .tables
}

fn resolve<'a>(
    arena: &'a Arena,
    tables: &Tables<'a>,
    name: &'a str,
    typ: &'a Located<Type<'a>>,
) -> Evidence<'a> {
    nash_solve::evidence::resolve(
        arena.as_bump(),
        tables,
        &Pred::Trait {
            trait_: QualifiedName {
                home: nash_ast::ModuleName {
                    package: None,
                    name: "Main",
                },
                name,
            },
            args: arena.alloc_slice_copy(&[typ]),
        },
    )
    .unwrap()
    .unwrap()
}

#[test]
fn nested_givens_close_in_their_own_binder_slots() {
    let arena = Arena::new();
    let tables = fixture(&arena);
    let first = NodeId::def(arena.alloc(Located::at_zero("first")));
    let second = NodeId::def(arena.alloc(Located::at_zero("second")));
    let variable = arena.alloc(Located::at_zero(Type::Var("a")));
    let int = named(&arena, "int", &[]);
    let givens = HashMap::from([
        (
            first,
            &*arena.alloc_slice_fill_iter([Evidence::Given {
                binder: second,
                index: 1,
            }]),
        ),
        (
            second,
            &*arena.alloc_slice_fill_iter([
                Evidence::Repr {
                    trait_: ReprTrait::Const,
                    typ: named(&arena, "bytes", &[]),
                },
                Evidence::Repr {
                    trait_: ReprTrait::Const,
                    typ: variable,
                },
            ]),
        ),
    ]);
    let result = ground(
        &arena,
        &tables,
        &Evidence::Given {
            binder: first,
            index: 0,
        },
        &Substitution::from([("a", int)]),
        &givens,
    )
    .unwrap();
    assert!(
        matches!(result, Evidence::Repr { trait_: ReprTrait::Const, typ } if typ.value == int.value)
    );
}

#[test]
fn superclass_substitutes_nested_impl_heads() {
    let arena = Arena::new();
    let tables = fixture(&arena);
    let int = named(&arena, "int", &[]);
    let list = named(&arena, "list", arena.alloc_slice_copy(&[int]));
    let twice = resolve(&arena, &tables, "Twice", list);
    let expected = resolve(&arena, &tables, "Keep", list);
    let actual = ground(
        &arena,
        &tables,
        &Evidence::Super {
            of: arena.alloc(twice),
            index: 0,
        },
        &Substitution::new(),
        &HashMap::new(),
    )
    .unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn repr_superclasses_and_executable_erasure_are_separate() {
    let arena = Arena::new();
    let tables = fixture(&arena);
    let int = named(&arena, "int", &[]);
    let bytes = named(&arena, "bytes", &[]);
    let proof = arena.alloc(Evidence::Repr {
        trait_: ReprTrait::Const,
        typ: int,
    });
    let superclass = ground(
        &arena,
        &tables,
        &Evidence::Super {
            of: proof,
            index: 1,
        },
        &Substitution::new(),
        &HashMap::new(),
    )
    .unwrap();
    assert!(matches!(
        superclass,
        Evidence::Repr {
            trait_: ReprTrait::Little,
            ..
        }
    ));
    assert_eq!(
        executable_identity(proof).unwrap(),
        ExecutableEvidence::Erased
    );
    assert_eq!(
        executable_identity(&Evidence::ReflexiveLift { typ: int }).unwrap(),
        executable_identity(&Evidence::ReflexiveLift { typ: bytes }).unwrap()
    );
    assert_eq!(
        executable_identity(&Evidence::StructuralEq { typ: int }).unwrap(),
        executable_identity(&Evidence::StructuralEq { typ: bytes }).unwrap()
    );
    assert_ne!(
        executable_identity(&resolve(&arena, &tables, "Keep", int)).unwrap(),
        executable_identity(&resolve(&arena, &tables, "Keep", bytes)).unwrap()
    );
}

#[test]
fn malformed_slots_and_open_types_return_errors() {
    let arena = Arena::new();
    let tables = fixture(&arena);
    let binder = NodeId::def(arena.alloc(Located::at_zero("f")));
    let given = Evidence::Given { binder, index: 0 };
    assert!(matches!(
        ground(
            &arena,
            &tables,
            &given,
            &Substitution::new(),
            &HashMap::new()
        ),
        Err(Error::MissingGiven(_))
    ));
    assert!(matches!(
        ground(
            &arena,
            &tables,
            &given,
            &Substitution::new(),
            &HashMap::from([(binder, &[][..])])
        ),
        Err(Error::GivenIndex { .. })
    ));
    let proof = arena.alloc(Evidence::Repr {
        trait_: ReprTrait::Const,
        typ: named(&arena, "int", &[]),
    });
    assert!(matches!(
        ground(
            &arena,
            &tables,
            &Evidence::Super {
                of: proof,
                index: 2
            },
            &Substitution::new(),
            &HashMap::new()
        ),
        Err(Error::SuperIndex { .. })
    ));
    let open = Evidence::Repr {
        trait_: ReprTrait::Const,
        typ: arena.alloc(Located::at_zero(Type::Var("a"))),
    };
    assert!(matches!(
        ground(
            &arena,
            &tables,
            &open,
            &Substitution::new(),
            &HashMap::new()
        ),
        Ok(Evidence::Repr { .. })
    ));
    let projected = ground(
        &arena,
        &tables,
        &Evidence::Super {
            of: arena.alloc(open),
            index: 0,
        },
        &Substitution::new(),
        &HashMap::new(),
    )
    .unwrap();
    assert!(
        matches!(projected, Evidence::Repr { trait_: ReprTrait::Storable, typ } if matches!(typ.value, Type::Var("a")))
    );
    let list = named(
        &arena,
        "list",
        arena.alloc_slice_copy(&[named(&arena, "int", &[])]),
    );
    let Evidence::Impl { impl_, args, .. } = resolve(&arena, &tables, "Twice", list) else {
        panic!("impl")
    };
    let open_impl = arena.alloc(Evidence::Impl {
        impl_,
        args,
        type_args: arena.alloc_slice_copy(&[&*arena.alloc(Located::at_zero(Type::Var("a")))]),
    });
    assert!(matches!(
        ground(
            &arena,
            &tables,
            &Evidence::Super {
                of: open_impl,
                index: 0
            },
            &Substitution::new(),
            &HashMap::new()
        ),
        Err(Error::NonGround("a"))
    ));
    assert!(matches!(
        executable_identity(&given),
        Err(Error::UnresolvedEvidence)
    ));
}

#[test]
fn cycles_and_deep_types_stop_before_recursive_allocation() {
    let arena = Arena::new();
    let tables = fixture(&arena);
    let binder = NodeId::def(arena.alloc(Located::at_zero("f")));
    let givens = HashMap::from([(
        binder,
        &*arena.alloc_slice_fill_iter([Evidence::Given { binder, index: 0 }]),
    )]);
    assert!(matches!(
        ground(
            &arena,
            &tables,
            &Evidence::Given { binder, index: 0 },
            &Substitution::new(),
            &givens
        ),
        Err(Error::Limit)
    ));
    let mut typ = named(&arena, "int", &[]);
    for _ in 0..150 {
        typ = named(&arena, "list", arena.alloc_slice_copy(&[typ]));
    }
    assert!(matches!(
        ground(
            &arena,
            &tables,
            &Evidence::Repr {
                trait_: ReprTrait::Const,
                typ
            },
            &Substitution::new(),
            &HashMap::new()
        ),
        Err(Error::Limit)
    ));
}

#[test]
fn argument_grounding_preserves_slots_but_identity_discards_repr_payloads() {
    let arena = Arena::new();
    let tables = fixture(&arena);
    let int = named(&arena, "int", &[]);
    let bytes = named(&arena, "bytes", &[]);
    let list = named(&arena, "list", arena.alloc_slice_copy(&[int]));
    let Evidence::Impl {
        impl_,
        type_args,
        args,
    } = resolve(&arena, &tables, "Keep", list)
    else {
        panic!("impl")
    };
    let altered_args = arena.alloc_slice_fill_iter(args.iter().map(|arg| match arg {
        Evidence::Repr { trait_, .. } => Evidence::Repr {
            trait_: *trait_,
            typ: bytes,
        },
        _ => ground(&arena, &tables, arg, &Substitution::new(), &HashMap::new()).unwrap(),
    }));
    let first = Evidence::Impl {
        impl_,
        type_args,
        args,
    };
    let second = Evidence::Impl {
        impl_,
        type_args: arena.alloc_slice_copy(&[bytes]),
        args: altered_args,
    };
    assert_eq!(
        executable_identity(&first).unwrap(),
        executable_identity(&second).unwrap()
    );
    let inputs = [
        Evidence::Repr {
            trait_: ReprTrait::Const,
            typ: int,
        },
        first,
    ];
    let output = ground_arguments(
        &arena,
        &tables,
        &inputs,
        &Substitution::new(),
        &HashMap::new(),
    )
    .unwrap();
    assert_eq!(output.len(), 2);
    assert!(matches!(output[0], Evidence::Repr { .. }));
}

#[test]
fn malformed_impl_arguments_and_unknown_impls_are_reported() {
    let arena = Arena::new();
    let tables = fixture(&arena);
    let int = named(&arena, "int", &[]);
    let list = named(&arena, "list", arena.alloc_slice_copy(&[int]));
    let Evidence::Impl {
        impl_,
        type_args,
        args,
    } = resolve(&arena, &tables, "Keep", list)
    else {
        panic!("impl")
    };
    for evidence in [
        Evidence::Impl {
            impl_,
            type_args: &[],
            args,
        },
        Evidence::Impl {
            impl_,
            type_args,
            args: &[],
        },
    ] {
        assert!(matches!(
            ground(
                &arena,
                &tables,
                &evidence,
                &Substitution::new(),
                &HashMap::new()
            ),
            Err(Error::MalformedImpl)
        ));
    }
    let mut wrong_home = impl_;
    wrong_home.home.name = "Unknown";
    let evidence = Evidence::Impl {
        impl_: wrong_home,
        type_args,
        args,
    };
    assert!(matches!(
        ground(
            &arena,
            &tables,
            &evidence,
            &Substitution::new(),
            &HashMap::new()
        ),
        Err(Error::UnknownImpl(_))
    ));
}

#[test]
fn substitutions_are_simultaneous_and_erased_payloads_can_stay_open() {
    let arena = Arena::new();
    let tables = fixture(&arena);
    let a = arena.alloc(Located::at_zero(Type::Var("a")));
    let b = arena.alloc(Located::at_zero(Type::Var("b")));
    let int = named(&arena, "int", &[]);
    let subst = Substitution::from([("a", &*b), ("b", int)]);
    let output = ground(
        &arena,
        &tables,
        &Evidence::StructuralEq { typ: a },
        &subst,
        &HashMap::new(),
    )
    .unwrap();
    assert!(
        matches!(output, Evidence::StructuralEq { typ } if matches!(typ.value, Type::Var("b")))
    );
    assert_eq!(
        executable_identity(&output).unwrap(),
        ExecutableEvidence::StructuralEq
    );
}
