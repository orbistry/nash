use bumpalo::Bump;
use nash_ast::NodeId;
use nash_can::Context;
use nash_constrain::UnionFind;

const PROP: &str =
    "module Prop exposing (..)\ntype generator 'a = Generator 'a\nunit = Generator ()\n";

fn canonicalize<'a>(bump: &'a Bump, source: &'a str) -> nash_can::CanResult<'a> {
    let generate = nash_parse::Parser::new(bump, PROP).module().unwrap();
    let generate = nash_can::canonicalize(
        bump,
        Context {
            package: Some(nash_ast::primitives::BASE),
            interfaces: None,
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
    let interfaces = std::collections::BTreeMap::from([(
        "Prop",
        nash_can::from_module(bump, &generate.module, &annotations),
    )]);
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
    insta::with_settings!({description => format!("{PROP}\n{source}"), omit_expression => true}, {
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
            insta::with_settings!({description => format!("{PROP}\n{source}"), omit_expression => true, info => &"diagnostic"}, {
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

#[test]
fn test_imports_do_not_grant_field_visibility_to_module_values() {
    let aux =
        "module Aux exposing (..)\ntype box = Box { value : () }\nboxed = Box { value = () }\n";
    let bridge = "module Bridge exposing (boxed)\nimport Aux\nboxed = Aux.boxed\n";
    let good = "module Main exposing (..)\nimport Bridge\ntests\n    import Aux\n    test \"visible\" = do\n        Bridge.boxed.value\n";
    let bad = "module Main exposing (..)\nimport Bridge\nleak = Bridge.boxed.value\ntests\n    import Aux\n    test \"visible\" = do\n        Bridge.boxed.value\n";
    let bump = Bump::new();
    let mut interfaces = std::collections::BTreeMap::new();
    for (name, source) in [("Aux", aux), ("Bridge", bridge)] {
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
    for source in [good, bad] {
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
        let result = nash_solve::run(&bump, &mut UnionFind::new(), &can.module, &can.tables);
        if source == good {
            result.unwrap();
        } else {
            let errors = result.unwrap_err();
            let localizer = nash_report::localizer::Localizer::from_module(&parsed, &[]);
            let text = nash_report::Source::new(source);
            let rendered = errors
                .iter()
                .map(|e| {
                    nash_report::render_plain(
                        &nash_report::type_::to_report(&localizer, e),
                        &text,
                        "Main.nash",
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            insta::with_settings!({description => format!("{aux}\n{bridge}\n{good}\n{bad}"), omit_expression => true, info => &"diagnostic"}, {
                insta::assert_snapshot!(rendered);
            });
        }
    }
}
