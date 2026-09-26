---
cargo/nash-fmt: minor
cargo/nash-cli: minor
cargo/nash-report: minor
cargo/nash-parse: patch
---

Add an AST-based Nash source formatter with 80-column layout, comment preservation,
and source snapshot tests. Expose `nash format` (`fmt`), in-place and stdin
formatting, and contextual `--check` diffs through the existing report style.
Correct whitespace handling after message keywords and before constructor docs.
Allow aligned explicit continuations inside `do` without merging statements.
