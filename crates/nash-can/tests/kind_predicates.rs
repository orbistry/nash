//! Replacement semantics for Haskell 98 kinds and representation predicates.
mod snapshot_support;
use snapshot_support::SnapshotInputs;

use bumpalo::Bump;
use nash_ast::Kind;
use nash_can::{Context, Error};
use std::collections::BTreeMap;

fn module_source(body: &str) -> String {
    format!(
        "module Main exposing (..)\n\nimport Primitive exposing (..)\nimport Builtin exposing (..)\n\n{body}\n"
    )
}

fn check<'a>(bump: &'a Bump, body: &str) -> Result<nash_can::CanResult<'a>, Vec<Error<'a>>> {
    let source = bump.alloc_str(&module_source(body));
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

macro_rules! check_snapshot {
    (@result $source:expr, $result:expr, $description:expr) => {{
        insta::with_settings!({description => $description, omit_expression => true}, {
            match $result {
                Ok(can) => insta::assert_debug_snapshot!(can.module),
                Err(errors) => insta::assert_snapshot!(snapshot_support::errors($source, errors)),
            }
        });
    }};
    ($bump:expr, $body:expr $(,)?) => {{
        let body = $body;
        let source = module_source(body);
        let result = check($bump, body);
        check_snapshot!(@result &source, &result, &source);
        result
    }};
}

#[test]
fn declaration_kinds_and_contexts_are_separate() {
    let snapshot_inputs = SnapshotInputs::default();
    let bump = Bump::new();
    let result = check(&bump, {
        let body =
            "type Box 'a = Box 'a\ntype option 'a = None | Some 'a\ntype wrap 'f 'a = Wrap ('f 'a)";
        snapshot_inputs.record(&format!(
            "module Main exposing (..)\n\nimport Primitive exposing (..)\nimport Builtin exposing (..)\n\n{body}\n"
        ));
        body
    })
    .unwrap();
    let unions = result.module.unions;

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
    check_snapshot!(
        &bump,
        "type alias Record 'a = ({ field : ('a : Big) } : Big)",
    )
    .unwrap();

    for source in [
        "type alias Record = ({ field : Int } : Term)",
        "type alias record = ({ field : int } : Big)",
    ] {
        check_snapshot!(&bump, source).unwrap_err();
    }
}

#[test]
fn bad_big_field_is_a_representation_error() {
    let snapshot_inputs = SnapshotInputs::default();
    let bump = Bump::new();
    let errors = check(&bump, {
        let body = "type Bad 'a = Bad (list 'a)";
        snapshot_inputs.record(&format!(
            "module Main exposing (..)\n\nimport Primitive exposing (..)\nimport Builtin exposing (..)\n\n{body}\n"
        ));
        body
    })
    .unwrap_err();

    insta::with_settings!({info => &"diagnostic", description => snapshot_inputs.description(), omit_expression => true}, {
        insta::assert_snapshot!(snapshot_inputs.errors(&errors));
    });
}

#[test]
fn annotations_enforce_higher_order_datatype_contexts() {
    let bump = Bump::new();
    check_snapshot!(&bump, "type option 'a = None | Some 'a\ntype wrap 'f 'a = Wrap ('f 'a)\nf : wrap list (option int) -> unit\nf x = ()").unwrap_err();
}

#[test]
fn lowercase_alias_rejects_big_body_after_substitution() {
    let bump = Bump::new();
    check_snapshot!(&bump, "type alias id 'a = 'a\ntype alias bad = id Int").unwrap_err();
}

#[test]
fn recursive_kind_occurs_check() {
    let bump = Bump::new();
    check_snapshot!(&bump, "type self 'f = Self ('f 'f)").unwrap_err();
}

#[test]
fn nested_recursion_can_have_a_finite_context() {
    let bump = Bump::new();
    check_snapshot!(&bump, "type Nest 'a = N (Nest (List 'a))").unwrap();
}

#[test]
fn applied_argument_growth_is_rejected() {
    let bump = Bump::new();
    check_snapshot!(&bump, "type r 'f 'a = R ('f 'a) (r 'f (list 'a))").unwrap_err();
}

#[test]
fn explicit_representation_contradictions_are_rejected() {
    let bump = Bump::new();
    check_snapshot!(&bump, "f : (Big 'a, Little 'a) => 'a -> 'a\nf x = x").unwrap_err();
}

#[test]
fn contradictory_application_annotation_context_is_rejected() {
    let bump = Bump::new();
    check_snapshot!(
        &bump,
        "f : (Big ('f 'a), Little ('f 'a)) => 'f 'a -> 'f 'a\nf x = x"
    )
    .unwrap_err();
}

#[test]
fn contradictory_application_impl_context_is_rejected() {
    let bump = Bump::new();
    check_snapshot!(
        &bump,
        "type box 'f = Box ('f int)\ntrait Keep 'a where\n    keep : 'a -> 'a\nimpl (Big ('f int), Little ('f int)) => Keep (box 'f) where\n    keep x = x"
    )
    .unwrap_err();
}

/// `Foo` promises `Big`, so this context admits no representation. The impl
/// defines no method, so no method annotation repeats the context check.
#[test]
fn impl_using_only_default_methods_rejects_a_superclass_contradiction() {
    let bump = Bump::new();
    check_snapshot!(
        &bump,
        "trait Foo ('a : Big) where\n\ntrait Keep 'a where\n    keep : 'a -> 'a\nimpl Keep ('a : Big) where\n    keep x = x\nimpl Keep ('a : Little) where\n    keep x = x\ntrait Keep 'a => Sub 'a where\n    sub : 'a -> 'a\n    sub x = x\nimpl (Foo 'a, Little 'a) => Sub 'a where"
    )
    .unwrap_err();
}

#[test]
fn marker_impl_rejects_a_superclass_contradiction() {
    let bump = Bump::new();
    check_snapshot!(
        &bump,
        "trait Foo ('a : Big) where\n\ntrait Keep 'a where\n    keep : 'a -> 'a\nimpl Keep ('a : Big) where\n    keep x = x\nimpl Keep ('a : Little) where\n    keep x = x\ntrait Keep 'a => Sub 'a where\n\nimpl (Foo 'a, Little 'a) => Sub 'a where"
    )
    .unwrap_err();
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
fn representation_contexts_separate_disjoint_impl_heads() {
    let bump = Bump::new();
    check_snapshot!(
        &bump,
        "trait T 'a where\nimpl Big 'a => T (list 'a) where\nimpl Const 'a => T (list 'a) where",
    )
    .unwrap();
}

#[test]
fn user_impls_of_representation_traits_are_rejected() {
    let bump = Bump::new();
    check_snapshot!(&bump, "impl Big int where").unwrap_err();
}

#[test]
fn alias_hidden_application_still_marks_recursive_parameters_relevant() {
    let bump = Bump::new();
    check_snapshot!(
        &bump,
        "type alias app 'f 'a = 'f 'a\ntype r 'f 'a = R (app 'f 'a) (r 'f (list 'a))",
    )
    .unwrap_err();
}

#[test]
fn late_applied_relevance_rechecks_existing_recursive_references() {
    let bump = Bump::new();
    check_snapshot!(
        &bump,
        "type r 'f 'a = R (s 'f 'a) (r 'f (list 'a))\ntype s 'g 'b = S ('g 'b) (r 'g 'b)",
    )
    .unwrap_err();
}

fn imported<'a>(bump: &'a Bump, body: &str) -> Result<nash_can::CanResult<'a>, Vec<Error<'a>>> {
    let builtin = nash_can::kinds::builtin_interface(bump);
    let interfaces = BTreeMap::from([("Builtin", builtin)]);
    let source = bump.alloc_str("module Types exposing (..)\nimport Primitive exposing (..)\nimport Builtin exposing (..)\ntype Box 'a = Box 'a\ntype wrap 'f 'a = Wrap ('f 'a)\ntype option 'a = None | Some 'a\ntype alias count = int\n");
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
    let provider: &str = source;
    let source = bump.alloc_str(&format!("module Main exposing (..)\nimport Primitive exposing (..)\nimport Builtin exposing (..)\nimport Types exposing (..)\n{body}\n"));
    let parsed = nash_parse::Parser::new(bump, source).module().unwrap();
    let result = nash_can::canonicalize(
        bump,
        Context {
            package: None,
            interfaces: Some(&interfaces),
        },
        &parsed,
    );
    insta::with_settings!({snapshot_suffix => std::thread::current().name().unwrap().rsplit("::").next().unwrap()}, {
        check_snapshot!(@result source, &result, format!("{provider}\n{source}"));
    });
    result
}

#[test]
fn imported_datatype_context_rejects_a_little_box_argument() {
    let bump = Bump::new();
    imported(&bump, "f : Box int -> unit\nf x = ()").unwrap_err();
}

#[test]
fn imported_apply_context_rejects_a_term_list_element() {
    let bump = Bump::new();
    imported(&bump, "f : wrap list (option int) -> unit\nf x = ()").unwrap_err();
}

#[test]
fn imported_context_and_alias_accept_valid_heads() {
    let bump = Bump::new();
    imported(&bump, "f : Box Int -> wrap list count -> unit\nf x y = ()").unwrap();
}

#[test]
fn specialization_preserves_method_context_on_another_trait_argument() {
    let bump = Bump::new();
    check_snapshot!(
        &bump,
        "trait T 'a where\n    keep : T 'b => 'a -> 'b -> 'b\nimpl T int where\n    keep x y = y",
    )
    .unwrap();
}

#[test]
fn transparent_alias_context_satisfies_representation_superclass() {
    let bump = Bump::new();
    check_snapshot!(&bump, "type alias Alias 'a = 'a\ntrait Big 'a => Keep 'a where\n    keep : 'a -> 'a\nimpl Keep (Alias 'a) where\n    keep x = x").unwrap();
}
