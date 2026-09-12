use bumpalo::Bump;
use nash_ast::{Evidence, Pred, QualifiedName, Type};
use nash_can::{Annotations, Context, environment::Tables};
use nash_region::Located;
use nash_solve::evidence::{Failure, resolve};

fn fixture<'a>(bump: &'a Bump, source: &str) -> (Tables<'a>, Annotations<'a>) {
    let source = bump.alloc_str(source);
    let parsed = nash_parse::Parser::new(bump, source).module().unwrap();
    let canonical = nash_can::canonicalize(
        bump,
        Context {
            package: Some(nash_ast::primitives::CORE),
            interfaces: None,
        },
        &parsed,
    )
    .unwrap();
    let mut uf = nash_constrain::UnionFind::new();
    let module = &canonical.module;
    let (annotations, _) = nash_solve::run(bump, &mut uf, module, &canonical.tables).unwrap();
    (canonical.tables, annotations)
}

fn input<'a>(annotations: &Annotations<'a>, name: &str) -> &'a Located<Type<'a>> {
    let Type::Lambda { from, .. } = annotations[name].typ.value else {
        panic!("function witness")
    };
    from
}

// Check compile-time representation markers before comparing runtime dictionary trees.
fn dictionaries<'a>(
    bump: &'a Bump,
    tables: &Tables<'a>,
    args: &[Evidence<'a>],
) -> &'a [Evidence<'a>] {
    bump.alloc_slice_fill_iter(
        args.iter()
            .filter_map(|evidence| match evidence {
                Evidence::Repr { trait_, typ } => {
                    let repr = nash_can::kinds::repr_of(bump, &tables.kinds, typ)
                        .expect("ground representation proof");
                    assert!(trait_.admits().contains(repr));
                    None
                }
                Evidence::Impl {
                    impl_,
                    type_args,
                    args,
                } => Some(Evidence::Impl {
                    impl_: *impl_,
                    type_args,
                    args: dictionaries(bump, tables, args),
                }),
                Evidence::StructuralEq { typ } => Some(Evidence::StructuralEq { typ }),
                Evidence::ReflexiveLift { typ } => Some(Evidence::ReflexiveLift { typ }),
                Evidence::Given { .. } | Evidence::Super { .. } => {
                    panic!("ground evidence must be closed")
                }
            })
            .collect::<Vec<_>>(),
    )
}

#[test]
fn nominally_disjoint_impls_select_matching_context_evidence() {
    let bump = Bump::new();
    let (tables, annotations) = fixture(
        &bump,
        indoc::indoc!(
            r#"
        module Main exposing (..)
        type Token = Token
        trait BigBound ('a : Big) where
            big : 'a -> 'a
        trait ConstBound ('a : Const) where
            little : 'a -> 'a
        trait Keep 'a where
            keep : 'a -> 'a
        impl BigBound Token where
            big x = x
        impl ConstBound int where
            little x = x
        impl BigBound 'a => Keep (List 'a) where
            keep x = x
        impl ConstBound 'a => Keep (list 'a) where
            keep x = x
        large : List Token -> List Token
        large x = keep x
        small : list int -> list int
        small x = keep x
        generic xs = keep xs
    "#
        ),
    );
    let trait_ = *tables
        .traits
        .keys()
        .find(|name| name.name == "Keep")
        .unwrap();
    for (name, bound) in [("large", "BigBound"), ("small", "ConstBound")] {
        let predicate = Pred::Trait {
            trait_,
            args: bump.alloc_slice_copy(&[input(&annotations, name)]),
        };
        let Evidence::Impl { args, .. } = resolve(&bump, &tables, &predicate).unwrap().unwrap()
        else {
            panic!("impl evidence")
        };
        let args = dictionaries(&bump, &tables, args);
        let [Evidence::Impl { impl_, .. }] = args else {
            panic!("one bound proof")
        };
        assert_eq!(impl_.key.trait_.name, bound);
    }
    assert_eq!(
        annotations["generic"].context.len(),
        1,
        "unresolved calls retain their predicate"
    );
}

