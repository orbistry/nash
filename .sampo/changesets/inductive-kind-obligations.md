---
cargo/nash-ast: minor
cargo/nash-can: minor
cargo/nash-constrain: minor
cargo/nash-solve: patch
cargo/nash-driver: patch
cargo/nash-cli: patch
---

Retain whole kind-application argument vectors and specialize supplied constructor
arguments without opening missing parameters. Require finite inductive proofs,
reject self/self cycles, and check outer shapes and supplied bounds before nested
obligations. Report finite-fragment restrictions and operational limits separately.

Preserve shared kind binders, rigid annotation promises, imported residual schemes,
and fail-closed coherence and evidence checks. Include argument order in exported
kind fingerprints. Sampo updates dependent requirements and publishes the AST
before canonicalization and constraints, then the solver, driver, and CLI.
