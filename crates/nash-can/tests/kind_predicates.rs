//! Replacement semantics for Haskell 98 kinds and representation predicates.
mod snapshot_support;
use snapshot_support::SnapshotInputs;

use bumpalo::Bump;
use nash_ast::{Kind, Pred, primitives::ReprTrait};
use nash_can::{Context, Error};
use std::collections::BTreeMap;

fn check<'a>(bump: &'a Bump, body: &str) -> Result<nash_can::CanResult<'a>, Vec<Error<'a>>> {
    let source = bump.alloc_str(&format!(
        "module Main exposing (..)\n\nimport Builtin exposing (..)\n\n{body}\n"
    ));
    let module = nash_parse::Parser::new(bump, source)
        .module()
        .expect("fixture parses");
    let interfaces = BTreeMap::from([("Builtin", nash_can::kinds::builtin_interface(bump))]);
    nash_can::canonicalize(
        bump,
        Context {
            package: None,
            interfaces: Some(&interfaces),
        },
        &module,
    )
}

#[test]
fn declaration_kinds_and_contexts_are_separate() {
    let snapshot_inputs = SnapshotInputs::default();
    let bump = Bump::new();
    let result = check(&bump, {
        let body =
            "type Box 'a = Box 'a\ntype option 'a = None | Some 'a\ntype wrap 'f 'a = Wrap ('f 'a)";
        snapshot_inputs.record(&format!(
            "module Main exposing (..)\n\nimport Builtin exposing (..)\n\n{body}\n"
        ));
        body
    })
    .unwrap();
    let unions = result.module.unions;
    assert_eq!(unions[0].value.kind, unions[1].value.kind);
    assert_eq!(unions[0].value.kind, &Kind::Arrow(&Kind::Type, &Kind::Type));
    assert_eq!(
        unions[0].value.context[0].trait_ref(),
        Some(ReprTrait::Big.qualified())
    );
    assert!(unions[1].value.context.is_empty());
    assert_eq!(
        unions[2].value.kind,
        &Kind::Arrow(
            &Kind::Arrow(&Kind::Type, &Kind::Type),
            &Kind::Arrow(&Kind::Type, &Kind::Type)
        )
    );
    assert!(matches!(unions[2].value.context, [Pred::Apply { .. }]));
    insta::with_settings!({description => snapshot_inputs.description(), omit_expression => true}, {
        insta::assert_debug_snapshot!(
            unions
                .iter()
                .map(|u| (u.value.name.value, u.value.kind, u.value.context))
                .collect::<Vec<_>>()
        );
    });
}

#[test]
fn record_body_bounds_use_the_alias_representation() {
    let bump = Bump::new();
    let result = check(
        &bump,
        "type alias Record 'a = ({ field : ('a : Big) } : Big)",
    )
    .unwrap();
    let context = result.module.aliases[0].value.context;
    assert_eq!(context.len(), 1);
    assert!(matches!(
        context[0].args()[0].value,
        nash_ast::Type::Var("a")
    ));
    for source in [
        "type alias Record = ({ field : Int } : Term)",
        "type alias record = ({ field : int } : Big)",
    ] {
        let errors = check(&bump, source).unwrap_err();
        assert!(matches!(
            errors.as_slice(),
            [Error::RepresentationMismatch { .. }]
        ));
    }
}

#[test]
fn bad_big_field_is_a_representation_error() {
    let snapshot_inputs = SnapshotInputs::default();
    let bump = Bump::new();
    let errors = check(&bump, {
        let body = "type Bad 'a = Bad (list 'a)";
        snapshot_inputs.record(&format!(
            "module Main exposing (..)\n\nimport Builtin exposing (..)\n\n{body}\n"
        ));
        body
    })
    .unwrap_err();
    assert!(matches!(
        errors.as_slice(),
        [Error::RepresentationMismatch { .. }]
    ));
    insta::with_settings!({info => &"diagnostic", description => snapshot_inputs.description(), omit_expression => true}, {
        insta::assert_snapshot!(snapshot_inputs.errors(&errors));
    });
}

#[test]
fn annotations_enforce_higher_order_datatype_contexts() {
    let bump = Bump::new();
    let errors = check(&bump, "type option 'a = None | Some 'a\ntype wrap 'f 'a = Wrap ('f 'a)\nf : wrap list (option int) -> unit\nf x = ()").unwrap_err();
    assert!(matches!(
        errors.as_slice(),
        [Error::RepresentationMismatch {
            required: ReprTrait::Storable,
            ..
        }]
    ));
}

#[test]
fn lowercase_alias_rejects_big_body_after_substitution() {
    let bump = Bump::new();
    let errors = check(&bump, "type alias id 'a = 'a\ntype alias bad = id Int").unwrap_err();
    assert!(matches!(
        errors.as_slice(),
        [Error::RepresentationMismatch {
            required: ReprTrait::Little,
            ..
        }]
    ));
}

#[test]
fn recursive_kind_occurs_check() {
    let bump = Bump::new();
    let errors = check(&bump, "type self 'f = Self ('f 'f)").unwrap_err();
    assert!(matches!(errors.as_slice(), [Error::KindInfinite { .. }]));
}

#[test]
fn nested_recursion_can_have_a_finite_context() {
    let bump = Bump::new();
    let result = check(&bump, "type Nest 'a = N (Nest (List 'a))").unwrap();
    assert_eq!(result.module.unions[0].value.context.len(), 1);
}

#[test]
fn applied_argument_growth_is_rejected() {
    let bump = Bump::new();
    let errors = check(&bump, "type r 'f 'a = R ('f 'a) (r 'f (list 'a))").unwrap_err();
    assert!(matches!(
        errors.as_slice(),
        [Error::IrregularRecursion { parameter: "a", .. }]
    ));
}

