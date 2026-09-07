# Plan 02: Kind inference

## Goal

Add the kind system of [docs/kinds.md](../docs/kinds.md) to the front end:
`Big` / `Const` / `Term` base kinds, kind arrows, bounded kind variables,
Haskell98-style inference over type declaration SCCs, the casing rule,
optional user kind annotations, kind checking of value annotations, kinds
in interfaces, and Elm-quality error data.

Implement chunks 1–7 in order, using `docs/overview.md` and `docs/kinds.md`
as the design authority and the current checkout as the implementation
reference. Complete the plan 01 nested-section regression fix and its
verification first. The prerequisite fix is now verified.

### Testing and completion contract

1. Before each behavior change, write a failing test for the intended
   behavior. Use direct assertions for kind-set and inference-engine
   invariants, and separate success/error snapshot macros for canonical
   kinds and diagnostics. Cover bounds, occurs checks, fresh
   instantiation, SCC inference, casing, records, higher-kinded parameters,
   and user annotations, including error regions and contexts.
2. Add source-level acceptance tests as each compiler path becomes usable.
   Parse full Nash modules through the real canonicalization/kind pass;
   assert the resulting kind schemes or specific kind errors. Include
   positive and negative cases for `Big` fields, `Storable` containers,
   little aliases, recursive groups, and top-level and let annotations.
   A parse error, unrelated arity error, or `Unsupported` result does not
   establish that kind checking rejected the input correctly.
3. Extend the existing `nash-driver/src/compile.rs` in-memory build tests
   for cross-module acceptance: export and import kinds, accept valid
   imported aliases, and reject invalid uses with a kind error. Check
   interface deep-copy and serialization round trips, and prove that a
   kind or bound change changes the interface fingerprint. Use explicit
   `Builtin` interfaces/imports until the implicit prelude exists.
4. Test `'f 'a` at the canonicalization/kind boundary. Full higher-kinded
   value unification and kind predicates at value instantiation remain
   plan 03 work. Acceptance tests here are Rust compiler integration tests;
   Nash `tests` block execution, UPLC behavior, and `nash test` belong to
   later plans.
5. Run focused tests while implementing. Review every new or changed
   snapshot before accepting it with `cargo insta accept`; rerun the
   affected tests afterward. At each completed chunk run
   `cargo fmt --all`,
   `cargo clippy --all-targets --all-features -- -D warnings`,
   `cargo insta test`, and `cargo test`. At completion also run
   `cargo insta test --unreferenced reject` and resolve all pending or stale
   snapshots.
6. Use `jj status` and `jj diff` to inspect and preserve work. Keep each
   verified implementation chunk in a separate described `jj` change,
   then use `jj new` for the next chunk. Do not discard existing changes
   or rewrite unrelated history. Record completed chunks in this plan,
   audit the per-chunk changesets, and tick SPEC only after all acceptance
   criteria and workspace checks pass. Report test results and any
   remaining limitation explicitly.

Resolve stale sketches against the design before coding. In particular,
the recursive-group test must itself obey the field casing rules, and a
serialized interface schema change needs a round-trip test. Cache files are
disposable; support only the current format.

## Progress

- [x] Prerequisite: nested-section fix, regression snapshots, inference acceptance test.
- [x] Chunk 1: kind vocabulary. Tests failed before implementation; bounds and result-kind tests pass. Formatting, strict Clippy, snapshot tests, and workspace tests pass.
- [x] Chunk 2: kind inference engine. Seven tests cover bounds, links, occurs checks, arrows, sharing, and fresh instantiation; tests failed before implementation. Formatting, strict Clippy, snapshot tests, and workspace tests pass.
- [x] Chunk 3: builtin kinds and environment. Tests verify all 17 primitive names, arities, representation bounds, and seeded lookup. Tests failed before implementation; formatting, strict Clippy, snapshot tests, and workspace tests pass.
- [x] Chunk 4: declaration inference. Source acceptance tests cover casing, SCCs, bounds, free application heads, alias substitution, and error recovery. Kind schemes also cross canonical interfaces so imported declarations can be checked. Formatting, strict Clippy, snapshot tests, and workspace tests pass.
- [x] Chunk 5: interfaces and value annotations. Checks retain original annotations across alias expansion and visit nested lets. Tests verify copied interfaces after source-arena drop, real cross-module builds, kind/bound fingerprint changes, current-format cache round trips. Formatting, strict Clippy, snapshot tests, and workspace tests pass.
- [x] Chunk 6: user parameter annotations. Tests cover Fix, Storable, base/arrow mismatches at annotation regions, separate bounded occurrences, narrowed alias applications, and constraints from all recursive-group uses. Formatting, strict Clippy, snapshot tests, workspace tests, and snapshot hygiene pass.
- [x] Chunk 7: changeset, final acceptance audit, and SPEC. Final record-field error snapshots check context and source regions; an explicit Term annotation has a success snapshot. All final gates pass.

### Follow-up: retained constructor settlement termination

Historical implementation record. The coinductive rules below are superseded
by [inductive residual obligations](02-kind-obligations.md), which rejects
required self/self cycles and adopts a finite expansion fragment with separate
restriction and limit diagnostics.

- [x] Close the retained-obligation self-application hang and verify acceptance.

Use coinductive known-head application memoization within each settlement.
Compare the constructor scheme and captured arguments through `same_kind`,
plus the supplied argument; unify repeated results. Keep constructor schemes
and independent Big/Const/Term bounds. Protect direct coherence settlement as
well as settlement entered through `unify`, and discard queued replay work
when settlement fails so it cannot produce a second error in another context.

The bounded probes do not support replacing this with a scheme-only occurs
check: `app (app tag) tag` is finite and legal, while `g g` for
`type g 'f = G ('f ('f 'f))` already reaches `KindMismatch` with the memo.
No production step budget or scheme-recursion restriction is introduced.
Elm's `Type/Unify.hs` and `Type/Occurs.hs` remain references for structural
unification and occurs checks; retained constructor obligations are Nash's
additional machinery.

Before the fix, the engine regression exhausted a test-only 256-pass limit.
The declaration, value annotation, impl head, and imported driver regressions
each timed out after three seconds in separately bounded test processes.
`self (self tag)` already rejected with a kind mismatch; it is a preservation
check, not evidence of the original hang.

Verification (2026-09-07):

- Ten new regression tests: three engine invariants, six canonicalization
  snapshots, and one driver test with two imported/local variants. All six
  new snapshots were reviewed before acceptance; no existing snapshot changed.
- `cargo fmt --all`, strict Clippy (`--all-targets --all-features -- -D warnings`),
  `cargo insta test`, `cargo insta test --unreferenced reject`, and `cargo test`
  pass. Each full test run reports 1,947 passed, zero failed, three ignored.
  Snapshot hygiene finds no pending or unreferenced snapshots.
- Real CLI acceptance passes for `pass/abstract-self-application`,
  `pass/retained-self-application` (two modules, declarations, annotations,
  impl heads, and finite nested applications), and `pass/core-do` (including
  Functor mapping from `option unit` to `option (unit, unit)`).
  `fail/infinite-kind/infinite` exits 1 with `KindInfinite`, verifying the
  diagnostic example below and in the design doc.
- Seventeen additional isolated CLI probes terminate with their expected
  outcomes. These include self and mutual/partial application, `g g`, alias
  self-application, and Storable rejection through abstraction, annotations,
  aliases, and partial application. The `g g` snapshot now accumulates an
  independent alias-casing error without replaying the failed obligation in
  the enclosing field's context.
