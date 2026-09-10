//! Convert compiler reports to LSP without changing their primary spans.
use nash_region::{Position as NashPosition, Region};
use nash_report::{Report, Severity, Source};
use tower_lsp_server::ls_types::{
    Diagnostic, DiagnosticRelatedInformation, DiagnosticSeverity, Location, NumberOrString,
    Position, Range, Uri,
};

pub fn to_lsp(report: &Report, source: &Source<'_>, uri: &Uri) -> Diagnostic {
    checked_to_lsp(report, source, uri).unwrap_or_else(|| Diagnostic {
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("nash".into()),
        message: "Cannot represent this diagnostic's source position in LSP: line or UTF-16 column exceeds the protocol's 32-bit limit.".into(),
        ..Diagnostic::default()
    })
}

fn checked_to_lsp(report: &Report, source: &Source<'_>, uri: &Uri) -> Option<Diagnostic> {
    let mut related = Vec::new();
    for label in &report.labels {
        related.push(DiagnosticRelatedInformation {
            location: Location {
                uri: uri.clone(),
                range: to_range(label.region, source)?,
            },
            message: label.text.clone(),
        });
    }
    related_reports(&report.related, uri, &mut related)?;
    let related_information = (!related.is_empty()).then_some(related);
    Some(Diagnostic {
        range: to_range(report.region, source)?,
        severity: Some(match report.severity {
            Severity::Error => DiagnosticSeverity::ERROR,
            Severity::Warning => DiagnosticSeverity::WARNING,
        }),
        code: Some(NumberOrString::String(report.code.to_string())),
        source: Some("nash".into()),
        message: report.message(),
        related_information,
        data: (!report.suggestions.is_empty()).then(|| serde_json::json!(report.suggestions)),
        ..Diagnostic::default()
    })
}

fn related_reports(
    modules: &[nash_report::ModuleReports],
    base_uri: &Uri,
    output: &mut Vec<DiagnosticRelatedInformation>,
) -> Option<()> {
    for module in modules {
        let path = std::path::Path::new(&module.path);
        let path = if path.is_absolute() {
            path.to_owned()
        } else {
            let base = url::Url::parse(base_uri.as_str())
                .ok()?
                .to_file_path()
                .ok()?;
            base.parent()?.join(path)
        };
        let uri: Uri = url::Url::from_file_path(path).ok()?.as_str().parse().ok()?;
        let source = Source::new(&module.source);
        for report in &module.reports {
            output.push(DiagnosticRelatedInformation {
                location: Location {
                    uri: uri.clone(),
                    range: to_range(report.region, &source)?,
                },
                message: report.message(),
            });
            for label in &report.labels {
                output.push(DiagnosticRelatedInformation {
                    location: Location {
                        uri: uri.clone(),
                        range: to_range(label.region, &source)?,
                    },
                    message: label.text.clone(),
                });
            }
            related_reports(&report.related, &uri, output)?;
        }
    }
    Some(())
}

pub fn to_range(region: Region, source: &Source<'_>) -> Option<Range> {
    Some(Range::new(
        to_position(region.start, source)?,
        to_position(region.end, source)?,
    ))
}

fn to_position(position: NashPosition, source: &Source<'_>) -> Option<Position> {
    let offset = source.offset(position);
    let prefix = &source.text()[..offset];
    let line = prefix.bytes().filter(|&b| b == b'\n').count();
    let character = prefix
        .rsplit('\n')
        .next()
        .unwrap_or("")
        .encode_utf16()
        .count();
    protocol_position(line, character)
}