#[test]
fn recursive_patterns_preserve_repeated_variables_and_inner_evidence() {
    let bump = Bump::new();
    let (tables, annotations) = fixture(
        &bump,
        indoc::indoc!(
            r#"
        module Main exposing (..)
        trait Keep 'a where
            keep : 'a -> 'a
        impl Keep int where
            keep x = x
        impl Keep (pair 'a 'a) where
            keep x = x
        impl Keep (pair int bytes) where
            keep x = x
        impl Keep 'a => Keep (list (pair 'a 'a)) where
            keep x = x
        same : pair int int -> pair int int
        same x = keep x
        different : pair int bytes -> pair int bytes
        different x = keep x
        nested : list (pair int int) -> list (pair int int)
        nested x = keep x
        unmatched : list (pair int bytes) -> list (pair int bytes)
        unmatched x = x
    "#
        ),
    );
    let trait_ = *tables
        .traits
        .keys()
        .find(|name| name.name == "Keep")
        .unwrap();
    for (name, expected_count) in [("same", 1), ("different", 0), ("nested", 1)] {
        let predicate = Pred::Trait {
            trait_,
            args: bump.alloc_slice_copy(&[input(&annotations, name)]),
        };
        let Evidence::Impl {
            type_args, args, ..
        } = resolve(&bump, &tables, &predicate).unwrap().unwrap()
        else {
            panic!("impl evidence")
        };
        let args = dictionaries(&bump, &tables, args);
        assert_eq!(type_args.len(), expected_count);
        if expected_count == 1 {
            assert!(
                matches!(type_args[0].value, Type::Named { reference, args: [] } if reference.name == "int")
            );
        }
        assert_eq!(args.len(), usize::from(name == "nested"));
        if name == "nested" {
            assert!(matches!(
                args,
                [Evidence::Impl {
                    type_args: [],
                    args: [],
                    ..
                }]
            ));
        }
    }
    let predicate = Pred::Trait {
        trait_,
        args: bump.alloc_slice_copy(&[input(&annotations, "unmatched")]),
    };
    assert_eq!(
        resolve(&bump, &tables, &predicate).unwrap_err().reason,
        Failure::MissingImpl
    );
}

#[test]
fn ground_resolution_preserves_nominal_aliases_and_nested_evidence() {
    let bump = Bump::new();
    let (tables, annotations) = fixture(
        &bump,
        indoc::indoc!(
            "
        module Main exposing (..)
        trait Keep 'a where
            keep : 'a -> 'a
        impl Keep int where
            keep x = x
        impl Keep 'a => Keep (list 'a) where
            keep x = x
        type alias bag 'a = list 'a
        impl Keep 'a => Keep (bag 'a) where
            keep x = x
        witness : list (bag int) -> list (bag int)
        witness x = x
        impl Keep 'e => Keep ('a, 'b, 'c, 'd, 'e) where
            keep x = x
        tuple : (int, bytes, string, bool, int) -> (int, bytes, string, bool, int)
        tuple x = keep x
    "
        ),
    );
    let trait_ = *tables.traits.keys().find(|key| key.name == "Keep").unwrap();
    let pred = Pred::Trait {
        trait_,
        args: bump.alloc_slice_copy(&[input(&annotations, "witness")]),
    };
    let Evidence::Impl {
        impl_: outer,
        type_args,
        args,
    } = resolve(&bump, &tables, &pred).unwrap().unwrap()
    else {
        panic!("list impl")
    };
    assert!(
        matches!(outer.key.heads, [nash_ast::Head::Named { reference: name, .. }] if name.name == "list")
    );
    assert!(matches!(type_args[0].value, Type::Alias { reference, .. } if reference.name == "bag"));
    let args = dictionaries(&bump, &tables, args);
    let [
        Evidence::Impl {
            impl_: alias,
            type_args,
            args,
        },
    ] = args
    else {
        panic!("nominal alias child")
    };
    assert!(
        matches!(alias.key.heads, [nash_ast::Head::Named { reference: name, .. }] if name.name == "bag")
    );
    assert!(matches!(type_args[0].value, Type::Named { reference, .. } if reference.name == "int"));
    assert!(
        matches!(args, [Evidence::Impl { impl_, type_args: [], args: [] }] if matches!(impl_.key.heads, [nash_ast::Head::Named { reference: name, .. }] if name.name == "int"))
    );
    let tuple = Pred::Trait {
        trait_,
        args: bump.alloc_slice_copy(&[input(&annotations, "tuple")]),
    };
    let Evidence::Impl {
        impl_,
        type_args,
        args,
    } = resolve(&bump, &tables, &tuple).unwrap().unwrap()
    else {
        panic!("tuple impl")
    };
    let args = dictionaries(&bump, &tables, args);
    assert!(matches!(impl_.key.heads, [nash_ast::Head::Tuple(args)] if args.len() == 5));
    assert_eq!(type_args.len(), 5);
    assert!(
        matches!(args, [Evidence::Impl { impl_, type_args: [], args: [] }] if matches!(impl_.key.heads, [nash_ast::Head::Named { reference: name, .. }] if name.name == "int"))
    );
    for (typ, expected) in type_args
        .iter()
        .zip(["int", "bytes", "string", "bool", "int"])
    {
        assert!(matches!(typ.value, Type::Named { reference, .. } if reference.name == expected));
    }
}

