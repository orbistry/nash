mod snapshot_support;
use bumpalo::Bump;
use nash_can::{Context, canonicalize};

fn parse<'a>(bump: &'a Bump, source: &'a str) -> nash_source::Module<'a> {
    nash_parse::Parser::new(bump, source).module().unwrap()
}

#[test]
fn private_values_and_sequential_bindings() {
    let source = "module Main exposing (public)\npublic = ()\nprivate = ()\ntests\n    test \"sequence\" within (cpu 12, mem 4) = do\n        x <- private\n        x\n        ()\n";
    let bump = Bump::new();
    let can = canonicalize(&bump, Context::default(), &parse(&bump, source)).unwrap();
    assert_eq!(can.module.tests.len(), 1);
    assert!(matches!(
        can.module.exports,
        nash_ast::Exports::Explicit([_])
    ));
    insta::with_settings!({description => source, omit_expression => true}, {
        insta::assert_debug_snapshot!(can.module.tests);
    });
}

macro_rules! diagnostic {
    ($name:ident, $source:literal) => {
        #[test]
        fn $name() {
            let bump = Bump::new();
            let source = $source;
            let errors = canonicalize(&bump, Context::default(), &parse(&bump, source)).unwrap_err();
            insta::with_settings!({description => source, omit_expression => true, info => &"diagnostic"}, {
                insta::assert_snapshot!(snapshot_support::errors(source, &errors));
            });
        }
    };
}

diagnostic!(
    duplicate_test_names,
    "module Main exposing (public)\npublic = ()\ntests\n    test \"same\" = do\n        ()\n    test \"same\" = do\n        ()\n"
);
diagnostic!(
    bind_rhs_cannot_see_its_own_name,
    "module Main exposing (public)\npublic = ()\ntests\n    test \"scope\" = do\n        x <- x\n        ()\n"
);
diagnostic!(
    bindings_do_not_leak_to_next_test,
    "module Main exposing (public)\npublic = ()\ntests\n    test \"first\" = do\n        x <- ()\n        x\n    test \"second\" = do\n        x\n"
);
diagnostic!(
    via_names_are_unique,
    "module Main exposing (public)\npublic = ()\ngen = ()\ntests\n    prop \"duplicate\" =\n        let\n            x via gen\n            x via gen\n        in\n        do\n            ()\n"
);
diagnostic!(
    nested_do_stays_monadic,
    "module Main exposing (public)\npublic = ()\ntests\n    test \"nested\" = do\n        x <- do\n            y <- ()\n            y\n        ()\n"
);

#[test]
fn test_imports_are_scoped() {
    let aux = "module Aux exposing (value)\nvalue = ()\n";
    let good = "module Main exposing (public)\npublic = ()\ntests\n    import Aux exposing (value)\n    test \"import\" = do\n        value\n";
    let bad = "module Main exposing (public)\npublic = ()\nleak = value\ntests\n    import Aux exposing (value)\n    test \"import\" = do\n        value\n";
    let bump = Bump::new();
    let aux = canonicalize(&bump, Context::default(), &parse(&bump, aux)).unwrap();
    let unit = bump.alloc(nash_region::Located::at_zero(nash_ast::Type::Named {
        reference: nash_ast::QualifiedName {
            home: nash_ast::primitives::primitive_home(),
            name: "unit",
        },
        args: &[],
    }));
    let annotation = bump.alloc(nash_ast::Annotation {
        free_vars: &[],
        context: &[],
        typ: unit,
    });
    let annotations = std::collections::BTreeMap::from([("value", &*annotation)]);
    let interfaces = std::collections::BTreeMap::from([(
        "Aux",
        nash_can::from_module(&bump, &aux.module, &annotations),
    )]);
    let context = Context {
        package: None,
        interfaces: Some(&interfaces),
    };
    let result = canonicalize(&bump, context, &parse(&bump, good)).unwrap();
    assert!(result.warnings.is_empty());
    let errors = canonicalize(&bump, context, &parse(&bump, bad)).unwrap_err();
    insta::with_settings!({description => format!("{good}\n{bad}"), omit_expression => true, info => &"diagnostic"}, {
        insta::assert_snapshot!(snapshot_support::errors(bad, &errors));
    });
}

#[test]
fn refutable_via_pattern_is_rejected() {
    let source = "module Main exposing (..)\ntype choice = One | Two\ngen = ()\ntests\n    prop \"pattern\" =\n        let One via gen in\n        do\n            ()\n";
    let bump = Bump::new();
    let can = canonicalize(&bump, Context::default(), &parse(&bump, source)).unwrap();
    let errors = nash_nitpick::check(&bump, &can.module).unwrap_err();
    let text = nash_report::Source::new(source);
    let rendered = errors
        .iter()
        .map(|e| nash_report::render_plain(&nash_report::pattern::to_report(e), &text, "Main.nash"))
        .collect::<Vec<_>>()
        .join("\n");
    insta::with_settings!({description => source, omit_expression => true, info => &"diagnostic"}, {
        insta::assert_snapshot!(rendered);
    });
}

#[test]
fn ordinary_let_inside_test_keeps_sequence() {
    let source = "module Main exposing (..)\ntests\n    test \"let\" = do\n        let\n            x = ()\n        x\n        ()\n";
    let bump = Bump::new();
    let can = canonicalize(&bump, Context::default(), &parse(&bump, source)).unwrap();
    insta::with_settings!({description => source, omit_expression => true}, {
        insta::assert_debug_snapshot!(can.module.tests);
    });
}
