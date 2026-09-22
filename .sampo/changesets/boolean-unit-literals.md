---
cargo/nash-ast: minor
cargo/nash-can: minor
cargo/nash-constrain: minor
cargo/nash-solve: minor
cargo/nash-codegen: minor
cargo/nash-driver: minor
cargo/nash-nitpick: patch
---

Overload bare boolean and unit expressions through FromBool and FromUnit, with
little defaults and bundled implementations for bool, Bool, unit, and Unit.
Like integer literals, unannotated values retain their literal constraints;
annotate concrete entry points where needed. Qualified constructors and patterns
retain their declared types. Add source snapshots for inference, diagnostics,
custom conversions, and execution.
