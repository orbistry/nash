# Implementation checklist

## Phase 0: frontend seam

- [ ] Add `nash-frontend`.
- [ ] Add `nash-frontend-nash`.
- [ ] Register one shared frontend registry for graph building, compilation, CLI, and LSP.
- [ ] Replace direct parser use in graph construction with `Frontend::inspect`.
- [ ] Retain inspection failures as module-local diagnostics; do not stop independent graph nodes.
- [ ] Replace direct parser use in `compile_module` with `Frontend::parse`.
- [ ] Add `ModuleCatalog` and remove `.nash` suffix inference.
- [ ] Convert frontend diagnostics in `nash-driver`; keep `nash-report` unchanged.
- [ ] Keep the current build-wide arena for source, canonical nodes, evidence, and interfaces.

## Phase 1: Aiken dependency and common syntax

- [ ] Resolve the Rust 1.92.0 versus 1.94.1 toolchain mismatch.
- [ ] Add exact-pinned `aiken-lang` behind the adapter crate.
- [ ] Add Aiken dependency inspection.
- [ ] Add Aiken import lowering and module mapping.
- [ ] Add type annotation lowering and primitive representation mapping.
- [ ] Add pattern lowering.
- [ ] Add integer range diagnostics or change Nash's literal representation.
- [ ] Add expression lowering.
- [ ] Add functions and constants.
- [ ] Add undecorated data types and aliases.
- [ ] Add public export generation.
- [ ] Preserve source regions in all lowered nodes.
- [ ] Run `nash check` on `.ak` fixtures.

## Phase 2: validators

- [ ] Define the accepted Aiken handler set.
- [ ] Synthesize purpose dispatch.
- [ ] Turn a handler result of `False` into explicit Nash failure.
- [ ] Insert boundary conversion and validation.
- [ ] Reject unsupported handler combinations with precise diagnostics.
- [ ] Compare CEK results and traces with the official Aiken compiler.

## Phase 3: dependency cleanup

- [ ] Propose an upstream parser-only `aiken-syntax` crate.
- [ ] Replace `aiken-lang` when the parser-only crate is available.
- [ ] Keep one Aiken AST version behind `nash-frontend-aiken`.
- [ ] Add an upstream sync policy and compatibility test matrix.

## Acceptance tests

Each implemented lowering function needs:

- One valid Aiken fixture.
- One equivalent native Nash fixture.
- One invalid or unsupported fixture with a stable diagnostic code.
- One source-region assertion.

Validator and data-layout features also need official Aiken differential tests.
