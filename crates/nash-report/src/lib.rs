//! Concise, source-aware diagnostics shared by terminal, JSON, and LSP.
//!
//! Each phase's error data (`nash_parse::error`, `nash_can::Error`, ...)
//! is turned into an owned `Report` that outlives the module arena. A
//! `Report` renders three ways: `render` (miette, terminal), `json`
//! (Elm's `--report=json` shape), and the LSP conversion in
//! `nash-language-server`.

pub mod canonicalize;
pub mod code;
pub mod doc;
pub mod json;
pub mod localizer;
pub mod pattern;
pub mod render_type;
pub mod suggest;
pub mod syntax;
pub mod type_;
pub mod type_diff;
pub use localizer::Localizer;
mod render;
pub mod warning;

use nash_region::Region;

pub use code::Source;
pub use doc::Doc;
pub use render::{Rendered, handler, render_plain};

/// An owned diagnostic with one primary span, arbitrary secondary labels, and
/// related reports that can refer to other source files.
#[derive(Clone, Debug)]
pub struct Report {
    pub code: &'static str,
    pub title: String,
    pub severity: Severity,
    pub region: Region,
    /// Text on the primary region, or None for a report without a source label.
    pub primary_label: Option<String>,
    pub labels: Vec<Label>,
    pub context: Option<Region>,
    pub related: Vec<ModuleReports>,
    pub before: Doc,
    pub after: Doc,
    pub suggestions: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Label {
    pub region: Region,
    pub text: String,
}

impl Report {
    /// `Report.Report title region [] (Code.toSnippet source region highlight (before, after))`.
    pub fn snippet(
        title: &str,
        region: Region,
        highlight: Option<Region>,
        before: Doc,
        after: Doc,
    ) -> Report {
        Report {
            code: "nash::diagnostic",
            title: title.to_string(),
            severity: Severity::Error,
            region: highlight.unwrap_or(region),
            primary_label: Some(String::new()),
            labels: Vec::new(),
            context: Some(region),
            related: Vec::new(),
            before,
            after,
            suggestions: Vec::new(),
        }
    }

    /// `Report.Report title r2 [] (Code.toPair source r1 r2 ...)`. The
    /// "one line" / "two chunks" wording choice Elm makes is replaced by
    /// two labels; `before` is Elm's `twoStart` and `after` its `twoEnd`.
    pub fn pair(title: &str, first: Label, second: Label, before: Doc, after: Doc) -> Report {
        let region = second.region;
        Report {
            code: "nash::diagnostic",
            title: title.to_string(),
            severity: Severity::Error,
            region,
            primary_label: Some(second.text),
            labels: vec![first],
            context: None,
            related: Vec::new(),
            before,
            after,
            suggestions: Vec::new(),
        }
    }

    /// Set the surrounding snippet while retaining the primary diagnostic region.
    pub fn with_region(mut self, surroundings: Region) -> Report {
        self.context = Some(surroundings);
        self
    }

    /// Plain text for clients that cannot draw source labels inline.
    pub fn message(&self) -> String {
        Doc::stack([
            self.before.clone(),
            self.primary_label.as_ref().map_or(Doc::Empty, Doc::text),
            self.after.clone(),
        ])
        .render(80, false)
    }

    pub fn with_code(mut self, code: &'static str) -> Report {
        self.code = code;
        self
    }

    pub fn with_label(mut self, label: Label) -> Report {
        self.labels.push(label);
        self
    }

    pub fn with_related(mut self, related: ModuleReports) -> Report {
        self.related.push(related);
        self
    }

    pub fn without_source(mut self) -> Report {
        self.primary_label = None;
        self.labels.clear();
        self.context = None;
        self
    }

    pub fn with_suggestions(mut self, suggestions: Vec<String>) -> Report {
        self.suggestions = suggestions;
        self
    }

    pub fn warning(mut self) -> Report {
        self.severity = Severity::Warning;
        self
    }
}

/// Owned reports remain valid after a compiler module's arena is dropped.
#[derive(Clone, Debug)]
pub struct ModuleReports {
    pub name: String,
    pub path: String,
    pub source: String,
    pub reports: Vec<Report>,
}

impl ModuleReports {
    pub fn json(&self) -> serde_json::Value {
        json::module_to_json(self)
    }
    pub fn render(&self, color: bool) -> Vec<Rendered> {
        let source = Source::new(&self.source);
        self.reports
            .iter()
            .map(|report| report.render(&source, &self.path, color))
            .collect()
    }

    /// Stable presentation order without removing distinct errors at one span.
    pub fn sort(&mut self) {
        for report in &mut self.reports {
            for module in &mut report.related {
                module.sort();
            }
            report.related.sort_by_cached_key(|module| {
                (
                    module.path.clone(),
                    module.name.clone(),
                    module.json().to_string(),
                )
            });
        }
        self.reports.sort_by_cached_key(|report| {
            (
                report.region,
                report.code,
                report.title.clone(),
                report.before.render(80, false),
                report.after.render(80, false),
                report.severity,
                report.primary_label.clone(),
                report.labels.clone(),
                report.context,
                report.suggestions.clone(),
                report
                    .related
                    .iter()
                    .map(|module| module.json().to_string())
                    .collect::<Vec<_>>(),
            )
        });
    }
}

/// Phase error data is converted while its arena is still alive.
pub enum ModuleError<'a> {
    Syntax(nash_parse::error::Error<'a>),
    Names(Vec<nash_can::Error<'a>>),
    Types(Localizer, Vec<nash_constrain::Error<'a>>),
    Patterns(Vec<nash_nitpick::Error<'a>>),
}

pub fn to_reports(
    source: &Source<'_>,
    expected_module_name: &str,
    error: &ModuleError<'_>,
) -> Vec<Report> {
    match error {
        ModuleError::Syntax(error) => vec![syntax::to_report(source, error)],
        ModuleError::Names(errors) => errors
            .iter()
            .map(|error| canonicalize::to_report_with_name(source, error, expected_module_name))
            .collect(),
        ModuleError::Types(localizer, errors) => errors
            .iter()
            .map(|error| type_::to_report(localizer, error))
            .collect(),
        ModuleError::Patterns(errors) => errors.iter().map(pattern::to_report).collect(),
    }
}
