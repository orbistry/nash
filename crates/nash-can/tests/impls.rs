use bumpalo::Bump;
use indoc::indoc;

#[test]
fn inline_impl_bounds_are_retained_for_superclasses() {
    let bump = Bump::new();
    let result = canonicalize(
        &bump,
        indoc!(
            "
        module Main exposing (..)
        trait Keep 'a where
            keep : 'a -> 'a
        trait Keep 'a => More 'a where
            more : 'a -> 'a
        impl Keep (list ('a : Big)) where
            keep x = x
        impl More (list ('a : Big)) where
            more x = x
    "
        ),
    )
    .unwrap();
    assert_eq!(result.tables.impls.len(), 2);
    for info in result.tables.impls.values() {
        assert!(
            info.context
                .iter()
                .any(|pred| pred.trait_ref()
                    == Some(nash_ast::primitives::ReprTrait::Big.qualified()))
        );
    }
}

#[test]
fn recursive_overlap_ignores_representation_contexts() {
    let bump = Bump::new();
    let mut keys = Vec::new();
    for bound in ["Big", "Const", "Storable"] {
        let source = bump.alloc_str(&format!(
            "module Main exposing (..)\ntrait Bound ('a : {bound}) where\n    bound : 'a -> 'a\ntrait Keep 'a where\n    keep : 'a -> 'a\nimpl Bound 'a => Keep (list (pair 'a 'a)) where\n    keep x = x\n"
        ));
        let result = canonicalize(&bump, source).unwrap();
        keys.push(*result.tables.impls.keys().next().unwrap());
    }
    let overlap = |a: usize, b: usize, budget: &mut usize| {
        nash_ast::head::overlaps(keys[a].heads, keys[b].heads, budget)
    };
    assert!(
        overlap(0, 1, &mut 16_384).unwrap(),
        "representation contexts do not make equal heads disjoint"
    );
    assert!(overlap(0, 2, &mut 16_384).unwrap());
    assert!(overlap(1, 2, &mut 16_384).unwrap());
    assert!(
        overlap(0, 0, &mut 16_384).unwrap(),
        "fresh binders preserve overlap"
    );
    let mut budget = 16_384;
    assert!(overlap(0, 0, &mut budget).unwrap());
    let mut exact_budget = 16_384 - budget;
    assert!(exact_budget > 0);
    assert!(overlap(0, 0, &mut exact_budget).unwrap());
    assert_eq!(exact_budget, 0);
    assert!(overlap(0, 0, &mut 0).is_err());
}

#[test]
fn impl_cannot_own_an_imported_trait_and_imported_heads() {
    let bump = Bump::new();
    let interfaces = std::collections::BTreeMap::from([
        ("Lift", core_lift(&bump)),
        ("Builtin", nash_can::kinds::builtin_interface(&bump)),
    ]);
    let source = indoc!(
        "
        module Main exposing (..)
        import Lift exposing (Lift)
        import Builtin exposing (List)
        impl Lift (List 'a) (List 'b) where
            lift x = x
            lower x = x
    "
    );
    let module = nash_parse::Parser::new(&bump, source).module().unwrap();
    let result = nash_can::canonicalize(
        &bump,
        nash_can::Context {
            package: None,
            interfaces: Some(&interfaces),
        },
        &module,
    )
    .unwrap_err();
    assert!(matches!(
        result.as_slice(),
        [nash_can::Error::OrphanImpl { .. }]
    ));
    insta::assert_debug_snapshot!(result);
}

#[test]
fn impl_heads_reject_non_constructor_shapes() {
    let bump = Bump::new();
    let mut errors = Vec::new();
    for head in [
        "'a",
        "('a : Big)",
        "('a -> 'b)",
        "(('a -> 'b) : Term)",
        "{ value : 'a }",
        "('f 'a)",
    ] {
        let source = format!(
            "module Main exposing (..)\ntrait Keep 'a where\n    keep : 'a -> 'a\nimpl Keep {head} where\n    keep x = x\n"
        );
        let result = canonicalize(&bump, &source).unwrap_err();
        assert!(matches!(
            result.as_slice(),
            [nash_can::Error::BadInstanceHead { .. }]
        ));
        errors.push((head, result));
    }
    insta::assert_debug_snapshot!(errors);
}

#[test]
fn explicit_lift_impls_cannot_overlap_the_big_reflexive_rule() {
    let bump = Bump::new();
    let interfaces = std::collections::BTreeMap::from([("Lift", core_lift(&bump))]);
    let mut results = Vec::new();
    for (declaration, heads) in [
        ("type Color = Red", "Color Color"),
        (
            "type Container 'a = Wrap 'a",
            "(Container 'a) (Container 'b)",
        ),
        ("type color = Red", "color color"),
        ("type alias Alias 'a = 'a", "(Alias 'a) (Alias 'b)"),
    ] {
        let source = bump.alloc_str(&format!("module Main exposing (..)\nimport Lift exposing (Lift)\n{declaration}\nimpl Lift {heads} where\n    lift x = x\n    lower x = x\n"));
        let module = nash_parse::Parser::new(&bump, source).module().unwrap();
        results.push(
            nash_can::canonicalize(
                &bump,
                nash_can::Context {
                    package: None,
                    interfaces: Some(&interfaces),
                },
                &module,
            )
            .map(|_| ()),
        );
    }
    assert!(matches!(
        results[0].as_ref().unwrap_err().as_slice(),
        [nash_can::Error::ReflexiveLiftOverlap { .. }]
    ));
    assert!(matches!(
        results[1].as_ref().unwrap_err().as_slice(),
        [nash_can::Error::ReflexiveLiftOverlap { .. }]
    ));
    assert!(results[2].is_ok());
    assert!(matches!(
        results[3].as_ref().unwrap_err().as_slice(),
        [nash_can::Error::ReflexiveLiftOverlap { .. }]
    ));
    insta::assert_debug_snapshot!(results);
}

fn core_lift<'a>(bump: &'a Bump) -> nash_can::Interface<'a> {
    let module = nash_parse::Parser::new(bump, "module Lift exposing (Lift)\ntrait Lift 'small 'big where\n    lift : 'small -> 'big\n    lower : 'big -> 'small\n").module().unwrap();
    let result = nash_can::canonicalize(
        bump,
        nash_can::Context {
            package: Some(nash_ast::primitives::CORE),
            interfaces: None,
        },
        &module,
    )
    .unwrap();
    nash_can::from_module(bump, &result.module, &Default::default())
}

#[test]
fn reflexive_lift_proves_big_without_narrowing_rigid_variables() {
    let bump = Bump::new();
    let interfaces = std::collections::BTreeMap::from([("Lift", core_lift(&bump))]);
    let mut results = Vec::new();
    for (container, lifted) in [
        ("Container", "'a"),
        ("container", "'a"),
        ("container", "(List 'a)"),
    ] {
        let source = bump.alloc_str(&format!("module Main exposing (..)\nimport Lift exposing (Lift)\ntype {container} 'a = Wrap 'a\ntrait Tag 'a where\n    tag : 'a -> 'a\ntrait Tag 'a => Top 'a where\n    top : 'a -> 'a\nimpl Lift {lifted} {lifted} => Tag ({container} 'a) where\n    tag x = x\nimpl Top ({container} 'a) where\n    top x = x\n"));
        let module = nash_parse::Parser::new(&bump, source).module().unwrap();
        results.push(
            nash_can::canonicalize(
                &bump,
                nash_can::Context {
                    package: None,
                    interfaces: Some(&interfaces),
                },
                &module,
            )
            .map(|_| ()),
        );
    }
    assert!(results[0].is_ok());
    assert!(results[1].is_err());
    assert!(results[2].is_err());
    insta::assert_debug_snapshot!(results);
}

#[test]
fn reflexive_lift_accepts_big_but_not_const() {
    let bump = Bump::new();
    let interfaces = std::collections::BTreeMap::from([("Lift", core_lift(&bump))]);
    let mut results = Vec::new();
    for head in ["Color", "()"] {
        let source = bump.alloc_str(&format!("module Main exposing (..)\nimport Lift exposing (Lift)\ntype Color = Red\ntrait Lift 'a 'a => RoundTrip 'a where\n    roundTrip : 'a -> 'a\nimpl RoundTrip {head} where\n    roundTrip x = x\n"));
        let module = nash_parse::Parser::new(&bump, source).module().unwrap();
        results.push(
            nash_can::canonicalize(
                &bump,
                nash_can::Context {
                    package: None,
                    interfaces: Some(&interfaces),
                },
                &module,
            )
            .map(|_| ()),
        );
    }
    assert!(results[0].is_ok());
    assert!(results[1].is_err());
    insta::assert_debug_snapshot!(results);
}

#[test]
fn reflexive_lift_requires_the_exact_core_trait_identity() {
    let bump = Bump::new();
    let mut enabled = Vec::new();
    for (module_name, package) in [
        ("Lift", Some(nash_ast::primitives::CORE)),
        ("Lift", None),
        ("Other", Some(nash_ast::primitives::CORE)),
        (
            "Lift",
            Some(nash_ast::PackageName {
                author: "someone",
                project: "core",
            }),
        ),
    ] {
        let source = bump.alloc_str(&format!("module {module_name} exposing (..)\ntrait Lift 'small 'big where\n    lift : 'small -> 'big\n    lower : 'big -> 'small\n"));
        let module = nash_parse::Parser::new(&bump, source).module().unwrap();
        let result = nash_can::canonicalize(
            &bump,
            nash_can::Context {
                package,
                interfaces: None,
            },
            &module,
        )
        .unwrap();
        enabled.push(result.tables.has_reflexive_lift());
        assert!(
            result.tables.impls.is_empty(),
            "compiler rule is not a constructor impl"
        );
    }
    assert_eq!(enabled, [true, false, false, false]);
}

#[test]
fn partially_applied_alias_binds_remaining_method_arguments() {
    let bump = Bump::new();
    let result = canonicalize(
        &bump,
        indoc!(
            "
        module Main exposing (..)
        type alias Pair 'left 'right = { first : 'left, second : 'right }
        trait Keep 'f where
            keep : 'f 'a -> 'f 'a
        impl Keep (Pair 'a) where
            keep x = x
    "
        ),
    )
    .unwrap();
    let nash_ast::Def::TypedDef {
        annotation,
        free_vars,
        ..
    } = result.module.impls[0].value.methods[0]
    else {
        panic!("typed method")
    };
    let nash_ast::Type::Lambda { from, .. } = annotation.value else {
        panic!("method arrow")
    };
    assert!(
        matches!(from.value, nash_ast::Type::Alias { .. }),
        "applied alias must normalize: {from:#?}"
    );
    insta::assert_debug_snapshot!((from, free_vars));
}

#[test]
fn superclass_givens_preserve_nominal_alias_identity() {
    let bump = Bump::new();
    let mut results = Vec::new();
    for given in ["Box", "Other"] {
        let source = format!(
            "module Main exposing (..)\ntype alias Box 'a = {{ value : 'a }}\ntype alias Other 'a = {{ value : 'a }}\ntrait Eq 'a where\n    eq : 'a -> 'a\ntrait Eq 'a => Ord 'a where\n    compare : 'a -> 'a\nimpl Eq ({given} 'a) => Ord (Box 'a) where\n    compare x = x\n"
        );
        results.push(canonicalize(&bump, &source).map(|_| ()));
    }
    insta::assert_debug_snapshot!(results);
}

#[test]
fn superclass_context_substitutes_higher_kinded_arguments() {
    let bump = Bump::new();
    let result = canonicalize(
        &bump,
        indoc!(
            "
        module Main exposing (..)
        type Wrap 'f 'a = Wrap ('f 'a)
        trait Eq 'a where
            eq : 'a -> 'a
        trait Eq 'a => Ord 'a where
            compare : 'a -> 'a
        impl Eq ('f 'a) => Eq (Wrap 'f 'a) where
            eq x = x
        impl Eq ('f 'a) => Ord (Wrap 'f 'a) where
            compare x = x
    "
        ),
    )
    .unwrap();
    insta::assert_debug_snapshot!(result.tables.impls.keys().collect::<Vec<_>>());
}

#[test]
fn superclass_impl_is_available_from_an_interface() {
    let bump = Bump::new();
    let base = canonicalize(
        &bump,
        indoc!(
            "
        module Base exposing (..)
        type Color = Red
        trait Eq 'a where
            eq : 'a -> 'a
        impl Eq Color where
            eq x = x
    "
        ),
    )
    .unwrap();
    let interfaces = std::collections::BTreeMap::from([(
        "Base",
        nash_can::from_module(&bump, &base.module, &Default::default()),
    )]);
    let source = indoc!(
        "
        module Main exposing (..)
        import Base exposing (Eq, Color)
        trait Eq 'a => Ord 'a where
            compare : 'a -> 'a
        impl Ord Color where
            compare x = x
    "
    );
    let module = nash_parse::Parser::new(&bump, source).module().unwrap();
    let result = nash_can::canonicalize(
        &bump,
        nash_can::Context {
            package: None,
            interfaces: Some(&interfaces),
        },
        &module,
    )
    .unwrap();
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    insta::assert_debug_snapshot!(result.tables.impls.keys().collect::<Vec<_>>());
}

#[test]
fn superclass_context_uses_given_superclasses() {
    let bump = Bump::new();
    let result = canonicalize(
        &bump,
        indoc!(
            "
        module Main exposing (..)
        trait Eq 'a where
            eq : 'a -> 'a
        trait Eq 'a => Ord 'a where
            compare : 'a -> 'a
        impl Eq 'a => Eq (List 'a) where
            eq x = x
        impl Ord 'a => Ord (List 'a) where
            compare x = x
    "
        ),
    )
    .unwrap();
    insta::assert_debug_snapshot!(result.tables.impls.keys().collect::<Vec<_>>());
}

#[test]
fn superclass_resolution_bounds_expanding_contexts() {
    let bump = Bump::new();
    let result = canonicalize(
        &bump,
        indoc!(
            "
        module Main exposing (..)
        trait Eq 'a where
            eq : 'a -> 'a
        trait Eq 'a => Ord 'a where
            compare : 'a -> 'a
        impl Eq (List (List 'a)) => Eq (List 'a) where
            eq x = x
        impl Ord (List 'a) where
            compare x = x
    "
        ),
    );
    insta::assert_debug_snapshot!(result.unwrap_err());
}

#[test]
fn missing_superclass_is_rejected() {
    let bump = Bump::new();
    let result = canonicalize(
        &bump,
        indoc!(
            "
        module Main exposing (..)
        type Color = Red
        trait Eq 'a where
            eq : 'a -> 'a
        trait Eq 'a => Ord 'a where
            compare : 'a -> 'a
        impl Ord Color where
            compare x = x
    "
        ),
    );
    insta::assert_debug_snapshot!(result.unwrap_err());
}

#[test]
fn superclass_impl_may_follow_its_use() {
    let bump = Bump::new();
    let result = canonicalize(
        &bump,
        indoc!(
            "
        module Main exposing (..)
        type Color = Red
        trait Eq 'a where
            eq : 'a -> 'a
        trait Eq 'a => Ord 'a where
            compare : 'a -> 'a
        impl Ord Color where
            compare x = x
        impl Eq Color where
            eq x = x
    "
        ),
    )
    .unwrap();
    insta::assert_debug_snapshot!(result.tables.impls.keys().collect::<Vec<_>>());
}

#[test]
fn superclass_resolution_rejects_context_cycles() {
    let bump = Bump::new();
    let result = canonicalize(
        &bump,
        indoc!(
            "
        module Main exposing (..)
        type Color = Red
        trait Eq 'a where
            eq : 'a -> 'a
        trait Eq 'a => Ord 'a where
            compare : 'a -> 'a
        impl Eq Color => Eq Color where
            eq x = x
        impl Ord Color where
            compare x = x
    "
        ),
    );
    insta::assert_debug_snapshot!(result.unwrap_err());
}

#[test]
fn global_overlap_between_core_modules() {
    let bump = Bump::new();
    let trait_module = canonicalize(
        &bump,
        indoc!(
            "
        module Keep exposing (Keep)
        trait Keep 'a where
            keep : 'a -> 'a
    "
        ),
    )
    .unwrap();
    let interfaces = std::collections::BTreeMap::from([(
        "Keep",
        nash_can::from_module(&bump, &trait_module.module, &Default::default()),
    )]);
    // Compile independently, then combine the interfaces as a build would.
    let mut compiled = Vec::new();
    for (name, head) in [
        ("First", "(list 'a, list 'a)"),
        ("Second", "(list int, list int)"),
    ] {
        let source = bump.alloc_str(&format!("module {name} exposing (..)\nimport Keep exposing (Keep)\nimpl Keep {head} where\n    keep x = x\n"));
        let module = nash_parse::Parser::new(&bump, source).module().unwrap();
        let result = nash_can::canonicalize(
            &bump,
            nash_can::Context {
                package: Some(nash_ast::primitives::CORE),
                interfaces: Some(&interfaces),
            },
            &module,
        )
        .unwrap();
        compiled.push((
            name,
            nash_can::from_module(&bump, &result.module, &Default::default()),
        ));
    }
    let mut all_interfaces = interfaces.clone();
    all_interfaces.extend(compiled);
    let module = nash_parse::Parser::new(&bump, "module Main exposing (..)\n")
        .module()
        .unwrap();
    let result = nash_can::canonicalize(
        &bump,
        nash_can::Context {
            package: None,
            interfaces: Some(&all_interfaces),
        },
        &module,
    );
    insta::assert_debug_snapshot!(result.unwrap_err());
}

#[test]
fn global_impl_metadata_is_available_without_imports() {
    let bump = Bump::new();
    let interface = {
        let source_arena = &bump;
        let result = canonicalize(
            source_arena,
            indoc!(
                "
            module Instances exposing (Keep)
            trait Hidden 'a where
                hidden : 'a -> 'a
            trait Keep 'a where
                keep : 'a -> 'a
            impl Hidden 'a => Keep (List 'a) where
                keep x = x
        "
            ),
        )
        .unwrap();
        nash_can::from_module(source_arena, &result.module, &Default::default())
    };
    let interfaces = std::collections::BTreeMap::from([("Instances", interface)]);
    let source = "module Main exposing (..)\n";
    let module = nash_parse::Parser::new(&bump, source).module().unwrap();
    let result = nash_can::canonicalize(
        &bump,
        nash_can::Context {
            package: None,
            interfaces: Some(&interfaces),
        },
        &module,
    )
    .unwrap();
    let traits: Vec<_> = result.tables.traits.keys().collect();
    insta::assert_debug_snapshot!((traits, result.tables.impls));
}

#[test]
fn unit_and_tuple_impls_belong_to_core() {
    let bump = Bump::new();
    let trait_module = canonicalize(
        &bump,
        indoc!(
            "
        module Keep exposing (Keep)
        trait Keep 'a where
            keep : 'a -> 'a
    "
        ),
    )
    .unwrap();
    let interfaces = std::collections::BTreeMap::from([(
        "Keep",
        nash_can::from_module(&bump, &trait_module.module, &Default::default()),
    )]);
    let source = indoc!(
        "
        module Instances exposing (..)
        import Keep exposing (Keep)
        impl Keep () where
            keep x = x
        impl Keep ('a, 'b) where
            keep x = x
    "
    );
    let module = nash_parse::Parser::new(&bump, source).module().unwrap();
    let ordinary = nash_can::canonicalize(
        &bump,
        nash_can::Context {
            package: None,
            interfaces: Some(&interfaces),
        },
        &module,
    )
    .unwrap_err();
    let core = nash_can::canonicalize(
        &bump,
        nash_can::Context {
            package: Some(nash_ast::primitives::CORE),
            interfaces: Some(&interfaces),
        },
        &module,
    )
    .unwrap();
    assert!(core.warnings.is_empty(), "{:?}", core.warnings);
    let heads: Vec<_> = core.module.impls.iter().map(|i| i.value.heads).collect();
    insta::assert_debug_snapshot!((ordinary, heads));
}

#[test]
fn tuple_impl_keys_preserve_large_arities() {
    let bump = Bump::new();
    let variables = (0..258)
        .map(|index| format!("'a{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let source = format!(
        "module Main exposing (..)\ntrait Keep 'a where\n    keep : 'a -> 'a\nimpl Keep ('a, 'b) where\n    keep x = x\nimpl Keep ({variables}) where\n    keep x = x\n"
    );
    let canonical = canonicalize(&bump, &source).unwrap();
    let arities = canonical
        .tables
        .impls
        .keys()
        .map(|key| match key.heads {
            [nash_ast::Head::Tuple(args)] => args.len(),
            _ => panic!("tuple impl key"),
        })
        .collect::<Vec<_>>();
    assert_eq!(arities, vec![2, 258]);
}

#[test]
fn impl_duplicate_methods_preserve_both_locations() {
    let bump = Bump::new();
    let result = canonicalize(
        &bump,
        indoc!(
            "
        module Main exposing (..)
        trait Keep 'a where
            keep : 'a -> 'a
        impl Keep int where
            keep x = x
            keep y = y
    "
        ),
    );
    let errors = result.unwrap_err();
    assert!(matches!(
        errors.as_slice(),
        [nash_can::Error::DuplicateMethod { .. }]
    ));
    insta::assert_debug_snapshot!(errors);
}

#[test]
fn impl_overapplied_heads_report_type_arity() {
    let bump = Bump::new();
    let mut errors = Vec::new();
    for head in ["list 'a 'b", "listAlias 'a 'b"] {
        let source = format!(
            "module Main exposing (..)\ntype alias listAlias 'a = list 'a\ntrait Keep 'a where\n    keep : 'a -> 'a\nimpl Keep ({head}) where\n    keep x = x\n"
        );
        let result = canonicalize(&bump, &source).unwrap_err();
        assert!(matches!(
            result.as_slice(),
            [nash_can::Error::BadArity { .. }]
        ));
        errors.push(result);
    }
    insta::assert_debug_snapshot!(errors);
}

#[test]
fn impl_unknown_method_precedes_missing_method() {
    let bump = Bump::new();
    let result = canonicalize(
        &bump,
        indoc!(
            "
        module Main exposing (..)
        trait Keep 'a where
            keep : 'a -> 'a
        impl Keep (List 'a) where
            typo x = x
    "
        ),
    );
    insta::assert_debug_snapshot!(result.unwrap_err());
}

#[test]
fn impl_missing_required_method() {
    let bump = Bump::new();
    let result = canonicalize(
        &bump,
        indoc!(
            "
        module Main exposing (..)
        trait Keep 'a where
            keep : 'a -> 'a
            other : 'a -> 'a
        impl Keep (List 'a) where
            keep x = x
    "
        ),
    );
    insta::assert_debug_snapshot!(result.unwrap_err());
}

#[test]
fn impl_reuses_one_variable_across_heads() {
    let bump = Bump::new();
    let result = canonicalize(
        &bump,
        indoc!(
            "
        module Main exposing (..)
        trait Convert 'a 'b where
            convert : 'a -> 'b
        impl Convert (List 'a) (List 'a) where
            convert x = x
    "
        ),
    );
    let result = result.unwrap();
    assert_eq!(result.module.impls[0].value.variables, &["a"]);
    assert_eq!(
        result.module.impls[0].value.heads[0].value,
        result.module.impls[0].value.heads[1].value
    );
}

#[test]
fn impl_context_cannot_introduce_variables() {
    let bump = Bump::new();
    let result = canonicalize(
        &bump,
        indoc!(
            "
        module Main exposing (..)
        trait Keep 'a where
            keep : 'a -> 'a
        impl Keep 'b => Keep (List 'a) where
            keep x = x
    "
        ),
    );
    insta::assert_debug_snapshot!(result.unwrap_err());
}

#[test]
fn impl_overlap_ignores_variable_names() {
    let bump = Bump::new();
    let result = canonicalize(
        &bump,
        indoc!(
            "
        module Main exposing (..)
        trait Keep 'a where
            keep : 'a -> 'a
        impl Keep (list (pair 'a 'a)) where
            keep x = x
        impl Keep (list (pair 'b 'b)) where
            keep x = x
    "
        ),
    );
    insta::assert_debug_snapshot!(result.unwrap_err());
}

#[test]
fn impl_default_does_not_require_an_override() {
    let bump = Bump::new();
    let result = canonicalize(
        &bump,
        indoc!(
            "
        module Main exposing (..)
        trait Keep 'a where
            keep : 'a -> 'a
            keep x = x
            other : 'a -> 'a
        impl Keep (List 'a) where
            other x = keep x
    "
        ),
    )
    .unwrap();
    insta::assert_debug_snapshot!(result.module.impls[0].value.methods);
}

fn canonicalize<'a>(
    bump: &'a Bump,
    source: &str,
) -> Result<nash_can::CanResult<'a>, Vec<nash_can::Error<'a>>> {
    let source = bump.alloc_str(source);
    let module = nash_parse::Parser::new(bump, source).module().unwrap();
    nash_can::canonicalize(bump, nash_can::Context::default(), &module)
}

#[test]
fn impl_method_substitution_does_not_capture_head_variables() {
    let bump = Bump::new();
    let result = canonicalize(
        &bump,
        indoc!(
            "
        module Main exposing (..)

        trait Keep 'a where
            keep : 'a -> 'b -> 'b

        impl Keep (List 'b) where
            keep xs value = value
    "
        ),
    )
    .unwrap();
    let nash_ast::Def::TypedDef {
        annotation,
        free_vars,
        context,
        ..
    } = result.module.impls[0].value.methods[0]
    else {
        panic!("typed method")
    };
    insta::assert_debug_snapshot!((annotation, free_vars, context));
}

#[test]
fn impl_unapplied_constructor() {
    let bump = Bump::new();
    let result = canonicalize(
        &bump,
        indoc!(
            "
        module Main exposing (..)

        trait Functor 'f where
            map : ('a -> 'b) -> 'f 'a -> 'f 'b

        impl Functor List where
            map f xs = xs
    "
        ),
    )
    .unwrap();
    insta::assert_debug_snapshot!(result.module.impls[0].value.heads);
}

#[test]
fn impl_method_retains_owner_kind_restriction() {
    let bump = Bump::new();
    let result = canonicalize(
        &bump,
        indoc!(
            "
        module Main exposing (..)

        type option 'a = None | Some 'a

        trait Keep 'f where
            keep : 'f ('a : Big) -> 'f 'a

        impl Keep option where
            keep value = value
        "
        ),
    )
    .unwrap();
    let nash_ast::Def::TypedDef {
        free_vars, context, ..
    } = result.module.impls[0].value.methods[0]
    else {
        panic!("typed method")
    };
    assert!(
        context
            .iter()
            .all(|pred| pred.trait_ref().is_none_or(|name| name.name != "Keep")),
        "owner dictionary is supplied by the impl"
    );
    assert_eq!(*free_vars, &["a"]);
    assert!(
        context
            .iter()
            .any(|pred| pred.trait_ref() == Some(nash_ast::primitives::ReprTrait::Big.qualified()))
    );
}

#[test]
fn impl_head_kind_mismatch() {
    let bump = Bump::new();
    let result = canonicalize(
        &bump,
        indoc!(
            "
        module Main exposing (..)

        type Color = Red
        trait Functor 'f where
            map : ('a -> 'b) -> 'f 'a -> 'f 'b

        impl Functor Color where
            map f xs = xs
    "
        ),
    );
    insta::assert_debug_snapshot!(result.unwrap_err());
}
