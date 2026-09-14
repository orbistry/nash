//! Official Aiken syntax, lowered into Nash's arena-owned source boundary.
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

fn syntax(input: SourceInput<'_, '_>, spans: &Spans) -> Result<UntypedModule, FrontendFailure> {
    match parser::module(input.source, ModuleKind::Lib) {
        Ok((mut module, _)) => {
            module.name = input.expected_module.as_str().replace('.', "/");
            validate::module(input, &module, spans)?;
            Ok(module)
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
        let ast = syntax(input, &spans)?;
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
            diagnostics: vec![],
        })
    }

    fn parse<'arena>(
        &self,
        arena: &'arena Bump,
        input: SourceInput<'arena, '_>,
    ) -> Result<ParseOutput<'arena>, FrontendFailure> {
        let spans = Spans::new(input.source);
        let ast = syntax(input, &spans)?;
        let module = lower::Lower::new(arena, &spans, &ast).module(input, &ast)?;
        Ok(ParseOutput {
            module,
            diagnostics: vec![],
        })
    }
}
