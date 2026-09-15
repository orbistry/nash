---
cargo/nash-source: minor
cargo/nash-ast: minor
cargo/nash-can: minor
cargo/nash-solve: minor
cargo/nash-ir: minor
cargo/nash-codegen: minor
cargo/nash-nitpick: patch
cargo/nash-frontend-aiken: minor
cargo/nash-frontend: minor
cargo/nash-frontend-nash: patch
cargo/nash-project-aiken: minor
cargo/nash-driver: minor
cargo/nash-cli: minor
cargo/nash-language-server: patch
cargo/nash-constrain: minor
cargo/nash-report: patch
cargo/nash-plutus: patch
---

Add explicit qualified Data layouts, encoded containers and checked conversions through the Nash pipeline. Preserve Aiken 1.1.23 default/custom/list constructor encoding, opaque single-field transparency, nested strict expect validation and arbitrary-precision fixed literals and patterns. Support all six validator purposes and raw-context fallback with shallow parameters, on-demand optional datum views and Boolean-to-unit/failure behavior. Keep native Nash representations unchanged. Correct negative CBOR bignum encoding/decoding and preserve bounded specialization failure without host-stack overflow. Use the pinned official compiler only as a test oracle, never as the production semantic or code-generation path.

Repair empty polymorphic list/Option encoding with operation-driven specialization demands, share source-site-aware assignment ascriptions between locals and constants, and reject incompatible named function return annotations during checking. Preserve independently verified lambda result types and pinned greater-than operand/trace order.

Extend the source path with grouped functions, resolved call labels, type holes, hygienic nested pipelines, record updates, solved record/module-name selection, encoded projections, polymorphic equality, trace formatting, the pinned compiler prelude and builtin surface, and checked test/benchmark declarations that are not production roots. Reject private type leaks under Aiken's export rules while keeping native literal methods, exports and record-update rules unchanged.

Load Aiken manifests, locked local packages, independent workspace members, environments and synthetic configuration through Nash-owned shared project contracts. Carry package-qualified compiler identities and owned solved boundary/layout metadata into separately named validator artifacts. Preserve located CLI/editor project errors, cache fallback warnings and untracked package sources. Blueprint serialization and execution/tooling commands remain excluded.

Repair nested Data constant Flat serialization and split oversized recursive inference/codegen frames exposed by the unchanged standard library and prelude. Honor pinned type/constructor decorator precedence and the prelude's observable tautology result discrepancy.
