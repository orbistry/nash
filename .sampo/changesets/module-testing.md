---
cargo/nash-config: minor
cargo/nash-codegen: minor
cargo/nash-driver: minor
cargo/nash-cli: minor
---

Add module-local unit tests and properties, scoped test dependencies, source-aware
power assertions, deterministic seeded fuzzing and counterexample shrinking.
Provide `nash test` with budget checks, labels, trace controls, parallel execution,
and terminal/JSON reports. Type-check tests with `nash check` while keeping them
out of production builds. Add core Fuzz and Test support with explicit imports.

Preserve short-circuit evaluation for the core boolean infix operators, including
inside instrumented assertions.
