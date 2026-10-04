---
cargo/nash-solve: minor
cargo/nash-codegen: minor
cargo/nash-ir: patch
cargo/nash-report: patch
---

Support read-only `.fst` and `.snd` field access and accessor functions for known builtin pair types, including aliases. Lower access directly to the pair builtins. Enable the restricted O1 pair-case rewrite only when constructor inverse cleanup removes the introduced projection, preserving strict producer evaluation.
