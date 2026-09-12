#![allow(dead_code)]
use std::cell::RefCell;

/// Explicit Nash inputs used by one snapshot test, in evaluation order.
#[derive(Default)]
pub struct SnapshotInputs(RefCell<Vec<String>>);

impl SnapshotInputs {
    pub fn record<'a>(&self, source: &'a str) -> &'a str {
        let mut inputs = self.0.borrow_mut();
        inputs.push(source.to_owned());
        source
    }

    pub fn description(&self) -> String {
        let inputs = self.0.borrow();
        let mut unique = Vec::new();
        for input in inputs.iter() {
            if !unique.contains(&input) {
                unique.push(input);
            }
        }
        unique
            .into_iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

pub fn errors(source: &str, errors: &[nash_can::Error<'_>]) -> String {
    let name = source
        .lines()
        .find_map(|line| {
            let line = line.strip_prefix("validator ").unwrap_or(line);
            line.strip_prefix("module ")
                .and_then(|rest| rest.split_whitespace().next())
        })
        .unwrap_or("Main");
    let text = nash_report::Source::new(source);
    errors
        .iter()
        .map(|error| {
            let report = nash_report::canonicalize::to_report_with_name(&text, error, name);
            nash_report::render_plain(
                &report,
                &text,
                &format!("src/{}.nash", name.replace('.', "/")),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn warnings(source: &str, warnings: &[nash_can::Warning<'_>]) -> String {
    let text = nash_report::Source::new(source);
    warnings
        .iter()
        .map(|warning| {
            nash_report::render_plain(
                &nash_report::warning::to_report(warning),
                &text,
                "src/Main.nash",
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

impl SnapshotInputs {
    /// Render errors from the most recently recorded input, before another case is recorded.
    pub fn errors(&self, diagnostics: &[nash_can::Error<'_>]) -> String {
        errors(
            self.0
                .borrow()
                .last()
                .expect("record the diagnostic source"),
            diagnostics,
        )
    }
}

impl SnapshotInputs {
    pub fn errors_before(&self, offset: usize, diagnostics: &[nash_can::Error<'_>]) -> String {
        let inputs = self.0.borrow();
        errors(&inputs[inputs.len() - 1 - offset], diagnostics)
    }
    pub fn results(&self, results: &[Result<(), Vec<nash_can::Error<'_>>>]) -> String {
        results
            .iter()
            .enumerate()
            .map(|(index, result)| match result {
                Ok(()) => format!("Case {}: accepted", index + 1),
                Err(diagnostics) => self.errors_before(results.len() - 1 - index, diagnostics),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