fn protocol_position(line: usize, character: usize) -> Option<Position> {
    Some(Position::new(
        line.try_into().ok()?,
        character.try_into().ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nash_report::{Doc, Label};
    fn region(sr: usize, sc: usize, er: usize, ec: usize) -> Region {
        Region::new(NashPosition::new(sr, sc), NashPosition::new(er, ec))
    }
    #[test]
    fn protocol_positions_do_not_truncate() {
        let max = u32::MAX as usize;
        assert_eq!(
            protocol_position(max, max),
            Some(Position::new(u32::MAX, u32::MAX))
        );
        if let Some(too_large) = max.checked_add(1) {
            assert_eq!(protocol_position(too_large, 0), None);
            assert_eq!(protocol_position(0, too_large), None);
        }
    }
    #[test]
    fn ranges_count_utf16_surrogate_pairs() {
        let source = Source::new("a😀éz\nlast");
        assert_eq!(
            to_range(region(1, 6, 1, 8), &source).unwrap(),
            Range::new(Position::new(0, 3), Position::new(0, 4))
        );
        assert_eq!(
            to_range(region(2, 5, 2, 5), &source).unwrap(),
            Range::new(Position::new(1, 4), Position::new(1, 4))
        );
    }
    #[test]
    fn pair_preserves_primary_and_related_range() {
        let uri: Uri = "file:///test/Main.nash".parse().unwrap();
        let report = Report::pair(
            "NAME CLASH",
            Label {
                region: region(1, 1, 1, 2),
                text: "first name".into(),
            },
            Label {
                region: region(2, 1, 2, 2),
                text: "second name".into(),
            },
            Doc::text("Duplicate names:"),
            Doc::text("Rename one."),
        );
        let source = Source::new("x\nx");
        let diagnostic = to_lsp(&report, &source, &uri);
        assert_eq!(diagnostic.range, to_range(report.region, &source).unwrap());
        assert_eq!(
            diagnostic.related_information.unwrap()[0].location.range,
            to_range(region(1, 1, 1, 2), &source).unwrap()
        );
        assert_eq!(
            diagnostic.message,
            "Duplicate names:\n\nsecond name\n\nRename one."
        );
    }
    #[test]
    fn highlighted_region_and_suggestions_survive() {
        let source = Source::new("a = unknown");
        let mut report = Report::snippet(
            "NAME",
            region(1, 1, 1, 2),
            Some(region(1, 5, 1, 12)),
            Doc::text("Name:"),
            Doc::Empty,
        )
        .with_suggestions(vec!["known".into()])
        .warning();
        let uri = "file:///test/Main.nash".parse().unwrap();
        let diagnostic = to_lsp(&report, &source, &uri);
        assert_eq!(diagnostic.severity, Some(DiagnosticSeverity::WARNING));
        assert_eq!(diagnostic.data, Some(serde_json::json!(["known"])));
        assert_eq!(
            diagnostic.range,
            to_range(region(1, 5, 1, 12), &source).unwrap()
        );
        report = report.without_source();
        assert!(to_lsp(&report, &source, &uri).related_information.is_none());
    }
}

#[cfg(test)]
mod structured_tests {
    use super::*;
    use nash_report::{Doc, Label, ModuleReports};
    fn region(column: usize) -> Region {
        Region::new(
            NashPosition::new(1, column),
            NashPosition::new(1, column + 1),
        )
    }
    #[test]
    fn labels_related_files_and_codes_survive_conversion() {
        let mut report = Report::snippet(
            "OLD TITLE",
            region(5),
            None,
            Doc::text("Type mismatch."),
            Doc::Empty,
        )
        .with_code("nash::type::mismatch")
        .with_label(Label {
            region: region(1),
            text: "first requirement".into(),
        })
        .with_label(Label {
            region: region(3),
            text: "second requirement".into(),
        });
        report.title = "A new display title".into();
        report.primary_label = Some("argument 2 of `f`".into());
        let mut origin = Report::snippet(
            "ORIGIN",
            region(5),
            None,
            Doc::text("Declared here."),
            Doc::Empty,
        );
        origin.primary_label = Some("annotation for `f`".into());
        let report = report.with_related(ModuleReports {
            name: "Other".into(),
            path: "Other #.nash".into(),
            source: "a b c".into(),
            reports: vec![origin],
        });
        let uri: Uri = "file:///project/Main.nash".parse().unwrap();
        let diagnostic = to_lsp(&report, &Source::new("a b c"), &uri);
        assert_eq!(
            diagnostic.code,
            Some(NumberOrString::String("nash::type::mismatch".into()))
        );
        assert!(
            diagnostic.message.contains("argument 2 of `f`"),
            "{diagnostic:?}"
        );
        let related = diagnostic.related_information.unwrap();
        assert_eq!(related.len(), 3);
        assert_eq!(
            related[2].location.uri.as_str(),
            "file:///project/Other%20%23.nash"
        );
        assert!(related[2].message.contains("annotation for `f`"));
    }
}
