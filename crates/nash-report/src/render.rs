//! `Report` -> `miette::Diagnostic`.

use std::fmt;

use miette::{
    Diagnostic, GraphicalReportHandler, GraphicalTheme, LabeledSpan, MietteHandler,
    MietteHandlerOpts, NamedSource, SourceCode,
};

use crate::code::Source;
use crate::{Report, Severity, Snippet};

pub const WIDTH: usize = 80;

/// A `Report` bound to its file, owning everything miette needs.
#[derive(Debug)]
pub struct Rendered {
    title: String,
    severity: Severity,
    message: String,
    help: Option<String>,
    labels: Vec<LabeledSpan>,
    source: RenderSource,
}

/// Expand miette's source read to the report's requested surrounding region.
/// Labels retain the narrow primary span; the wider source is context only.
#[derive(Debug)]
struct RenderSource {
    source: NamedSource<String>,
    surroundings: Option<miette::SourceSpan>,
}

impl SourceCode for RenderSource {
    fn read_span<'a>(
        &'a self,
        span: &miette::SourceSpan,
        before: usize,
        after: usize,
    ) -> Result<Box<dyn miette::SpanContents<'a> + 'a>, miette::MietteError> {
        // miette reads a label without context to locate the header. Expanding
        // that read would move the displayed location away from the problem.
        if before == 0 && after == 0 {
            return self.source.read_span(span, before, after);
        }
        let expanded = self.surroundings.map_or(*span, |context| {
            let start = span.offset().min(context.offset());
            let end = (span.offset() + span.len()).max(context.offset() + context.len());
            (start, end - start).into()
        });
        self.source.read_span(&expanded, before, after)
    }
}

impl Report {
    pub fn render(&self, source: &Source<'_>, path: &str, color: bool) -> Rendered {
        // miette needs a display cell at EOF to draw an insertion caret. This
        // padding is terminal-only: source positions and JSON/LSP stay unchanged.
        let span = |region| {
            let raw = source.span(region);
            if raw.is_empty() {
                (raw.offset(), 1).into()
            } else {
                raw
            }
        };
        let labels = match &self.snippet {
            Snippet::Region { region, highlight } => vec![LabeledSpan::new_with_span(
                None,
                span(highlight.unwrap_or(*region)),
            )],
            Snippet::Pair { first, second } => vec![
                LabeledSpan::new_with_span(Some(first.text.clone()), span(first.region)),
                LabeledSpan::new_primary_with_span(Some(second.text.clone()), span(second.region)),
            ],
            Snippet::None => Vec::new(),
        };
        let mut display_source = source.text().to_string();
        if labels
            .iter()
            .any(|label| label.offset() + label.len() > display_source.len())
        {
            display_source.push('\n');
        }
        let after = self.after.render(WIDTH, color);
        Rendered {
            title: self.title.clone(),
            severity: self.severity,
            message: self.before.render(WIDTH, color),
            help: (!after.is_empty()).then_some(after),
            labels,
            source: RenderSource {
                source: NamedSource::new(path, display_source),
                surroundings: match self.snippet {
                    Snippet::Region { region, .. } => Some(span(region)),
                    _ => None,
                },
            },
        }
    }
}

impl fmt::Display for Rendered {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Rendered {}

impl Diagnostic for Rendered {
    fn code(&self) -> Option<Box<dyn fmt::Display + '_>> {
        Some(Box::new(&self.title))
    }

    fn severity(&self) -> Option<miette::Severity> {
        Some(match self.severity {
            Severity::Error => miette::Severity::Error,
            Severity::Warning => miette::Severity::Warning,
        })
    }

    fn help(&self) -> Option<Box<dyn fmt::Display + '_>> {
        self.help
            .as_ref()
            .map(|help| Box::new(help) as Box<dyn fmt::Display>)
    }

    fn source_code(&self) -> Option<&dyn SourceCode> {
        Some(&self.source)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        Some(Box::new(self.labels.iter().cloned()))
    }
}

