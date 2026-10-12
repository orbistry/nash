---
cargo/nash-ast: minor
---

Add `select_impl` with `SelectPolicy` and `ImplSelection`, the one impl selection used by inference, superclass entailment, and evidence. Every candidate is matched, and a second match stops the compiler as a coherence bug instead of picking one.
