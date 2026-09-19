---
cargo/nash-codegen: patch
---

Lower mutually recursive functions using native constructor packets and case dispatch instead of selector lambdas. Preserve mixed arities, partial applications, lexical captures and lazy branch selection.