/// The handler the CLI installs through `miette::set_hook`: fixed width,
/// no re-wrapping (Doc already wrapped), color decided by the CLI.
pub fn handler(color: bool) -> MietteHandler {
    MietteHandlerOpts::new()
        .width(WIDTH)
        .force_graphical(true)
        .wrap_lines(false)
        .color(color)
        .unicode(true)
        .build()
}

/// Plain-text rendering for snapshot tests: the same options
/// `MietteHandlerOpts::build` applies, on a handler that can write to a
/// `String`.
pub fn render_plain(report: &Report, source: &Source<'_>, path: &str) -> String {
    let rendered = report.render(source, path, false);
    let mut out = String::new();
    GraphicalReportHandler::new_themed(GraphicalTheme::unicode_nocolor())
        .with_width(WIDTH)
        .with_wrap_lines(false)
        .render_report(&mut out, &rendered)
        .expect("render");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Doc, Label};
    use nash_region::{Position, Region};
    fn region(a: u16, b: u16) -> Region {
        Region::new(Position::new(1, a), Position::new(1, b))
    }
    fn snippet() -> Report {
        Report::snippet(
            "TEST",
            region(5, 6),
            None,
            Doc::text("Before:"),
            Doc::text("After."),
        )
    }
    fn plain(report: &Report) -> String {
        render_plain(report, &Source::new("f = x + 1"), "Main.nash")
    }
    #[test]
    fn render_snippet_report() {
        insta::assert_snapshot!(plain(&snippet()));
    }
    #[test]
    fn render_warning_header() {
        insta::assert_snapshot!(plain(&snippet().warning()));
    }
    #[test]
    fn render_pair_report() {
        let pair = Report::pair(
            "TEST",
            Label {
                region: region(1, 2),
                text: "first".into(),
            },
            Label {
                region: region(5, 6),
                text: "second".into(),
            },
            Doc::text("Before:"),
            Doc::text("After."),
        );
        let output = plain(&pair);
        assert!(output.contains("[Main.nash:1:5]"));
        insta::assert_snapshot!(output);
    }
    #[test]
    fn render_no_snippet_report() {
        let mut none = snippet();
        none.snippet = Snippet::None;
        insta::assert_snapshot!(plain(&none));
    }
    #[test]
    fn render_zero_width_region_gets_caret() {
        let zero = Report::snippet(
            "TEST",
            region(5, 5),
            None,
            Doc::text("Before:"),
            Doc::text("After."),
        );
        insta::assert_snapshot!(plain(&zero));
    }
    #[test]
    fn render_eof_insertion() {
        let eof = Report::snippet(
            "MISSING EXPRESSION",
            region(4, 4),
            None,
            Doc::text("I need an expression here:"),
            Doc::text("Add an expression after the equals sign."),
        );
        insta::assert_snapshot!(render_plain(&eof, &Source::new("f ="), "Main.nash"));
    }
    #[test]
    fn surrounding_region_keeps_primary_highlight() {
        let report = snippet().with_region(region(1, 10));
        assert_eq!(report.region, region(5, 6));
        assert!(plain(&report).contains("[Main.nash:1:5]"));
        assert!(
            matches!(report.snippet, Snippet::Region { region: wide, highlight: Some(narrow) } if wide == region(1,10) && narrow == region(5,6))
        );
        let source = Source::new("f =\n    case x of\n        _ -> ()\n        () -> ()");
        let narrow = Region::new(Position::new(4, 9), Position::new(4, 11));
        let wide = Region::new(Position::new(2, 5), Position::new(4, 17));
        let report = Report::snippet("TEST", narrow, None, Doc::text("Before:"), Doc::Empty)
            .with_region(wide);
        let output = render_plain(&report, &source, "Main.nash");
        assert!(output.contains("[Main.nash:4:9]"), "{output}");
        assert!(output.contains("case x of"), "{output}");
    }
}
