---
cargo/nash-ir: patch
cargo/nash-codegen: patch
---

Cancel proven integer, byte, list, map and UTF-8 representation round trips, including let-bound operands and constructor Data projections/reconstruction, while preserving validation, traces and evaluation order. Run cancellation in the O1 cleanup loop as well as before normalization.
