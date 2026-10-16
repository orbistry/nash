#[path = "snapshot_support/literals.rs"]
mod literals;

use bumpalo::Bump;
use nash_ast::NodeId;
use nash_can::Context;
use nash_constrain::UnionFind;

const PROP: &str = "module Prop exposing (..)\nimport Option\nimport Literal\ntype prng = Seeded\ntype alias generator 'a = prng -> Option.option ('a, prng)\nunit p = Option.Some ((), p)\n";

fn canonicalize<'a>(bump: &'a Bump, source: &'a str) -> nash_can::CanResult<'a> {
    let mut interfaces = literals::literal_interfaces(bump);
    let option = nash_parse::Parser::new(
        bump,
        "module Option exposing (type option(..))\ntype option 'a = Some 'a | None\n",
    )
    .module()
    .unwrap();
    let option = nash_can::canonicalize(
        bump,
        Context {
            package: Some(nash_ast::primitives::BASE),
            interfaces: Some(&interfaces),
        },
        &option,
    )
    .unwrap();
    interfaces.insert(
        "Option",
        nash_can::from_module(bump, &option.module, &Default::default()),
    );
    let generate = nash_parse::Parser::new(bump, PROP).module().unwrap();
    let generate = nash_can::canonicalize(
        bump,
        Context {
            package: Some(nash_ast::primitives::BASE),
            interfaces: Some(&interfaces),
        },
        &generate,
    )
    .unwrap();
    let (annotations, _) = nash_solve::run(
        bump,
        &mut UnionFind::new(),
        &generate.module,
        &generate.tables,
    )
    .unwrap();
    interfaces.insert(
        "Prop",
        nash_can::from_module(bump, &generate.module, &annotations),
    );
    let module = nash_parse::Parser::new(bump, source).module().unwrap();
    nash_can::canonicalize(
        bump,
        Context {
            package: None,
            interfaces: Some(&interfaces),
        },
        &module,
    )
    .unwrap()
}

#[test]
fn test_nodes_retain_solved_types_without_exporting_tests() {
    let source = "module Main exposing (public)\npublic = ()\nprivate = ()\ntests\n    import Prop\n    test \"private\" = do\n        x <- private\n        x\n        ()\n    prop \"generated\" =\n        let x via Prop.unit in\n        do\n            x\n";
    let bump = Bump::new();
    let can = canonicalize(&bump, source);
    let (annotations, types) =
        nash_solve::run(&bump, &mut UnionFind::new(), &can.module, &can.tables).unwrap();
    assert_eq!(
        annotations.keys().copied().collect::<Vec<_>>(),
        ["private", "public"]
    );
    for test in can.module.tests {
        assert!(types.exprs.contains_key(&NodeId::expr(test.body)));
        for binder in test.binders {
            assert!(types.exprs.contains_key(&NodeId::expr(binder.generator)));
            assert!(
                types
                    .patterns
                    .contains_key(&NodeId::pattern(binder.pattern))
            );
        }
    }
    assert_eq!(
        nash_can::from_module(&bump, &can.module, &annotations)
            .values
            .len(),
        1
    );
    insta::with_settings!({description => source, omit_expression => true}, {
        insta::assert_debug_snapshot!(can.module.tests.iter().map(|test| types.exprs[&NodeId::expr(test.body)]).collect::<Vec<_>>());
    });
}

macro_rules! type_error {
    ($name:ident, $source:literal) => {
        #[test]
        fn $name() {
            let source = $source;
            let bump = Bump::new();
            let can = canonicalize(&bump, source);
            let errors = nash_solve::run(&bump, &mut UnionFind::new(), &can.module, &can.tables).unwrap_err();
            let parsed = nash_parse::Parser::new(&bump, source).module().unwrap();
            let localizer = nash_report::localizer::Localizer::from_module(&parsed, &[]);
            let text = nash_report::Source::new(source);
            let errors = errors.iter().map(|e| nash_report::render_plain(&nash_report::type_::to_report(&localizer, e), &text, "Main.nash")).collect::<Vec<_>>().join("\n");
            insta::with_settings!({description => source, omit_expression => true, info => &"diagnostic"}, {
                insta::assert_snapshot!(errors);
            });
        }
    }
}