- Sampo 0.21.0 `release --dry-run` passes in an isolated checkout on `main`
  (Sampo rejects the original detached Git HEAD). After materializing the
  release only in that checkout, `publish --dry-run --cargo-args=--allow-dirty`
  preserves the order source -> ast -> parse -> can -> constrain -> solve ->
  driver -> cli and packages/verifies nash-source 0.5.0. It then stops at
  nash-ast because nash-source 0.5.0 is not yet on crates.io. Remaining package
  publish verification is therefore incomplete; nothing was uploaded.
- Read-only implementation and coverage reviews found no actionable issues.

Limits: this memo resolves repeated known-head obligations; it is not a
production resource budget or a proof of termination for every possible
inference graph. These are front-end checks, not runtime-encoding evidence.
Plan 04 and the deferred exhaustiveness, rendering, and codegen work remain
unchanged. The Sampo changeset is `kind-settle-termination.md` (nash-can patch).

### Pair kind correction

The builtin `pair` has two independent Storable component bounds. This admits
`unConstrData : Data -> pair int (list Data)` and the polymorphic `fstPair` and
`sndPair` signatures. Only the builtin API restricts construction through
`mkPairData : Data -> Data -> pair Data Data`; the kind does not encode that API
restriction. Snapshots cover Big/Const combinations, Term rejection at either
argument, and all four signatures. No plan 01 grammar change is needed.
The correction has its own Sampo changeset: nash-ast minor and nash-can patch.

### Test audit

Removed 14 redundant tests and nine snapshots: six parser cases already
covered by equivalent or nested syntax, three repeated canonicalizer/kind
cases, and five basic driver checks covered by stronger retained tests.
The retained cache test now checks behavior after the underlying source changes;
the graph tests check exact dependency levels, ordering, and cycle diagnostics.
The 994 Plutus conformance cases remain intact. No runtime code changed.

### Final verification

- `cargo fmt --all`: pass.
- `cargo clippy --all-targets --all-features -- -D warnings`: pass.
- `cargo insta test`: pass; new snapshots reviewed before acceptance.
- `cargo test`: 1,764 passed, 0 failed, 3 ignored.
- `cargo insta test --unreferenced reject`: pass; no unreferenced or pending snapshots.
- The 50 source-level kind acceptance tests cover declarations, recursive groups,
  annotations, record fields, imported contracts, and copied interfaces. Driver
  tests cover cross-module builds, kind and bound fingerprints, and cache round trips. Solver tests retain the nested-section
  regression, verify that Builtin.List annotations match list literals and
  patterns, and explicitly reject unsupported higher-kinded value applications.
- Read-only agent audits found no blocking issue. Each implementation chunk has
  its own verified, described jj change. The six per-chunk changesets cover all five changed
  compiler crates and are included in their matching implementation changes.

Higher-kinded value unification and value-use kind predicates remain in plan 03.
Interface fingerprints are produced and tested; this plan does not introduce a
new incremental build engine.

## Prerequisites

- plans/01 (syntax): `'a` type variables, lowercase type names in
  `type_named`, `nash_source::Type::VarApp { name, args }` for `'f 'a`, and
  `nash_source::TypeParam { name, kind }` with the `kind` annotation grammar
  from docs/kinds.md. Chunks 1 to 5 only need `'a` and lowercase names;
  chunk 6 needs `TypeParam`; chunk 4 handles `VarApp` if it exists.

## Crates touched

`nash-ast`, `nash-can`, `nash-driver`. `nash-constrain` and `nash-solve`
receive an explicit diagnostic for unsupported type applications;
higher-kinded unification and kind predicates on value schemes belong to
plans/03 (traits).

## Reference

- Elm has no kinds. The declaration walk is `Canonicalize/Environment/Local.hs`
  (`addTypes`, `addAliases`, `addAlias`, `canonicalizeUnion`,
  `canonicalizeAlias`, `checkUnionFreeVars`) and its port in
  `crates/nash-can/src/module.rs:326-567` and
  `crates/nash-can/src/environment/local.rs`.
- Unification structure mirrors `crates/nash-solve/src/unify.rs`
  (`guarded_unify`, `merge`) in miniature.
- Aiken has no kinds either; its `crates/aiken-lang/src/tipo/environment.rs`
  `register_types` is the analogue of the declaration pass.
- Haskell 98 Report section 4.6 (kind inference) is the algorithm.

## Implementation decisions

- Typed definitions retain their original canonical annotation in addition
  to the split argument/result types used by the value solver. Checking a
  reconstructed function type would lose stricter alias parameter kinds.
  Both top-level and let annotations use the retained original tree.
- Driver results expose public interface metadata generated from actual
  canonical interfaces. Files directly serialize the current interface structure
  with bincode, without a version marker or format migration. Fingerprints
  include kind-variable bounds;
  this plan does not add a new persistent incremental build engine.

- Canonical `Type::App { head, args }` preserves source `VarApp` and permits
  substitution into both the head and arguments. Both solver conversion
  paths return `UnsupportedTypeApplication` when value inference needs
  plan 03 support; no application is erased or treated as a nominal type.
  Plan 03 now normalizes applications whose heads become known during
  substitution, including partial nominal aliases; unresolved heads retain
  `App` and still need its later solver support.
- Interface kind fields and deep copying landed with chunk 4, since even
  declaration-only inference must know the schemes of imported opaque
  types. Chunk 5 verifies their cross-module and serialization behavior.
- Preserve exact named-constructor arity checks before kind inference.
  `int Int` and unapplied `List` remain `BadArity`, as allowed by the
  Errors section of `docs/kinds.md`. A base-bounded variable application
  tests `KindTooManyArgs`; value-position tests must use a higher-kinded
  variable to reach kind checking. Named partial application remains plan 03.
- Allocate large `KindContext` error data in the arena. Keep the existing
  alias-only cycle check in addition to the combined kind SCC pass. Skip
  dependents of failed SCCs, continue independent groups, and report every
  alias result unification failure.
- Implicit List annotations, explicit Builtin imports, list literals, and list
  patterns all use the same nash/core Builtin.List identity. There is no
  alternate List.List kind registration or special import replacement.

## Design summary

- `nash-ast` gets `BaseKind`, `KindSet`, `Kind<'a>`, `KindScheme<'a>`.
  `Union` and `Alias` carry a `KindScheme`.
- `nash-can/src/kinds.rs` is a self-contained inference engine with its own
  union-find over kind variables, plus the declaration and annotation
  walkers. It runs after `canonicalize_aliases`/`canonicalize_unions` and
  before `add_ctors`.
- Kinds of imported types come from `Interface`. Kinds of builtins come from
  a compiler table in `crates/nash-ast/src/primitives.rs`, exported as
  `nash_ast::primitives::{PRIMITIVES, CORE, builtin_home}` and homed in
  the `nash/core` `Builtin` module.
- `types.rs` does not kind-check during canonicalization: local recursive
  groups have no kinds yet at that point. All checks live in the pass.

---

## Chunk 1: Kind types in `nash-ast`

**Files**: `crates/nash-ast/src/lib.rs`

**Change**: add the kind vocabulary. No existing struct changes yet.

**Code**:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BaseKind {
    Big,
    Const,
    Term,
}

/// The shapes a kind variable may take. Bit set over `BaseKind` plus arrow.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KindSet(u8);

impl KindSet {
    pub const BIG: KindSet = KindSet(0b0001);
    pub const CONST: KindSet = KindSet(0b0010);
    pub const TERM: KindSet = KindSet(0b0100);
    pub const ARROW: KindSet = KindSet(0b1000);
    pub const ANY: KindSet = KindSet(0b0111);
    pub const ALL: KindSet = KindSet(0b1111);
    pub const STORABLE: KindSet = KindSet(0b0011);
    pub const LITTLE: KindSet = KindSet(0b0110);

