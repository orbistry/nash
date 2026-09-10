//! Convert compiler reports to LSP without changing their primary spans.
use nash_region::{Position as NashPosition, Region};
use nash_report::{Report, Severity, Snippet, Source};
use tower_lsp_server::ls_types::{
    Diagnostic, DiagnosticRelatedInformation, DiagnosticSeverity, Location, NumberOrString,
    Position, Range, Uri,
};

pub fn to_lsp(report: &Report, source: &Source<'_>, uri: &Uri) -> Diagnostic {
    let related = match &report.snippet {
        Snippet::Pair { first, .. } => Some((first.region, first.text.clone())),
        Snippet::Region {
            highlight: Some(highlight),
            ..
        } if *highlight != report.region => Some((*highlight, "Related source".into())),
        _ => None,
    };
    Diagnostic {
        range: to_range(report.region, source),
        severity: Some(match report.severity {
            Severity::Error => DiagnosticSeverity::ERROR,
            Severity::Warning => DiagnosticSeverity::WARNING,
        }),
        code: Some(NumberOrString::String(report.title.clone())),
        source: Some("nash".into()),
        message: format!(
            "{}\n\n{}",
            report.before.render(80, false),
            report.after.render(80, false)
        ),
        related_information: related.map(|(region, message)| {
            vec![DiagnosticRelatedInformation {
                location: Location {
                    uri: uri.clone(),
                    range: to_range(region, source),
                },
                message,
            }]
        }),
        data: (!report.suggestions.is_empty()).then(|| serde_json::json!(report.suggestions)),
        ..Diagnostic::default()
    }
}

pub fn to_range(region: Region, source: &Source<'_>) -> Range {
    Range::new(
        to_position(region.start, source),
        to_position(region.end, source),
    )
}

fn to_position(position: NashPosition, source: &Source<'_>) -> Position {
    let offset = source.offset(position);
    let prefix = &source.text()[..offset];
    let line = prefix.bytes().filter(|&b| b == b'\n').count() as u32;
    let character = prefix
        .rsplit('\n')
        .next()
        .unwrap_or("")
        .encode_utf16()
        .count() as u32;
    Position::new(line, character)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nash_report::{Doc, Label};
    fn region(sr: u16, sc: u16, er: u16, ec: u16) -> Region {
        Region::new(NashPosition::new(sr, sc), NashPosition::new(er, ec))
    }
    #[test]
    fn ranges_count_utf16_surrogate_pairs() {
        let source = Source::new("a😀éz\nlast");
        assert_eq!(
            to_range(region(1, 6, 1, 8), &source),
            Range::new(Position::new(0, 3), Position::new(0, 4))
        );
        assert_eq!(
            to_range(region(2, 5, 2, 5), &source),
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
        assert_eq!(diagnostic.range, to_range(report.region, &source));
        assert_eq!(
            diagnostic.related_information.unwrap()[0].location.range,
            to_range(region(1, 1, 1, 2), &source)
        );
        assert_eq!(diagnostic.message, "Duplicate names:\n\nRename one.");
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
        assert!(diagnostic.related_information.is_some());
        report.snippet = Snippet::None;
        assert!(to_lsp(&report, &source, &uri).related_information.is_none());
    }
}
