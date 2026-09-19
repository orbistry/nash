---
cargo/nash-ast: minor
cargo/nash-can: patch
cargo/nash-ir: minor
cargo/nash-codegen: minor
---

Restrict Builtin to actual Plutus operations and remove compiler cast nodes,
cast expansion, generated validation checkers, and privileged core intrinsics.
Implement core identity, Data encoding, decoding, and representation bridges in
Nash using concrete builtins and existing Data patterns.

Data builtins now preserve nominal Int, Bytes, List, and Map types. Core fromData
reconstructs and validates nested values; shallow unchecked reinterpretation is
no longer available. Existing Data wire formats are preserved.
