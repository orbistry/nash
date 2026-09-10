---
cargo/nash-can: minor
---

Borrow module data and local binding maps during canonicalization instead of cloning the full environment at each scope. Preserve shadowing, diagnostics, and error recovery.
