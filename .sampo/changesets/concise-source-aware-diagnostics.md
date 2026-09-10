---
cargo/nash-parse: minor
cargo/nash-constrain: minor
cargo/nash-solve: patch
cargo/nash-report: minor
cargo/nash-language-server: minor
cargo/nash-cli: minor
---

Use concise diagnostics with full expected/actual type comparisons, expectation-origin labels, and stable codes independent of display titles. Retain parser opening positions for closing-delimiter reports. Support arbitrary secondary labels and related reports across source files.

Extend diagnostic JSON with code, severity, labels, suggestions, and related reports. JSON messages now contain styled prose without embedded source drawings; consumers should render the structured labels. LSP diagnostic codes now use stable identifiers instead of titles and include secondary and related source locations.
