use super::{column, row, source_text};
use crate::*;
use std::{fmt::Write, time::Duration};
pub fn render_assert(source: &str, report: &AssertReport) -> String {
    let text = source_text(source, report.site.region);
    let mut captures = report
        .site
        .captures
        .iter()
        .filter_map(|cap| {
            let value = report
                .values
                .iter()
                .find(|(i, _)| *i == cap.index)
                .map(|(_, v)| v.as_str())
                .or(if cap.shown { None } else { Some("?") })?;
            Some((
                row(report.site.region, cap.region),
                column(source, report.site.region, cap.region),
                value,
            ))
        })
        .collect::<Vec<_>>();
    captures.sort_by_key(|(row, column, _)| (*row, *column));
    let source_lines = text.split('\n').collect::<Vec<_>>();
    let mut out = String::new();
    for (row, text) in source_lines.iter().enumerate() {
        writeln!(
            out,
            "{}{}{}",
            if row == 0 {
                "× assert ("
            } else {
                "          "
            },
            text,
            if row + 1 == source_lines.len() {
                ")"
            } else {
                ""
            }
        )
        .unwrap();
        let cols = captures
            .iter()
            .filter(|(r, _, _)| *r == row)
            .map(|(_, c, v)| (*c, *v))
            .collect::<Vec<_>>();
        let pipes = |count: usize| {
            let width = cols
                .iter()
                .take(count)
                .map(|(c, _)| 10 + c + 1)
                .max()
                .unwrap_or(0);
            let mut line = vec![' '; width];
            for (c, _) in cols.iter().take(count) {
                line[10 + c] = '│';
            }
            line
        };
        if !cols.is_empty() {
            writeln!(out, "{}", pipes(cols.len()).into_iter().collect::<String>()).unwrap();
        }
        for i in (0..cols.len()).rev() {
            let mut prefix = pipes(i);
            prefix.resize(10 + cols[i].0, ' ');
            let prefix = prefix.into_iter().collect::<String>();
            // Every continuation stays under its capture, preserving the other
            // capture guides instead of leaking unindented text into the report.
            for value_line in cols[i].1.split('\n') {
                writeln!(out, "{prefix}{value_line}").unwrap();
            }
        }
    }
    out
}
fn quantity(n: i64) -> String {
    let raw = format!("{:.1}", n as f64 / 1000.0);
    let (whole, fraction) = raw.split_once('.').unwrap();
    let mut grouped = String::new();
    for (i, c) in whole.chars().enumerate() {
        if i > 0 && (whole.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(c);
    }
    format!("{grouped}.{fraction}K")
}
pub fn render(outcomes: &[Outcome], coverage: Coverage, seed: u32, elapsed: Duration) -> String {
    let mut out = String::new();
    let mut module = None;
    let width = outcomes
        .iter()
        .map(|o| o.test.name.chars().count())
        .max()
        .unwrap_or(0);
    for o in outcomes {
        if module != Some(o.test.module.as_str()) {
            writeln!(
                out,
                "  Testing {} ({})\n",
                o.test.module,
                o.test.source_path.display()
            )
            .unwrap();
            module = Some(&o.test.module);
        }
        let status = if o.status == Status::Pass {
            "PASS"
        } else {
            "FAIL"
        };
        write!(
            out,
            "  {status} {}{}",
            o.test.name,
            " ".repeat(width - o.test.name.chars().count() + 2)
        )
        .unwrap();
        if matches!(o.test.programs, Programs::Prop { .. }) {
            write!(
                out,
                "[after {} test{}]",
                o.iterations,
                if o.iterations == 1 { "" } else { "s" }
            )
            .unwrap();
        }
        if matches!(o.test.programs, Programs::Unit { .. }) || o.test.budget.is_some() {
            write!(
                out,
                "[mem: {:>8}, cpu: {:>8}]",
                quantity(o.budget.mem),
                quantity(o.budget.cpu)
            )
            .unwrap();
        }
        out.push('\n');
        if let Some(ce) = &o.counterexample {
            writeln!(
                out,
                "  {} counterexample",
                if o.expected_failure { "★" } else { "×" }
            )
            .unwrap();
            for (name, value) in ce {
                writeln!(out, "  │ {name} = {value}").unwrap();
            }
        }
        if let Some(assert) = &o.assert
            && (o.status != Status::Pass || o.expected_failure)
        {
            for line in render_assert(&o.test.source, assert).lines() {
                writeln!(out, "  {line}").unwrap();
            }
            writeln!(
                out,
                "    {}:{}:{}",
                o.test.source_path.display(),
                assert.site.region.start.line,
                assert.site.region.start.column
            )
            .unwrap();
        }
        if let Status::Fail(f) = &o.status {
            match f {
                Failure::BudgetExceeded { limit, used } => writeln!(
                    out,
                    "  × budget exceeded: cpu {}, mem {}; limit {limit:?}",
                    used.cpu, used.mem
                )
                .unwrap(),
                Failure::Fuzzer { message } => {
                    writeln!(out, "  × fuzzer failed unexpectedly: {message}").unwrap()
                }
                Failure::InvalidProgram { message } => {
                    writeln!(out, "  × invalid test program: {message}").unwrap()
                }
                Failure::NoCounterexample => writeln!(out, "  × no counterexample found").unwrap(),
                Failure::Body => {}
            }
        }
        if o.status == Status::Pass
            && matches!(o.test.programs, Programs::Prop { .. })
            && !o.labels.is_empty()
        {
            out.push_str("  · with coverage\n");
            let total = match coverage {
                Coverage::Labels => o.labels.values().sum(),
                Coverage::Tests => o.iterations,
            };
            let mut labels = o.labels.iter().collect::<Vec<_>>();
            labels.sort_by(|(a, n), (b, m)| m.cmp(n).then(a.cmp(b)));
            for (label, n) in labels {
                writeln!(
                    out,
                    "  | {label}  {:.1}%",
                    100.0 * *n as f64 / total.max(1) as f64
                )
                .unwrap();
            }
        }
        if (o.status != Status::Pass || o.expected_failure) && !o.traces.is_empty() {
            out.push_str("  · with traces\n");
            for trace in &o.traces {
                for line in trace.lines() {
                    writeln!(out, "  | {line}").unwrap();
                }
            }
        }
    }
    let passed = outcomes.iter().filter(|o| o.status == Status::Pass).count();
    writeln!(
        out,
        "\n  Summary {passed} passed, {} failed, 0 skipped   seed {seed}   {:.2}s",
        outcomes.len() - passed,
        elapsed.as_secs_f64()
    )
    .unwrap();
    out
}
