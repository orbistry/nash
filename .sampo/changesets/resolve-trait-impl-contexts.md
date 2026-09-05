---
cargo/nash-constrain: minor
cargo/nash-solve: minor
---

Resolve known trait heads through coherent impls and their contexts. Preserve
impl substitutions and child evidence, report missing impls at the original
call, and stop expanding contexts with a diagnostic.
