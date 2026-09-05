use bumpalo::Bump;
use indoc::indoc;

fn parse<'a>(bump: &'a Bump, source: &str) -> &'a nash_source::Module<'a> {
    let source = bump.alloc_str(source);
    bump.alloc(
        nash_parse::Parser::new(bump, source.as_bytes())
            .module()
            .unwrap(),
    )
}

#[test]
fn superclass_and_default_method() {
    let bump = Bump::new();
    let source = parse(
        &bump,
        indoc!(
            "
        module Main exposing (Same)
        import Builtin exposing (..)

        trait Eq 'a where
            eq : 'a -> 'a -> bool

        trait Eq 'a => Same 'a where
            same : 'a -> 'a -> bool
            same x y = eq x y
    "
        ),
    );
    let interfaces =
        std::collections::BTreeMap::from([("Builtin", nash_can::kinds::builtin_interface(&bump))]);
    let result = nash_can::canonicalize(
        &bump,
        nash_can::Context {
            package: None,
            interfaces: Some(&interfaces),
        },
        source,
    )
    .unwrap();
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    insta::assert_debug_snapshot!((&result.module.traits, &result.module.exports));
}

#[test]
fn superclass_cycle() {
    let bump = Bump::new();
    let source = parse(
        &bump,
        indoc!(
            "
        module Main exposing (..)

        trait Second 'a => First 'a where
            first : 'a -> 'a

        trait First 'a => Second 'a where
            second : 'a -> 'a
    "
        ),
    );
    insta::assert_debug_snapshot!(
        nash_can::canonicalize(&bump, nash_can::Context::default(), source).unwrap_err()
    );
}

#[test]
fn method_requires_each_trait_parameter() {
    let bump = Bump::new();
    let source = parse(
        &bump,
        indoc!(
            "
        module Main exposing (..)

        trait Convert 'a 'b where
            convert : 'a -> 'a
    "
        ),
    );
    insta::assert_debug_snapshot!(
        nash_can::canonicalize(&bump, nash_can::Context::default(), source).unwrap_err()
    );
}

#[test]
fn method_quantifiers_have_independent_kinds() {
    let bump = Bump::new();
    let source = parse(
        &bump,
        indoc!(
            "
        module Main exposing (..)

        trait Carry 'a where
            ordinary : 'a -> List 'b -> 'a
            higher : 'a -> 'b 'c -> 'a
    "
        ),
    );
    let result = nash_can::canonicalize(&bump, nash_can::Context::default(), source).unwrap();
    let trait_ = &result.module.traits[0].value;
    let variables: Vec<_> = trait_
        .methods
        .iter()
        .map(|m| (m.name.value, m.annotation.free_vars))
        .collect();
    insta::assert_debug_snapshot!((&trait_.kind, variables));
}

#[test]
fn method_predicate_checks_argument_kind() {
    let bump = Bump::new();
    let source = parse(
        &bump,
        indoc!(
            "
        module Main exposing (..)
        import Builtin exposing (..)

        trait BigOnly ('a : Big) where
            bigOnly : 'a -> 'a

        trait Uses 'a where
            use : BigOnly int => 'a -> 'a
    "
        ),
    );
    let interfaces =
        std::collections::BTreeMap::from([("Builtin", nash_can::kinds::builtin_interface(&bump))]);
    insta::assert_debug_snapshot!(
        nash_can::canonicalize(
            &bump,
            nash_can::Context {
                package: None,
                interfaces: Some(&interfaces)
            },
            source
        )
        .unwrap_err()
    );
}

#[test]
fn default_body_checks_nested_annotation_kinds() {
    let bump = Bump::new();
    let source = parse(
        &bump,
        indoc!(
            "
        module Main exposing (..)
        import Builtin exposing (..)

        trait Keep 'a where
            keep : 'a -> 'a
            keep x =
                let
                    bad : list ('b -> 'b)
                    bad = []
                in
                x
    "
        ),
    );
    let interfaces =
        std::collections::BTreeMap::from([("Builtin", nash_can::kinds::builtin_interface(&bump))]);
    insta::assert_debug_snapshot!(
        nash_can::canonicalize(
            &bump,
            nash_can::Context {
                package: None,
                interfaces: Some(&interfaces)
            },
            source
        )
        .unwrap_err()
    );
}

#[test]
fn mutually_referencing_method_contexts_are_not_superclass_cycles() {
    let bump = Bump::new();
    let source = parse(
        &bump,
        indoc!(
            "
        module Main exposing (..)

        trait First 'a where
            first : Second 'a => 'a -> 'a

        trait Second 'a where
            second : First 'a => 'a -> 'a
    "
        ),
    );
    let result = nash_can::canonicalize(&bump, nash_can::Context::default(), source).unwrap();
    let schemes: Vec<_> = result
        .module
        .traits
        .iter()
        .map(|t| (t.value.name.value, t.value.kind))
        .collect();
    insta::assert_debug_snapshot!(schemes);
}

#[test]
fn default_parameter_cannot_shadow_local_method() {
    let bump = Bump::new();
    let source = parse(
        &bump,
        indoc!(
            "
        module Main exposing (..)

        trait Keep 'a where
            keep : 'a -> 'a
            keep keep = keep
    "
        ),
    );
    insta::assert_debug_snapshot!(
        nash_can::canonicalize(&bump, nash_can::Context::default(), source).unwrap_err()
    );
}
