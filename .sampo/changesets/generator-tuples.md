---
cargo/nash-driver: minor
cargo/nash-codegen: minor
cargo/nash-test: minor
cargo/nash-solve: patch
cargo/nash-plutus: patch
---

Use a function alias for generators, returning value and next PRNG state per draw.
Remove the redundant replay count. Prepare properties once and retain their
state, body function, and deferred display function without rerunning generators.
Preserve function aliases in typed definitions and captured variables inside
native constructor and case nodes when returning CEK closures.
