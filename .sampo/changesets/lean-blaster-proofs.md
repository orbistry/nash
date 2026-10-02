---
cargo/nash-source: minor
cargo/nash-ast: minor
cargo/nash-parse: minor
cargo/nash-can: minor
cargo/nash-constrain: minor
cargo/nash-solve: minor
cargo/nash-nitpick: patch
cargo/nash-report: minor
cargo/nash-fmt: minor
cargo/nash-codegen: minor
cargo/nash-driver: minor
cargo/nash-cli: minor
cargo/nash-proof: minor
---

Add module-local proof blocks and a Lean-blaster proof command over compiled UPLC,
with symbolic primitive and Cardano ledger domains, portable pinned Lean projects,
structured results, explicit SMT trust, and CEK execution limits with exhaustion treated as rejection.

Support partial correctness with `Proof.returns`, separate postcondition evaluation
limits, and explicit inconclusive checker exhaustion.

Represent proofs with dedicated source and canonical AST types, symbolic-domain
binders, and distinct execution and successful-return obligations. Reject proof
budgets during parsing and report proof-specific domain, expectation and
postcondition errors before code generation.
