---
cargo/nash-ast: minor
cargo/nash-can: minor
cargo/nash-constrain: patch
cargo/nash-solve: minor
---

Match recursive impl patterns consistently during inference, ground resolution
and superclass checks. Preserve repeated variables and full impl identity,
reject structural overlaps, and retain nested patterns in diagnostics.
