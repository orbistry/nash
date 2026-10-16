---
cargo/nash-ast: minor
cargo/nash-solve: minor
cargo/nash-codegen: minor
---

Add the representation coverage rule. When a rigid type variable keeps impl selection from deciding, because the impls are split by its representation class (for example `Eq (list ('a : Big))` and `Eq 'a => Eq (list ('a : Little))`), inference accepts the predicate if the annotation states the trait at that variable and every admitted representation selects an impl whose context holds. `same : Eq 'a => list 'a -> list 'a -> bool` now compiles. The evidence is the new `Evidence::Specialize`; code generation selects the impl for each use and keeps the selecting types in the specialization key. A missing element constraint is now reported as a missing constraint, such as `Eq 'a`, instead of a missing impl for the container. Superclass proofs do not use the rule: an impl names a split requirement in its context. nash-ast also gains `head::cover_impls` and `ReprTrait::exactly`.
