//! Prevent source-driven snapshots from losing their Nash input or recording
//! Rust assertion expressions. Pure constructed fixtures are listed explicitly.
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

// Reviewed fixtures built directly from Rust IR, type, report, or Doc values.
// They have no Nash source input. Keep this list specific to each fixture;
// a new source-driven test in the same Rust module must still meet the rule.
const CONSTRUCTED_FIXTURES: &[&str] = &[
    "inference__qualified_annotation_keeps_context_only_types_and_reserves_their_names.snap",
    "kinds__alias_substitution_preserves_application_head_and_argument.snap",
    "kinds__annotation_kinds_keep_application_parameters_correlated.snap",
    "kinds__annotation_retains_one_storable_predicate.snap",
    "nash_can__module__tests__to_public_alias_private_returns_none.snap",
    "nash_can__module__tests__to_public_alias_public_passes_through.snap",
    "nash_can__module__tests__to_public_union_closed_strips_ctors.snap",
    "nash_can__module__tests__to_public_union_open_passes_through.snap",
    "nash_can__module__tests__to_public_union_private_returns_none.snap",
    "nash_can__types__tests__partial_alias_keeps_formal_parameters_bound.snap",
    "nash_ir__pretty__tests__pretty_case_data.snap",
    "nash_ir__pretty__tests__pretty_case_tag.snap",
    "nash_ir__pretty__tests__pretty_let_app.snap",
    "nash_ir__pretty__tests__pretty_letrec_static.snap",
    "nash_report__doc__tests__chunks_merge_plain_runs.snap",
    "nash_report__doc__tests__cycle_box.snap",
    "nash_report__doc__tests__hang_aligns_continuations.snap",
    "nash_report__doc__tests__reflow_inside_indent_keeps_indent.snap",
    "nash_report__doc__tests__reflow_wraps_at_80.snap",
    "nash_report__doc__tests__sep_breaks_when_too_wide.snap",
    "nash_report__doc__tests__sep_flat_when_fits.snap",
    "nash_report__doc__tests__stack_separates_with_blank_line.snap",
    "nash_report__json__tests__paired_regions_are_self_contained.snap",
    "nash_report__pattern__tests__literal_witnesses_escape_source_text.snap",
    "nash_report__type___tests__every_category.snap",
    "nash_report__type___tests__every_pattern_category.snap",
    "nash_report__type___tests__hint_arity_fewer.snap",
    "nash_report__type___tests__hint_arity_more.snap",
    "nash_report__type___tests__hint_big_little_need.snap",
    "nash_report__type___tests__hint_double_rigid.snap",
    "nash_report__type___tests__hint_field_typo.snap",
    "nash_report__type___tests__hint_missing_fields.snap",
    "nash_report__type___tests__hint_option.snap",
];

fn snapshots(directory: &Path, output: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            snapshots(&path, output);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "snap")
        {
            output.push(path);
        }
    }
}

#[test]
fn source_snapshots_include_input_and_omit_rust_expressions() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut paths = Vec::new();
    snapshots(crates, &mut paths);
    paths.sort();
    assert!(!paths.is_empty());
    let mut exemptions: BTreeSet<_> = CONSTRUCTED_FIXTURES.iter().copied().collect();
    let mut failures = Vec::new();
    for path in paths {
        let snapshot = std::fs::read_to_string(&path).unwrap();
        let (metadata, body) = snapshot
            .strip_prefix("---\n")
            .and_then(|text| text.split_once("\n---\n"))
            .expect("insta snapshot header");
        let name = path.file_name().unwrap().to_str().unwrap();
        let constructed = exemptions.remove(name);
        if snapshot.contains('\x1b') {
            failures.push(format!("{} contains terminal escape codes", path.display()));
        }
        if body.trim_start().starts_with("Err(") {
            failures.push(format!(
                "{} contains an unrendered error result",
                path.display()
            ));
        }
        if constructed {
            continue;
        }
        if !metadata
            .lines()
            .any(|line| line.starts_with("description:"))
        {
            failures.push(format!(
                "{} is missing its input description",
                path.display()
            ));
        }
        if metadata.lines().any(|line| line.starts_with("expression:")) {
            failures.push(format!(
                "{} captures a Rust assertion expression",
                path.display()
            ));
        }
        // A diagnostic renderer test must snapshot the actual terminal report,
        // not a title/Region/prose concatenation. JSON is a separate contract.
        let rendered_diagnostic = metadata.lines().any(|line| {
            line == "info: diagnostic"
                || (line.starts_with("source: crates/nash-report/src/")
                    && !line.ends_with("/json.rs"))
        });
        if rendered_diagnostic && !body.contains('×') && !body.contains('⚠') {
            failures.push(format!("{} is not a rendered diagnostic", path.display()));
        }
    }
    assert!(
        exemptions.is_empty(),
        "stale constructed-fixture exceptions: {exemptions:?}"
    );
    assert!(
        failures.is_empty(),
        "snapshot hygiene failures:\n{}",
        failures.join("\n")
    );
}

/// Error snapshot macros must opt into the rendered-diagnostic contract.
/// This catches a copied raw Debug helper even before its snapshots exist.
#[test]
fn error_snapshot_macros_require_rendered_diagnostics() {
    fn check(directory: &Path, failures: &mut Vec<String>) {
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                check(&path, failures);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                let source = std::fs::read_to_string(&path).unwrap();
                for definition in source.split("macro_rules! ").skip(1) {
                    let name = definition.split_whitespace().next().unwrap_or("");
                    if !name.contains("error_snapshot") && name != "assert_diagnostics_snapshot" {
                        continue;
                    }
                    // All snapshot macros use a braced definition. Balance its
                    // braces to avoid accidentally inspecting the next test.
                    let start = definition.find('{').unwrap();
                    let mut depth = 0;
                    let mut end = start;
                    for (offset, ch) in definition[start..].char_indices() {
                        match ch {
                            '{' => depth += 1,
                            '}' => {
                                depth -= 1;
                                if depth == 0 {
                                    end = start + offset;
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                    let body = &definition[start..end];
                    if body.contains("assert_debug_snapshot!")
                        || (!body.contains("info => &\"diagnostic\"")
                            && !body.contains("assert_diagnostics_snapshot!"))
                    {
                        failures.push(format!("{}: {name}", path.display()));
                    }
                }
            }
        }
    }
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut failures = Vec::new();
    check(crates, &mut failures);
    assert!(
        failures.is_empty(),
        "raw error snapshot macros: {failures:?}"
    );
}
