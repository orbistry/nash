//! Formatting differences use the same report handler as compiler diagnostics.
use crate::{Doc, Report};
use nash_region::Region;
use similar::{ChangeTag, TextDiff};

/// Return a contextual diff only when the source needs formatting.
/// TextDiff computes edits; this module owns their terminal presentation.
pub fn difference(path: &str, original: &str, formatted: &str) -> Option<Report> {
    if original == formatted {
        return None;
    }
    let diff = TextDiff::from_lines(original, formatted);
    let width = original
        .lines()
        .count()
        .max(formatted.lines().count())
        .max(1)
        .to_string()
        .len();
    let mut lines = vec![Doc::text("This file needs formatting."), Doc::text("")];
    for group in diff.grouped_ops(3) {
        let line = group.first().map_or(1, |op| op.old_range().start + 1);
        lines.push(Doc::text(format!("{:width$} {:width$} ╭─[{path}:{line}]", "", "")).cyan());
        for op in group {
            for change in diff.iter_changes(&op) {
                let old = change
                    .old_index()
                    .map_or(String::new(), |n| (n + 1).to_string());
                let new = change
                    .new_index()
                    .map_or(String::new(), |n| (n + 1).to_string());
                let marker = match change.tag() {
                    ChangeTag::Equal => ' ',
                    ChangeTag::Delete => '-',
                    ChangeTag::Insert => '+',
                };
                let value = change.value().trim_end_matches(['\r', '\n']);
                // Make whitespace-only edits visible without changing context lines.
                let value = if change.tag() != ChangeTag::Equal {
                    let trimmed = value.trim_end_matches([' ', '\t']);
                    let trailing = value[trimmed.len()..].replace(' ', "·").replace('\t', "→");
                    format!("{trimmed}{trailing}")
                } else {
                    value.to_string()
                };
                let ending = if change.tag() != ChangeTag::Equal && change.value().ends_with("\r\n")
                {
                    " ␍"
                } else {
                    ""
                };
                let row = Doc::text(format!(
                    "{old:>width$} {new:>width$} │{marker} {value}{ending}"
                ));
                lines.push(match change.tag() {
                    ChangeTag::Equal => row,
                    ChangeTag::Delete => row.red(),
                    ChangeTag::Insert => row.green(),
                });
                if change.missing_newline() {
                    lines.push(Doc::text(format!(
                        "{:width$} {:width$} │  No newline at end of file",
                        "", ""
                    )));
                }
            }
        }
        lines.push(Doc::text(format!("{:width$} {:width$} ╰────", "", "")).cyan());
    }
    Some(
        Report::snippet(
            "FORMATTING",
            Region::one(),
            None,
            Doc::vcat(lines),
            Doc::text("Run `nash format` to apply formatting."),
        )
        .without_source()
        .with_code("nash::format"),
    )
}
