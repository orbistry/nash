//! Elm's `Reporting/Error.hs` JSON schema and complete styled messages.

use crate::{Doc, ModuleReports, Report, Snippet, Source};
use nash_region::Region;
use serde_json::{Value, json};

/// Elm's `toJson`. Report ordering is established by `ModuleReports::sort`.
pub fn module_to_json(module: &ModuleReports) -> Value {
    let source = Source::new(&module.source);
    json!({
        "path": module.path,
        "name": module.name,
        "problems": module.reports.iter().map(|report| report_to_json(&source, report)).collect::<Vec<_>>(),
    })
}

/// Compile-error envelope used by Elm's command-line reporting protocol.
pub fn compile_errors(modules: &[ModuleReports]) -> Value {
    json!({"type": "compile-errors", "errors": modules.iter().map(module_to_json).collect::<Vec<_>>()})
}

/// Nash extension using the same module/problem schema for warnings.
pub fn compile_warnings(modules: &[ModuleReports]) -> Value {
    json!({"type": "compile-warnings", "errors": modules.iter().map(module_to_json).collect::<Vec<_>>()})
}

fn report_to_json(source: &Source<'_>, report: &Report) -> Value {
    let message = match report.snippet {
        Snippet::None => Doc::stack([report.before.clone(), report.after.clone()]),
        _ => Doc::vcat([
            report.before.clone(),
            Doc::Empty,
            source.snippet_doc(&report.snippet),
            report.after.clone(),
        ]),
    };
    json!({"title": report.title, "region": encode_region(report.region), "message": message.encode()})
}

/// Elm's one-based, half-open source region schema.
pub fn encode_region(region: Region) -> Value {
    json!({
        "start": {"line": region.start.line, "column": region.start.column},
        "end": {"line": region.end.line, "column": region.end.column},
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Doc, Label, Snippet};
    use nash_region::Position;

    fn region(sr: usize, sc: usize, er: usize, ec: usize) -> Region {
        Region::new(Position::new(sr, sc), Position::new(er, ec))
    }

    #[test]
    fn elm_schema_preserves_primary_span_and_styles() {
        let report = Report::snippet(
            "NAMING ERROR",
            region(2, 5, 2, 12),
            None,
            Doc::text("I cannot find this name:"),
            Doc::text("Try "),
        )
        .with_region(region(1, 1, 2, 12))
        .with_suggestions(vec!["found".into()]);
        let mut report = report;
        report.after = Doc::cat([
            Doc::text("Try "),
            Doc::text("found").green(),
            Doc::text("."),
        ]);
        let module = ModuleReports {
            name: "Main".into(),
            path: "src/Main.nash".into(),
            source: "main =\n    missing".into(),
            reports: vec![report],
        };
        let value = module_to_json(&module);
        assert_eq!(
            value["problems"][0]["region"],
            encode_region(region(2, 5, 2, 12))
        );
        assert_eq!(value.as_object().unwrap().len(), 3);
        assert_eq!(value["problems"][0].as_object().unwrap().len(), 3);
        insta::assert_snapshot!(serde_json::to_string_pretty(&value).unwrap());
    }

    #[test]
    fn no_snippet_does_not_add_empty_code_lines() {
        let mut report = Report::snippet(
            "MODULE NAME MISSING",
            Region::zero(),
            None,
            Doc::text("First."),
            Doc::text("Second."),
        );
        report.snippet = Snippet::None;
        assert_eq!(
            report_to_json(&Source::new(""), &report)["message"],
            serde_json::json!(["First.\n\nSecond."])
        );
    }

    #[test]
    fn paired_regions_are_self_contained() {
        let report = Report::pair(
            "NAME CLASH",
            Label {
                region: region(1, 1, 1, 2),
                text: "first".into(),
            },
            Label {
                region: region(2, 1, 2, 2),
                text: "second".into(),
            },
            Doc::text("Both names occur here:"),
            Doc::text("Choose another name."),
        );
        insta::assert_snapshot!(
            serde_json::to_string_pretty(&report_to_json(&Source::new("x\nx"), &report)).unwrap()
        );
    }

    #[test]
    fn error_and_warning_envelopes() {
        assert_eq!(
            compile_errors(&[]),
            serde_json::json!({"type":"compile-errors","errors":[]})
        );
        assert_eq!(
            compile_warnings(&[]),
            serde_json::json!({"type":"compile-warnings","errors":[]})
        );
    }
}
