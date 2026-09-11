# nash-language-server

## 0.4.0 — 2026-09-10

### Minor changes

- [e3e72a7](https://github.com/orbistry/nash/commit/e3e72a7432f96f2de01777edd6c5a935e88d48fb) Use concise diagnostics with full expected/actual type comparisons, expectation-origin labels, and stable codes independent of display titles. Retain parser opening positions for closing-delimiter reports. Support arbitrary secondary labels and related reports across source files.
  
  Extend diagnostic JSON with code, severity, labels, suggestions, and related reports. JSON messages now contain styled prose without embedded source drawings; consumers should render the structured labels. LSP diagnostic codes now use stable identifiers instead of titles and include secondary and related source locations. — Thanks @MicroProofs!

### Patch changes

- [8bb97f6](https://github.com/orbistry/nash/commit/8bb97f680047cc8818b890af407247cdd585000e) Use source-sized coordinates and diagnostic widths throughout parsing and reporting. Check LSP coordinate conversion instead of truncating. Reject oversized Unicode escapes without integer overflow, and make arbitrary lookahead offsets safe. — Thanks @MicroProofs!
- Updated dependencies: nash-driver@0.6.0, nash-region@0.3.0, nash-report@0.3.0

## 0.3.0 — 2026-09-10

### Minor changes

- [b7ff823](https://github.com/orbistry/nash/commit/b7ff823b144beb39d5cc355052710b7032f0509e) Collect independent compiler errors with dependency-aware recovery, retain failed module dependencies, and render owned diagnostics in the terminal, JSON, and language server. Preserve trait-method call names in error context. Add JSON and warning controls to `nash check`, and publish diagnostics for unsaved editor buffers with UTF-16 ranges. — Thanks @MicroProofs!

### Patch changes

- Updated dependencies: nash-driver@0.5.0, nash-report@0.2.0

## 0.2.0 — 2026-03-10

### Minor changes

- [45b79ae](https://github.com/utxo-company/nash/commit/45b79ae1c94701b4f16a612e381a57c986b099e0) Add the minimal Nash language server skeleton and CLI entry point for running it over stdio.
  
  Includes commit `f2b7657c1fa70a22dc2ac450ec54cc0623bd2f3f` and the follow-up cleanup to the initial LSP scaffolding. — Thanks @MicroProofs!

