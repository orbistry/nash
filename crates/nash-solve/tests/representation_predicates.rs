use bumpalo::Bump;
use nash_constrain::{UnionFind, error::Error};
use std::collections::BTreeMap;

fn infer<'a>(
    bump: &'a Bump,
    body: &str,
) -> Result<(nash_can::Annotations<'a>, nash_solve::SolvedTypes<'a>), Vec<Error<'a>>> {
    let source = bump.alloc_str(&format!(
        "module Main exposing (..)\nimport Builtin exposing (..)\n{body}\n"
    ));
    let parsed = nash_parse::Parser::new(bump, source.as_bytes())
        .module()
        .unwrap();
    let interfaces = BTreeMap::from([("Builtin", nash_can::kinds::builtin_interface(bump))]);
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
    let constraint = nash_constrain::constrain(bump, &mut uf, &canonical.module);
    nash_solve::run(bump, &mut uf, &constraint, &canonical.tables)
}

#[test]
fn explicit_representation_context_rejects_a_known_bad_use() {
    let bump = Bump::new();
    let errors = infer(
        &bump,
        "consume : Big 'a => 'a -> unit\nconsume x = ()\nuse : int -> unit\nuse x = consume x",
    )
    .unwrap_err();
    assert!(errors.iter().any(|error| matches!(error, Error::MissingImpl { trait_, .. } if *trait_ == nash_ast::primitives::ReprTrait::Big.qualified())));
}

#[test]
fn superclass_given_satisfies_representation_requirement() {
    let bump = Bump::new();
    infer(&bump, "consume : Storable 'a => 'a -> unit\nconsume x = ()\nuse : Big 'a => 'a -> unit\nuse x = consume x").unwrap();
}

#[test]
fn retained_apply_context_rejects_a_later_function_element() {
    let bump = Bump::new();
    let errors = infer(
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
    let errors = infer(&bump, "bad = [\\x -> x]").unwrap_err();
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
    let (annotations, solved) = infer(&bump, "singleton x = [x]\ngood = singleton True").unwrap();
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
    let errors = infer(&bump, "singleton x = [x]\nbad = singleton (\\x -> x)").unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, Error::MissingImpl { .. }))
    );
}

#[test]
fn a_partial_constructor_cannot_be_a_value_type() {
    let bump = Bump::new();
    let mut uf = UnionFind::new();
    let typ = bump.alloc(nash_constrain::Type::AppN {
        home: nash_ast::primitives::builtin_home(),
        name: "list",
        args: &[],
    });
    let constraint = nash_constrain::Constraint::Equal(
        nash_region::Region::zero(),
        nash_constrain::error::Category::List,
        typ,
        nash_constrain::error::Expected::NoExpectation(typ),
    );
    let result = nash_solve::run(
        &bump,
        &mut uf,
        &constraint,
        &nash_can::environment::Tables::default(),
    );
    assert!(
        matches!(result, Err(errors) if errors.iter().any(|e| matches!(e, Error::BadKind { .. })))
    );
}

#[test]
fn annotation_kinds_are_fixed_before_instantiation() {
    let bump = Bump::new();
    let errors = infer(&bump, "type higher 'f = Higher ('f int)\nidfa : 'f 'a -> 'f 'a\nidfa x = x\nbad = idfa (Higher [])").unwrap_err();
    assert!(errors.iter().any(|e| matches!(e, Error::BadKind { .. })));
}

#[test]
fn an_explicit_higher_kind_is_preserved_at_local_calls() {
    let bump = Bump::new();
    infer(&bump, "type higher 'f = Higher ('f int)\nidh : higher 'f -> higher 'f\nidh x = x\ngood = idh (Higher [])").unwrap();
}

#[test]
fn imported_scheme_defaults_are_fixed_before_instantiation() {
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
            "module {name} exposing (..)\nimport Builtin exposing (..)\n{body}\n"
        ));
        let parsed = nash_parse::Parser::new(&bump, source.as_bytes())
            .module()
            .unwrap();
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
        let constraint = nash_constrain::constrain(&bump, &mut uf, &canonical.module);
        let result = nash_solve::run(&bump, &mut uf, &constraint, &canonical.tables);
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
            "module {name} exposing (..)\nimport Builtin exposing (..)\n{body}\n"
        ));
        let parsed = nash_parse::Parser::new(&bump, source.as_bytes())
            .module()
            .unwrap();
        let canonical = nash_can::canonicalize(
            &bump,
            nash_can::Context {
                package: (name != "Main").then_some(nash_ast::primitives::CORE),
                interfaces: Some(&interfaces),
            },
            &parsed,
        )
        .unwrap();
        let mut uf = UnionFind::new();
        let constraint = nash_constrain::constrain(&bump, &mut uf, &canonical.module);
        let (annotations, solved) =
            nash_solve::run(&bump, &mut uf, &constraint, &canonical.tables).unwrap();
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
