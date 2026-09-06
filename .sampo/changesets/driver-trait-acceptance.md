---
cargo/nash-driver: patch
---

Verify direct and transitive trait impl resolution through the driver, including
impls owned by a type's module and orphan and overlap diagnostics. Correct the
driver documentation to describe sequential compilation after source fetching.
