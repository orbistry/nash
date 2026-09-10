//! Elm Reporting/Warning.hs for the warnings currently emitted by Nash.
use crate::{Doc, Report};
use nash_can::{Warning, WarningContext};
pub fn to_report(warning: &Warning<'_>) -> Report {
    match warning {
        Warning::UnusedImport {
            region,
            module_name,
        } => Report::snippet(
            "unused import",
            *region,
            None,
            Doc::reflow(&format!(
                "Nothing from the `{module_name}` module is used in this file."
            )),
            Doc::text("I recommend removing unused imports."),
        )
        .warning(),
        Warning::UnusedVariable {
            region,
            context,
            name,
        } => {
            let (title, advice) = match context {
                WarningContext::Def => ("unused definition", "If you are sure there is no typo, remove the definition. This way future readers will not have to wonder why it is there!".to_string()),
                WarningContext::Pattern => ("unused variable", format!("If you are sure there is no typo, replace `{name}` with _ so future readers will not have to wonder why it is there!")),
            };
            Report::snippet(
                title, *region, None,
                Doc::reflow(&format!("You are not using `{name}` anywhere.")),
                Doc::stack([
                    Doc::reflow(&format!("Is there a typo? Maybe you intended to use `{name}` somewhere but typed another name instead?")),
                    Doc::reflow(&advice),
                ]),
            ).warning()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Severity, Source, render_plain};

    fn only_warning(input: &str) -> Report {
        let bump = bumpalo::Bump::new();
        let text = bump.alloc_str(input);
        let module = nash_parse::Parser::new(&bump, text.as_bytes())
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
        let module = nash_parse::Parser::new(&bump, text.as_bytes())
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
        insta::assert_snapshot!(render_plain(&report, &Source::new(input), "src/Main.nash"));
    }
    #[test]
    fn unused_variable_pattern() {
        let input = "module Main exposing (..)\nf unused = 1\n";
        insta::assert_snapshot!(render_plain(
            &only_warning(input),
            &Source::new(input),
            "src/Main.nash"
        ));
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
        insta::assert_snapshot!(render_plain(
            &only_warning(input),
            &Source::new(input),
            "src/Main.nash"
        ));
    }
}
