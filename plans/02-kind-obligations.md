# Follow-up to Plan 02: inductive residual kind obligations

Status: historical implemented plan. The retained-obligation engine remains
in the checkout. Its replacement is
[02-kind-predicates](02-kind-predicates.md) (Haskell 98 kinds +
representation predicates), specified in [kinds](../docs/kinds.md). No
production replacement has landed.

The former `docs/kind-obligations.md` design was removed. The checked items
below record the earlier implementation, including its finite admission
fragment and cost safeguard; they are not claims about the replacement.

## 0. Review the semantic boundary

- [x] Review residual substitution, supplied-prefix checks, result-slot scope,
  rigid capability implication and all 22 case derivations.
- [x] Adopt the finite expansion fragment. It guarantees termination but deliberately
  rejects some finite proofs. Its necessity for unrestricted n-ary rules has
  not been established.
- [x] Keep unrestricted expansion outside this implementation. The proof counts
  admitted vectors, root partitions, bounds, and acyclic bindings, then bounds
  the depth and size of the finitely branching proof forest.
- [x] Make any chosen broader fragment precise before claiming termination;
  retain a distinct unsupported diagnostic where the proof does not apply.

Deliverable: implemented rules and reviewed proof boundary in the design.

## 1. Separate operational safeguard

The allowance remains a secondary cost safeguard. The finite-fragment proof
does not rely on it:

- [x] Route apply, unify-triggered settlement and direct settlement through
  one per-operation allowance, following the existing fail-closed proof-query
  policy (`proves_extension` has a 16,384-step allowance).
- [x] Count expansion work at bounded primitives, not just outer loop passes.
- [x] Return a distinct limit outcome with a source diagnostic. Do not map
  exhaustion to a proved infinite kind or silently accept it.
- [x] Preserve the primary failure, stop dependent cascades, and let unrelated
  declarations report their own errors with independent allowances.
- [x] Propagate limit through overlap, ground-kind and annotation proof APIs.
  In particular it cannot mean disjoint impls or successful evidence.
- [x] Replace the earlier prototype's limit-as-Infinite expectations before
  accepting any snapshots. Do not count its all-error result as the semantic
  expectation for the 22 cases: case 3 must ultimately pass.

Acceptance: low injected allowances exercise each entry point and prove
failure propagation. A generous allowance does not establish finiteness.

## 2. Preserve application spines and scopes

- [x] Add snapshot tests for n-ary retained constraints before changing the
  representation. Include two and three arguments, nested argument spines,
  annotated heads and observable prefix results, partial aliases, and shared result constraints.
- [x] Change `nash_ast::KindApplication` and inference `Application` to a head,
  argument slice and one result. Add explicit binder/slot ownership as needed.
- [x] In `Walker::infer_type` / `apply_args`, collect the whole spine. Keep
  source regions and argument indexes; do not flatten argument subexpressions.
- [x] Update instantiation, generalization, connected-variable traversal,
  interfaces and fingerprints together. Audit all producers and consumers in
  can/constrain/solve; do not infer scope from vector position accidentally.
- [x] Preserve the distinction between externally shared roots and local
  result slots. Test that a local witness cannot specialize a protected root.

Acceptance: no retained result slot is a head solely because one source spine
was split into binary applications. Existing Plan 03 existential-witness and
shared-binder regressions continue to pass.

## 3. Specialize residual constructors

- [x] Keep eager declaration-SCC inference. Add declaration-local Storable
  narrowing and conflicting-bound errors, checking diagnostic locations.
- [x] Replace `open_constructor` replay with capture-avoiding simultaneous
  specialization of captures and the supplied argument vector.
- [x] Keep unsupplied parameters in the residual binder, without generating
  global fresh roots. Retain all dependent premises in that residual.
- [x] Check already supplied bounds, sharing and determined prefix obligations
  now, including phantom consumers and alias parameter bounds.
- [x] Follow producer dependencies when splitting residual constraints. Check
  known arity even with unresolved arguments; test partial `p self` where
  `p f a = P (f f) a`, whose closed self/self premise must already fail.
- [x] Read outer result shape without claiming validity; enqueue demanded
  saturated constraints separately. Keep residual qualification when checking
  an abstract constructor application or unqualified arrow capability.
