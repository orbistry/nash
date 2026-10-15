---
cargo/nash-ast: minor
cargo/nash-solve: minor
cargo/nash-codegen: minor
---

Resolve superclass evidence when a type variable stays open. A phantom parameter that no use fixes, as in `wrapper : tag 'p -> bool; wrapper t = same t t` with `same : Ord 'a => 'a -> 'a -> bool`, passed type checking but stopped `nash test` with "superclass resolution requires a ground type". `nash_solve::evidence::resolve` now keeps open type variables. An open variable equals only itself. An impl that matches whatever the variable is proves the predicate, and resolution reports `NonGround` only when no impl does and impl selection, a representation test, or the requirements of a type need the variable. The new `nash_ast::head::Open` matches impl heads against open types. Under `SelectPolicy::Exact`, `select_impl` now returns `Deferred` when no impl is selected and a candidate is undecided. `nash_codegen::evidence::Error::NonGround` is removed.