    pub fn of(base: BaseKind) -> KindSet {
        match base {
            BaseKind::Big => KindSet::BIG,
            BaseKind::Const => KindSet::CONST,
            BaseKind::Term => KindSet::TERM,
        }
    }

    pub fn contains(self, other: KindSet) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn intersect(self, other: KindSet) -> KindSet {
        KindSet(self.0 & other.0)
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// A kind after inference. `Var` indexes the enclosing `KindScheme`.
#[derive(Debug)]
pub enum Kind<'a> {
    Base(BaseKind),
    Var(u16),
    Arrow(&'a Kind<'a>, &'a Kind<'a>),
}

/// `forall k0 .. kn. kind`, with one bound per variable.
#[derive(Clone, Copy, Debug)]
pub struct KindScheme<'a> {
    pub bounds: &'a [KindSet],
    pub kind: &'a Kind<'a>,
}

impl<'a> KindScheme<'a> {
    pub fn mono(kind: &'a Kind<'a>) -> KindScheme<'a> {
        KindScheme { bounds: &[], kind }
    }

    /// The result kind after all parameters are applied.
    pub fn result(&self) -> &'a Kind<'a> {
        let mut kind = self.kind;
        while let Kind::Arrow(_, to) = kind {
            kind = to;
        }
        kind
    }
}
```

**Tests**: unit tests in `nash-ast` for `KindSet::intersect`/`contains`
and `KindScheme::result` on `Big -> Big -> Big`.

**Done when**: workspace compiles; nothing uses the types yet.

---

## Chunk 2: The kind inference engine

**Files**: new `crates/nash-can/src/kinds.rs`; `crates/nash-can/src/lib.rs`
(`pub mod kinds;`).

**Change**: an inference-time kind representation, a union-find over kind
variables, unification with bounds and occurs check, instantiation and
generalization. Pure; no AST walking yet.

**Code**:

```rust
//! Kind inference: Haskell98-style, over type declaration SCCs.
//! See docs/kinds.md.

use bumpalo::Bump;
use nash_ast::{BaseKind, Kind, KindScheme, KindSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KindVar(u32);

/// Inference-time kind. `Var` is a union-find index.
#[derive(Clone, Copy, Debug)]
pub enum K<'a> {
    Base(BaseKind),
    Var(KindVar),
    Arrow(&'a K<'a>, &'a K<'a>),
}

#[derive(Clone, Copy, Debug)]
enum Node<'a> {
    Unbound(KindSet),
    Bound(&'a K<'a>),
    Link(KindVar),
}

#[derive(Debug)]
pub enum Mismatch<'a> {
    /// Neither side is a variable and the shapes differ, or a bound excludes the binding.
    Shapes { expected: &'a K<'a>, actual: &'a K<'a> },
    Infinite(KindVar),
}

pub struct Infer<'a> {
    bump: &'a Bump,
    nodes: Vec<Node<'a>>,
}

impl<'a> Infer<'a> {
    pub fn new(bump: &'a Bump) -> Self {
        Infer { bump, nodes: Vec::new() }
    }

    pub fn fresh(&mut self, bound: KindSet) -> KindVar {
        self.nodes.push(Node::Unbound(bound));
        KindVar(self.nodes.len() as u32 - 1)
    }

    pub fn fresh_k(&mut self, bound: KindSet) -> &'a K<'a> {
        let var = self.fresh(bound);
        self.bump.alloc(K::Var(var))
    }

    fn find(&mut self, var: KindVar) -> KindVar {
        match self.nodes[var.0 as usize] {
            Node::Link(next) => {
                let root = self.find(next);
                self.nodes[var.0 as usize] = Node::Link(root);
                root
            }
            _ => var,
        }
    }

    /// Resolve one level: a bound variable becomes its binding.
    fn head(&mut self, kind: &'a K<'a>) -> &'a K<'a> {
        match kind {
            K::Var(var) => {
                let root = self.find(*var);
                match self.nodes[root.0 as usize] {
                    Node::Bound(bound) => self.head(bound),
                    _ => self.bump.alloc(K::Var(root)),
                }
            }
            _ => kind,
        }
    }

