//! Error reports: Elm's `Reporting/*` prose as miette diagnostics.
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
pub mod render_type;
pub mod suggest;
pub mod syntax;
pub mod type_diff;
pub use localizer::Localizer;
mod render;

use nash_region::Region;

pub use code::Source;
pub use doc::Doc;
pub use render::{Rendered, handler, render_plain};

/// Elm's `Reporting.Report.Report` with the snippet placement split out
/// so miette can draw the code.
#[derive(Clone, Debug)]
pub struct Report {
    pub title: String,
    pub severity: Severity,
    pub region: Region,
    pub snippet: Snippet,
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
pub enum Snippet {
    /// `Code.toSnippet source region highlight`.
    Region {
        region: Region,
        highlight: Option<Region>,
    },
    /// `Code.toPair source r1 r2`.
    Pair { first: Label, second: Label },
    /// No code shown.
    None,
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
            title: title.to_string(),
            severity: Severity::Error,
            region,
            snippet: Snippet::Region { region, highlight },
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
            title: title.to_string(),
            severity: Severity::Error,
            region,
            snippet: Snippet::Pair { first, second },
            before,
            after,
            suggestions: Vec::new(),
        }
    }

    /// Set the surrounding snippet while retaining the primary diagnostic region.
    pub fn with_region(mut self, surroundings: Region) -> Report {
        if let Snippet::Region { region, highlight } = &mut self.snippet {
            *highlight = Some(highlight.unwrap_or(self.region));
            *region = surroundings;
        }
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
        self.reports.sort_by_cached_key(|report| {
            (
                report.region,
                report.title.clone(),
                report.before.render(80, false),
                report.after.render(80, false),
                report.severity,
                report.snippet.clone(),
                report.suggestions.clone(),
            )
        });
    }
}
