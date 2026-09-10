# nash-language-server

## 0.3.0 — 2026-09-10

### Minor changes

- [b7ff823](https://github.com/orbistry/nash/commit/b7ff823b144beb39d5cc355052710b7032f0509e) Collect independent compiler errors with dependency-aware recovery, retain failed module dependencies, and render owned diagnostics in the terminal, JSON, and language server. Preserve trait-method call names in error context. Add JSON and warning controls to `nash check`, and publish diagnostics for unsaved editor buffers with UTF-16 ranges. — Thanks @MicroProofs!

### Patch changes

- Updated dependencies: nash-driver@0.5.0, nash-report@0.2.0

## 0.2.0 — 2026-03-10

### Minor changes

- [45b79ae](https://github.com/utxo-company/nash/commit/45b79ae1c94701b4f16a612e381a57c986b099e0) Add the minimal Nash language server skeleton and CLI entry point for running it over stdio.
  
  Includes commit `f2b7657c1fa70a22dc2ac450ec54cc0623bd2f3f` and the follow-up cleanup to the initial LSP scaffolding. — Thanks @MicroProofs!

