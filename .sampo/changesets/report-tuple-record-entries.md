---
cargo/nash-report: minor
---

Break a tuple or record type that does not fit the line with one entry per line, as Elm does. The entries were joined without break points, so only the last entries broke and the type drifted to the right. Add `Doc::pcat`, Elm's `D.cat`.
