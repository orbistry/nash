---
cargo/nash-ast: minor
cargo/nash-can: minor
cargo/nash-cli: minor
cargo/nash-codegen: minor
cargo/nash-constrain: minor
cargo/nash-docs: patch
cargo/nash-driver: minor
cargo/nash-language-server: minor
cargo/nash-report: minor
cargo/nash-solve: minor
---

Replace every separate search budget with one compile fuel counter, `nash_ast::fuel`. Trait resolution, impl overlap checks, superclass entailment, evidence resolution and code generation share it: each search step takes a `tick`, and the driver installs a fresh `Fuel` for each module's compile. `nash check`, `build`, `test` and `docs` run with unlimited fuel, so programs that the old fixed limits rejected now compile, such as an impl context that rewraps its argument used at 200 nested types. The language server runs with 10 million steps per module, so an editor gives up on a looping program after a second or two instead of freezing.

Running out of fuel is one error kind, `COMPILATION STEP LIMIT` (`nash::type::out_of_fuel`), replacing `ImplResolutionLimit` (`nash::type::impl_resolution_limit`), `ImplPatternLimit` (`nash::names::impl_pattern_limit`), the superclass search limit, and the code generation specialization, evidence and recursion normalization limits. The solver reports it at the use that needed the search, canonicalization at the impl being checked, and code generation as a plain error. `nash_driver::build`, `build_with` and `test_with` take a `Fuel` argument, re-exported as `nash_driver::Fuel`.
