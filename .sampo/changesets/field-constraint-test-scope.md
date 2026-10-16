---
cargo/nash-constrain: minor
cargo/nash-solve: patch
---

Record on each record-field constraint whether it comes from a module test, in place of the `Constraint::TestScope` wrapper. `Constraint::Record` and `Constraint::Field` gain a `tests` field, `TestScope` is removed, and `pattern::State` and `pattern::empty_state` take the flag. Field visibility is unchanged: fields that only a tests-block import makes visible are visible in tests and nowhere else.
