//! Official Aiken syntax, lowered into Nash's arena-owned source boundary.
mod docs;
mod lower;
mod profile;
mod spans;
mod validate;

use aiken_lang::{
    ast::{Definition, ModuleKind, Span, UntypedModule},
    parser,
};
use bumpalo::Bump;
use miette::Diagnostic;
use nash_frontend::{
    Frontend, FrontendDescriptor, FrontendDiagnostic, FrontendFailure, InspectOutput,
    ModuleDependency, ModuleName, ParseOutput, SourceInput,
};
use spans::Spans;

pub struct AikenFrontend;

static DESCRIPTOR: FrontendDescriptor = FrontendDescriptor {
    id: "aiken",
    extensions: &["ak"],
};

fn syntax(
    input: SourceInput<'_, '_>,
    spans: &Spans,
) -> Result<(UntypedModule, Vec<FrontendDiagnostic>), FrontendFailure> {
    let kind = match input.role {
        Some(nash_frontend::ModuleRole::Validator) => ModuleKind::Validator,
        Some(nash_frontend::ModuleRole::Environment) => ModuleKind::Env,
        Some(nash_frontend::ModuleRole::Configuration) => ModuleKind::Config,
        Some(nash_frontend::ModuleRole::Library) | None => ModuleKind::Lib,
    };
    match parser::module(input.source, kind) {
        Ok((mut module, extra)) => {
            module.name = input.expected_module.as_str().replace('.', "/");
            if input.role.is_none()
                && module
                    .definitions
                    .iter()
                    .any(|def| matches!(def, Definition::Validator(_)))
            {
                module.kind = ModuleKind::Validator;
            }
            docs::attach(&mut module, &extra, input.source);
            let mut diagnostics = Vec::new();
            let dependency = input.origin == nash_frontend::SourceOrigin::Dependency;
            module.definitions.retain(|def| {
                if dependency
                    && matches!(
                        def,
                        Definition::Test(_) | Definition::Benchmark(_) | Definition::Validator(_)
                    )
                {
                    return false;
                }
                if !module.kind.is_validator() && matches!(def, Definition::Validator(_)) {
                    let mut warning = FrontendDiagnostic::error(
                        "NAF2501",
                        "Validator outside validator source root",
                        "This validator declaration is ignored in a library or environment module.",
                        Some(spans.region(def.location())),
                    );
                    warning.severity = nash_frontend::Severity::Warning;
                    diagnostics.push(warning);
                    return false;
                }
                true
            });
            validate::module(input, &module, spans)?;
            Ok((module, diagnostics))
        }
        Err(errors) => {
            let mut diagnostics = errors.into_iter().map(|error| {
                let labels = error
                    .labels()
                    .into_iter()
                    .flatten()
                    .map(|label| {
                        (
                            spans.region(Span {
                                start: label.offset(),
                                end: label.offset().saturating_add(label.len()),
                            }),
                            label.label().unwrap_or("").to_owned(),
                        )
                    })
                    .collect::<Vec<_>>();
                let mut diagnostic = FrontendDiagnostic::error(
                    "NAF2001",
                    "Aiken syntax error",
                    error.to_string(),
                    labels.first().map(|(region, _)| *region),
                );
                diagnostic.primary_label = labels.first().map(|(_, text)| text.clone());
                diagnostic.labels = labels
                    .into_iter()
                    .skip(1)
                    .map(|(region, text)| nash_frontend::Label { region, text })
                    .collect();
                if let Some(help) = error.help() {
                    diagnostic.help.push(help.to_string());
                }
                diagnostic
            });
            let first = diagnostics.next().unwrap_or_else(|| {
                FrontendDiagnostic::error(
                    "NAF2001",
                    "Aiken syntax error",
                    "The official parser rejected this module.",
                    None,
                )
            });
            let mut failure = FrontendFailure::from(first);
            failure.rest.extend(diagnostics);
            Err(failure)
        }
    }
}

impl Frontend for AikenFrontend {
    fn descriptor(&self) -> &'static FrontendDescriptor {
        &DESCRIPTOR
    }

    fn inspect(&self, input: SourceInput<'_, '_>) -> Result<InspectOutput, FrontendFailure> {
        let spans = Spans::new(input.source);
        let (ast, diagnostics) = syntax(input, &spans)?;
        let dependencies = ast
            .definitions
            .iter()
            .filter_map(|def| match def {
                Definition::Use(import) => Some(ModuleDependency {
                    module: ModuleName::new(profile::module_name(&import.module)),
                    region: spans.region(import.location),
                }),
                _ => None,
            })
            .collect();
        Ok(InspectOutput {
            dependencies,
            diagnostics,
        })
    }

    fn parse<'arena>(
        &self,
        arena: &'arena Bump,
        input: SourceInput<'arena, '_>,
    ) -> Result<ParseOutput<'arena>, FrontendFailure> {
        let spans = Spans::new(input.source);
        let (ast, diagnostics) = syntax(input, &spans)?;
        let mut lower = lower::Lower::new(arena, &spans, &ast);
        let module = lower.module(input, &ast)?;
        Ok(ParseOutput {
            module,
            diagnostics,
            entry_points: arena.alloc_slice_copy(&lower.entry_points),
            reject_private_types_in_exports: true,
        })
    }
}
