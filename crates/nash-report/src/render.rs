//! `Report` -> `miette::Diagnostic`.

use std::fmt;

use miette::{
    Diagnostic, GraphicalReportHandler, GraphicalTheme, LabeledSpan, MietteHandler,
    MietteHandlerOpts, NamedSource, SourceCode,
};

use crate::code::Source;
use crate::{Report, Severity};

pub const WIDTH: usize = 80;

/// A `Report` bound to its file, owning everything miette needs.
#[derive(Debug)]
pub struct Rendered {
    severity: Severity,
    message: String,
    help: Option<String>,
    labels: Vec<LabeledSpan>,
    source: RenderSource,
    related: Vec<Rendered>,
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
        let labels: Vec<_> = self
            .primary_label
            .iter()
            .map(|text| {
                LabeledSpan::new_primary_with_span(
                    (!text.is_empty()).then(|| text.clone()),
                    span(self.region),
                )
            })
            .chain(self.labels.iter().map(|label| {
                LabeledSpan::new_with_span(Some(label.text.clone()), span(label.region))
            }))
            .collect();
        let mut display_source = source.text().to_string();
        if labels
            .iter()
            .any(|label| label.offset() + label.len() > display_source.len())
        {
            display_source.push('\n');
        }
        let after = self.after.render(WIDTH, color);
        Rendered {
            severity: self.severity,
            message: self.before.render(WIDTH, color),
            help: (!after.is_empty()).then_some(after),
            labels,
            related: self
                .related
                .iter()
                .flat_map(|module| module.render(color))
                .collect(),
            source: RenderSource {
                source: NamedSource::new(path, display_source),
                surroundings: self.context.map(span),
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

    fn related(&self) -> Option<Box<dyn Iterator<Item = &dyn Diagnostic> + '_>> {
        (!self.related.is_empty()).then(|| {
            Box::new(self.related.iter().map(|report| report as &dyn Diagnostic))
                as Box<dyn Iterator<Item = &dyn Diagnostic>>
        })
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
    fn region(a: usize, b: usize) -> Region {
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
    fn arbitrary_labels_and_related_sources_are_rendered() {
        let report = snippet()
            .with_label(Label {
                region: region(1, 2),
                text: "first requirement".into(),
            })
            .with_label(Label {
                region: region(9, 10),
                text: "second requirement".into(),
            })
            .with_related(crate::ModuleReports {
                name: "Other".into(),
                path: "Other.nash".into(),
                source: "x = y".into(),
                reports: vec![Report::snippet(
                    "RELATED",
                    region(5, 6),
                    None,
                    Doc::text("Declared here."),
                    Doc::Empty,
                )],
            });
        let output = plain(&report);
        for expected in [
            "first requirement",
            "second requirement",
            "Other.nash",
            "Declared here.",
            "[Main.nash:1:5]",
        ] {
            assert!(output.contains(expected), "{output}");
        }
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
    fn terminal_omits_identifiers_for_errors_and_warnings() {
        for report in [snippet(), snippet().warning()] {
            let rendered = report.render(&Source::new("f = x + 1"), "Main.nash", false);
            assert!(rendered.code().is_none());
            let output = plain(&report);
            assert!(!output.contains(&report.title));
            assert!(!output.contains(report.code));
            assert!(output.contains("Before:"));
        }
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
        none = none.without_source();
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
        assert_eq!(report.context, Some(region(1, 10)));
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

#[cfg(test)]
mod source_edge_tests {
    use super::*;
    use crate::{Doc, Label};
    use nash_region::{Position, Region};
    #[test]
    fn unicode_crlf_tabs_and_final_empty_line_render_without_losing_labels() {
        let source = Source::new("a😀é\r\n\tx\r\n");
        let region = |r, c, e| Region::new(Position::new(r, c), Position::new(r, e));
        let report = Report::snippet(
            "SOURCE EDGES",
            region(3, 1, 1),
            None,
            Doc::text("Missing value."),
            Doc::Empty,
        )
        .with_code("nash::test::source_edges")
        .with_label(Label {
            region: region(1, 2, 6),
            text: "Unicode origin".into(),
        })
        .with_label(Label {
            region: region(2, 2, 3),
            text: "tabbed origin".into(),
        });
        let rendered = report.render(&source, "Main.nash", false);
        let labels = rendered.labels().unwrap().collect::<Vec<_>>();
        assert_eq!((labels[1].offset(), labels[1].len()), (1, 4));
        assert_eq!((labels[2].offset(), labels[2].len()), (10, 1));
        let output = render_plain(&report, &source, "Main.nash");
        for expected in ["Unicode origin", "tabbed origin", "Main.nash:3:1"] {
            assert!(output.contains(expected), "{output}");
        }
        insta::assert_snapshot!(output);
    }
}