- [x] Freshen independent use slots and preserve external identities across
  retained interfaces and whole `ValueKinds` instantiation.

Acceptance: `self tag`, `tag tag`, finite `app (app tag) tag`, independently
bounded constructors, dependent aliases, and Functor option across Const/Term.
Reject partial pair with a Term element, including beneath phantom tag.

## 4. Schedule local errors and enforce inductive proofs

- [x] Collect and check outer arity/value-shape and ground bound constraints
  before expanding nested obligations; repeat this priority after substitution.
- [x] Track ancestor paths on pending applications and expansions. Compare schemes
  and ordered arguments through live roots, preserving captured arguments and
  external sharing. Use no completed-proof cache or shared mutable use slots.
- [x] Reject required active cycles. Keep the structural occurs check distinct.
- [x] Compare ancestor keys under current root refinement. No old completed proof
  is cached, so there is no success to invalidate or reuse after narrowing.
- [x] Implement the agreed admission rule before recursive expansion, including
  the fixed atom universe and non-escaping slots if using the proposed fragment.
- [x] Verify that every graph operation matches one of the design judgments.
  Review the finite-state argument against actual storage and allocation.

Acceptance: `self self` and the saturated arity-two cousin reject inductively;
all acyclic controls pass. Capture-distinct goals do not become false cycles.
Fragment rejection and allowance exhaustion are different from KindInfinite.

## 5. Lock down source behavior and Plan 03 boundaries

- [x] Add all 22 complete fixtures in supplied order: 20 outer-value mismatches,
  case 2 internal field mismatch, case 3 success. Check these expectations from
  the design, independently of the old binary solver's output.
- [x] Exercise declaration, value annotation and impl-head routes. Split the
  current mixed self/tag annotation fixture so self/tag success remains clear
  while self/self gets its own error snapshot.
- [x] Update `retained_self_application_*` success snapshots to the agreed
  inductive outcomes. Preserve finite nested-application snapshots.
- [x] Check `proves_extension`, `proves_values`, ground Big/signature proofs,
  annotation specialization and impl overlap under every failure category.
- [x] Preserve rigid annotation promises, superclass/trait evidence policy,
  independent application kinds, alias bounds and cross-module shared binders.
- [x] Add explicit `tag (self self)` rejection, imported residual success/cycle
  cases, and an unused declaration whose parameter has incompatible Storable
  and applicable requirements. Eager declaration errors cannot wait for use.
- [x] Include a finite example outside the chosen fragment as a documented
  restriction test, and alpha-renaming/sharing counterexamples for graph keys.

Acceptance: no incomplete query can grant evidence, narrow a rigid annotation,
or remove an overlap. No default-import, exhaustiveness or codegen work enters
this change.

## 6. Validation and documentation

- [x] Run focused snapshot tests; inspect then accept intended changes.
- [x] Run `cargo fmt --all`, strict all-target/all-feature Clippy and `cargo test`.
- [x] Run the CLI against the 22 fixture projects and controls. Bound execution
  externally to detect regressions; report this as a runtime check, not a proof.
- [x] Update `docs/kinds.md` self-application and partial-application sections to
  the implemented rules; distinguish design acceptance from current behavior.
- [x] Add Sampo changesets for changed public AST/interface and checker behavior,
  with downstream dependency/publication order preserved.
- [x] Mark these chunks complete only as each lands; record any narrower
  admission boundary and tests in the design.

Validation (2026-09-07): focused semantic/snapshot tests, strict Clippy, and the
full test suite pass. All 22 supplied CLI fixtures match the design (case 3
passes; 21 KindMismatch failures). CLI invocations used external timeouts, which
are runtime regression checks, not the termination proof. Snapshot changes
were inspected before acceptance.

The CLI case sources are retained in
`crates/nash-can/tests/fixtures/kind-obligations.md`, in supplied order. The
read-only termination audit checked root bindings, frozen capability binders,
and admission before premise matching. The test audit checked imports, phantom
consumers, cycles, eager errors, retained vectors, and Plan 03 boundaries.

The Sampo changeset covers AST, can, constrain, solve, driver, and CLI. Cargo
metadata confirms the existing dependency/publication order: AST before can
and constrain, then solve, driver, CLI. No package version or dependency edge
was reordered. Broader fragment admission remains future work; it requires a
new proof, not a larger allowance.

