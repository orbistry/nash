//! Elm Reporting/Warning.hs for the warnings currently emitted by Nash.
use crate::{Doc, Report};
use nash_can::{Warning, WarningContext};
pub fn to_report(warning: &Warning<'_>) -> Report {
    let (title, code, region, message, hint) = match warning {
        Warning::UnusedImport {
            region,
            module_name,
        } => (
            "unused import",
            "nash::warning::unused_import",
            *region,
            format!("Unused import `{module_name}`."),
            "Remove the import.".into(),
        ),
        Warning::UnusedVariable {
            region,
            context,
            name,
        } => match context {
            WarningContext::Def => (
                "unused definition",
                "nash::warning::unused_definition",
                *region,
                format!("Unused definition `{name}`."),
                "Remove the definition if it is not needed.".into(),
            ),
            WarningContext::Pattern => (
                "unused variable",
                "nash::warning::unused_variable",
                *region,
                format!("Unused variable `{name}`."),
                format!("Replace `{name}` with `_`."),
            ),
        },
    };
    Report::snippet(title, region, None, Doc::text(message), Doc::text(hint))
        .with_code(code)
        .warning()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Severity, Source, render_plain};

    fn only_warning(input: &str) -> Report {
        let bump = bumpalo::Bump::new();
        let text = bump.alloc_str(input);
        let module = nash_parse::Parser::new(&bump, text)
            .module()
            .expect("parse");
        let can = nash_can::canonicalize(&bump, nash_can::Context::default(), &module)
            .expect("canonicalize");
        assert_eq!(can.warnings.len(), 1);
        let report = to_report(&can.warnings[0]);
        assert_eq!(report.severity, Severity::Warning);
        report
    }
    #[test]
    fn unused_import() {
        let input = "module Main exposing (..)\nimport Tools\nx = 1\n";
        let bump = bumpalo::Bump::new();
        let interfaces = std::collections::BTreeMap::from([(
            "Tools",
            nash_can::Interface {
                home: nash_ast::ModuleName {
                    package: None,
                    name: "Tools",
                },
                impls: &[],
                traits: &[],
                values: &[],
                aliases: &[],
                unions: &[],
                binops: &[],
            },
        )]);
        let text = bump.alloc_str(input);
        let module = nash_parse::Parser::new(&bump, text)
            .module()
            .expect("parse");
        let can = nash_can::canonicalize(
            &bump,
            nash_can::Context {
                package: None,
                interfaces: Some(&interfaces),
            },
            &module,
        )
        .expect("canonicalize");
        assert_eq!(can.warnings.len(), 1);
        let report = to_report(&can.warnings[0]);
        assert_eq!(report.severity, Severity::Warning);
        insta::with_settings!({ description => format!("Code:\n\n{input}"), omit_expression => true }, {
            insta::assert_snapshot!(render_plain(&report, &Source::new(input), "src/Main.nash"));
        });
    }
    #[test]
    fn unused_variable_pattern() {
        let input = "module Main exposing (..)\nf unused = 1\n";
        insta::with_settings!({ description => format!("Code:\n\n{input}"), omit_expression => true }, {
            insta::assert_snapshot!(render_plain(&only_warning(input), &Source::new(input), "src/Main.nash"));
        });
    }
    #[test]
    fn unused_definition() {
        let input = indoc::indoc! {r#"
            module Main exposing (..)
            f =
                let
                    unused = 1
                in
                2
        "#};
        insta::with_settings!({ description => format!("Code:\n\n{input}"), omit_expression => true }, {
            insta::assert_snapshot!(render_plain(&only_warning(input), &Source::new(input), "src/Main.nash"));
        });
    }
}
