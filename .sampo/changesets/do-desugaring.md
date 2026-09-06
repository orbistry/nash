---
cargo/nash-ast: minor
cargo/nash-can: minor
cargo/nash-solve: patch
---

Desugar do statements through the checked core Monad.bind method with sequential pattern and let scope. Reject refutable bind patterns and missing core Monad declarations. Verify inferred constraints and evidence against explicit nested bind calls.
