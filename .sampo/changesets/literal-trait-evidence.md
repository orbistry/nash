---
cargo/nash-ast: minor
cargo/nash-can: minor
cargo/nash-constrain: minor
cargo/nash-solve: patch
---

Constrain integer, string and bytes literals through their core literal traits, retaining ordered literal and Eq evidence for patterns. Canonicalize bytes literals and preserve qualified polymorphic schemes for let-destructuring at the original pattern node.
