mod snapshot_support;
use snapshot_support::SnapshotInputs;

use bumpalo::Bump;
use nash_constrain::{UnionFind, error::Error};
use std::collections::BTreeMap;

fn module_source(body: &str) -> String {
    format!(
        "module Main exposing (..)\nimport Primitive exposing (..)\nimport Builtin exposing (..)\n{body}\n"
    )
}

fn infer<'a>(
    bump: &'a Bump,
    body: &str,
) -> Result<(nash_can::Annotations<'a>, nash_solve::SolvedTypes<'a>), Vec<Error<'a>>> {
    let source = bump.alloc_str(&module_source(body));
    let parsed = nash_parse::Parser::new(bump, source).module().unwrap();
    let interfaces = snapshot_support::literals::literal_interfaces(bump);
    let canonical = nash_can::canonicalize(
        bump,
        nash_can::Context {
            package: None,
            interfaces: Some(&interfaces),
        },
        &parsed,
    )
    .unwrap();
    let mut uf = UnionFind::new();
    let module = &canonical.module;
    nash_solve::run(bump, &mut uf, module, &canonical.tables)
}

macro_rules! infer_snapshot {
    (@result $bump:expr, $source:expr, $result:expr, $description:expr) => {{
        insta::with_settings!({description => $description, omit_expression => true}, {
            match $result {
                Ok((annotations, _)) => insta::assert_debug_snapshot!(annotations),
                Err(errors) => {
                    let parsed = nash_parse::Parser::new($bump, $source).module().unwrap();
                    let localizer = nash_report::localizer::Localizer::from_module(&parsed, &[]);
                    let view = nash_report::Source::new($source);
                    let rendered = errors.iter().map(|error| nash_report::render_plain(
                        &nash_report::type_::to_report(&localizer, error), &view, "Main.nash"
                    )).collect::<Vec<_>>().join("\n");
                    insta::assert_snapshot!(rendered);
                }
            }
        });
    }};
    ($bump:expr, $body:expr $(,)?) => {{
        let body = $body;
        let source = module_source(body);
        let result = infer($bump, body);
        infer_snapshot!(@result $bump, &source, &result, &source);
        result
    }};
}

#[test]
fn explicit_representation_context_rejects_a_known_bad_use() {
    let bump = Bump::new();
    let errors = infer_snapshot!(
        &bump,
        "consume : Big 'a => 'a -> unit\nconsume x = ()\nuse : int -> unit\nuse x = consume x",
    )
    .unwrap_err();
    assert!(errors.iter().any(|error| matches!(error, Error::MissingImpl { trait_, .. } if *trait_ == nash_ast::primitives::ReprTrait::Big.qualified())));
}

#[test]
fn superclass_given_satisfies_representation_requirement() {
    let bump = Bump::new();
    infer_snapshot!(&bump, "consume : Storable 'a => 'a -> unit\nconsume x = ()\nuse : Big 'a => 'a -> unit\nuse x = consume x").unwrap();
}

#[test]
fn retained_apply_context_rejects_a_later_function_element() {
    let bump = Bump::new();
    let errors = infer_snapshot!(
        &bump,
        "type wrap 'f 'a = Wrap ('f 'a)\nwrap xs = Wrap xs\nbad = wrap [\\x -> x]",
    )
    .unwrap_err();
    assert!(errors.iter().any(|error| matches!(error, Error::MissingImpl { trait_, .. } if *trait_ == nash_ast::primitives::ReprTrait::Storable.qualified())));
    assert!(
        errors.iter().any(|error| matches!(error,
            Error::MissingImpl { because, .. } if because.iter().any(|requirement|
                matches!(requirement, nash_constrain::error::Requirement::Application { .. })
            )
        )),
        "{errors:#?}"
    );
}

#[test]
fn directly_inferred_list_rejects_function_elements() {
    let bump = Bump::new();
    let errors = infer_snapshot!(&bump, "bad = [\\x -> x]").unwrap_err();
    assert!(errors.iter().any(|error| matches!(error, Error::MissingImpl { trait_, .. } if *trait_ == nash_ast::primitives::ReprTrait::Storable.qualified())));
    assert!(
        errors.iter().any(|error| matches!(error,
            Error::MissingImpl { because, .. } if matches!(because,
                [nash_constrain::error::Requirement::Formation(_)]
            )
        )),
        "{errors:#?}"
    );
}

#[test]
fn inferred_list_context_is_retained_and_instantiated() {
    let bump = Bump::new();
    let (annotations, solved) =
        infer_snapshot!(&bump, "singleton x = [x]\ngood = singleton Primitive.True").unwrap();
    let context = annotations["singleton"].context;
    assert_eq!(context.len(), 1);
    assert!(context[0].hidden());
    assert_eq!(
        context[0].trait_ref(),
        Some(nash_ast::primitives::ReprTrait::Storable.qualified())
    );
    assert!(annotations["good"].context.is_empty());
    assert!(solved.instances.values().any(|instance| matches!(
        instance.evidence,
        [nash_ast::Evidence::Repr {
            trait_: nash_ast::primitives::ReprTrait::Storable,
            ..
        }]
    )));
    let errors =
        infer_snapshot!(&bump, "singleton x = [x]\nbad = singleton (\\x -> x)").unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, Error::MissingImpl { .. }))
    );
}

