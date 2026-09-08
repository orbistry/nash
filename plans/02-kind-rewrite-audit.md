# Haskell 98 replacement: cleanup audit

Scope: finish the replacement required by [kinds.md](../docs/kinds.md),
including the approved Plan 03 integration. This audit checks the role of
APIs, state and control paths, not just whether old symbol names disappeared.
The work started from a clean working copy. No commit, push or publication
is part of this delivery.

Status: complete. Verified on 2026-09-08 against the working tree.

## Removed

- `head::overlaps` no longer accepts a kind-compatibility callback. Its
  only production caller had replaced the old engine with an unconditional
  `true`. Structural head unification now expresses the rule directly:
  heads have already been checked against their trait's closed kinds.
  Independent binders, repeated variables, occurs checks and work limits
  remain part of head matching.
- `insert_impl` no longer receives a kind environment it ignores.
  `check_annotation` no longer receives an ignored module name. All
  production and test callers use the smaller APIs.
- `KindHead` and the unproduced `KindContext::TypeArg`, `ValuePosition`
  and `ParamAnnotation` variants are removed with their diagnostic arms.
- The parser's obsolete recursive kind-error structure is removed.
  Representation annotations use `repr_annotation`, `error::Repr` and
  representation-specific wrapper names. Unused kind-parenthesis/end/indent
  branches are deleted. The `Arrow` error still rejects the removed source
  kind-arrow syntax at its original location.
- `Primitive.arity` is removed. Its closed `Kind` is the sole source of
  arity. The inventory retains all 17 builtins, their representation
  contexts, the bool constructors and the special Data constructors.
- `Mismatch::Infinite` no longer carries an inference-variable identifier
  discarded by every consumer.
- `record_definitions` no longer returns a guaranteed-empty error vector
  or makes callers extend their errors with it. Kind-contract failures
  continue through `kind_errors`. `freeze_kind_contracts` records one
  boxed error on failure; the box keeps the large diagnostic out of the
  `Result` stack layout.

The deleted `nash-solve/src/kinds.rs` remains absent. The canonical AST
contains no `BaseKind`, `KindSet`, `KindScheme`, `KindApplication`,
`ValueKinds` or `Type::Kinded`. No compatibility path invokes the old
lattice or retained-kind settlement engine.

## Retained structures and their roles

| Area reviewed | Current structure and reason to keep it | Evidence |
|---|---|---|
| Source and parser | `Type::Repr` and `TypeParam.repr` preserve representation sugar. Closed Haskell 98 kinds have no source syntax. | Parser snapshots cover valid nested annotations, unknown/empty annotations and rejected arrows. |
| AST and primitives | `Kind::{Type, Arrow}` describes type application; `Repr` and `ReprTrait` describe representation independently. `ReprSet` checks contradictory representation premises only. | `nash-ast/tests/representations.rs`; canonical contradiction tests. |
| Declaration kinds | `Infer` has local variables, equality unification, an occurs check and final defaulting. `TypeChecker` shares kinds within declaration groups and respects each parameter scope. | Core unifier tests and `nash-can/tests/kinds.rs`. |
| Declaration formation | `Formation`, group references, predicate identity and the context worklist propagate datatype requirements. Applied-relevant recursive substitutions must rename bare parameters. | `nash-can/tests/kind_predicates.rs`, including late relevance, growth rejection and a permutation requiring more than 256 steps. |
| Environments and interfaces | `KindEnv` supplies closed kinds, datatype contexts and superclass metadata to both checkers. Private metadata lets downstream uses check exported schemes without exposing private names. | Imported canonicalizer tests and driver fingerprint tests. |
| Constraint generation | Type formation and scheme instantiation use the same lexical type substitutions. `Apply` includes its head and all ordered arguments. | `nash-constrain/tests/predicate_instantiation.rs`; solver representation regressions. |
| Generalization and specialization | Frozen contracts retain the closed Haskell 98 kind of a quantified type variable across local instantiation. Captured variables remain shared. These are not polymorphic kind schemes or representation bounds. | Higher-kinded inference, rigid-head, recursive-group and independent-instantiation tests. |
| Trait resolution | Nominal head matching, superclass search, literal defaulting and delayed selection still implement Plan 03. The head matcher is shared by canonical and inference types. | `nash-can/tests/impls.rs`, solver inference and evidence tests. |
| Predicate evidence | `Apply` is a dictionary-free type-formation requirement. Representation evidence is a compile-time marker. Ordinary trait evidence retains stable dictionary slots, child proofs and recursive uses. | `nash-solve/tests/evidence.rs` and `representation_predicates.rs`. |
| Driver and diagnostics | Ordered application arguments and hidden contexts affect fingerprints; errors retain formation/application provenance. Only currently produced diagnostic contexts remain. | Driver unit tests and `predicate_fingerprint.rs`; real CLI fixtures. |

