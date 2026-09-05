---
cargo/nash-constrain: patch
cargo/nash-solve: minor
---

Reduce inferred trait contexts and assign retained requirements to their
definition's evidence slots. Keep recursive argument and result variables in
the group scope so all members retain the same type relationships and context.
