---
cargo/nash-source: minor
cargo/nash-ast: minor
cargo/nash-parse: minor
cargo/nash-can: minor
cargo/nash-constrain: minor
cargo/nash-solve: minor
cargo/nash-nitpick: minor
cargo/nash-report: patch
cargo/nash-ir: minor
cargo/nash-codegen: minor
cargo/nash-driver: patch
---

Support `pair(first, second)` patterns for builtin pairs in bindings, function arguments, lambdas, and case expressions. Preserve the distinction from tuples and enforce Storable component types.

Lower pair patterns to native UPLC case even when a field is ignored. Use the same Core pair case for Data constructor payloads and implement Base Pair.fst and Pair.snd with Nash patterns.