#[test]
fn annotation_kinds_are_fixed_before_instantiation() {
    let bump = Bump::new();
    let errors = infer_snapshot!(&bump, "type higher 'f = Higher ('f int)\nidfa : 'f 'a -> 'f 'a\nidfa x = x\nbad = idfa (Higher [])").unwrap_err();
    assert!(errors.iter().any(|e| matches!(e, Error::BadKind { .. })));
}

#[test]
fn an_explicit_higher_kind_is_preserved_at_local_calls() {
    let bump = Bump::new();
    infer_snapshot!(&bump, "type higher 'f = Higher ('f int)\nidh : higher 'f -> higher 'f\nidh x = x\ngood = idh (Higher [])").unwrap();
}

#[test]
fn imported_scheme_defaults_are_fixed_before_instantiation() {
    let inputs = SnapshotInputs::default();
    let bump = Bump::new();
    let mut interfaces = BTreeMap::from([("Builtin", nash_can::kinds::builtin_interface(&bump))]);
    for (name, body) in [
        ("Source", "idfa : 'f 'a -> 'f 'a\nidfa x = x"),
        (
            "Main",
            "import Source exposing (..)\ntype higher 'f = Higher ('f int)\nbad = idfa (Higher [])",
        ),
    ] {
        let source = bump.alloc_str(&format!(
            "module {name} exposing (..)\nimport Primitive exposing (..)\nimport Builtin exposing (..)\n{body}\n"
        ));
        inputs.record(source);
        let parsed = nash_parse::Parser::new(&bump, source).module().unwrap();
        let canonical = nash_can::canonicalize(
            &bump,
            nash_can::Context {
                package: None,
                interfaces: Some(&interfaces),
            },
            &parsed,
        )
        .unwrap();
        let mut uf = UnionFind::new();
        let module = &canonical.module;
        let result = nash_solve::run(&bump, &mut uf, module, &canonical.tables);
        infer_snapshot!(@result &bump, &*source, &result, inputs.description());
        if name == "Source" {
            let (annotations, _) = result.unwrap();
            interfaces.insert(
                name,
                nash_can::from_module(&bump, &canonical.module, &annotations),
            );
        } else {
            assert!(
                result
                    .unwrap_err()
                    .iter()
                    .any(|e| matches!(e, Error::BadKind { .. }))
            );
        }
    }
}

#[test]
fn representation_givens_follow_transparent_alias_bodies() {
    let inputs = SnapshotInputs::default();
    let bump = Bump::new();
    let mut interfaces = BTreeMap::from([("Builtin", nash_can::kinds::builtin_interface(&bump))]);
    for (name, body) in [
        ("Eq", "trait Eq 'a where\n    eq : 'a -> 'a -> bool"),
        (
            "Lift",
            "trait Lift 'a 'b where\n    lift : 'a -> 'b\n    lower : 'b -> 'a",
        ),
        (
            "Main",
            "import Eq exposing (Eq)\nimport Lift exposing (Lift)\ntype alias Alias 'a = 'a\nsame : Alias 'a -> Alias 'a -> bool\nsame x y = eq x y\nroundTrip : Alias 'a -> Alias 'a\nroundTrip x = lift x",
        ),
    ] {
        let source = bump.alloc_str(&format!(
            "module {name} exposing (..)\nimport Primitive exposing (..)\nimport Builtin exposing (..)\n{body}\n"
        ));
        inputs.record(source);
        let parsed = nash_parse::Parser::new(&bump, source).module().unwrap();
        let canonical = nash_can::canonicalize(
            &bump,
            nash_can::Context {
                package: (name != "Main").then_some(nash_ast::primitives::BASE),
                interfaces: Some(&interfaces),
            },
            &parsed,
        )
        .unwrap();
        let mut uf = UnionFind::new();
        let module = &canonical.module;
        let result = nash_solve::run(&bump, &mut uf, module, &canonical.tables);
        if name == "Main" {
            infer_snapshot!(@result &bump, &*source, &result, inputs.description());
        }
        let (annotations, solved) = result.unwrap();
        if name == "Main" {
            assert!(solved.instances.values().any(|instance| matches!(
                instance.evidence,
                [nash_ast::Evidence::StructuralEq { .. }]
            )));
            assert!(solved.instances.values().any(|instance| matches!(
                instance.evidence,
                [nash_ast::Evidence::ReflexiveLift { .. }]
            )));
        }
        interfaces.insert(
            name,
            nash_can::from_module(&bump, &canonical.module, &annotations),
        );
    }
}
