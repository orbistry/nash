---
cargo/nash-source: minor
cargo/nash-ast: minor
cargo/nash-parse: minor
cargo/nash-can: minor
cargo/nash-constrain: minor
cargo/nash-solve: minor
cargo/nash-driver: patch
cargo/nash-cli: patch
---

Replace row polymorphism with nominal record aliases. Resolve literals by
visible field sets, preserve declaration-order metadata, and resolve record
operations before generalization. Support qualified lowercase type names and
alias constructor functions while preserving trait-based literals and
representation predicates.
Use the complete primitive type inventory under the Builtin qualifier and
represent unit uniformly as a named builtin type throughout inference and
instance selection.
Preserve labeled constructor metadata, support construction and pattern sugar,
and permit field projection only through visible single-constructor unions.
Keep parenthesized record literals as positional constructor arguments.