The ordinary type unifier, trait predicate store, nominal alias handling and
Plan 03 evidence machinery remain because the replacement must integrate
with them. They are not adapters around the removed kind engine. The audit
reviewed their affected call paths, including annotation construction,
constructor/builtin formation, scheme copying, generalization, impl method
specialization, superclass proofs, exported annotations and ground evidence.

## Regression and document cleanup

Existing source cases and expected outcomes are preserved. Tests that used
names such as "finite inductive proof", "retained self application" or
"kind bounds" now name the actual Haskell 98 or representation behavior.
Canonical rejection tests explicitly require their intended diagnostic
variants instead of accepting any kind-or-representation error. All 22
historical nontermination cases still require declaration-time `KindInfinite`.
The fixture document labels the old timing and mechanism as historical.

Sixty snapshot moves preserve their contents byte-for-byte. The parser's five changed snapshots
replace the old `Kind` wrapper with `Repr`, with the same inner error and
source location; these changes were reviewed before acceptance.

Current documents and future plans no longer describe representations as
base kinds or expose the removed kind-scheme API. In particular:

- codegen reads `Ty::repr()` and uses the existing representation enum;
- the proposed macro AST separates closed kinds, representation metadata
  and source representation annotations;
- validator/comptime sketches use representation checks, not a kind engine;
- future list optimization is restricted to Eq, including the Plan 08
  implementation sketch; representation alone cannot replace Ord or Show;
- Plans 01, 02 and the old Plan 03 sections are explicitly historical where
  superseded. Their old algorithms are not current implementation guidance.

These are contract corrections for later work. Runtime/codegen, optimizer
implementation, macros, default imports, Fuzz and the full validator example
remain deferred. Trait search keeps its existing 128-level and 16,384-work
limits; no Paterson restriction or new impl admission policy is introduced.

## Validation

| Check | Result |
|---|---|
| `cargo fmt --all` | Pass. |
| `cargo clippy --all-targets --all-features -- -D warnings` | Pass. |
| `cargo test --workspace` | 2,027 passed; zero failures; three existing doctests ignored. |
| `cargo insta test --workspace --check --unreferenced delete` | Pass; no unreferenced snapshots and no snapshots to review. |
| `cargo run -q -p nash-cli -- check tests/core` | 23 modules and 215 declarations compiled. |
| `git diff --check` | Pass. |

Cross-module acceptance includes imported higher-kinded applications,
transparent aliases, inferred wrapper contexts, partial pair contexts,
recursive impls, superclass requirements, head-only overlap and shipping
option/result do. Negative projects reject imported representation failures,
invalid inferred `Apply`, contradictory predicates, infinite kinds and
irregular recursive contexts. Driver tests also check closed-kind/context
fingerprints and error propagation from invalid producer modules.

The 22 historical source blocks are byte-for-byte unchanged. Snapshot review
confirmed 60 pure renames and five one-line `Kind` → `Repr` wrapper changes.
No pending snapshots remain. Core source files, Cargo manifests
and the lockfile are unchanged. A separate Sampo changeset records public
AST/canonicalizer/parser API removals and solver/driver cleanup; it preserves
the existing release metadata and dependency graph.

Independent read-only reviews covered the engine, constraint/solver
integration and interface/diagnostic/document edges. No unresolved old-engine
adapter or behavior gap was found in the audited scope. This conclusion uses
source/dataflow review plus acceptance results; passing tests and symbol
searches alone were not treated as proof.
