---
cargo/nash-codegen: patch
cargo/nash-plutus: patch
---

Run constant folding, unused-parameter removal and cleanup until unchanged without optimizer resource or output-growth limits. Evaluate all pure representable builtin calls directly and return errors instead of panicking for out-of-range constant indices and constructor tags. Correct whole-byte left shifts and handle arbitrary-size shift, rotate and list-drop inputs.
