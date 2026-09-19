---
cargo/nash-codegen: patch
---

Lower boolean and list matches directly to native case without an extra scrutinee binding. Dispatch Data shapes directly with lazy chooseData branches, removing the manufactured integer tag and second dispatch. Destructure unConstrData pairs with native case.
