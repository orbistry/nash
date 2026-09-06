---
cargo/nash-ast: minor
cargo/nash-can: minor
cargo/nash-solve: minor
---

Compare recursive impl-pattern equations with fresh kind binders through the
shared kind engine, preserving inner bounds and bounded pattern traversal.
Prove candidate bounds during inference, superclass checks and ground evidence
resolution without narrowing callers. Resolve ground evidence with an explicit
work stack so expanding contexts reach the resolution limit safely.
