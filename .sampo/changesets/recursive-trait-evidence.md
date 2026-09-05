---
cargo/nash-constrain: patch
cargo/nash-solve: patch
---

Preserve definition identities on untyped recursive headers and fill their
calls' evidence slots once the group's trait context is known. Keep each
definition's identity separate from the group's evidence binder.
