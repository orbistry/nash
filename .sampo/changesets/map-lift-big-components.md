---
cargo/nash-driver: patch
---

Make the bundled map Lift instance explicitly require Big keys and values. Add regression coverage for rejecting native pair components and preserving already encoded map entries through lift and lower.
