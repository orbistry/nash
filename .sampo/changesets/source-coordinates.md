---
cargo/nash-region: minor
cargo/nash-source: minor
cargo/nash-parse: minor
cargo/nash-can: patch
cargo/nash-report: minor
cargo/nash-language-server: patch
---

Use source-sized coordinates and diagnostic widths throughout parsing and reporting. Check LSP coordinate conversion instead of truncating. Reject oversized Unicode escapes without integer overflow, and make arbitrary lookahead offsets safe.
