---
cargo/nash-ir: patch
cargo/nash-codegen: patch
---

Enable bounded constant builtin evaluation in O1. Fold saturated literal calls through ANF bindings and repeat cleanup, preserving traces and runtime failures. Keep O0 and explicit comptime policies unchanged.
