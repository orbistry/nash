---
cargo/nash-solve: patch
---

Reject value uses that narrow an enclosing annotation's kind signature, including captures used by nested helpers. Preserve the shared kind relationships across the full enclosing annotation scope.
