---
cargo/nash-frontend: minor
cargo/nash-frontend-nash: minor
cargo/nash-frontend-aiken: minor
cargo/nash-source: minor
cargo/nash-ast: minor
cargo/nash-can: minor
cargo/nash-solve: minor
cargo/nash-nitpick: patch
cargo/nash-codegen: minor
cargo/nash-driver: minor
cargo/nash-cli: minor
cargo/nash-language-server: minor
---

Add a parser-neutral frontend boundary and an exact-pinned official Aiken parser adapter. Discover and check `.ak` modules through Nash canonicalization, type inference, exhaustiveness and interfaces, including mixed-source graphs and CLI/LSP diagnostics. Lower fixed primitives through Core/UPLC without changing native literal overloading. Build bounded Aiken mint/fallback validators with checked primitive boundaries, purpose dispatch and Boolean-to-failure semantics; compare results and traces against pinned official Aiken execution. Keep other handler sets and custom boundary layouts explicitly unsupported. Replace URI-only module origins with source-root-aware catalogs while retaining the backend finish callback, reject ambiguous identities, and select Rust 1.94.1 for the parser dependency.
