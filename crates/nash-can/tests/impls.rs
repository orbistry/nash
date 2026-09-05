use bumpalo::Bump;
use indoc::indoc;

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
    let module = nash_parse::Parser::new(&bump, source.as_bytes())
        .module()
        .unwrap();
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
    for name in ["First", "Second"] {
        let source = bump.alloc_str(&format!("module {name} exposing (..)\nimport Keep exposing (Keep)\nimpl Keep () where\n    keep x = x\n"));
        let module = nash_parse::Parser::new(&bump, source.as_bytes())
            .module()
            .unwrap();
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
    let module = nash_parse::Parser::new(&bump, b"module Main exposing (..)\n")
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
fn global_impl_metadata_survives_arena_drop_without_imports() {
    let bump = Bump::new();
    let interface = {
        let source_arena = Bump::new();
        let result = canonicalize(
            &source_arena,
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
        nash_can::deep_copy_interface(
            &bump,
            &nash_can::from_module(&source_arena, &result.module, &Default::default()),
        )
    };
    let interfaces = std::collections::BTreeMap::from([("Instances", interface)]);
    let source = "module Main exposing (..)\n";
    let module = nash_parse::Parser::new(&bump, source.as_bytes())
        .module()
        .unwrap();
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
    let module = nash_parse::Parser::new(&bump, source.as_bytes())
        .module()
        .unwrap();
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
fn impl_rejects_repeated_variables_across_heads() {
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
    insta::assert_debug_snapshot!(result.unwrap_err());
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
        impl Keep (List 'a) where
            keep x = x
        impl Keep (List 'b) where
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
    let module = nash_parse::Parser::new(bump, source.as_bytes())
        .module()
        .unwrap();
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
