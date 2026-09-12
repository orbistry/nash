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
    "nash_codegen__casts__tests__repeated_casts_share_one_checker.snap",
    "nash_codegen__decision_tree__tests__shared_default_leaf.snap",
    "nash_codegen__lower__tests__boolean_case_does_not_evaluate_unselected_failure.snap",
    "nash_codegen__lower__tests__field_projection_and_tag_order_are_semantic.snap",
    "nash_codegen__lower__tests__let_application_evaluates.snap",
    "nash_codegen__lower__tests__trace_precedes_failure.snap",
    "nash_codegen__program__tests__assemble_lets_chain.snap",
    "nash_codegen__program__tests__comptime_reports_open_terms_errors_and_nonconstants.snap",
    "nash_codegen__recursion__tests__static_parameter_core.snap",
    "nash_constrain__module__tests__term_parameter.snap",
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
            line.starts_with("source: crates/nash-report/src/") && !line.ends_with("/json.rs")
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
