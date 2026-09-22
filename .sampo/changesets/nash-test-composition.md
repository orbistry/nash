---
cargo/nash-driver: minor
cargo/nash-codegen: patch
cargo/nash-parse: patch
---

Move property sampling, rejection, tuple preparation and deferred body/display
functions into Nash Test helpers. Keep codegen responsible for source callbacks
and pattern binding. Compose assertion capture traces through Nash helpers while
preserving failure-only evaluation and compiler trace settings. Base Test uses
Builtin.trace directly. Allow keyword-named qualified references in source,
removing the compiler special case for Test-module traces.
