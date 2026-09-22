//! Prevent source-driven snapshots from losing their Nash input or recording
//! Rust assertion expressions or injected Base sources.
use std::path::{Path, PathBuf};

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
    let mut boilerplate = Vec::new();
    for entry in std::fs::read_dir(crates.join("nash-driver/base/src")).unwrap() {
        let path = entry.unwrap().path();
        if path
            .extension()
            .is_some_and(|extension| extension == "nash")
        {
            boilerplate.push(std::fs::read_to_string(path).unwrap());
        }
    }
    for path in [
        "nash-driver/src/compile/fixtures/Eq.nash",
        "nash-driver/src/compile/fixtures/Literal.nash",
        "nash-driver/src/compile/fixtures/Monad.nash",
        "nash-codegen/tests/fixtures/VestingLiteral.nash",
        "nash-codegen/tests/fixtures/VestingLift.nash",
    ] {
        boilerplate.push(std::fs::read_to_string(crates.join(path)).unwrap());
    }
    let boilerplate: Vec<_> = boilerplate
        .iter()
        .map(|source| {
            let quoted = serde_json::to_string(source.trim()).unwrap();
            quoted[1..quoted.len() - 1].to_owned()
        })
        .collect();
    let mut failures = Vec::new();
    for path in paths {
        let snapshot = std::fs::read_to_string(&path).unwrap();
        let (metadata, body) = snapshot
            .strip_prefix("---\n")
            .and_then(|text| text.split_once("\n---\n"))
            .expect("insta snapshot header");
        if snapshot.contains('\x1b') {
            failures.push(format!("{} contains terminal escape codes", path.display()));
        }
        if body.trim_start().starts_with("Err(") {
            failures.push(format!(
                "{} contains an unrendered error result",
                path.display()
            ));
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
        if boilerplate.iter().any(|source| metadata.contains(source)) {
            failures.push(format!("{} includes injected Base source", path.display()));
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
                if path
                    .file_name()
                    .is_some_and(|name| name == "snapshot_hygiene.rs")
                {
                    continue;
                }
                // Keep snapshots external so this suite validates every artifact.
                if source.contains(", @\"") || source.contains(", @r") {
                    failures.push(format!("{} contains an inline snapshot", path.display()));
                }
                let test_source = if path.components().any(|part| part.as_os_str() == "tests")
                    || path
                        .file_stem()
                        .unwrap()
                        .to_string_lossy()
                        .ends_with("_tests")
                {
                    source.as_str()
                } else {
                    source
                        .split_once("#[cfg(test)]")
                        .map_or("", |(_, tests)| tests)
                };
                if test_source.contains("Command::new") || test_source.contains("process::Command")
                {
                    failures.push(format!("{} launches a process in tests", path.display()));
                }
                if source.contains("CARGO_BIN_EXE")
                    || source.contains("cargo_bin(")
                    || source.contains("assert_cmd")
                {
                    failures.push(format!("{} invokes a CLI binary in tests", path.display()));
                }
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
