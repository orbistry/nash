---
cargo/nash-ast: minor
cargo/nash-can: minor
cargo/nash-solve: patch
---

Expose the real bool and Data constructors through the synthetic Builtin
interface. Recognize bool patterns by the exact core identity and count their
imports correctly, removing the old Basics.Bool special case.
