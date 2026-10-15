---
cargo/nash-report: patch
---

Stop `nash check` from crashing with "Doc::text must not contain newlines" when a missing-constraint or missing-impl predicate is wider than 80 columns. The help line is now part of the report layout instead of pre-rendered text. When a sentence with a predicate does not fit the line, the predicate moves to its own line, indented by 4. Sentences that fit render as before.
