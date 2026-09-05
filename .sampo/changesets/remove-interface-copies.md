---
cargo/nash-can: minor
---

Remove the superseded interface deep-copy API. Compiled interfaces now borrow the retained build arena, preserving original type and evidence identities.