#[test]
fn ground_higher_kinded_context_keeps_partial_alias_and_head_order() {
    let bump = Bump::new();
    let (tables, annotations) = fixture(
        &bump,
        indoc::indoc!(
            "
        module Main exposing (..)
        type alias pairAlias 'a 'b = pair 'a 'b
        type box 'f = Box ('f int)
        trait Keep 'a where
            keep : 'a -> 'a
        impl Keep (pairAlias 'a 'b) where
            keep x = x
        trait Use 'a 'b where
            use : 'a -> 'b -> 'a
        impl Keep ('f int) => Use (box 'f) (pair 'a 'b) where
            use x _ = x
        boxIt : 'f int -> box 'f
        boxIt x = Box x
        pairIdentity : pairAlias int int -> pairAlias int int
        pairIdentity x = x
        secondIdentity : pair string bytes -> pair string bytes
        secondIdentity x = x
        witness x y = (boxIt (pairIdentity x), secondIdentity y)
        explicit : box (pairAlias int) -> box (pairAlias int)
        explicit x = x
    "
        ),
    );
    let Type::Lambda { to, .. } = annotations["witness"].typ.value else {
        panic!("first argument")
    };
    let Type::Lambda { to, .. } = to.value else {
        panic!("second argument")
    };
    let Type::Tuple {
        first: from,
        second,
        ..
    } = to.value
    else {
        panic!("ground result types")
    };
    let explicit = input(&annotations, "explicit");
    assert!(matches!(explicit.value, Type::Named { args: [partial], .. }
        if matches!(&partial.value, Type::Alias { arguments, remaining, .. }
            if arguments.len() == 1 && *remaining == ["b"])));
    let trait_ = *tables
        .traits
        .keys()
        .find(|name| name.name == "Use")
        .unwrap();
    let pred = Pred::Trait {
        trait_,
        args: bump.alloc_slice_copy(&[from, second]),
    };
    let Evidence::Impl {
        type_args, args, ..
    } = resolve(&bump, &tables, &pred).unwrap().unwrap()
    else {
        panic!("Use impl")
    };
    let [partial, first, second] = type_args else {
        panic!("head variables in order")
    };
    assert!(
        matches!(&partial.value, Type::Alias { reference, arguments, remaining, .. } if reference.name == "pairAlias" && arguments.len() == 1 && *remaining == ["b"])
    );
    assert!(matches!(first.value, Type::Named { reference, .. } if reference.name == "string"));
    assert!(matches!(second.value, Type::Named { reference, .. } if reference.name == "bytes"));
    let args = dictionaries(&bump, &tables, args);
    let [
        Evidence::Impl {
            impl_,
            type_args,
            args: [],
        },
    ] = args
    else {
        panic!("applied partial alias context")
    };
    assert!(
        matches!(impl_.key.heads, [nash_ast::Head::Named { reference: name, .. }] if name.name == "pairAlias")
    );
    assert_eq!(type_args.len(), 2);
    assert!(
        type_args.iter().all(
            |typ| matches!(typ.value, Type::Named { reference, .. } if reference.name == "int")
        )
    );
}

