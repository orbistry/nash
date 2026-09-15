//! Native Nash syntax adapted to the parser-neutral frontend boundary.

use bumpalo::Bump;
use nash_frontend::{
    Frontend, FrontendDescriptor, FrontendDiagnostic, FrontendFailure, InspectOutput, Label,
    ModuleDependency, ModuleName, ModuleRole, ParseOutput, Severity, SourceInput,
};
use nash_parse::error::Error;
use nash_report::{Doc, Report, Source};
use nash_source::ModuleKind;

pub struct NashFrontend;

static DESCRIPTOR: FrontendDescriptor = FrontendDescriptor {
    id: "nash",
    extensions: &["nash"],
};

impl Frontend for NashFrontend {
    fn descriptor(&self) -> &'static FrontendDescriptor {
        &DESCRIPTOR
    }

    fn inspect(&self, input: SourceInput<'_, '_>) -> Result<InspectOutput, FrontendFailure> {
        let arena = Bump::new();
        let parsed = self.parse(&arena, input)?;
        let dependencies = parsed
            .module
            .imports
            .iter()
            .map(|import| ModuleDependency {
                module: ModuleName::new(import.import.value),
                region: import.import.region,
            })
            .collect();
        Ok(InspectOutput {
            dependencies,
            diagnostics: parsed.diagnostics,
        })
    }

    fn parse<'arena>(
        &self,
        arena: &'arena Bump,
        input: SourceInput<'arena, '_>,
    ) -> Result<ParseOutput<'arena>, FrontendFailure> {
        let source = input.source;
        let module = nash_parse::Parser::new(arena, source)
            .module()
            .map_err(|error| syntax_failure(source, &Error::ParseError(&error)))?;
        let expected = input.expected_module.as_str();
        let name = module
            .name
            .ok_or_else(|| syntax_failure(source, &Error::ModuleNameUnspecified(expected)))?;
        if name.value != expected {
            return Err(syntax_failure(
                source,
                &Error::ModuleNameMismatch {
                    expected,
                    actual: name.value,
                    row: name.region.start.line,
                    col: name.region.start.column,
                },
            ));
        }

        let actual_role = match module.kind {
            ModuleKind::Normal => ModuleRole::Library,
            ModuleKind::Validator(_) => ModuleRole::Validator,
        };
        if let Some(role) = input.role
            && role != actual_role
        {
            let expected = match role {
                ModuleRole::Library => "library",
                ModuleRole::Validator => "validator",
                ModuleRole::Environment => "environment",
                ModuleRole::Configuration => "configuration",
            };
            let actual = match actual_role {
                ModuleRole::Validator => "validator",
                _ => "library",
            };
            let region = match module.kind {
                ModuleKind::Validator(region) => region,
                ModuleKind::Normal => name.region,
            };
            return Err(diagnostic(Report::snippet(
                "MODULE ROLE MISMATCH",
                region,
                None,
                Doc::text(format!(
                    "Project metadata requires a {expected} module, but this header declares a {actual} module."
                )),
                Doc::text("Make the module header and project module role agree."),
            ).with_code("nash::syntax::module_role_mismatch")).into());
        }

        Ok(ParseOutput {
            module: arena.alloc(module),
            diagnostics: Vec::new(),
            entry_points: &[],
            reject_private_types_in_exports: false,
        })
    }
}

fn syntax_failure(source: &str, error: &Error<'_>) -> FrontendFailure {
    diagnostic(nash_report::syntax::to_report(&Source::new(source), error)).into()
}

fn diagnostic(report: Report) -> FrontendDiagnostic {
    let after = report.after.render(80, false);
    FrontendDiagnostic {
        code: report.code,
        severity: match report.severity {
            nash_report::Severity::Error => Severity::Error,
            nash_report::Severity::Warning => Severity::Warning,
        },
        title: report.title,
        message: report.before.render(80, false),
        region: Some(report.region),
        primary_label: report.primary_label,
        labels: report
            .labels
            .into_iter()
            .map(|label| Label {
                region: label.region,
                text: label.text,
            })
            .collect(),
        context: report.context,
        help: if after.is_empty() {
            Vec::new()
        } else {
            vec![after]
        },
        suggestions: report.suggestions,
    }
}
