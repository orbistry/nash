# Plan 02 follow-up: Haskell 98 kinds and representation predicates

Design: [docs/kinds.md](../docs/kinds.md). This replaces the old lattice,
retained kind obligations, and value-kind engine outright. The three work
areas below form one delivery with the approved Plan 03 scope.

Status: complete for the approved Haskell 98 and Plan 03 scope.
The subsequent cleanup and its verification are recorded in
[02-kind-rewrite-audit.md](02-kind-rewrite-audit.md).

## Declaration checking

- [x] Closed canonical kinds are `Type | Arrow`. Inference variables exist
  only inside the new unification engine. Unification checks occurs and full
  arrow structure; unconstrained variables default to `Type`.
- [x] Infer declaration groups in dependency order and collect errors from
  independent groups. Skip dependents of invalid declarations.
- [x] Remove `BaseKind`, `KindSet`, `KindScheme`, `KindApplication`,
  `ValueKinds`, canonical `Type::Kinded`, and their callers. Delete the old
  solver kind engine; source representation sugar becomes `Type::Repr`.
- [x] Compiler-owned Big, Const, Term, Storable and Little traits have the
  documented superclass relations. Representation queries never choose a
  type for an unresolved variable.
- [x] Datatype contexts retain ordinary predicates, hidden representation
  predicates and internal `Apply` obligations. Partial applications check a
  predicate as soon as all its free parameters are supplied.
- [x] Substitute saturated transparent alias bodies before representation
  checks. Preserve nominal record aliases, enforce alias casing, and resolve
  bounds around record bodies using the enclosing alias representation.
- [x] Close SCC contexts with a deduplicated worklist and no iteration cap.
  Replay rejects constructed substitutions for applied-relevant parameters.
  The permutation regression requires more than 256 replay steps.
- [x] Infer trait parameter kinds across methods and superclasses; check
  specialized method signatures and recursive impl heads against them.
- [x] Keep head-only coherence. Representation contexts cannot distinguish
  otherwise overlapping heads. Reject user implementations of representation
  traits and compiler-owned Big Eq/reflexive Lift instances, including
  transparent aliases with variable bodies.

## Inference, generalization and evidence

- [x] Enforce formation contexts for annotations, constructors, imports,
  lists, builtin values, instantiated schemes, aliases and variable-headed
  applications. Descriptor normalization preserves supplied argument order.
- [x] Reduce known `Apply` heads to datatype contexts; defer unknown heads.
  Attach child requirements to their parent obligation. Internal `Apply`
  consumes no dictionary slot and emits no runtime evidence.
- [x] Preserve Haskell 98 kind contracts through generalization,
  specialization and independent local/imported instantiation. Kind checks
  cannot silently specialize quantified variables to higher kinds.
- [x] Normalize transparent aliases only for representation proof comparison;
  ordinary trait matching and nominal alias identity remain unchanged.
- [x] Carry representation evidence and stable dictionary slots through
  superclass paths, recursive calls, retained impl children and interfaces.
- [x] Reject incompatible representation requirements before ambiguity
  defaulting or scheme publication. Keep ordinary literal defaulting,
  recursive impl patterns, operators, and option/result do behavior.
- [x] Export closed kinds and datatype/value contexts; include ordered
  application arguments and representation requirements in fingerprints.

## Shipping core and acceptance

- [x] Use one elementwise `Eq (list 'a)` implementation. Keep structural
  Big Eq compiler-owned. The later Big-list fast path is restricted to Eq;
  no optimizer implementation is part of this delivery.
- [x] Replace obsolete lattice expectations with Haskell 98 regressions.
  Keep all 22 supplied declaration fixtures with explicit expected outcomes.
  Cover local and imported higher-kinded applications, partial arguments,
  alias substitution, contradictions, and infinite kinds.
- [x] Review changed snapshots, including contexts and dictionary indices.
- [x] Final formatting, strict Clippy and full workspace tests.
- [x] Snapshot verification and unreferenced-snapshot hygiene.
- [x] Real `tests/core` CLI fixture and focused cross-module acceptance.
- [x] Sampo changeset, dependency propagation and publication-order audit in
  a disposable release copy.

Public API changes affect nash-source, nash-ast, nash-parse, nash-can,
nash-constrain, nash-solve and nash-driver. Release preparation must update
all dependent version requirements before publication. No commit, push or
publication is authorized by this plan.

The approved later-plan deferrals remain runtime/codegen, optimizer
implementation, default imports, Fuzz and the full validator example.

## Final acceptance record

Formatting and strict Clippy pass. `cargo test --workspace` passes 2,027 tests;
three existing doctests are ignored. `cargo insta test --workspace --check
--unreferenced delete` passes with no unreferenced snapshots and no pending
review. Source searches find none of the removed kind types or callers.

The shipping CLI fixture compiles 23 modules and 215 declarations. Fresh
24-module cross-module acceptance covers exported inferred wrapper contexts,
partial pair heads, and generic do instantiated at option and result. Separate
projects reject a Term list element through imported `Apply`, a Term supplied
partial argument, and a user Eq impl for a generic transparent Big alias.
Representation diagnostics include their originating formation/application
chain and source line. Canonical, inference and retained-evidence regressions
cover both positive and negative cases.

Sampo 0.21.0 release planning and preparation were run only in a disposable
copy. All crates are pre-1.0, so the planned bumps are minor for
source, ast, parse, can, constrain, solve and driver, and patch for the CLI. All 28 internal dependency requirements match the
prepared versions. Publish order is source, ast, parse, can, constrain, solve,
driver, CLI. The publish dry run packages and verifies source, then stops at
ast because the new source version is not available on crates.io. Thus dependency
propagation and publication order are verified; full downstream registry
package verification is not established. No upload, tag, commit or push was
performed. The working checkout's Cargo manifests and lockfile are unchanged.