#[test]
fn ground_reflexive_lift_requires_exact_core_identity_and_big() {
    let bump = Bump::new();
    let (mut tables, annotations) = fixture(
        &bump,
        indoc::indoc!(
            "
        module Lift exposing (..)
        trait Lift 'small 'big where
            lift : 'small -> 'big
            lower : 'big -> 'small
        impl Lift () () where
            lift x = x
            lower x = x
        big : Int -> Int
        big x = x
        otherBig : Int -> Int
        otherBig x = x
        type alias First = Int
        type alias Second = Int
        first : First -> First
        first x = x
        second : Second -> Second
        second x = x
        small : int -> int
        small x = x
        unit : () -> ()
        unit x = x
        type Phantom 'a = Phantom
        wrap : 'a -> Phantom 'a
        wrap _ = Phantom
        type alias record = { field : unit }
        record = wrap { field = () }
    "
        ),
    );
    let trait_ = nash_ast::primitives::lift_trait();
    assert!(matches!(
        resolve(
            &bump,
            &tables,
            &Pred::Trait {
                trait_,
                args: bump.alloc_slice_copy(&[
                    input(&annotations, "big"),
                    input(&annotations, "otherBig")
                ]),
            }
        ),
        Ok(Some(Evidence::ReflexiveLift { .. }))
    ));
    assert_eq!(
        resolve(
            &bump,
            &tables,
            &Pred::Trait {
                trait_,
                args: bump.alloc_slice_copy(&[
                    input(&annotations, "first"),
                    input(&annotations, "second")
                ]),
            }
        )
        .unwrap_err()
        .reason,
        Failure::MissingImpl
    );
    let record = annotations["record"].typ;
    let Type::Named {
        args: [carrier], ..
    } = record.value
    else {
        panic!("phantom record argument")
    };
    let narrowed = bump.alloc(Located::at_zero(Type::Named {
        reference: QualifiedName {
            home: nash_ast::primitives::builtin_home(),
            name: "List",
        },
        args: bump.alloc_slice_copy(&[*carrier]),
    }));
    assert!(
        nash_can::kinds::repr_of(&bump, &tables.kinds, narrowed)
            == Some(nash_ast::primitives::Repr::Big),
        "representation depends on the head; formation checks the element requirement independently"
    );
    assert!(matches!(
        resolve(
            &bump,
            &tables,
            &Pred::Trait {
                trait_,
                args: bump.alloc_slice_copy(&[record, record]),
            }
        ),
        Ok(Some(Evidence::ReflexiveLift { .. }))
    ));
    let predicate = |name| Pred::Trait {
        trait_,
        args: bump.alloc_slice_copy(&[input(&annotations, name), input(&annotations, name)]),
    };
    assert!(matches!(
        resolve(&bump, &tables, &predicate("big")),
        Ok(Some(Evidence::ReflexiveLift { .. }))
    ));
    assert!(matches!(
        resolve(&bump, &tables, &predicate("unit")),
        Ok(Some(Evidence::Impl { .. }))
    ));
    assert_eq!(
        resolve(&bump, &tables, &predicate("small"))
            .unwrap_err()
            .reason,
        Failure::MissingImpl
    );
    let foreign = Pred::Trait {
        trait_: QualifiedName {
            home: nash_ast::ModuleName {
                package: None,
                name: "Lift",
            },
            name: "Lift",
        },
        args: predicate("big").args(),
    };
    assert_eq!(
        resolve(&bump, &tables, &foreign).unwrap_err().reason,
        Failure::MissingImpl
    );
    tables.traits.remove(&trait_);
    assert_eq!(
        resolve(&bump, &tables, &predicate("big"))
            .unwrap_err()
            .reason,
        Failure::MissingImpl
    );
}

#[test]
fn ground_resolution_reports_missing_open_and_expanding_requirements() {
    let bump = Bump::new();
    let (tables, annotations) = fixture(
        &bump,
        indoc::indoc!(
            "
        module Main exposing (..)
        trait Keep 'a where
            keep : 'a -> 'a
        impl Keep (list (list 'a)) => Keep (list 'a) where
            keep x = x
        impl Keep bool => Keep bool where
            keep x = x
        growing : list int -> list int
        growing x = x
        missing : int -> int
        missing x = x
        open : 'a -> 'a
        open x = x
        cycle : bool -> bool
        cycle x = x
        function : (int -> int) -> (int -> int)
        function x = x
    "
        ),
    );
    let trait_ = *tables.traits.keys().find(|key| key.name == "Keep").unwrap();
    let results: Vec<_> = ["missing", "open", "growing", "cycle", "function"]
        .into_iter()
        .map(|name| {
            let pred = Pred::Trait {
                trait_,
                args: bump.alloc_slice_copy(&[input(&annotations, name)]),
            };
            (name, resolve(&bump, &tables, &pred).unwrap_err().reason)
        })
        .collect();
    assert_eq!(
        results,
        vec![
            ("missing", Failure::MissingImpl),
            ("open", Failure::NonGround),
            ("growing", Failure::Limit),
            ("cycle", Failure::Limit),
            ("function", Failure::MissingImpl),
        ]
    );
    let mut deep = input(&annotations, "missing");
    for _ in 0..140 {
        deep = bump.alloc(Located::at_zero(Type::Named {
            reference: QualifiedName {
                home: nash_ast::primitives::builtin_home(),
                name: "list",
            },
            args: bump.alloc_slice_copy(&[deep]),
        }));
    }
    assert_eq!(
        resolve(
            &bump,
            &tables,
            &Pred::Trait {
                trait_,
                args: bump.alloc_slice_copy(&[deep])
            }
        )
        .unwrap_err()
        .reason,
        Failure::Limit
    );
}
