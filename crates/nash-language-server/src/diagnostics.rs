//! Convert compiler reports to LSP without changing their primary spans.
use nash_region::{Position as NashPosition, Region};
use nash_report::{Report, Severity, Snippet, Source};
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
    let related = match &report.snippet {
        Snippet::Pair { first, .. } => Some((first.region, first.text.clone())),
        Snippet::Region {
            highlight: Some(highlight),
            ..
        } if *highlight != report.region => Some((*highlight, "Related source".into())),
        _ => None,
    };
    let related_information = match related {
        Some((region, message)) => Some(vec![DiagnosticRelatedInformation {
            location: Location {
                uri: uri.clone(),
                range: to_range(region, source)?,
            },
            message,
        }]),
        None => None,
    };
    Some(Diagnostic {
        range: to_range(report.region, source)?,
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
        related_information,
        data: (!report.suggestions.is_empty()).then(|| serde_json::json!(report.suggestions)),
        ..Diagnostic::default()
    })
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