type_error!(
    final_statement_requires_unit,
    "module Main exposing (public)\npublic = ()\ntests\n    test \"type\" = do\n        \\x -> x\n"
);
type_error!(
    expression_statement_requires_unit,
    "module Main exposing (public)\npublic = ()\ntests\n    test \"type\" = do\n        (\\x -> x)\n        ()\n"
);
type_error!(
    via_requires_generator,
    "module Main exposing (public)\npublic = ()\ntests\n    prop \"type\" =\n        let x via () in\n        do\n            x\n"
);

const FIELD_AUX: &str =
    "module Aux exposing (..)\ntype box = Box { value : () }\nboxed = Box { value = () }\n";
const FIELD_BRIDGE: &str = "module Bridge exposing (boxed)\nimport Aux\nboxed = Aux.boxed\n";

/// Solves `good` and `bad` against Aux and Bridge; `good` must type-check.
/// Returns the rendered errors of `bad`. Both import Aux only in tests.
fn test_field_visibility(good: &str, bad: &str) -> String {
    let bump = Bump::new();
    let mut interfaces = literals::literal_interfaces(&bump);
    for (name, source) in [("Aux", FIELD_AUX), ("Bridge", FIELD_BRIDGE)] {
        let parsed = nash_parse::Parser::new(&bump, source).module().unwrap();
        let can = nash_can::canonicalize(
            &bump,
            Context {
                package: None,
                interfaces: Some(&interfaces),
            },
            &parsed,
        )
        .unwrap();
        let (annotations, _) =
            nash_solve::run(&bump, &mut UnionFind::new(), &can.module, &can.tables).unwrap();
        interfaces.insert(
            name,
            nash_can::from_module(&bump, &can.module, &annotations),
        );
    }
    let solve = |source: &str| -> Result<(), String> {
        let parsed = nash_parse::Parser::new(&bump, source).module().unwrap();
        let can = nash_can::canonicalize(
            &bump,
            Context {
                package: None,
                interfaces: Some(&interfaces),
            },
            &parsed,
        )
        .unwrap();
        let Err(errors) = nash_solve::run(&bump, &mut UnionFind::new(), &can.module, &can.tables)
        else {
            return Ok(());
        };
        let localizer = nash_report::localizer::Localizer::from_module(&parsed, &[]);
        let text = nash_report::Source::new(source);
        Err(errors
            .iter()
            .map(|e| {
                nash_report::render_plain(
                    &nash_report::type_::to_report(&localizer, e),
                    &text,
                    "Main.nash",
                )
            })
            .collect::<Vec<_>>()
            .join("\n"))
    };
    solve(good).unwrap();
    solve(bad).unwrap_err()
}

#[test]
fn test_imports_do_not_grant_field_visibility_to_module_values() {
    let good = "module Main exposing (..)\nimport Bridge\ntests\n    import Aux\n    test \"visible\" = do\n        Bridge.boxed.value\n";
    let bad = "module Main exposing (..)\nimport Bridge\nleak = Bridge.boxed.value\ntests\n    import Aux\n    test \"visible\" = do\n        Bridge.boxed.value\n";
    let rendered = test_field_visibility(good, bad);
    insta::with_settings!({description => format!("{FIELD_AUX}\n{FIELD_BRIDGE}\n{good}\n{bad}"), omit_expression => true, info => &"diagnostic"}, {
        insta::assert_snapshot!(rendered);
    });
}

/// The accessor, update, and record pattern forms see the same test-only
/// fields as `.value` access, and stay hidden from module values.
#[test]
fn test_imports_grant_field_visibility_to_every_field_form_in_tests() {
    let good = "module Main exposing (..)\nimport Bridge\ntests\n    import Aux\n    test \"accessor\" = do\n        .value Bridge.boxed\n    test \"update\" = do\n        let b = Bridge.boxed in let _ = { b | value = () } in ()\n    test \"pattern\" = do\n        let { value } = Bridge.boxed in value\n";
    let bad = "module Main exposing (..)\nimport Bridge\nleakAccessor = .value Bridge.boxed\nleakUpdate = let b = Bridge.boxed in { b | value = () }\nleakPattern = let { value } = Bridge.boxed in value\ntests\n    import Aux\n    test \"visible\" = do\n        Bridge.boxed.value\n";
    let rendered = test_field_visibility(good, bad);
    insta::with_settings!({description => format!("{FIELD_AUX}\n{FIELD_BRIDGE}\n{good}\n{bad}"), omit_expression => true, info => &"diagnostic"}, {
        insta::assert_snapshot!(rendered);
    });
}
