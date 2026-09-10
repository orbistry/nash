//! Structured diagnostics in the Elm compile-error envelope.

use crate::{Doc, ModuleReports, Report, Severity};
use nash_region::Region;
use serde_json::{Value, json};

/// Elm's `toJson`. Report ordering is established by `ModuleReports::sort`.
pub fn module_to_json(module: &ModuleReports) -> Value {
    json!({
        "path": module.path,
        "name": module.name,
        "problems": module.reports.iter().map(report_to_json).collect::<Vec<_>>(),
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

pub fn report_to_json(report: &Report) -> Value {
    let labels: Vec<_> = report
        .primary_label
        .iter()
        .map(|text| {
            json!({
                "region": encode_region(report.region), "text": text, "primary": true,
            })
        })
        .chain(report.labels.iter().map(|label| {
            json!({
                "region": encode_region(label.region), "text": label.text, "primary": false,
            })
        }))
        .collect();
    json!({
        "code": report.code,
        "title": report.title,
        "severity": match report.severity { Severity::Error => "error", Severity::Warning => "warning" },
        "region": encode_region(report.region),
        "message": Doc::stack([report.before.clone(), report.after.clone()]).encode(),
        "labels": labels,
        "suggestions": report.suggestions,
        "related": report.related.iter().map(module_to_json).collect::<Vec<_>>(),
    })
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
    use crate::{Doc, Label};
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
        assert_eq!(value["problems"][0]["suggestions"], json!(["found"]));
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
        report = report.without_source();
        assert_eq!(
            report_to_json(&report)["message"],
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
        insta::assert_snapshot!(serde_json::to_string_pretty(&report_to_json(&report)).unwrap());
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

#[cfg(test)]
mod structured_tests {
    use super::*;
    use crate::Label;
    #[test]
    fn labels_codes_and_related_reports_are_structured() {
        let mut report = Report::snippet(
            "OLD TITLE",
            Region::zero(),
            None,
            Doc::text("Problem."),
            Doc::text("Hint."),
        )
        .with_code("nash::type::mismatch")
        .with_suggestions(vec!["replacement".into()]);
        report.title = "NEW TITLE".into();
        report.primary_label = Some("failing argument".into());
        for text in ["annotation", "earlier argument"] {
            report.labels.push(Label {
                region: Region::zero(),
                text: text.into(),
            });
        }
        report.related.push(ModuleReports {
            name: "Other".into(),
            path: "Other.nash".into(),
            source: "other".into(),
            reports: vec![
                Report::snippet(
                    "RELATED",
                    Region::zero(),
                    None,
                    Doc::text("Origin."),
                    Doc::Empty,
                )
                .with_code("nash::type::origin"),
            ],
        });
        let value = report_to_json(&report);
        assert_eq!(value["code"], "nash::type::mismatch");
        assert_eq!(value["title"], "NEW TITLE");
        assert_eq!(value["labels"].as_array().unwrap().len(), 3);
        assert_eq!(value["labels"][0]["primary"], true);
        assert_eq!(value["labels"][1]["text"], "annotation");
        assert_eq!(value["labels"][2]["primary"], false);
        assert_eq!(value["related"][0]["path"], "Other.nash");
        assert_eq!(
            value["related"][0]["problems"][0]["code"],
            "nash::type::origin"
        );
        assert_eq!(value["suggestions"], json!(["replacement"]));
        assert_eq!(value["message"], json!(["Problem.\n\nHint."]));
    }
}
