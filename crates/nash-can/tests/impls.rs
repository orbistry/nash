use bumpalo::Bump;
use indoc::indoc;

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