#[test]
fn explicit_representation_contradictions_are_rejected() {
    let bump = Bump::new();
    let errors = check(&bump, "f : (Big 'a, Little 'a) => 'a -> 'a\nf x = x").unwrap_err();
    assert!(matches!(
        errors.as_slice(),
        [Error::ContradictoryRepresentation { .. }]
    ));
}

#[test]
fn trait_parameter_kinds_come_from_method_uses() {
    let bump = Bump::new();
    let result = check(
        &bump,
        "trait Functor 'f where\n    map : ('a -> 'b) -> 'f 'a -> 'f 'b",
    )
    .unwrap();
    assert_eq!(
        result.module.traits[0].value.kinds,
        &[&Kind::Arrow(&Kind::Type, &Kind::Type)]
    );
}

#[test]
fn representation_contexts_do_not_separate_overlapping_impl_heads() {
    let bump = Bump::new();
    let errors = check(
        &bump,
        "trait T 'a where\nimpl Big 'a => T (list 'a) where\nimpl Const 'a => T (list 'a) where",
    )
    .unwrap_err();
    assert!(matches!(
        errors.as_slice(),
        [Error::OverlappingImpls { .. }]
    ));
}

#[test]
fn user_impls_of_representation_traits_are_rejected() {
    let bump = Bump::new();
    let errors = check(&bump, "impl Big int where").unwrap_err();
    assert!(matches!(
        errors.as_slice(),
        [Error::ImplOfBuiltinTrait { .. }]
    ));
}

#[test]
fn alias_hidden_application_still_marks_recursive_parameters_relevant() {
    let bump = Bump::new();
    let errors = check(
        &bump,
        "type alias app 'f 'a = 'f 'a\ntype r 'f 'a = R (app 'f 'a) (r 'f (list 'a))",
    )
    .unwrap_err();
    assert!(matches!(
        errors.as_slice(),
        [Error::IrregularRecursion { parameter: "a", .. }]
    ));
}

#[test]
fn late_applied_relevance_rechecks_existing_recursive_references() {
    let bump = Bump::new();
    let errors = check(
        &bump,
        "type r 'f 'a = R (s 'f 'a) (r 'f (list 'a))\ntype s 'g 'b = S ('g 'b) (r 'g 'b)",
    )
    .unwrap_err();
    assert!(matches!(
        errors.as_slice(),
        [Error::IrregularRecursion { .. }]
    ));
}

fn imported<'a>(bump: &'a Bump, body: &str) -> Result<nash_can::CanResult<'a>, Vec<Error<'a>>> {
    let builtin = nash_can::kinds::builtin_interface(bump);
    let interfaces = BTreeMap::from([("Builtin", builtin)]);
    let source = bump.alloc_str("module Types exposing (..)\nimport Builtin exposing (..)\ntype Box 'a = Box 'a\ntype wrap 'f 'a = Wrap ('f 'a)\ntype option 'a = None | Some 'a\ntype alias count = int\n");
    let parsed = nash_parse::Parser::new(bump, source).module().unwrap();
    let checked = nash_can::canonicalize(
        bump,
        Context {
            package: None,
            interfaces: Some(&interfaces),
        },
        &parsed,
    )
    .unwrap();
    let interface = nash_can::from_module(bump, &checked.module, &BTreeMap::new());
    let interfaces = BTreeMap::from([("Builtin", builtin), ("Types", interface)]);
    let source = bump.alloc_str(&format!("module Main exposing (..)\nimport Builtin exposing (..)\nimport Types exposing (..)\n{body}\n"));
    let parsed = nash_parse::Parser::new(bump, source).module().unwrap();
    nash_can::canonicalize(
        bump,
        Context {
            package: None,
            interfaces: Some(&interfaces),
        },
        &parsed,
    )
}

#[test]
fn imported_datatype_context_rejects_a_little_box_argument() {
    let bump = Bump::new();
    let errors = imported(&bump, "f : Box int -> unit\nf x = ()").unwrap_err();
    assert!(matches!(
        errors.as_slice(),
        [Error::RepresentationMismatch {
            required: ReprTrait::Big,
            ..
        }]
    ));
}

#[test]
fn imported_apply_context_rejects_a_term_list_element() {
    let bump = Bump::new();
    let errors = imported(&bump, "f : wrap list (option int) -> unit\nf x = ()").unwrap_err();
    assert!(matches!(
        errors.as_slice(),
        [Error::RepresentationMismatch {
            required: ReprTrait::Storable,
            ..
        }]
    ));
}

#[test]
fn imported_context_and_alias_accept_valid_heads() {
    let bump = Bump::new();
    imported(&bump, "f : Box Int -> wrap list count -> unit\nf x y = ()").unwrap();
}

#[test]
fn specialization_preserves_method_context_on_another_trait_argument() {
    let bump = Bump::new();
    let result = check(
        &bump,
        "trait T 'a where\n    keep : T 'b => 'a -> 'b -> 'b\nimpl T int where\n    keep x y = y",
    )
    .unwrap();
    let nash_ast::Def::TypedDef { context, .. } = result.module.impls[0].value.methods[0] else {
        panic!("specialized method is typed")
    };
    assert!(
        context
            .iter()
            .any(|pred| pred.trait_ref().is_some_and(|name| name.name == "T")
                && matches!(pred.args()[0].value, nash_ast::Type::Var("b")))
    );
}

#[test]
fn transparent_alias_context_satisfies_representation_superclass() {
    let bump = Bump::new();
    check(&bump, "type alias Alias 'a = 'a\ntrait Big 'a => Keep 'a where\n    keep : 'a -> 'a\nimpl Keep (Alias 'a) where\n    keep x = x").unwrap();
}