    pub fn unify(&mut self, expected: &'a K<'a>, actual: &'a K<'a>) -> Result<(), Mismatch<'a>> {
        let expected = self.head(expected);
        let actual = self.head(actual);
        match (expected, actual) {
            (K::Var(a), K::Var(b)) if a == b => Ok(()),
            (K::Var(a), K::Var(b)) => {
                let (Node::Unbound(sa), Node::Unbound(sb)) =
                    (self.nodes[a.0 as usize], self.nodes[b.0 as usize])
                else {
                    unreachable!("head returns unbound roots")
                };
                let joined = sa.intersect(sb);
                if joined.is_empty() {
                    return Err(Mismatch::Shapes { expected, actual });
                }
                self.nodes[a.0 as usize] = Node::Link(*b);
                self.nodes[b.0 as usize] = Node::Unbound(joined);
                Ok(())
            }
            (K::Var(var), other) | (other, K::Var(var)) => self.bind(*var, other, expected, actual),
            (K::Base(a), K::Base(b)) if a == b => Ok(()),
            (K::Arrow(a1, r1), K::Arrow(a2, r2)) => {
                self.unify(a1, a2)?;
                self.unify(r1, r2)
            }
            _ => Err(Mismatch::Shapes { expected, actual }),
        }
    }

    fn bind(
        &mut self,
        var: KindVar,
        kind: &'a K<'a>,
        expected: &'a K<'a>,
        actual: &'a K<'a>,
    ) -> Result<(), Mismatch<'a>> {
        let Node::Unbound(bound) = self.nodes[var.0 as usize] else {
            unreachable!("head returns unbound roots")
        };
        let allowed = match kind {
            K::Base(base) => bound.contains(KindSet::of(*base)),
            K::Arrow(..) => bound.contains(KindSet::ARROW),
            K::Var(_) => unreachable!("var/var handled by unify"),
        };
        if !allowed {
            return Err(Mismatch::Shapes { expected, actual });
        }
        if self.occurs(var, kind) {
            return Err(Mismatch::Infinite(var));
        }
        self.nodes[var.0 as usize] = Node::Bound(kind);
        Ok(())
    }

    fn occurs(&mut self, var: KindVar, kind: &'a K<'a>) -> bool {
        match self.head(kind) {
            K::Var(other) => *other == var,
            K::Base(_) => false,
            K::Arrow(from, to) => self.occurs(var, from) || self.occurs(var, to),
        }
    }

    /// Apply `kind` to one argument: returns the parameter and result kinds.
    /// A variable head becomes a fresh arrow (its bound must allow arrows).
    pub fn apply(&mut self, kind: &'a K<'a>) -> Result<(&'a K<'a>, &'a K<'a>), Mismatch<'a>> {
        match self.head(kind) {
            K::Arrow(param, result) => Ok((param, result)),
            head => {
                let param = self.fresh_k(KindSet::ALL);
                let result = self.fresh_k(KindSet::ALL);
                let arrow = self.bump.alloc(K::Arrow(param, result));
                self.unify(arrow, head)?;
                Ok((param, result))
            }
        }
    }

    /// Instantiate a scheme with fresh variables carrying the scheme's bounds.
    pub fn instantiate(&mut self, scheme: &KindScheme<'_>) -> &'a K<'a> {
        let vars: Vec<KindVar> = scheme.bounds.iter().map(|b| self.fresh(*b)).collect();
        self.instantiate_help(scheme.kind, &vars)
    }

    fn instantiate_help(&mut self, kind: &Kind<'_>, vars: &[KindVar]) -> &'a K<'a> {
        match kind {
            Kind::Base(base) => self.bump.alloc(K::Base(*base)),
            Kind::Var(index) => self.bump.alloc(K::Var(vars[*index as usize])),
            Kind::Arrow(from, to) => {
                let from = self.instantiate_help(from, vars);
                let to = self.instantiate_help(to, vars);
                self.bump.alloc(K::Arrow(from, to))
            }
        }
    }

    /// Zonk and generalize: every unbound variable becomes a scheme variable.
    pub fn generalize(&mut self, kind: &'a K<'a>) -> KindScheme<'a> {
        let mut vars: Vec<(KindVar, KindSet)> = Vec::new();
        let kind = self.generalize_help(kind, &mut vars);
        KindScheme {
            bounds: self.bump.alloc_slice_fill_iter(vars.iter().map(|(_, b)| *b)),
            kind,
        }
    }

    fn generalize_help(&mut self, kind: &'a K<'a>, vars: &mut Vec<(KindVar, KindSet)>) -> &'a Kind<'a> {
        match self.head(kind) {
            K::Base(base) => self.bump.alloc(Kind::Base(*base)),
            K::Var(var) => {
                let index = match vars.iter().position(|(v, _)| v == var) {
                    Some(i) => i,
                    None => {
                        let Node::Unbound(bound) = self.nodes[var.0 as usize] else {
                            unreachable!("head returns unbound roots")
                        };
                        vars.push((*var, bound));
                        vars.len() - 1
                    }
                };
                self.bump.alloc(Kind::Var(index as u16))
            }
            K::Arrow(from, to) => {
                let from = self.generalize_help(from, vars);
                let to = self.generalize_help(to, vars);
                self.bump.alloc(Kind::Arrow(from, to))
            }
        }
    }

    /// Zonk without generalizing, for error reporting. Unbound vars keep their id.
    pub fn zonk(&mut self, kind: &'a K<'a>) -> &'a K<'a> {
        match self.head(kind) {
            K::Arrow(from, to) => {
                let from = self.zonk(from);
                let to = self.zonk(to);
                self.bump.alloc(K::Arrow(from, to))
            }
            head => head,
        }
    }
}
```

**Tests** (unit tests in `kinds.rs`):

- `unify_base_same`, `unify_base_differs` (`Big` vs `Term` is `Shapes`).
- `storable_var_rejects_term`: `fresh(STORABLE)` unified with `Term` fails;
  with `Big` and with `Const` succeeds.
- `var_var_intersects_bounds`: `STORABLE` with `LITTLE` gives a var that
  then only accepts `Const`.
- `any_var_rejects_arrow`, `all_var_accepts_arrow`.
- `occurs_check`: `k ~ k -> Big` is `Infinite`.
- `generalize_roundtrip`: `k1 -> k2 -> Big` generalizes to two bounds and
  `instantiate` gives fresh distinct vars.

**Done when**: engine tests pass; nothing else uses the module yet.

---

## Chunk 3: Builtin kinds and the kind environment

**Files**: new `crates/nash-ast/src/primitives.rs`; `crates/nash-ast/src/lib.rs`
(`pub mod primitives;`); `crates/nash-can/src/kinds.rs`.

**Change**: a compiler table of builtin type names with their kind schemes,
homed in the `nash/core` `Builtin` module, and a `KindEnv` keyed by
`QualifiedName` that later chunks fill from interfaces and local results.
The table lives in `nash-ast` because `nash-constrain` (plans/04 chunk C1)
needs the same inventory and has no dependency on `nash-can`.

**Code** (`nash-ast/src/primitives.rs`):

```rust
//! Compiler-known types of the `nash/core` `Builtin` module.

use crate::{BaseKind, Kind, KindScheme, KindSet, ModuleName, PackageName};

pub const CORE: PackageName<'static> = PackageName { author: "nash", project: "core" };

pub const fn builtin_home() -> ModuleName<'static> {
    ModuleName { package: Some(CORE), name: "Builtin" }
}

const BIG: &Kind<'static> = &Kind::Base(BaseKind::Big);
const CONST: &Kind<'static> = &Kind::Base(BaseKind::Const);
const K0: &Kind<'static> = &Kind::Var(0);

const BIG_TO_BIG: &Kind<'static> = &Kind::Arrow(BIG, BIG);
const BIG2_TO_BIG: &Kind<'static> = &Kind::Arrow(BIG, BIG_TO_BIG);
const STORABLE_TO_CONST: &Kind<'static> = &Kind::Arrow(K0, CONST);
const K1: &Kind<'static> = &Kind::Var(1);
const STORABLE2_TO_CONST: &Kind<'static> = &Kind::Arrow(K0, &Kind::Arrow(K1, CONST));

pub struct Primitive {
    pub name: &'static str,
    pub arity: usize,
    pub kind: KindScheme<'static>,
}

const fn mono(kind: &'static Kind<'static>) -> KindScheme<'static> {
    KindScheme { bounds: &[], kind }
}

pub const PRIMITIVES: &[Primitive] = &[
    Primitive { name: "Data", arity: 0, kind: mono(BIG) },
    Primitive { name: "Int", arity: 0, kind: mono(BIG) },
    Primitive { name: "Bytes", arity: 0, kind: mono(BIG) },
    Primitive { name: "List", arity: 1, kind: mono(BIG_TO_BIG) },
    Primitive { name: "Map", arity: 2, kind: mono(BIG2_TO_BIG) },
    Primitive { name: "int", arity: 0, kind: mono(CONST) },
    Primitive { name: "bytes", arity: 0, kind: mono(CONST) },
    Primitive { name: "string", arity: 0, kind: mono(CONST) },
    Primitive { name: "bool", arity: 0, kind: mono(CONST) },
    Primitive { name: "unit", arity: 0, kind: mono(CONST) },
    Primitive { name: "bls_g1", arity: 0, kind: mono(CONST) },
    Primitive { name: "bls_g2", arity: 0, kind: mono(CONST) },
    Primitive { name: "bls_mlr", arity: 0, kind: mono(CONST) },
    Primitive { name: "value", arity: 0, kind: mono(CONST) },
    Primitive {
        name: "list",
        arity: 1,
        kind: KindScheme { bounds: &[KindSet::STORABLE], kind: STORABLE_TO_CONST },
    },
    Primitive {
        name: "array",
        arity: 1,
        kind: KindScheme { bounds: &[KindSet::STORABLE], kind: STORABLE_TO_CONST },
    },
    Primitive { name: "pair", arity: 2, kind: KindScheme { bounds: &[KindSet::STORABLE, KindSet::STORABLE], kind: STORABLE2_TO_CONST } },
];
```

`KindEnv` (`kinds.rs`):

```rust
use std::collections::BTreeMap;
use nash_ast::{QualifiedName, primitives};

/// Kind schemes of every type constructor visible to the module.
pub struct KindEnv<'a> {
    schemes: BTreeMap<QualifiedName<'a>, KindScheme<'a>>,
}

