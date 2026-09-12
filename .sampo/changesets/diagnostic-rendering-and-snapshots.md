---
cargo/nash-parse: patch
cargo/nash-can: patch
cargo/nash-constrain: patch
cargo/nash-solve: patch
cargo/nash-report: patch
cargo/nash-codegen: patch
cargo/nash-driver: patch
cargo/nash-cli: patch
---

Preserve miette diagnostic codes and error/warning markers in terminal output. Show source paths relative to the project root while keeping file hyperlinks absolute.

Expand colorless rendered diagnostic snapshot coverage across parser, canonicalizer, solver, driver, and CLI tests. Record Nash source instead of Rust assertion expressions in source-driven snapshots, move codegen snapshots to source-compilation tests, and check snapshot metadata for regressions. Keep direct assertions for internal error values and hand-built Core behavior.