impl<'a> KindEnv<'a> {
    pub fn from_interfaces(interfaces: Option<&BTreeMap<&'a str, Interface<'a>>>) -> Self {
        let mut schemes = BTreeMap::new();
        for p in primitives::PRIMITIVES {
            schemes.insert(
                QualifiedName { home: primitives::builtin_home(), name: p.name },
                p.kind,
            );
        }
        for interface in interfaces.into_iter().flat_map(|m| m.values()) {
            for union in interface.unions {
                schemes.insert(QualifiedName { home: interface.home, name: union.name }, union.kind);
            }
            for alias in interface.aliases {
                schemes.insert(QualifiedName { home: interface.home, name: alias.name }, alias.kind);
            }
        }
        KindEnv { schemes }
    }

    pub fn insert(&mut self, name: QualifiedName<'a>, scheme: KindScheme<'a>) {
        self.schemes.insert(name, scheme);
    }

    /// Every `Type::Named` reference was resolved by `types.rs`, so absence is a bug.
    pub fn scheme(&self, name: QualifiedName<'a>) -> KindScheme<'a> {
        *self.schemes.get(&name).expect("kind env covers every resolved type")
    }
}
```

`QualifiedName` needs `PartialOrd, Ord` derives for the `BTreeMap` key
(`nash-ast`); `ModuleName` and `PackageName` too.

The `union.kind` / `alias.kind` fields on `InterfaceUnion`/`InterfaceAlias`
are added in chunk 5; in this chunk `from_interfaces` only seeds
primitives, and the interface loop is added in chunk 5.

**Tests**: `primitives_have_declared_arity`: for each primitive, the arrow
depth of `kind.kind` equals `arity`.

**Done when**: `KindEnv::from_interfaces(None)` resolves `Builtin.list` to
a scheme with one `STORABLE` bound.

---

## Chunk 4: Declaration inference in the canonicalization pipeline

**Files**: `crates/nash-ast/src/lib.rs`, `crates/nash-can/src/kinds.rs`,
`crates/nash-can/src/module.rs`, `crates/nash-can/src/environment/local.rs`,
`crates/nash-can/src/error.rs`, `crates/nash-can/src/interface.rs`.

**Change**:

1. `nash_ast::Union` and `nash_ast::Alias` gain `pub kind: KindScheme<'a>`.
2. `canonicalize_unions` (`module.rs:326`) and `canonicalize_single_alias`
   (`module.rs:450`) stop allocating `CanUnion`/`CanAlias`. They produce
   `PreUnion`/`PreAlias` values; `kinds::infer_declarations` runs over them
   and returns the schemes; then `module.rs` allocates the final
   `Located<CanUnion>` / `Located<CanAlias>` with kinds. `add_alias_type`
   (`local.rs:40`) takes `(name, parameters, typ)` instead of `&CanAlias`
   because it is called before kinds exist.
3. Pipeline order in `canonicalize` (`module.rs:48-69`) becomes:
   `add_union_types` -> `check_union_free_vars` -> `canonicalize_aliases`
   (pre) -> `add_vars` -> `canonicalize_unions` (pre) ->
   **`kinds::infer_declarations`** -> allocate unions/aliases ->
   `add_ctors` -> `check_binops` -> decls.
4. New `Error` variants.

**Code** (`module.rs`):

```rust
pub(crate) struct PreUnion<'a> {
    pub source: &'a Located<SourceUnion<'a>>,
    pub name: &'a Located<&'a str>,
    pub parameters: &'a [&'a str],
    pub ctors: &'a [&'a CanCtor<'a>],
    pub alternatives: u16,
    pub options: CtorOpts,
}

pub(crate) struct PreAlias<'a> {
    pub source: &'a Located<SourceAlias<'a>>,
    pub name: &'a Located<&'a str>,
    pub parameters: &'a [&'a str],
    pub typ: &'a Located<CanType<'a>>,
}
```

and in `canonicalize`:

```rust
    let pre_aliases = canonicalize_aliases(bump, &mut env, module.aliases)?;
    environment::local::add_vars(&mut env, module.values)?;
    let pre_unions = canonicalize_unions(bump, &env, module.unions)?;

    let mut kind_env = kinds::KindEnv::from_interfaces(context.interfaces);
    let schemes = kinds::infer_declarations(bump, &mut kind_env, home, &pre_unions, &pre_aliases)?;
    let unions = bump.alloc_slice_fill_iter(pre_unions.iter().map(|u| {
        &*bump.alloc(Located::at(u.source.region, CanUnion {
            name: u.name,
            parameters: u.parameters,
            ctors: u.ctors,
            alternatives: u.alternatives,
            options: u.options,
            kind: schemes.union(u.name.value),
        }))
    }));
    let aliases = bump.alloc_slice_fill_iter(pre_aliases.iter().map(|a| {
        &*bump.alloc(Located::at(a.source.region, CanAlias {
            name: a.name,
            parameters: a.parameters,
            typ: a.typ,
            kind: schemes.alias(a.name.value),
        }))
    }));
    environment::local::add_ctors(bump, &mut env, module.unions, unions, aliases)?;
```

`kinds.rs` declaration walker:

```rust
pub struct Schemes<'a> {
    unions: BTreeMap<&'a str, KindScheme<'a>>,
    aliases: BTreeMap<&'a str, KindScheme<'a>>,
}

impl<'a> Schemes<'a> {
    pub fn union(&self, name: &str) -> KindScheme<'a> {
        self.unions[name]
    }
    pub fn alias(&self, name: &str) -> KindScheme<'a> {
        self.aliases[name]
    }
}

enum Decl<'p, 'a> {
    Union(&'p PreUnion<'a>),
    Alias(&'p PreAlias<'a>),
}

impl<'p, 'a> Decl<'p, 'a> {
    fn name(&self) -> &'a Located<&'a str> { .. }
    fn parameters(&self) -> &'a [&'a str] { .. }
}

fn is_big_name(name: &str) -> bool {
    name.chars().next().is_some_and(char::is_uppercase)
}

/// Infer one module's type declarations, SCC by SCC, and record every
/// scheme in `env` under `home`.
pub fn infer_declarations<'a>(
    bump: &'a Bump,
    env: &mut KindEnv<'a>,
    home: ModuleName<'a>,
    unions: &[PreUnion<'a>],
    aliases: &[PreAlias<'a>],
) -> Result<Schemes<'a>, Vec<Error<'a>>> {
    let decls: Vec<Decl<'_, 'a>> = unions.iter().map(Decl::Union).chain(aliases.iter().map(Decl::Alias)).collect();
    let local: BTreeSet<&str> = decls.iter().map(|d| d.name().value).collect();

    let nodes = decls
        .iter()
        .map(|decl| {
            let mut deps = Vec::new();
            match decl {
                Decl::Union(u) => {
                    for ctor in u.ctors {
                        for arg in ctor.arguments {
                            local_type_edges(&arg.value, home, &local, &mut deps);
                        }
                    }
                }
                Decl::Alias(a) => local_type_edges(&a.typ.value, home, &local, &mut deps),
            }
            scc::Node { key: decl.name().value, value: decl, deps }
        })
        .collect();

    let mut schemes = Schemes { unions: BTreeMap::new(), aliases: BTreeMap::new() };
    let mut errors = Vec::new();
    for component in scc::strongly_connected_components(nodes) {
        let group: Vec<&Decl<'_, 'a>> = match &component {
            scc::Scc::Acyclic(d) => vec![d],
            scc::Scc::Cyclic(ds) => ds.iter().collect(),
        };
        match infer_group(bump, env, home, &group) {
            Ok(results) => {
                for (decl, scheme) in group.iter().zip(results) {
                    let name = decl.name().value;
                    env.insert(QualifiedName { home, name }, scheme);
                    match decl {
                        Decl::Union(_) => schemes.unions.insert(name, scheme),
                        Decl::Alias(_) => schemes.aliases.insert(name, scheme),
                    };
                }
            }
            Err(errs) => errors.extend(errs),
        }
    }
    if errors.is_empty() { Ok(schemes) } else { Err(errors) }
}

/// Edges to local declarations, both `Named` (unions) and `Alias` references.
fn local_type_edges<'a>(typ: &CanType<'a>, home: ModuleName<'a>, local: &BTreeSet<&str>, edges: &mut Vec<&'a str>) {
    match typ {
        CanType::Named { reference, args } => {
            if reference.home == home && local.contains(reference.name) {
                edges.push(reference.name);
            }
            for arg in *args { local_type_edges(&arg.value, home, local, edges); }
        }
        CanType::Alias { reference, arguments, .. } => {
            if reference.home == home && local.contains(reference.name) {
                edges.push(reference.name);
            }
            for arg in *arguments { local_type_edges(&arg.typ.value, home, local, edges); }
        }
        CanType::Lambda { from, to } => { .. }
        CanType::Record { fields, .. } => { .. }
        CanType::Tuple { first, second, rest } => { .. }
        CanType::VarApp { args, .. } => { .. }
        CanType::Var(_) | CanType::Unit => {}
    }
}
```

The group solver:

```rust
struct Scope<'a> {
    /// Type parameter name -> its kind, for the declaration being walked.
    params: BTreeMap<&'a str, &'a K<'a>>,
}

struct Walker<'e, 'a> {
    bump: &'a Bump,
    infer: Infer<'a>,
    env: &'e KindEnv<'a>,
    /// Monomorphic kinds of the SCC members, by name.
    group: BTreeMap<&'a str, &'a K<'a>>,
    home: ModuleName<'a>,
    errors: Vec<Error<'a>>,
}

fn infer_group<'a>(
    bump: &'a Bump,
    env: &KindEnv<'a>,
    home: ModuleName<'a>,
    group: &[&Decl<'_, 'a>],
) -> Result<Vec<KindScheme<'a>>, Vec<Error<'a>>> {
    let mut w = Walker { bump, infer: Infer::new(bump), env, group: BTreeMap::new(), home, errors: Vec::new() };

    // Monomorphic kinds first, so recursive references resolve.
    let mut scopes = Vec::with_capacity(group.len());
    for decl in group {
        let params: Vec<(&'a str, &'a K<'a>)> = decl
            .parameters()
            .iter()
            .map(|p| (*p, w.infer.fresh_k(KindSet::ALL)))
            .collect();
        let result = match decl {
            Decl::Union(u) if is_big_name(u.name.value) => bump.alloc(K::Base(BaseKind::Big)),
            Decl::Union(_) => bump.alloc(K::Base(BaseKind::Term)),
            Decl::Alias(_) => w.infer.fresh_k(KindSet::ALL),
        };
        let kind = params.iter().rev().fold(&*result, |acc, (_, p)| &*bump.alloc(K::Arrow(p, acc)));
        w.group.insert(decl.name().value, kind);
        scopes.push((Scope { params: params.into_iter().collect() }, result));
    }

    for (decl, (scope, result)) in group.iter().zip(&scopes) {
        match decl {
            Decl::Union(u) => {
                let big = is_big_name(u.name.value);
                for ctor in u.ctors {
                    for (index, arg) in ctor.arguments.iter().enumerate() {
                        let k = w.infer_type(scope, arg);
                        let expected = if big { K::Base(BaseKind::Big) } else { K::Var(w.infer.fresh(KindSet::ANY)) };
                        let expected = bump.alloc(expected);
                        let context = if big {
                            KindContext::BigField { union: u.name.value, ctor: ctor.name, index: index as u16 }
                        } else {
                            KindContext::LittleField { union: u.name.value, ctor: ctor.name, index: index as u16 }
                        };
                        w.expect(arg.region, context, expected, k);
                    }
                }
            }
            Decl::Alias(a) => {
                let big = is_big_name(a.name.value);
                let k = match &a.typ.value {
                    CanType::Record { fields, .. } => w.infer_record_body(scope, a.name.value, big, fields),
                    _ => w.infer_type(scope, a.typ),
                };
                w.infer.unify(result, k).ok(); // result is a fresh var: cannot fail
                let expected: &K = if big { bump.alloc(K::Base(BaseKind::Big)) } else { w.infer.fresh_k(KindSet::LITTLE) };
                w.expect(a.typ.region, KindContext::AliasCasing { alias: a.name.value, big }, expected, result);
            }
        }
    }

    if !w.errors.is_empty() {
        return Err(w.errors);
    }
    Ok(group.iter().map(|d| w.infer.generalize(w.group[d.name().value])).collect())
}
```

The type walker (`infer_type`) is shared with chunk 5:

```rust
impl<'e, 'a> Walker<'e, 'a> {
    fn infer_type(&mut self, scope: &Scope<'a>, typ: &'a Located<CanType<'a>>) -> &'a K<'a> {
        match &typ.value {
            CanType::Var(name) => scope.params[name],
            CanType::VarApp { name, args } => {
                let head = scope.params[name];
                self.apply_args(scope, typ.region, KindHead::Var(name), head, args)
            }
            CanType::Named { reference, args } => {
                let head = self.head_kind(*reference);
                self.apply_args(scope, typ.region, KindHead::Named(*reference), head, args)
            }
            CanType::Alias { reference, arguments, .. } => {
                let head = self.head_kind(*reference);
                let args: Vec<_> = arguments.iter().map(|a| a.typ).collect();
                self.apply_args(scope, typ.region, KindHead::Named(*reference), head, &args)
            }
            CanType::Lambda { from, to } => {
                self.expect_any(scope, from);
                self.expect_any(scope, to);
                self.bump.alloc(K::Base(BaseKind::Term))
            }
            CanType::Tuple { first, second, rest } => {
                self.expect_any(scope, first);
                self.expect_any(scope, second);
                for r in *rest { self.expect_any(scope, r); }
                self.bump.alloc(K::Base(BaseKind::Term))
            }
            CanType::Unit => self.bump.alloc(K::Base(BaseKind::Const)),
            CanType::Record { .. } => {
                // Only legal as an alias body (plans/04 chunk A1); handled by infer_record_body.
                unreachable!("record types only appear as alias bodies")
            }
        }
    }

    fn head_kind(&mut self, reference: QualifiedName<'a>) -> &'a K<'a> {
        if reference.home == self.home {
            if let Some(kind) = self.group.get(reference.name) {
                return kind;
            }
        }
        let scheme = self.env.scheme(reference);
        self.infer.instantiate(&scheme)
    }

    fn apply_args(&mut self, scope: &Scope<'a>, region: Region, head: KindHead<'a>, mut kind: &'a K<'a>, args: &[&'a Located<CanType<'a>>]) -> &'a K<'a> {
        for (index, arg) in args.iter().enumerate() {
            let (param, result) = match self.infer.apply(kind) {
                Ok(pair) => pair,
                Err(_) => {
                    self.errors.push(Error::KindTooManyArgs { region, head, applied: args.len(), accepted: index });
                    return self.infer.fresh_k(KindSet::ALL);
                }
            };
            let actual = self.infer_type(scope, arg);
            self.expect(arg.region, KindContext::TypeArg { head, index: index as u16 }, param, actual);
            kind = result;
        }
        kind
    }

    fn expect_any(&mut self, scope: &Scope<'a>, typ: &'a Located<CanType<'a>>) {
        let k = self.infer_type(scope, typ);
        let any = self.infer.fresh_k(KindSet::ANY);
        self.expect(typ.region, KindContext::ValuePosition, any, k);
    }

    fn infer_record_body(&mut self, scope: &Scope<'a>, alias: &'a str, big: bool, fields: &'a [FieldType<'a>]) -> &'a K<'a> {
        for field in fields {
            let k = self.infer_type(scope, field.typ);
            let expected: &K = if big { self.bump.alloc(K::Base(BaseKind::Big)) } else { self.infer.fresh_k(KindSet::ANY) };
            self.expect(field.typ.region, KindContext::RecordField { alias, field: field.field, big }, expected, k);
        }
        self.bump.alloc(K::Base(if big { BaseKind::Big } else { BaseKind::Term }))
    }

    fn expect(&mut self, region: Region, context: KindContext<'a>, expected: &'a K<'a>, actual: &'a K<'a>) {
        match self.infer.unify(expected, actual) {
            Ok(()) => {}
            Err(Mismatch::Shapes { .. }) => {
                let expected = self.render(expected);
                let actual = self.render(actual);
                self.errors.push(Error::KindMismatch { region, context, expected, actual });
            }
            Err(Mismatch::Infinite(_)) => self.errors.push(Error::KindInfinite { region, context }),
        }
    }

    /// Zonk to a `nash_ast::Kind` for error data; unbound vars are numbered in order of appearance.
    fn render(&mut self, kind: &'a K<'a>) -> KindScheme<'a> {
        self.infer.generalize(kind)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum KindHead<'a> {
    Named(QualifiedName<'a>),
    Var(&'a str),
}
```

`Error` (`error.rs`):

```rust
    // --- Kind errors (docs/kinds.md) ---
    KindMismatch {
        region: Region,
        context: KindContext<'a>,
        expected: KindScheme<'a>,
        actual: KindScheme<'a>,
    },
    KindInfinite {
        region: Region,
        context: KindContext<'a>,
    },
    KindTooManyArgs {
        region: Region,
        head: KindHead<'a>,
        applied: usize,
        accepted: usize,
    },
```

```rust
#[derive(Clone, Copy, Debug)]
pub enum KindContext<'a> {
    TypeArg { head: KindHead<'a>, index: u16 },
    BigField { union: &'a str, ctor: &'a str, index: u16 },
    LittleField { union: &'a str, ctor: &'a str, index: u16 },
    RecordField { alias: &'a str, field: &'a str, big: bool },
    AliasCasing { alias: &'a str, big: bool },
    /// Function argument/result or tuple component: must be a base kind.
    ValuePosition,
    Annotation { name: &'a str },
    ParamAnnotation { type_name: &'a str, param: &'a str },
    /// The `index`th type in an `impl Trait T1 .. Tn` head (plans/03).
    ImplHead { trait_: QualifiedName<'a>, index: u16 },
}
```

`interface.rs`: `extract_unions`/`extract_aliases` unchanged in this chunk
(kinds reach interfaces in chunk 5). Existing test helpers that build
`CanUnion`/`CanAlias` by hand (`module.rs` tests, `pattern.rs:403-445`,
`types.rs` tests) add `kind: KindScheme::mono(&Kind::Base(BaseKind::Big))`.

**Elm reference**: `Canonicalize/Environment/Local.hs` `addTypes`,
`addAliases` (the SCC), `canonicalizeUnion`, `canonicalizeAlias`. The SCC
port is `crates/nash-can/src/scc.rs`.

**Tests** (`module.rs` test module, `assert_module_snapshot!` on the
canonical module so the `kind` fields show in the Debug snapshot;
`assert_module_error_snapshot!` for errors):

- `kind_big_union_int_field`: `type Box = Box Int` — expect `Box : Big`.
- `kind_big_union_parameter`: `type Box 'a = Box 'a` — `Big -> Big`.
- `kind_phantom_parameter_generalized`: `type Tag 'a = Tag Int` —
  `bounds: [ALL]`, `k0 -> Big`.
- `independent_instantiations` covers `type option 'a = None | Some 'a`
  (`bounds: [ANY]`, `k0 -> Term`) and distinct Big/Const instantiations.
- `kind_little_union_function_field`: `type thunk 'a = Thunk (unit -> 'a)`.
- `kind_higher_kinded_parameter`: `type wrap 'f 'a = Wrap ('f 'a)` —
  `'f : k0 -> k1`, `'a : k0`, with `k0` bounded `ALL` and `k1` bounded
  `ANY` (the field position); scheme `(k0 -> k1) -> k0 -> Term`.
- `kind_mutual_union_alias`: `type Tree = Node (List Branch)` +
  `type alias Branch = Tree` — both solved in one SCC, with Big fields.
- `kind_alias_big_body`: `type alias Id = Int`.
- `kind_alias_little_const_body`: `type alias count = int`.
- `kind_big_record_alias`: `type alias Vault = { owner : Bytes, amount : Int }`.
- `kind_little_record_alias`: `type alias acc = { total : int, seen : list Int }`.
- Errors: `kind_error_big_field_const` (`type Datum = Datum bytes`),
  `kind_error_big_field_tuple`, `kind_error_list_of_little`
  (`type alias xs = list (option int)` with `option` declared),
  `kind_error_lowercase_alias_big_body` (`type alias id = Int`),
  `kind_error_uppercase_alias_little_body` (`type alias Count = int`),
  `named_constructor_arity_remains_a_canonicalization_error` (`type alias x = int Int`, expected `BadArity`),
  `base_kinded_parameter_cannot_be_applied` (expected `KindTooManyArgs`),
  `kind_error_infinite` (`type bad 'f = Bad (bad bad)`),
  `pair_requires_storable` (`type alias p = pair (option int) Int`),
  `kind_errors_all_reported` (two independent bad declarations give two
  errors).

Tests get builtins in scope through `Context.interfaces` seeded with a
`Builtin` interface built from `PRIMITIVES` (`primitives::interface(bump)`,
`InterfaceUnion { ctors: &[], kind, .. }`), imported with
`import Builtin exposing (..)` in the test source until the implicit
prelude lands (plans/04 chunk C1).

**Done when**: all snapshot tests pass; `cargo clippy` clean; every
`CanUnion`/`CanAlias` carries a scheme.

---

## Chunk 5: Interfaces and value annotations

**Files**: `crates/nash-can/src/interface.rs`,
`crates/nash-can/src/environment/foreign.rs`, `crates/nash-can/src/kinds.rs`,
`crates/nash-can/src/module.rs`, `crates/nash-driver/src/interface.rs`.

**Change**:

1. `InterfaceUnion` and `InterfaceAlias` gain `pub kind: KindScheme<'a>`;
   `extract_unions`/`extract_aliases` copy it from the declaration;
   `KindEnv::from_interfaces` reads it (the loop sketched in chunk 3).
   Plan 03 retains the build arena, so interfaces borrow the original kind
   schemes; the earlier interface-copy helpers are removed.
2. Value annotations are kind-checked after `canonicalize_decls`
   (`module.rs:72`): walk `Decls` for `Def::TypedDef { typ, free_vars, .. }`
   and run `kinds::check_annotation`. The walk returns the kind of every
   free variable; this chunk discards it (plans/03 stores it as predicates).
3. `nash-driver` fingerprint: `Export::Type` gains `kind: String` (the
   scheme rendered with `k0 -> Big` notation and explicit bounds).
   `Interface::from_canonical` builds this metadata from successful compiler
   output, and `BuildResult.interfaces` exposes it. Interface files use direct
   bincode serialization of the current structure. The
   current driver rebuilds all modules and does not yet use persistent
   fingerprints to skip compilation.

**Code**:

```rust
// kinds.rs
/// Kind-check one value annotation. Returns the kind of each free variable,
/// name-sorted like `Annotation.free_vars`.
pub fn check_annotation<'a>(
    bump: &'a Bump,
    env: &KindEnv<'a>,
    home: ModuleName<'a>,
    name: &'a str,
    annotation: &'a Annotation<'a>,
) -> Result<Vec<(&'a str, KindScheme<'a>)>, Vec<Error<'a>>> {
    let mut w = Walker { bump, infer: Infer::new(bump), env, group: BTreeMap::new(), home, errors: Vec::new() };
    let scope = Scope {
        params: annotation.free_vars.iter().map(|v| (*v, w.infer.fresh_k(KindSet::ALL))).collect(),
    };
    let k = w.infer_type(&scope, annotation.typ);
    let any = w.infer.fresh_k(KindSet::ANY);
    w.expect(annotation.typ.region, KindContext::Annotation { name }, any, k);
    if !w.errors.is_empty() {
        return Err(w.errors);
    }
    Ok(scope.params.iter().map(|(n, k)| (*n, w.infer.generalize(k))).collect())
}
```

and in `module.rs`, after `canonicalize_decls`:

```rust
    kinds::check_decl_annotations(bump, &kind_env, home, decls)?;
```

which walks `Decls::Declare`/`DeclareRec` and calls `check_annotation` on
every `TypedDef`, accumulating errors. Let-bound annotations inside
expressions are checked in the same walk by recursing into
`Expr::Let`/`LetRec` definitions (`expression.rs` already produces
`Def::TypedDef` there).

**Tests**:

- `module.rs`: `annotation_kind_ok` (`f : 'a -> list 'a -> list 'a`),
  `annotation_kind_error_list_of_little` (`f : list (option int) -> int`),
  `annotation_kind_error_arrow_arg_higher_kinded`
  (`f : ('f 'a, 'f) -> int`, a higher-kinded variable in a value position),
  `let_annotation_kind_error`. Variable applications (`f : 'f 'a -> 'f 'a`)
  must canonicalize successfully in the solver's
  `higher_kinded_value_inference_is_explicitly_deferred` regression.
- `interface_from_module_exports_kinds` (Debug snapshot shows `kind`).
- `nash-driver`: the real-export kind and bound fingerprint tests.
- `nash-driver` `compile.rs` integration: a module importing a Big alias
  from another module and using it as a `list` element compiles; using a
  little alias fails with a kind error.

**Done when**: kinds cross module boundaries through interfaces and value
annotations are kind-checked, with snapshots for each error.

---

## Chunk 6: User kind annotations on parameters

**Files**: `crates/nash-ast/src/lib.rs`, `crates/nash-can/src/module.rs`,
`crates/nash-can/src/kinds.rs`, `crates/nash-can/src/types.rs`.

**Prerequisite**: plans/01 delivered `nash_source::TypeParam { name: &Located<&str>, kind: Option<&Located<nash_source::Kind>> }`
with `nash_source::Kind = Big | Const | Term | Storable | Arrow { from: &Located<Kind>, to: &Located<Kind> }`.

**Change**: `PreUnion`/`PreAlias` carry `annotations: &[Option<&Located<SourceKind>>]`
parallel to `parameters`. In `infer_group`, a parameter with an annotation
gets `annotation_to_k` instead of a fresh `ALL` variable; because the
annotation *is* the parameter's kind, any later conflicting use produces a
`KindMismatch` whose context names the parameter.

**Code**:

```rust
impl<'e, 'a> Walker<'e, 'a> {
    fn annotation_to_k(&mut self, kind: &Located<SourceKind<'_>>) -> &'a K<'a> {
        match &kind.value {
            SourceKind::Big => self.bump.alloc(K::Base(BaseKind::Big)),
            SourceKind::Const => self.bump.alloc(K::Base(BaseKind::Const)),
            SourceKind::Term => self.bump.alloc(K::Base(BaseKind::Term)),
            SourceKind::Storable => self.infer.fresh_k(KindSet::STORABLE),
            SourceKind::Arrow { from, to } => {
                let from = self.annotation_to_k(from);
                let to = self.annotation_to_k(to);
                self.bump.alloc(K::Arrow(from, to))
            }
        }
    }
}
```

To report "annotation says X, usage says Y" rather than a bare mismatch at
the use site, keep the annotated kind separate: give the parameter a fresh
`ALL` variable, walk the declaration, then unify the variable with the
annotation under `KindContext::ParamAnnotation { type_name, param }` at the
parameter's region. The annotation region is `TypeParam.kind.region`.

**Tests**: `kind_annotation_ok` (`type Fix ('f : Big -> Big) = Fix ('f (Fix 'f))`),
`kind_annotation_storable` (`type alias xs ('a : Storable) = list 'a`),
`kind_annotation_mismatch` (`type Box ('a : Const) = Box 'a` on a Big
union), `kind_annotation_arrow_mismatch`
(`type wrap ('f : Big) 'a = Wrap ('f 'a)`).

**Done when**: annotations constrain inference and mismatches are reported
at the annotation.

---

## Chunk 7: Changeset and SPEC

**Files**: `.sampo/changesets/`, `SPEC.md`.

Each implementation chunk includes its own changeset, following plan 01:

- Chunk 1: `kind-vocabulary.md` — nash-ast.
- Chunk 2: `kind-unification.md` — nash-can.
- Chunk 3: `builtin-kind-schemes.md` — nash-ast, nash-can.
- Chunk 4: `declaration-kind-inference.md` — nash-ast, nash-can,
  nash-constrain, nash-solve.
- Chunk 5: `annotation-kind-contracts.md` — nash-ast, nash-can,
  nash-constrain, nash-driver.
- Chunk 6: `parameter-kind-annotations.md` — nash-can.

Chunk 7 audits coverage and marks SPEC complete. It adds no duplicate release
entry for verification-only work. Combined bump levels remain minor for
nash-ast and nash-can, and patch for nash-constrain, nash-solve, and nash-driver.

Grammar lives only in docs/syntax.md (the `kind` and `type_param` rules
are plans/01's). SPEC.md: add a "Kinds" checklist under Canonicalization
pointing to docs/kinds.md.

**Done when**: `cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`,
`cargo insta test` all pass.

---

## Open questions

- **Where checks happen**: the lead's brief suggested kind-checking each
  `Type::Named` application inside `types.rs` canonicalization. That cannot
  work for declarations in the same recursive group (their kinds are still
  variables), so this plan checks everything in one pass after
  canonicalization. If per-application checks in `types.rs` are still
  wanted for imported types only, they would duplicate errors; not planned.
- **Defaulting**: the brief asked whether unconstrained kind variables
  should default (to `Big` for Big-named declarations). This plan
  generalizes instead, which is what makes `option : Any -> Term` and
  phantom parameters work; nothing in the front end needs a ground kind.
- **Kind predicates on values**: enforcing `'a : Storable` at every
  instantiation needs qualified types; this plan returns the per-variable
  kinds from `check_annotation` and plans/03 stores and discharges them.
  Until then, `cons (Some 1) nil` at a call site is only rejected by
  codegen's kind computation.
- **`VarApp` in the solver**: `'f 'a` is kind-checked here but
  `nash-constrain`/`nash-solve` cannot unify it yet (`FlatType::App1` is
  saturated and nominal). That is HKT unification work for plans/03.
- **Exact arity**: `check_arity` in `types.rs:268` still requires full
  application. Relaxing it to `<=` for unions (partial application in impl
  heads) belongs with plans/03.
