# Plan 08 — O1 optimizer (Core and UPLC)

## Status and accepted scope

**O1 application fusion, native packing and late binding cleanup are part of
this plan (5 October 2026).** The user moved these items here from Plan 15 and
requested implementation. The original eleven chunks remain complete; the
additional scope in Chunk 12 is complete and validated. All 3,876 workspace tests
and strict Clippy pass; all 199 performance cases match the reviewed baseline and
produce identical complete O1 code after one, two and three runs.
O2 silence remains separate in Plan 15.

The existing Core passes retain one ANF normalization and their joint fixed
point, with no optimizer resource or iteration caps. Recursion is encoded once;
late UPLC cleanup has its own shrinking fixed point. See the
[convergence follow-up](../docs/research/optimizer-convergence.md) for the prior
195-case checkpoint and the [application report](../docs/research/application-packing-trial.md)
for the new scope.

**Placement correction (4 October 2026).** Constructor folding and representation
cancellation run only in the post-ANF loops. The early `reduce_constr` and
`inverse::reduce` calls were removed at the user's request. Existing bound-constructor
and inverse loops handle bindings produced by codegen and opportunities exposed by
inlining. Earlier pre-ANF placement/measurements below are historical, superseded
by this decision; synthetic adjacent-expression savings do not justify early passes.
The 23 source/validator performance rows are unchanged. The direct-expression
regressions and changed constructor cases are recorded in the refreshed baseline.

Chunks 1 and 2 are accepted and complete, including mandatory Core typing,
pre-ANF static-parameter lifting and ANF. Chunk 3 is complete with an isolated
performance runner. The user
accepted Chunk 4 rules 1 and 2, including repeated-constant propagation and
their fixed-point loop. Rule 3 was accepted on 27 September 2026. The measured Rule 4 identity and
builtin-wrapper cases are accepted, including their measured size tradeoffs;
conditional bodies are excluded for now, including recursive-only inlining.
Broader body-size heuristics, partial/indirect-call expansion and further Rule 4
experiments are deferred. The retained cases are implemented in `nash-ir::small_inline` and integrated
with the candidate/performance pipelines. Chunk 5 step 1 adds outermost forced-builtin
sharing during optimized lowering. Step 2 is accepted with a two-occurrence minimum
for one leading literal; the retained Chunk 5 scope is complete. Chunk 6 safe
unused-binding removal, recursive-member reachability and pre-ANF nonrecursive
unused-parameter removal are accepted. Direct force/delay cancellation in Chunk 8
is also accepted. Recursive unused-parameter removal completes the approved Chunk 6 scope. Known Boolean, direct/bound native-constructor and
native-list folding in Chunk 7 are accepted. Literal Data-shape folding is also
accepted, as is IData/BData/ListData/MapData/ConstrData producer case folding;
known native-constructor field and native integer/byte case folding are accepted
as well. The retained Chunk 7 scope is complete. Chunks 9 and 10 are now
integrated; the retained scope and Chunk 11 review are complete. The retained Chunk 8 representation cancellations
are accepted, including integer/byte/list/map Data round trips, UTF-8 round trips
and bound constructor Data projections/reconstruction. Normal build/test defaults are O1; explicit O0 is available in build/test and project config.
Current assembly in
`nash-codegen/src/program.rs`
selects O0 or the shared accepted O1 pipeline, then rewrites recursion and lowers.
Reuse its existing Core, Builder, traversal and free-variable facilities.

Prior experiment executables were removed on 28 September 2026 at the user's
request. The measurements below are historical records; accepted semantic
snapshots and the explicit budget regression runner remain. Future trial sources
are temporary and should be removed after recording their decision.

Test layout cleanup (28 September 2026): codegen optimizer tests are grouped by
pass. `known_case.rs` contains Boolean, constructor, list and Data submodules;
`dead_code.rs` contains binding and recursive reachability submodules.
`unused_params.rs` is one pre-ANF suite using the actual accepted placement;
the old post-ANF test helper is removed, while its useful semantic fixtures remain.
Complete calls with later effectful arguments now test successful pre-ANF reduction,
rather than retaining the obsolete post-ANF no-change expectation. Independent
properties follow snapshots; snapshots alone specify exact visible structure.
Production pass behavior, the accepted pipeline and performance baseline are unchanged.
At this cleanup checkpoint, Data folding was still isolated pending acceptance;
historical measurement records remain.
Cleanup validation: 498 codegen and 69 IR tests pass, along with strict workspace
Clippy and formatting. All 188 snapshot moves retain their fixtures; O0 sections
are unchanged. All 23 performance baseline rows still match. Nextest stalled
during discovery, so the focused suites were run through Cargo instead.

Accepted decisions (26 September 2026, reconciled with later keep decisions):

- Start with binder hygiene, static-parameter lifting and A-normal form (ANF),
  retaining explicit recursive workers in Core. Lift before ANF splits calls.
  The later accepted adjacent-call fusion rule may recover a call only under
  its single-use, atomic-operand and type-view guards.
- Optimize while recursive functions remain explicit `LetRec`, then rewrite
  recursion once and lower the generated code directly. Normalize only once,
  before Core optimization; do not rerun ANF after recursion rewriting.
- Consider inlining small functions even when used multiple times. This is a
  candidate to measure, not blanket permission to duplicate code.
- Evaluate optimizations one chunk or one individual rewrite at a time. Review
  the result with the user and keep, revise or discard it before proceeding.
- Keep permanent performance regression tests and temporary per-chunk
  experiments, on an explicit performance-only execution path. Ordinary
  `cargo test` and `cargo nextest run` must not run either category.
- Decide CPU, memory and serialized-size tradeoffs case by case. There is no
  universal priority order or requirement that every metric improve.
- Decide the accepted optimizations before deciding flags or optimization levels.
  Previous O1/O2 assignments and numerical thresholds are not accepted policy.
- Preserve O0 as the existing pre-optimizer semantics baseline. Do not replace its
  snapshots with optimized output.

Specification: [docs/codegen.md](../docs/codegen.md), section 6.
Progress is tracked in [SPEC.md](../SPEC.md); listing a candidate here does not
mean it is implemented or accepted for the final optimizer.

## Execution and review contract

For each chunk, or each independently reviewable optimization within a chunk:

1. State the exact rewrite, its preconditions, and the expected benefit.
2. Add ordinary semantic tests and paired before/after snapshots first. A Core
   optimization shows `--- core before` and `--- core after` in the same snapshot;
   a UPLC optimization shows `--- uplc before` and `--- uplc after`. Include
   downstream UPLC and evaluation sections as needed to verify semantics.
   Integration fixtures use the same sectioned format as unit fixtures; do not
   split one fixture's Core, UPLC and outcomes into separate snapshot files.
3. Implement the smallest candidate; use temporary internal harness wiring to
   exercise it without enabling a public optimization mode.
4. Compare against the unchanged baseline: results, traces, errors, termination,
   CPU, memory, and serialized size. Record inputs, target version, cost model,
   baseline revision and candidate revision. Include favorable and adverse cases.
5. Present the concrete code, snapshots and measurements to the user. A passing
   test suite alone does not approve a candidate. Wait for the keep/revise/discard
   decision before advancing to the next chunk or optimization.
6. If kept, retain functional coverage and meaningful performance regressions,
   record the decision and make a focused local jj commit. If rejected, remove
   candidate code and candidate-only fixtures; retain a concise finding and any
   independently useful semantic coverage. Delete disposable experiment code
   after review unless promoted into the permanent performance suite.

A shared chunk may contain multiple candidates, but do not implement the whole
plan before review. The candidate review is the user-requested stopping point,
not a request to reauthorize routine implementation or testing within that chunk.

## Pipeline and invariants

```text
O0: Core -> recursion rewrite -> UPLC lowering

Candidate optimized pipeline:
Core with LetRec -> unique names -> static-parameter lifting -> ANF
  -> accepted main passes -> refresh recursive groups
  -> recursion rewrite once -> unique names -> UPLC lowering
```

Assembly coordinates the phases. Passes belong in `nash-ir`; recursion rewriting,
lowering and the evaluation adapter remain in `nash-codegen`. Do not introduce
an IR-to-codegen dependency. Reuse the existing nash-plutus evaluator and encoder.

Every rewrite must preserve values, trace content/order, failures and termination.
An unused computation can still be strict and observable. Conservative safety
analysis must cover logging and divergence as well as throwing. Allocation and
execution budgets can change; report those changes separately from semantics.

### ANF and evaluation order

- Define one atom predicate shared by normalization, invariant checking and
  passes. Variables and literals are atoms; lambda/delay bodies normalize in
  their own scopes. Decide precisely how values and bare builtin references fit.
- Name non-atomic operands of applications, builtins, constructors, projections,
  forces and case subjects. Let RHSs and tail positions may be computations.
  Reassociate administrative lets without capture or pointless literal aliases.
- Fresh binders must carry accurate types. Establish intermediate application
  types through the existing type/representation machinery; do not invent fake
  types or erase representation merely to manufacture a binding.
- Keep work inside its original case branch, lambda or delay unless movement
  has a separate semantic proof. Preserve strict subject evaluation exactly once.
- Preserve staged application: applying an earlier argument can fail before a
  later argument is evaluated. Do not hoist all arguments ahead of those stages.
- Preserve trace order: evaluate the message, emit it, then evaluate the body.
- Preserve strict constructor fields, including ignored fields of a folded case.
  Big field extraction and shared wildcard helpers retain their selected scopes.
- Each main optimization pass preserves ANF. Inline atoms or splice binding
  sequences at call sites; do not substitute a compound expression into an atomic operand. Avoid cycles
  between substitution and reintroducing identical administrative bindings.

### Recursion boundary

- Traverse `LetRec` bodies under their parameter scopes, retaining simultaneous
  group scope and the continuation. Creating a group does not execute its bodies.
- Remove dead members by reachability from the continuation, including escaping
  references, rather than counting internal self uses.
- Do not unfold recursive calls in the inliner or repeatedly expand generated
  self-application/dispatchers during cleanup. Nonrecursive helpers may inline
  into recursive bodies.
- Remove recursive parameters only when all affected uses can be safely rewritten,
  including group-internal calls. Preserve strict argument evaluation and staging;
  keep unsafe partial/escaping signatures. Do not create unsupported zero-parameter
  recursive values.
- Lift static parameters before ANF while complete source calls remain visible.
  Retain explicit recursive workers, so main optimization still precedes knot
  rewriting. Recompute recursive groups before final rewriting. Discovering
  additional static parameters after optimization is a separate future candidate;
  do not add ANF application-chain recovery as a prerequisite.
- Freshen binder occurrences again after rewriting: generated lambda subtrees can
  be shared at multiple use sites. Do not normalize again: generated wrappers,
  self-applications, packets and cases may contain non-atomic operands. Lowering
  accepts this nested Core. Do not run ANF-dependent passes across this boundary.
  Any future post-rewrite cleanup requires a separate review and must accept
  nested Core or operate on UPLC; it must not reintroduce a second ANF pass.

## Chunk 1 — Shared analysis and hygiene

**Complete — kept by user review (26 September 2026).**
`nash-ir::analysis` provides lexical occurrence reports, free variables,
conservative discard safety and structural size. `nash-ir::hygiene` provides
scope/uniqueness checks, alpha-renaming and capture-free substitution. Existing
Builder/name supply and traversal are reused; codegen re-exports the existing
free-variable algorithm from IR. Assembly does not call any new transformation.

Occurrence reports distinguish branch, lambda, delay and recursive-body scopes;
empty-parameter lambdas add no execution boundary. Substitution freshens each
inserted copy and preserves free names, but does not by itself justify inlining
an effectful or strict expression. Discard safety assumes variables already
denote values and conservatively rejects calls, saturated builtins, forces,
cases, projections and recursive groups. Size counts nodes and binders, including
repeated occurrences; it excludes literal payload and is not a Flat-size estimate.
Semantic tests belong to normal Cargo discovery; this chunk adds no
performance experiments or performance baselines.

Validation: 17 new semantic tests cover the analyses and transformations,
including evaluated capture avoidance and strict-effect fixtures. Core/UPLC and
analysis snapshots were reviewed. `cargo fmt --all`, Clippy with all targets and
features and warnings denied, and `cargo test` pass. Existing O0 snapshots are
unchanged. No performance claim is made for this infrastructure-only chunk.

Audit and reuse existing `nash-ir/src/traverse.rs`, Core/Builder and codegen name
supply facilities. Add only missing capture-free substitution, binder hygiene,
occurrence analysis, safe-to-discard analysis and size estimation.

Occurrences must track execution scope and lambda/delay boundaries, not just use
counts. Size analysis must support `LetRec`: recursion has not been rewritten yet.
Estimated Core size guides candidates; actual Flat size judges the emitted result.
Check name uniqueness and binding scope after transformations; renaming alone is
not evidence that a faulty substitution preserved semantics.

Tests: shadowed binders, unbound/duplicate names, recursive groups, branch-local
uses, delayed uses, partial builtins and strict fields. Safe-to-discard must reject
tracing, failing and potentially diverging computations.

**Done when:** shared analyses are tested and reviewed; no optimization is enabled.

## Chunk 2 — Static-parameter lifting and ANF normalization

**Typing prerequisite accepted (26 September 2026):**
Core is now `Core { ty, kind }`, with mandatory result metadata supplied or derived
at construction. Source translation retains solved runtime-specialized types;
delays, knot workers and heterogeneous dispatch use explicit internal runtime
descriptors. Existing deliberate parametric erasure remains supported. There is
no optional metadata lookup or missing-type presence check. Tests cover source
nominal identity, partial functions, erased generics, fields and recursion-created
values. Core annotation snapshots change; emitted UPLC and evaluation remain the
baseline. The user accepted this prerequisite and the paired snapshot contract;
The accepted static-lifting and ANF implementation below builds on this prerequisite.

Validation: 11 new tests cover builder typing, source metadata, recursion-created
types and substitution preserving explicit coercion views. Formatting and strict
workspace Clippy pass; the full workspace test run passes (3,577 passed, 3 ignored).
All 154 updated existing codegen snapshots containing UPLC retain byte-identical
UPLC, evaluation results, traces and budgets. The vesting Core annotations change
only delayed binder types; their UPLC and ledger checks pass unchanged. Vesting
fixtures now keep Core, UPLC and ledger outcomes together in one sectioned snapshot.

**27 September pipeline revision:** normalize once, before the main optimization
passes. Rewrite recursion after those passes, freshen generated binder occurrences,
and lower directly. The previous second ANF pass added a binding around `self self`
on each recursive step; it is removed along with its ANF-dependent cleanup loop.
Semantic phase snapshots retain the pre-ANF, post-ANF and rewritten Core, plus
baseline/optimized UPLC. Earlier two-normalization measurements below are history;
Chunk 3's explicit baseline is updated for this revised pipeline.

Validation: formatting, strict Clippy for both workspaces, all 20 explicit
performance cases, and the full workspace suite pass (3664 passed, 3 ignored).
The new regression test fails under the old pipeline and proves nested recursive
self-application lowers correctly with preserved results and trace order. Reviewed
19 updated phase snapshots and one new snapshot; removed the obsolete fixture
that normalized already-rewritten recursion. O0 snapshot sections are unchanged.

**Static-parameter lifting and ANF accepted (26 September 2026):**
`nash-ir::anf` provides `normalize`, the shared atom predicate and an ANF shape
checker. Atoms are variables, literals, nonempty lambdas, delays and bare builtin
references. Normalize value bodies locally; empty lambdas/applications unwrap
with their original type view. Reassociate let prefixes under globally unique
names, retaining exact result types. Preserve atomic application runs and execute
earlier stages before a later non-atomic argument. Builtins retain their n-ary
form because valid builtin nodes only execute on saturation.

Test-only codegen wiring lifts static parameters before the sole ANF pass,
then rewrites recursion and freshens binder occurrences, without normalizing again.
Production assembly and public flags are unchanged. Ten IR tests cover lifting, shape, types, hygiene and idempotence;
Twenty semantic tests produce twenty paired phase snapshots. Existing source fixtures
also compare original versus optimized execution across the recursion boundary,
including Big/little wildcard, captured/function fallback, Logic and Lift cases.
Ground values, error categories/messages and trace order are compared; returned
opaque functions are exercised through dedicated applications rather than checked
for identical lambda syntax. New optimizer snapshots do not pin performance budgets.

26 September validation: formatting and strict all-target/all-feature Clippy pass.
The full workspace test run passes (3,606 passed, 3 ignored), including doctests. The
combined implementation added 29 tests and 28 paired snapshots relative to the accepted
typing prerequisite. No snapshots are pending. The interleaved-parameter fixture
proves both captured values, retained dynamic-parameter order and initial argument
trace order; ordinary O0 source snapshots remain unchanged.

Initial ANF-only measurements (superseded by the pre-ANF lifting revision below)
ran outside Cargo test discovery using
`/tmp/nash-anf-measure.rs` (temporary experiment, not a permanent runner). Baseline
is `9f454f1c`; candidate code was working-copy revision `07d012061767ba5f875121f3218f019190724b4b`.
Target: Plutus V3 / UPLC 1.1.0, repository `nash-plutus` default V3 cost model,
default evaluation budget; size is raw Flat bytes. Results and logs agree in all
five cases. The nested arithmetic returns 43; the other cases return 42. Countdown
uses `equalsInteger n 0` and `subtractInteger n 1`. Values below are baseline →
candidate; none are acceptance thresholds.

| Input | CPU | Memory | Flat bytes |
| --- | ---: | ---: | ---: |
| Atomic `addInteger 20 22` | 181308 → 181308 | 602 → 602 | 10 → 10 |
| Nested `addInteger (multiplyInteger 6 7) 1` | 336261 → 384261 | 1004 → 1304 | 15 → 18 |
| Curried addition: `trace "first" 20`, `trace "second" 22`; body traces `"stage"` | 791802 → 935802 | 3398 → 4298 | 55 → 62 |
| Countdown 3 to 0, returning static second argument 42 | 1681056 → 2209056 | 7410 → 10710 | 39 → 49 |
| Same countdown, static argument first and computed argument second | 1681056 → 2401056 | 7410 → 11910 | 39 → 48 |

The final row exposed a loss of static lifting when ANF split a complete call.
The user rejected recovering ANF call chains and chose early static lifting.
`static_lift::lift` now captures unchanged parameters before normalization and
retains `LetRec` workers for later optimization. An all-static worker is explicitly
delayed and forced per call, without adding a dummy parameter or memoization.
The original wrapper retains its arity and argument evaluation behavior.
Genuine partial/escaping self uses and mutual groups retain the existing policy.

Revised measurements used `/tmp/nash-static-lift-measure.rs`, outside Cargo test
discovery, on candidate code revision `1a516a43fbef5d42dbffc9ce02f1843e7f0d82a9`.
With the same inputs, baseline and cost model, both countdown parameter orders
now cost 2,209,056 CPU, 10,710 memory and 49 Flat bytes. The static-first case no
longer loses capture during ANF. The other three measurement rows are unchanged.
Results and logs agree with the baseline in all five cases. ANF still adds binding
overhead; this fixes the lifting loss without claiming a speedup over O0. The user
accepted this implementation. Temporary measurement scripts and binaries were
removed after recording the results; functional tests and paired snapshots remain.

Implement `anf::normalize` and an invariant checker using the contract above.
Normalize once at main-phase entry, before recursion rewriting. Add temporary harness
access to inspect phases without choosing public optimizer flags.

Tests cover nested applications/builtins, lets, fields, constructors, case subjects,
force/delay, explicit recursion and rewritten recursion. Differential evaluation
covers trace-before-body, subject once, ignored strict fields, partial and
oversaturated calls, intermediate failure, and unselected/unforced/uncalled bodies.
Reuse Big/little wildcard fixtures, captured/function-valued fallback results,
shared helpers and `Logic` short-circuit/selected-Lift fixtures. Test normalization
idempotence and valid types/names, not just pretty output.

**Done when:** normalization preserves semantics and its invariant, with paired
before/after Core sections and unchanged baseline output; review binding overhead
in UPLC.

## Chunk 3 — Explicit performance-only test path

Implemented in `tools/optimizer-perf`, an unpublished package with its own
workspace and lockfile, explicitly excluded from the root workspace. Commands:
`measure`, `check [baseline]`, `record NEW.json`, and `experiment MODULE.nash`.
See its README for exact invocations and fixture scope. There are no performance
test targets or ordinary CI changes.

The initial 20 rows compare O0 against accepted static lifting, ANF and rules
1–3 before recursion rewriting. Inputs cover list traversal, static recursion,
Data hits/misses, field decoding, validation pass/fail, real base Logic helpers,
and six ledger scenarios for each of Vesting and VestingParam. Reports embed
source inputs, record runtime/cost-model settings and provenance, and measure
CPU, memory and raw Flat bytes. Validator size excludes applied ledger arguments;
their evaluation budgets include those arguments. Known results are checked
before recording; both pipelines must agree on results and traces.

`check` requires exact metrics, inputs and outcomes, including improvements;
`record` refuses to overwrite existing files. Initial baselines document current
accepted-pass behavior, including ANF overhead; they are not universal performance
targets. Temporary source experiments remain outside permanent fixtures and
baselines. A 120-second watchdog bounds workload compilation and execution;
each CEK run has explicit CPU/memory caps and budget exhaustion fails the command.

Representative current O0 → optimized figures (full rows in `baseline.json`):

| Input | CPU | Memory | Flat bytes |
| --- | ---: | ---: | ---: |
| List sum of 1–8 | 5788660 → 4764660 | 27872 → 21472 | 102 → 93 |
| Static countdown 8, returning 42 | 4736761 → 4448761 | 21725 → 19925 | 52 → 39 |
| Data integer match | 978518 → 690518 | 5496 → 3696 | 58 → 41 |
| Vesting claim after deadline | 2621392 → 2381392 | 14525 → 13025 | 271 → 249 |
| VestingParam claim after deadline | 2834600 → 2642600 | 15227 → 14027 | 275 → 256 |

These compare the full accepted pipeline with O0, not rule 3 in isolation.
The normalize-once revision saves 384000 CPU and 2400 memory on each recursive
fixture versus the previous pipeline; Flat size falls 137 → 134 for list traversal
and 62 → 60 for countdown. The other 18 rows, all O0 results, and all optimized
results/traces are unchanged. This baseline update was explicit and reviewed.
The later Rule 4 integration removes the remaining countdown, boolean-helper,
and decoding regressions. The table above includes the subsequent Chunk 5 step 1
sharing costs documented below. Every O0 row
and all outcomes remain unchanged. Six optimized rows reduce CPU/memory, seven
reduce serialized size, and none regresses relative to rules 1–3.

Validation (27 September 2026): all 20 explicit baselines match. Deliberately
lowering a CPU baseline by one unit makes `check` fail; changed fixture sources
also fail even with identical metrics. `record` rejects overwrites. A temporary
source experiment returns 42, and a diverging experiment fails at the explicit
budget limit. Both workspace formatting checks and strict Clippy checks pass.
Root all-feature executable tests pass (3662); the separate doctest rerun passes
(1 passed, 3 ignored). The first doctest run encountered a crate-ID collision
while concurrent builds were active; the rerun completed after those builds.
Root nextest passes all 3662 tests. Root metadata, Cargo test listing and nextest
discovery exclude the performance package; its metadata confirms a separate
workspace. Normal CI and ordinary semantic snapshots remain unchanged.

Keep two categories:

- **Permanent regression cases:** retained representative inputs and reviewed
  CPU/memory/serialized-size baselines or limits for accepted optimizations.
- **Temporary experiments:** per-chunk exploration, alternative implementations,
  threshold sweeps and adverse examples; delete after recording the decision.

Use an isolated package such as `tools/optimizer-perf/`, with its own workspace
boundary and explicit exclusion from the root workspace where needed. Neither
category belongs in automatically discovered root integration tests. A dedicated
runner can execute regression checks and experiments through explicit commands,
for example `cargo run --manifest-path tools/optimizer-perf/Cargo.toml --release
-- check`. Final runner names/arguments are implementation details, not optimizer
level decisions. Keep the harness small and reuse existing evaluation/encoding.

Do not rely solely on ignored tests or a root `required-features` gate: broader
root test commands can enable those. Verify root `cargo test`, `cargo test
--workspace --all-features`, and `cargo nextest run --workspace --all-features`
do not execute or discover the performance workload. Ordinary semantic tests
still run normally. New optimizer semantic fixtures assert behavior and code
shape, not performance thresholds. Preserve historical O0 snapshots, including
any incidental budget output already present; new dedicated performance workloads
and regression limits belong only to the explicit runner.

Require explicit baseline updates; never accept changed budgets automatically as
part of ordinary snapshot acceptance. A dedicated CI performance job may be added
only by an explicit later decision; normal CI test jobs remain unaffected.

Record versions, cost models, inputs and before/after figures. Evaluate tradeoffs
case by case with the user; no unconditional memory-first or size-first policy.
Include vesting paths, list traversal, static recursion, Data matching, validation,
decoding and boolean helper compositions. Guard experiments against unbounded
compilation/evaluation. Verify that an intentional regression fails the explicit
regression command while ordinary test discovery remains unaffected.

**Done when:** both permanent and temporary workflows work only through the
special path, isolation is verified, and initial measurements are reproducible.

## Chunk 4 — Inlining and binding cleanup

Review these independently: atom/alias propagation; direct lambda application;
safe single-use bindings; small functions with multiple call sites.

### Rule 1 — atom/alias propagation (accepted)

`nash-ir::propagate::propagate` removes variable aliases transitively, then
propagates literals with zero or one remaining syntactic uses. On 27 September,
the user also approved duplication of integer constants (no magnitude cap), byte
strings up to 64 bytes inclusive, and all three BLS constant variants (G1, G2,
Miller-loop result), regardless of use count. Other repeated literals stay shared.
Count uses after alias removal: `let x = largeString; let y = x; use y y` must retain the
shared string binding. The byte-string limit is explicitly user-selected. Lambda, delay, bare
builtin and computed bindings remain for later rules. Alias substitution never
moves the target computation. Globally unique binder IDs prevent capture;
substitutions preserve occurrence and root type views and preserve ANF.

This accepted rule is exercised before recursion rewriting in the test-only pipeline. Production assembly is unchanged. Paired Core/UPLC
semantic snapshots cover scope, sharing, strict failures and delayed captures;
performance measurements run separately from normal test discovery. The user
kept this rule on 26 September 2026. Validation: formatting, strict all-target /
all-feature Clippy, and the full workspace run (3618 passed, 3 ignored). Added
12 tests and 11 paired snapshots; existing snapshots remain unchanged.

Temporary isolated measurements (Plutus V3 default cost model, raw Flat bytes;
accepted ANF baseline versus this rule alone):

| Fixture | CPU before → after | Memory before → after | Bytes before → after |
| --- | ---: | ---: | ---: |
| Two-binding literal chain | 112100 → 16100 | 800 → 200 | 11 → 6 |
| Shared string through alias | 536842 → 488842 | 1210 → 910 | 31 → 28 |
| Alias of computed integer | 277308 → 229308 | 1202 → 902 | 15 → 13 |
| Already minimal integer | 16100 → 16100 | 200 → 200 | 6 → 6 |

These are small rule-isolation examples, not whole-program performance claims.
The temporary runner was explicitly compiled and run outside Cargo test
discovery, then removed after the keep decision. Chunk 3's permanent runner
remains pending.

### Rule 2 — direct lambda application and the rules 1 + 2 loop (accepted)

`nash-ir::beta::reduce` rewrites direct `App(Lam(...), args)` into strict
parameter bindings. Partial application binds supplied arguments outside the
remaining lambda. Exact saturation exposes the body. Oversaturation evaluates
the saturated body before applying its result to extra arguments; a non-atomic
result receives a fresh ANF binding. It does not inline named function bindings.

Leading `Let`/`LetRec` sequences are spliced into the surrounding strict context,
without crossing lambda, delay, branch, trace or recursive-function scopes.
Each pass preserves typed ANF and unique binders; no whole-tree re-normalization
is needed between iterations. Fresh names avoid all input binder and use IDs.

`beta::simplify` runs rule 1 followed by rule 2 until neither changes Core.
Unchanged-pointer preservation supplies the change flag; this is not a node-count
comparison or a fixed iteration limit. Each beta rewrite consumes at least one
lambda parameter, and rule 1 introduces none. Partial applications therefore
also make progress. The loop runs before recursion rewriting and after its final
ANF, only in the test pipeline. No UPLC pass is added.

Snapshots cover partial capture, multiple iterations, nested and recursive RHS
prefixes, explicit type views, fresh-builder name collisions, strict unused
arguments, argument order, saturated-body failure, oversaturation success and
suspended branch/delay bodies. Performance experiments remain outside ordinary
Cargo test discovery.

Temporary isolated experiments compare accepted ANF + rule 1 against rules 1 + 2
(Plutus V3 default cost model, raw Flat bytes):

| Fixture | CPU before → after | Memory before → after | Bytes before → after |
| --- | ---: | ---: | ---: |
| Identity applied to literal | 64100 → 16100 | 500 → 200 | 8 → 6 |
| Identity applied to computation | 277308 → 229308 | 1202 → 902 | 15 → 13 |
| Three nested direct applications | 160100 → 16100 | 1100 → 200 | 15 → 6 |
| Partial application then named call | 325308 → 277308 | 1502 → 1202 | 18 → 15 |
| Oversaturation with shared parameter and computed result | 448806 → 496806 | 1934 → 2234 | 35 → 37 |
| Already minimal integer | 16100 → 16100 | 200 → 200 | 6 → 6 |

The original oversaturation regression came from an added ANF binding for the
computed function result, while the multiply-used integer parameter retained its
binding under the original rule 1 policy. The table above records that initial
experiment; the approved repeated-constant policy is measured below. This
initial experiment is retained as history; the repeated-constant refinement
below removes that particular regression. No UPLC cleanup or cost heuristic
has been introduced.
The temporary runner was explicitly compiled and run outside Cargo test
discovery, then removed after the keep decision.

Repeated-constant refinement (27 September): compare ANF before these rewrites
against rules 1 + 2 with repeated integers/BLS and byte strings up to 64 bytes.

| Fixture | CPU before → after | Memory before → after | Bytes before → after |
| --- | ---: | ---: | ---: |
| Original oversaturated integer example | 448806 → 448806 | 1934 → 1934 | 35 → 35 |
| Twice-used 64-byte string | 131868 → 83868 | 916 → 616 | 78 → 142 |
| Twice-used 65-byte string | 132214 → 132214 | 918 → 918 | 79 → 79 |

The integer example loses its `x` binding, offsetting the new function binding.
The byte-string policy intentionally permits serialized-size growth to remove
binding evaluation costs. This is the user-selected 64-byte limit, not a claim
that all metrics improve. Snapshot coverage includes 64/65-byte boundaries,
large integers, all three BLS constant variants and the original oversaturation
example. These tests concern literal constants; computed BLS operations remain
bound. Native BLS Miller-loop constants have no UPLC text literal; their Core
snapshot uses the existing unsupported-constant display.

Accepted on 27 September 2026, including the repeated-constant refinement.
Validation: formatting and strict all-target/all-feature Clippy pass;
full workspace tests pass (3643 passed, 3 ignored). Added 25 tests and 23 paired
snapshots. Existing snapshots remain unchanged.

### Rule 3 — single-use values and immediate computed returns (accepted)

`nash-ir::single_use::inline` tries two ANF-preserving rewrites:

- Substitute an ANF atom at its sole syntactic use. This includes single-use
  lambda, delay and unforced bare builtin bindings. Forced builtin references
  are excluded even when returned directly: their bindings must remain shared
  for top-level hoisting. Moving a value into a branch or
  delayed scope does not execute its body. No binder-bearing body is duplicated.
- Replace `let x = computation in x` with the computation. This preserves the
  exact evaluation point, including failure, divergence and trace effects.

Other computed bindings stay bound. A sole use inside a branch, lambda or delay,
or after a trace, does not permit moving the original computation. Computed
function operands also stay bound: inlining them would violate ANF. This trial
therefore does not resolve the separate post-ANF/UPLC cleanup decision.

Count uses by globally unique ID, then rewrite bottom-up using mapped children.
Substituting original RHS pointers could resurrect bindings removed inside a
lambda or delay; tests cover that trap. Preserve occurrence and enclosing result
type views. No freshening or function-size threshold is needed. Local substitution
walks can be quadratic for long chains; retain the simple implementation absent
measured need for more machinery.

`single_use::simplify` alternates the accepted rules 1 + 2 fixed point with rule 3
until unchanged. Rule 3 removes bindings without duplicating lambda parameters,
so it cannot undo the progress argument for beta reduction. Pointer identity
remains the change flag. The test pipeline runs the candidate before recursion
rewriting only; production assembly remains unchanged.

Tests cover immediate computed returns, newly exposed beta reduction, nested
function bindings, delayed values, bare builtins, argument failures, trace order,
computed captures, computed function operands, shared functions, type views and
fixed-point idempotence. Paired snapshots show both Core and downstream UPLC.
Temporary measurements run separately through `/tmp/nash-single-measure.rs`.
Temporary experiments compare accepted rules 1 + 2 with the candidate loop
(Plutus V3 default cost model, raw Flat bytes):

| Fixture | CPU before → after | Memory before → after | Bytes before → after |
| --- | ---: | ---: | ---: |
| Immediate computed return | 229308 → 181308 | 902 → 602 | 13 → 10 |
| Single-use bound function | 277308 → 181308 | 1202 → 602 | 15 → 10 |
| Single-use builtin | 229308 → 181308 | 902 → 602 | 13 → 10 |
| Single-use delay | 96100 → 48100 | 700 → 400 | 9 → 7 |
| Computed function operand | 283598 → 283598 | 1532 → 1532 | 26 → 26 |
| Shared function | 208100 → 208100 | 1400 → 1400 | 16 → 16 |
| Forced builtin used once inside a function called eight times | 1836084 → 1836084 | 8856 → 8856 | 61 → 61 |

The initial candidate inlined the last fixture's forced builtin, adding 64000
CPU and 400 memory while saving 3 bytes. One syntactic use is not one runtime
evaluation: moving the forced builtin into the repeated function loses shared
forcing work. On 27 September the user decided
that forced builtin references must always be top-level hoisted. Rule 3 therefore
preserves every bare builtin binding whose `force_count()` is nonzero, regardless
of use count or execution scope, including direct returns. With that exception,
the eight-call fixture is unchanged: CPU 1836084, memory 8856, Flat size 61 bytes
before and after. All other measurements in the table remain as shown. Chunk 5
owns the actual top-level hoisting pass. This is a fixed policy, not a
cost/frequency heuristic.

Validation: formatting and strict all-target/all-feature Clippy pass. The full
workspace run passes 3663 tests with 3 ignored. This candidate adds 20 semantic
tests and 19 snapshots; no pending snapshots remain. Performance measurements
stay in the explicit temporary experiment, outside normal test runs.

The user accepted rule 3 on 27 September 2026. Temporary measurement source and
binary were removed after recording the results; semantic tests and snapshots remain.

### Rule 4 trial 1 — multiple-use identity helper

The first of three concrete experiments is `\x -> x`. It is measured before
choosing any size threshold; builtin wrappers and conditional helpers remain
separate experiments in this historical trial.

The trial selects a fixture binding by unique ID, freshens its lambda at each
fully applied direct call, and reuses the accepted rules 1–3 loop. It removes the
shared pure lambda binding only if all uses were replaced. The trial does not
recognize identity bodies as a compiler special case and installs no production
pass or general selection policy. It normalizes once, before these rewrites;
recursion rewriting and lowering follow without another ANF pass.

Baseline is the accepted normalize-once/rules 1–3 pipeline, not O0. Measurements
use Plutus V3, UPLC 1.1.0, the bundled default V3 model and raw Flat bytes. The
experiment asserts equal ground results and trace logs, expected results/logs,
ANF before recursion rewriting, binder hygiene and root types. It has explicit
CPU/memory caps and a 120-second watchdog. Paired Core and UPLC are printed by
the command; the permanent performance baseline is not changed.

| Identity fixture | CPU before → after | Memory before → after | Flat bytes before → after |
| --- | ---: | ---: | ---: |
| Two calls, summing two ones | 634516 → 394516 | 2804 → 1304 | 30 → 18 |
| Eight calls, summing eight ones | 2489764 → 1673764 | 10616 → 5516 | 99 → 60 |
| Recursive caller: setup call plus eight iterations | 7930425 → 7018425 | 36641 → 30941 | 67 → 55 |
| Two calls with traced arguments | 1073512 → 833512 | 4868 → 3368 | 60 → 48 |
| First argument fails; later trace must not execute | 59598 → 59598 | 132 → 132 | 59 → 47 |
| One selected branch; other argument traces and fails | 363598 → 219598 | 2032 → 1132 | 52 → 39 |

The recursive fixture has two syntactic uses of the identity helper and nine
runtime calls. Its recursion is retained; only the nonrecursive helper is inlined.
The simple identity body disappears after beta reduction and alias propagation,
so these cases incur no duplicated body cost in final UPLC. This does not establish
an acceptable threshold for larger helpers. No general partial/escaping-call or
polymorphic-inlining claim is made by this first trial.

The review has moved on to the builtin-wrapper experiment below. No general
inlining rule or threshold is accepted yet.
The temporary example was removed after recording the decision; accepted rules
retain semantic snapshot coverage.

### Rule 4 trial 2 — multiple-use builtin wrapper

The same temporary harness also measured `\x -> addInteger x 1`.

It uses the same selected-binding rewrite, accepted baseline, semantic checks,
measurement settings and six call patterns as trial 1. Only the helper body and
expected results change. No builtin-wrapper exception is installed in the compiler.

| Wrapper fixture | CPU before → after | Memory before → after | Flat bytes before → after |
| --- | ---: | ---: | ---: |
| Two calls | 964932 → 820932 | 3608 → 2708 | 34 → 32 |
| Eight calls | 3811428 → 3379428 | 13832 → 11132 | 104 → 117 |
| Recursive caller: setup call plus eight iterations | 9417297 → 8937297 | 40259 → 37259 | 72 → 70 |
| Two calls with traced arguments | 1403928 → 1259928 | 5672 → 4772 | 65 → 62 |
| First argument fails; later trace must not execute | 59598 → 59598 | 132 → 132 | 63 → 61 |
| One selected branch; other argument traces and fails | 528806 → 432806 | 2434 → 1834 | 57 → 53 |

All six preserve results and logs. Unlike identity, the builtin body remains at
each call site: eight static uses save CPU and memory but add 13 Flat bytes. The
recursive caller has only two static uses and nine runtime calls, so it saves
runtime costs while reducing size by two bytes. Review this tradeoff before the
third (conditional-helper) experiment; no size threshold follows from this alone.

### Rule 4 — applying the accepted identity case to source workloads

The user kept all measured identity and builtin-wrapper cases, including the
small size growth. The experiment also measured source workloads.

This compiles the existing `booleanHelpers` and `staticRecursion` workloads,
selects each fixture's outer literal-conversion helper binding, and applies the
same selected-helper inlining plus rules 1–3 cleanup. No new case folding,
second ANF normalization, or general function-selection policy is added.

| Workload | CPU O0 / rules 1–3 / plus Rule 4 trial | Memory O0 / rules 1–3 / plus Rule 4 trial | Flat bytes O0 / rules 1–3 / plus Rule 4 trial |
| --- | ---: | ---: | ---: |
| Boolean helpers | 400100 / 496100 / 160100 | 2600 / 3200 / 1100 | 29 / 34 / 18 |
| Static countdown | 4736761 / 6320761 / 4448761 | 21725 / 31625 / 19925 | 52 / 60 / 39 |

Both source-workload regressions disappear in this trial. Results and empty trace
logs agree across all three variants; the trial asserts expected results, ANF,
binder hygiene, and unchanged root types. The boolean expression retains its
case-result binding and known cases; countdown retains bindings for its comparison
and decrement. The permanent baseline and production pipeline remain unchanged.

### Rule 4 trial 3 — smallest comparison-based conditional helper

Measured `nonNegative x = if lessThanInteger x 0 then 0 else x` through the same
explicit selected-helper experiment.

This is one comparison and one conditional, with only a literal and a variable
in its branches. Body size is fixed. No constant evaluation or case folding is
added. Baseline is accepted rules 1–3; candidate adds selected-helper inlining
and their existing cleanup loop, with ANF only once. Same V3 model, raw Flat
sizes, budgets, watchdog, result/log comparison and IR checks as earlier trials.

The 25 samples cover 2/4/8/16 fully applied call sites with all positive, all
negative, and alternating inputs; 2/4/8/16 mutually exclusive sites with only the
first or last selected and every other argument tracing then failing; two static
sites making nine runtime calls via recursion with positive or negative inputs;
traced computed arguments; first-argument failure; and the zero boundary.

| All sites execute | CPU before → after | Memory before → after | Flat bytes before → after |
| --- | ---: | ---: | ---: |
| 2 sites | 1013096 → 869096 | 4606 → 3706 | 41 → 48 |
| 4 sites | 2010092 → 1770092 | 9012 → 7512 | 65 → 92 |
| 8 sites | 4004084 → 3572084 | 17824 → 15124 | 111 → 180 |
| 16 sites | 7992068 → 7176068 | 35448 → 30348 | 204 → 357 |

Positive, negative, and alternating inputs have the same costs in this matrix.

| Other usage | CPU before → after | Memory before → after | Flat bytes before → after |
| --- | ---: | ---: | ---: |
| 2 sites, first selected | 333390 → 237390 | 1901 → 1301 | 42 → 47 |
| 4 sites, first selected | 333390 → 237390 | 1901 → 1301 | 80 → 105 |
| 8 sites, first selected | 333390 → 237390 | 1901 → 1301 | 156 → 221 |
| 16 sites, first selected | 333390 → 237390 | 1901 → 1301 | 308 → 453 |
| 16 sites, last selected | 781390 → 685390 | 4701 → 4101 | 307 → 453 |
| 2 static sites, 9 recursive runtime calls (either input sign) | 9634035 → 9154035 | 44750 → 41750 | 79 → 85 |
| Traced computed arguments, both signs | 1569300 → 1425300 | 6772 → 5872 | 80 → 85 |
| First argument fails | 59598 → 59598 | 132 → 132 | 70 → 75 |
| Zero boundary | 799888 → 655888 | 3904 → 3004 | 34 → 40 |

All 25 semantic checks pass; 24 reduce CPU and memory, and the early failure is
unchanged. All grow in size. This supports runtime call-overhead savings for
this particular body, but not unlimited duplication: cold sites add bytes without
adding runtime savings. No threshold or production rule is chosen from these
synthetic measurements. The user subsequently deferred conditional inlining; retain these measurements
as evidence, not as an accepted transformation.

### Rule 4 trial 4 — conditional inlining only inside recursive bodies

At the user's request, the same conditional is inlined only at direct call sites
inside `LetRec` function bodies. The `LetRec` continuation is not selected. This
is a syntactic trial restriction, not a loop-frequency estimate or a production
policy. Identity and builtin-wrapper acceptance is unchanged.

The matrix uses 1/2/4/8/16 sites and 0/1/4/8 iterations, with two outside calls
so the shared helper remains after normal rules 1–3 cleanup. It also covers
mutually exclusive sites (first/last selected; unselected arguments trace and
fail), and two cases with no outside uses. All 30 pass expected outcomes,
trace equality, ANF, type and binder checks under the existing 100M CPU cap.
An initial 32-iteration sweep exceeded that cap for larger site counts; it was
replaced by the bounded 0/1/4/8 sweep, not treated as a semantic mismatch.

| Recursive-only case | CPU saved | Memory saved | Flat bytes before → after |
| --- | ---: | ---: | ---: |
| 1 site, zero iterations, two outside calls | 0 | 0 | 98 → 108 |
| 1 site, eight iterations, two outside calls | 384000 | 2400 | 98 → 108 |
| 2 sites, eight iterations, two outside calls | 768000 | 4800 | 109 → 130 |
| 8 sites, eight iterations, two outside calls | 3072000 | 19200 | 179 → 263 |
| 16 sites, eight iterations, two outside calls | 6144000 | 38400 | 272 → 440 |
| 16 sites, eight iterations, only first executes | 384000 | 2400 | 375 → 536 |
| 2 sites, eight iterations, no outside calls | 816000 | 5100 | 91 → 97 |

Compared with unrestricted inlining on the identical matrix, retaining the two
outside calls saves roughly 6–7 bytes but gives up 144000 CPU and 900 memory.
If no outside calls exist, the two strategies are identical. Restricting by
recursive scope does not prevent duplication of cold branches within that scope.

The previous 25 conditional samples also pass with `--recursive-only`: all 23
nonrecursive controls have unchanged metrics. The two recursive samples each
leave just one outside helper use, which accepted Rule 3 then inlines; their final
output is identical to unrestricted inlining. This cleanup interaction is expected
and is why the new matrix retains two outside uses.

Decision (27 September 2026): leave conditional bodies out for now, including
this recursive-only variant. Defer further Rule 4 experiments and broad heuristic
design; retain the already accepted identity and builtin-wrapper cases. No
production code or permanent performance baseline changes. Strict experiment
Clippy and formatting pass.

### Rule 4 retained scope and deferred work

The user accepted the established identity and small builtin-wrapper cases,
including their measured size growth. Conditional bodies are excluded for now;
recursive placement does not override that exclusion. This decision does not
restrict the accepted identity/builtin-wrapper cases to recursive bodies.

Defer broader body-size/growth heuristics, larger bodies, partial-application
expansion, indirect/escaping-call expansion, and further exploratory experiments.
Do not pick an arbitrary size threshold or add per-program search/tuning machinery.

Implementation: `nash-ir::small_inline` selects the accepted shapes, preserves
strict arguments through existing beta reduction, freshens copied parameters,
retains definitions with other uses, and composes with rules 1–3 cleanup. The
candidate harness and explicit performance runner use the composed pass. Paired
semantic snapshots exercise the accepted shapes and exclusions. The reviewed
20-row baseline now includes Rule 4; O0 and all results/traces are unchanged.
Build configuration remains deferred to Chunk 11, rather than enabling an
unreviewed default or inventing flags.

Validation (27 September 2026): all 458 IR/codegen tests pass, including 12 new
paired semantic snapshots. Formatting, strict workspace Clippy and strict
performance-runner Clippy pass. All 20 reviewed performance baselines match.
Chunk 4's retained scope is complete; the deferred cases remain out of scope.

**Done when:** retained cases are implemented with semantic snapshots, verified
measurements and the keep decision recorded. Deferred cases do not block completion.

## Chunk 5 — Builtin sharing

Treat force caching and constant currying as separate review units.

- Always hoist forced builtin references to the validator's outermost binding
  prefix, outside its argument lambdas, and reuse them throughout its body,
  including one-use references. For non-validator entry points, use the same
  outermost program scope. Bind each distinct forced builtin once per program.
  This is the user's
  27 September decision; no use-count, size or budget threshold gates it.
  Hoist only the forced builtin value, never its applied arguments or a
  saturated computation. Rule 3 must preserve these bindings. Measure standalone
  calls, loops and branch-local uses to document costs, not to choose placement.
- Share repeated constant partial applications at a safe common scope. Only hoist
  safe partial applications; never pre-evaluate a failing saturated call.
  Move a constant across operands only when the operation and evaluation order
  permit it. Equality/addition examples do not justify reordering subtraction or
  comparisons indiscriminately.

Preserve ANF and correct types. Measure cached forces together with pair projections
later. The old minimum-use constant of two does not apply to forced builtin
references. Ensure cleanup does not inline away intentional sharing and recreate it
indefinitely.

**Done when:** each accepted sharing rule has measurements and regressions for
profitable and unfavorable cases, including lazy scopes.

### Step 1 — Outermost forced-builtin references

Implemented in `nash-codegen::lower::lower_with_builtin_sharing`, used by the
candidate semantic pipeline and explicit performance runner. O0 `lower` and
normal build assembly remain unchanged. This rule runs during lowering, after
Core cleanup, so it also covers lowering-generated `Trace` and `ChooseData` and
cannot be undone by Rule 3. Core types and the single-ANF pipeline are unchanged.

Cache by builtin identity, independent of erased type instantiation or use count.
Each cache entry receives a fresh name above existing Core names. On completion,
wrap the whole UPLC root with strict lambda/application bindings of fully forced
builtin values, in deterministic encounter order. Arguments, partial applications,
saturated calls, delays and branches retain their evaluation positions. Filter
out references discarded with exhaustive case defaults before wrapping the root.
No constant partial-application sharing is implemented in this step.

Paired UPLC snapshots cover one/repeated uses, placement outside validator
arguments, recursion, delays, bare/partial references, polymorphic uses, one/two
forces, lowering-generated Data/Trace builtins, unchanged zero-force builtins,
trace/failure order, unselected branches, and discarded Bool/List/Data defaults.
De Bruijn conversion checks closed scope; repeated lowering checks determinism.

CPU/memory/Flat-byte deltas versus identical lowering without sharing (positive
means extra cost). Both programs include a validator argument and are applied:

| Reference / workload | CPU delta | Memory delta | Flat bytes delta |
| --- | ---: | ---: | ---: |
| One force, one call | +48000 | +300 | +2 |
| One force, two sites | +32000 | +200 | +3 |
| One force, eight sites | -64000 | -400 | +3 |
| One force, loop zero calls | +64000 | +400 | +2 |
| One force, loop eight calls | -64000 | -400 | +2 |
| One force, loop 64 calls | -960000 | -6000 | +2 |
| One force, unselected branch | +64000 | +400 | +2 |
| Two forces, one call | +48000 | +300 | +3 |
| Two forces, two sites | +16000 | +100 | +2 |
| Two forces, eight sites | -176000 | -1100 | -4 |
| Two forces, loop zero calls | +80000 | +500 | +2 |
| Two forces, loop eight calls | -176000 | -1100 | +2 |
| Two forces, loop 64 calls | -1968000 | -12300 | +2 |
| Two forces, unselected branch | +80000 | +500 | +2 |

The 20-row baseline preserves all O0 rows and all results/traces. Relative to
rules 1–4 alone, list traversal saves 64000 CPU/400 memory; short Data and
validation successes add 48000–112000 CPU, and successful vesting paths add
96000 CPU/600 memory. Sizes increase by 2–4 bytes on affected fixtures. These
are recorded tradeoffs of the accepted unconditional placement, not a new
profitability threshold. Step 2 was subsequently accepted with its two-occurrence minimum, as recorded below.

Validation: all 469 IR/codegen tests pass, including 14 new paired UPLC
snapshots; existing snapshots are unchanged. Formatting and strict workspace
and performance-runner Clippy pass. All 20 explicit baselines match, and the
16 isolated experiment cases preserve results and traces. Step 1 is complete.

### Step 2 — One repeated leading literal (accepted)

`lower::lower_with_constant_sharing` is used by the accepted candidate pipeline
and permanent performance runner. Normal builds remain O0 pending Chunk 11.
It starts with Step 1 force sharing and operates on the surviving
lowered body before the force-cache wrappers are added. Typed Core, ANF and the
Core cleanup loop are unchanged.

The accepted rule recognizes one leading literal argument of a known builtin
whose arity exceeds one. Two or more occurrences of the same builtin and
structurally equal literal share one partial value at the outermost program
scope. Saturated call sites can use it, but only their first argument is shared.
Unary calls, single occurrences, trailing literals, computed operands, variable
aliases, operand reordering and longer constant prefixes are outside this rule.

The first argument is stored by the CEK builtin runtime without executing or
type-checking the builtin; execution occurs only at saturation. Thus these
closed partial values can move outside branches, delays and lambdas without
moving later-argument computations, traces or failures. Force-cache bindings
wrap the partial bindings they supply. Counting after structural lowering keeps
discarded exhaustive defaults out of the occurrence count. Fresh names avoid
all source and lowering names; applying sharing twice makes no further change.

Semantic snapshots compare Step 1 UPLC with the candidate UPLC. They cover
separately allocated equal literals, different builtins/literals, strict later
arguments, noncommutative operations, unary saturation, selected/unselected
failure, trace timing, delays, returned partials, polymorphic trace calls,
validator/lambda scopes, ternary builtins and discarded defaults.

Measured against Step 1, independently of the accepted baseline:

52-case experiment, 27 September 2026: every result and trace matched. The six
successful source workloads (list traversal, countdown, Data hit/miss, decoding,
validation pass) are unchanged; none exposes this repeated-prefix shape after
the accepted Core passes. Synthetic cases isolate the lowering rule and are not
claims about real-world validator distributions.

| Workload | CPU delta | Memory delta | Flat bytes before → after |
| --- | ---: | ---: | ---: |
| Integer, one site | 0 | 0 | 11 → 11 |
| Integer, two executed sites | +16000 | +100 | 22 → 21 |
| Integer, three executed sites | -16000 | -100 | 32 → 27 |
| Integer, eight executed sites | -176000 | -1100 | 83 → 60 |
| Trace, two executed sites | +16000 | +100 | 36 → 30 |
| Trace, eight executed sites | -176000 | -1100 | 134 → 75 |
| 64-byte prefix, two sites | +16000 | +100 | 160 → 95 |
| 64-byte prefix, eight sites | -176000 | -1100 | 637 → 163 |
| 1024-byte prefix, two sites | +16000 | +100 | 2088 → 1059 |
| 1024-byte prefix, eight sites | -176000 | -1100 | 8349 → 1127 |
| Two sites per loop, zero iterations | +80000 | +500 | 53 → 52 |
| Two sites per loop, eight iterations | -432000 | -2700 | 53 → 52 |
| Two sites per loop, 64 iterations | -4016000 | -25100 | 54 → 53 |
| Eight exclusive branches, one taken | +48000 | +300 | 125 → 102 |
| Eight cold integer sites | +80000 | +500 | 88 → 65 |
| Unused lambda with eight sites | +80000 | +500 | 81 → 58 |

One lexical site inside a loop is intentionally unchanged regardless of runtime
iteration count. The rule counts emitted occurrences, not inferred frequency.
Across the synthetic payloads, one shared binding has 80000 CPU/500 memory setup
cost and saves 32000 CPU/200 memory per executed use. Three executed uses beat
that setup cost; three syntactic sites do not guarantee three executed uses.
Therefore increasing a global use-count cutoff would not solve cold/exclusive
branch costs. Retaining the two-occurrence rule is a size/runtime tradeoff, not
an assertion that every program gets cheaper.

Validation: 481 IR/codegen tests pass, including 15 new paired UPLC snapshots
and an idempotence check; existing snapshots are unchanged. Formatting and
strict workspace/performance-runner Clippy pass. The 52-case explicit experiment
preserves every result and trace. At this trial stage the accepted baseline was unchanged.

Keep decision (27 September 2026): retain the two-occurrence minimum, including
the recorded startup costs for two executed uses and cold paths. No later cleanup
undoes sharing. Longer prefixes, operand reordering and frequency heuristics
remain deferred. Chunk 5's retained scope is complete.

Integration: the candidate semantic harness and explicit performance runner now
use both accepted sharing steps. The permanent baseline has 23 rows: all original
20 rows are unchanged, plus two-use, cold and loop constant-prefix regressions.
The completed experiment also ran those three sources (55 cases total).
Their Step 2 deltas match the review: +16000 CPU/+100 memory for two uses,
+80000/+500 for cold uses, and -432000/-2700 for the eight-iteration loop.
The loop's combined optimized cost is still 144000 CPU/900 memory above O0;
Step 2 improves it but does not eliminate the earlier pipeline overhead. This
remaining gap is retained in the baseline, not hidden by changing O0.
Integration validation: 481 IR/codegen tests, all 23 explicit baselines, formatting,
and strict workspace/performance-runner Clippy pass; the expanded 55-case
experiment preserves results and traces. Existing semantic snapshots are unchanged.

## Chunk 6 — Dead bindings, functions and parameters

**First rule retained (27 September 2026): safe unused nonrecursive bindings.**
`nash_ir::dead_code::simplify_bindings` removes unused nonrecursive lets only when
`analysis::safe_to_discard` proves the RHS terminates without failure or trace.
It counts uses by unique binder identity and repeats after removal, so dead
closures/aliases can release captured bindings. It preserves expression types
and ANF. It does not remove recursive groups or parameters. Zero-use forced
builtin references may disappear; sharing still applies to surviving references.

Semantic snapshots cover literal/closure/delay/partial-builtin removal, cascading
aliases/captures, live captures, strict constructor fields, traces, failures,
forced failures, saturated builtin calls and strict partial-call arguments.
The diverging recursive-call fixture checks exact retained Core without running
an infinite program. The pass runs inside `small_inline::simplify` alongside
rules 1–4 until the cleanup loop reaches a fixed point, before recursion rewriting.
Recursive-member removal is retained below; nonrecursive parameter removal has
a separate trial below.

Across 15 cases, unused literal/closure/delay removal saves 48,000 CPU and 300
memory; the partial-builtin example saves 80,000 CPU and 500 memory. Flat sizes
fall by 2–5 bytes. Strict trace/saturated-call cases and all nine existing source
workloads are unchanged. This does not fix the existing recursive-fixture
regressions. All 23 permanent baseline rows remain unchanged after integration;
the recorded pipeline settings now include dead-binding cleanup.

Validation: 489 IR/codegen nextest tests passed. The original 16 semantic
snapshots are supplemented by paired Core snapshots for safe ignored delayed
arguments and strict tracing arguments through the accepted cleanup loop. Root
and isolated strict Clippy and formatting passed.

**Second rule retained (27 September 2026): recursive reachability.**
`nash_ir::dead_code::prune_recursive` visits groups bottom-up. It seeds a worklist with
members referenced anywhere in the continuation, then follows references in
reachable member bodies. Unreachable cycles do not seed themselves. A member
counts as used when returned, partially applied, passed as a value, captured by
a lambda/delay, or referenced in a branch, even if that branch is known cold.
There is no call-shape or path-sensitive heuristic.

Retained members keep source order, parameter slices, static-parameter indices
and types. An empty live set removes the group; a retained singleton remains a
`LetRec` for the existing recursion rewrite. Function definitions and supported
singleton delayed workers defer their bodies, so unused groups can disappear
without running those bodies. Unsupported zero-parameter recursive values remain
rejected. Singleton lowering already rechecks static metadata before using it;
this rule neither re-infers static parameters nor performs another ANF pass.

The pass runs in the accepted cleanup loop before recursion rewriting. Repeating
the loop removes newly unused safe captures while preserving strict effects.
Semantic snapshots cover unreachable self/mutual cycles,
transitive reachability and retention order, live mutual recursion, failures,
partial/returned/suspended uses, nested groups, cold references, delayed workers,
and mutual-to-singleton capture/static-metadata preservation.

Measured 30 cases: 21 direct Core scenarios and nine source workloads. No CPU,
memory or Flat-size regressions occurred. Representative savings:

| Fixture | CPU saved | Memory saved | Flat bytes saved |
| --- | ---: | ---: | ---: |
| Unused singleton | 144,000 | 900 | 12 |
| Two members, one live identity | 144,000 | 900 | 23 |
| Eight members, one live identity | 432,000 | 2,700 | 121 |
| Two members, one live countdown, 64 recursive calls | 3,216,000 | 20,100 | 29 |
| Eight members, one live countdown, 64 recursive calls | 3,504,000 | 21,900 | 155 |

All-live groups, live delayed workers and the nine source workloads are unchanged.
The permanent 23-case baseline also matches unchanged. These are explicit synthetic
and existing fixture results, not claims about a real validator distribution.
Validation after integration: 499 IR/codegen nextest tests passed. The original
16 paired Core/UPLC snapshots are supplemented by two paired Core snapshots
covering cleanup of dead captures and retention of strict effectful initializers. Root and isolated strict Clippy and formatting
passed. Read-only review found no correctness issues.

**Third rule, initial post-ANF trial (27 September 2026), superseded by the accepted pre-ANF placement below: nonrecursive unused parameters.**
`nash_ir::unused_params::reduce` shortens a let-bound lambda only when every
reference to that binding is a direct application with exactly its declared
arity. A parameter is unused only if its unique ID has no occurrence anywhere
in the lambda body, including nested lambdas, delays and recursive bodies.
Partial, staged, escaping and oversaturated uses leave the whole helper unchanged.
Recursive binder parameters were outside this first trial; they are required
in the fourth rule below.

Input is typed, hygienic Core, before or after ANF. Every non-atomic argument
gets a strict call-local binding in source order, including retained arguments,
before the shortened call. Discarded atoms need no binding; preceding strict
argument computations remain at their original stage. Each function
type view is shortened independently while retaining its own result type; views
without the matching function shape are left untouched. If all parameters go,
the definition becomes a delay and each original full call becomes a force.
That preserves cold bodies and repeated trace/failure behavior without introducing
empty lambdas or zero-argument applications.

This historical trial was separate from the accepted pipeline. Its experiment
executable was removed after review.
It compares 24 direct Core cases, nine existing source workloads and four targeted
source fixtures. Ordinary source literal conversions can make ANF stage a full
call into partial applications that cleanup does not rejoin. This trial deliberately
skips those staged uses; review placement before adding any call-chain recovery.

The captured source Core confirms `let stage = helper 100 in stage 1` remains
after accepted cleanup. Naming source arguments first leaves full direct calls
and enables the rewrite. The pre-ANF trial below handles these original full
calls without reconstructing staged call chains.

Measured 37 cases, with no CPU, memory or Flat-size regressions. Representative
savings (post-ANF trial only):

| Fixture | CPU saved | Memory saved | Flat bytes saved |
| --- | ---: | ---: | ---: |
| One unused parameter, one direct call | 48,000 | 300 | 3 |
| One unused parameter, eight direct calls | 384,000 | 2,400 | 22 |
| Three unused parameters, eight direct calls | 896,000 | 5,600 | 63 |
| Source helper with named arguments, two calls | 96,000 | 600 | 8 |
| Source unary all-unused helper, two calls | 32,000 | 200 | 7 |

Cold paths retain their CPU/memory costs and shrink in size. Nine existing source
workloads and the literal/staged helper fixtures are unchanged. The explicit
23-case accepted baseline remains unchanged. Do not interpret the synthetic
wins as coverage of arbitrary source call shapes.

Validation: 507 IR/codegen nextest tests passed, including eight new test
functions and 16 reviewed snapshots. Root and isolated strict Clippy and
formatting passed. Read-only review found no correctness defects under the
typed ANF preconditions. The trial remains outside the accepted pipeline.

**Third rule, pre-ANF trial (27 September 2026), accepted.**
The same pass now supports raw typed Core. Trial order is freshening, static
lifting, unused-parameter removal, one ANF normalization, then accepted cleanup.
Bindings for argument work remain inside their original branch, delay or lambda.
Fresh names avoid both existing binders and free variable IDs; generated bindings
retain each argument's type view. Empty-parameter lambdas are computations and
must still execute when passed as discarded arguments. The pass makes one traversal;
it is not a general fixed-point removal of newly exposed signatures.

The completed `unused_params_pre_anf` experiment compared accepted cleanup, late
removal, and early removal across 39 cases, checking results and trace logs.
Unlike the earlier direct-Core experiment, every comparison includes accepted
cleanup. Savings against accepted cleanup for newly handled source calls:

| Fixture | CPU saved | Memory saved | Flat bytes saved |
| --- | ---: | ---: | ---: |
| Two ordinary literal calls | 192,000 | 1,200 | 13 |
| Same calls with a traced discarded argument | 192,000 | 1,200 | 12 |
| Traces in retained and discarded arguments | 192,000 | 1,200 | 11 |
| Two all-unused two-parameter calls | 224,000 | 1,400 | 16 |

Named-argument and unary all-unused source calls retain the late trial's savings.
Nine existing source workloads remain unchanged. One hot synthetic case regresses:
all parameters unused with a single call costs an extra 32,000 CPU, 200 memory,
and one byte. Accepted beta cleanup previously eliminated that lambda call;
early removal instead leaves `force (delay body)` after single-use substitution,
which the current cleanup does not cancel. Its cold counterpart costs one extra
byte with unchanged CPU/memory. No other measured regressions. Delay/force
cancellation is a separate follow-up, not silently included in this trial.

Validation: 513 IR/codegen nextest tests passed. Eleven new reviewed snapshots
cover strict argument order, failure at every position, repeated all-unused calls,
cold branches, empty lambdas, fresh IDs/type views and structural retention of a
diverging argument. Prior snapshots are unchanged. Root and isolated strict Clippy
and formatting passed; all 23 accepted baseline cases still match. Read-only
review found no correctness defects.

Keep decision: adopt pre-ANF removal after static lifting in the candidate pipeline
and explicit performance harness. Preserve the measured single-call tradeoff;
The later accepted Chunk 8 force/delay cancellation removes that tradeoff. Production assembly remains O0 until Chunk 11. Adoption
validation passed all 513 IR/codegen tests with no snapshot changes, root and
isolated strict Clippy, and formatting. All 23 performance rows remain identical;
only baseline pipeline settings and revision metadata were refreshed.

**Fourth rule, integrated (5 October 2026): recursive unused parameters.**
The existing pre-ANF pass now handles self-recursive and mutual groups after
static lifting. Parameter liveness starts at real uses and propagates backward
through direct, bare-variable forwarding dependencies until stable. Forwarding
alone does not keep a parameter live. Compound argument evaluation remains strict,
so its parameter references are consumers even when the receiving slot is unused.
All entry and recursive calls retain non-atomic arguments in source order.

Any partial, escaping, oversaturated or unsupported type view leaves the group
unchanged. Retained static indices are remapped and each direct call keeps its
own result type view. All-unused workers become delayed recursive values;
recursion lowering and dead-member cleanup support mutual groups containing such
workers. Captures, cold entry, repeated execution and divergence are preserved.
No second ANF pass or general unused-signature fixed point was added.

Semantic snapshots cover self/mutual forwarding, permuted slots, real consumers,
compound arguments, traces/failures, all-unused/mixed workers, partial/escaping/
oversaturated calls, static indices, type views, captured returned closures and
unevaluated divergence. Source fixtures cover static-lift interaction and mutual
recursion. Explicit measurements and validation are recorded in
[the recursive parameter report](../docs/research/recursive-parameters.md).

Remove unused bindings only when their evaluation is safe to discard. Remove
unreachable recursive members by continuation reachability. Remove unused
parameters only for rewritable uses; keep strict evaluation of dropped arguments
at the correct call stage, even if that needs an unused let.

Test tracing/failing/diverging unused RHSs, strict ignored arguments, saturated
and partial/escaping calls, self/mutual recursion and all-static workers. Check
that parameter removal preserves worker captures and does not leave stale
parameter indices. Newly exposed static parameters may be evaluated as a separate
future candidate; early lifting must not depend on recovering ANF call chains.

**Done when:** each retained rule preserves semantics and demonstrates a reviewed
benefit; refresh recursion metadata before rewriting.

## Chunk 7 — Known-case and field simplification

Optimization semantic tests are grouped in
`crates/nash-codegen/src/optimizer_tests/`, with colocated snapshots.
All executable codegen snapshots show original Core and O0 UPLC followed by
accepted optimized Core and UPLC through the shared test-only pipeline.
Prepare each pipeline once per fixture. Snapshot rendering, target validation
and evaluation reuse the same named and closed UPLC programs; equivalence
checks run after the snapshot and do not repeat optimization or lowering.
Preparation remains non-evaluating for deliberate-divergence snapshots.
Pass fixtures retain isolated transformation evidence after that comparison.
Source fixtures reuse the Base compiler, including Boolean helpers, a cold trace
and a retained strict trace. Budgets remain in the explicit performance workspace.

**Known Boolean subjects: trial 27 September 2026, accepted 28 September 2026.**
`known_case::reduce_bool` selects the actual True/False branch or default only for a
literal Boolean subject. It validates the local Boolean table before folding:
no field binders, no non-Boolean tests and no duplicate alternatives. Missing
matches without a default remain untouched so runtime failure is preserved.
It retains the case result type view and neither evaluates nor moves the subject.
Earlier strict lets remain in place. Bottom-up traversal can expose another
literal Boolean case; accepted cleanup can expose further matches, so the
experiment also compares a cleanup/folding fixed point without another ANF pass.
The keep decision adds folding to the accepted cleanup fixed point after
force/delay cancellation and before beta cleanup. Source snapshots now use the
shared accepted pipeline directly; no extra Boolean-only loop is needed.

The completed `known_bool` experiment compared frozen pre-adoption cleanup, one
folding pass, and accepted cleanup with Boolean folding across 40 cases. Fourteen improve and 26 stay
unchanged, with no measured CPU, memory or Flat-size regressions. The actual base
`booleanHelpers` fixture changes from 160,100 CPU / 1,100 memory / 18 bytes to
16,100 / 200 / 5 with repeated cleanup (one folding pass alone: 96,100 / 700 / 11).
Cold helper fixtures shrink to six bytes; cleanup removes newly unreachable
helper bindings as well as the case. `constantPrefixCold` changes from
128,100 / 900 / 30 to 16,100 / 200 / 6. Other existing source workloads are unchanged.
These are fixture results, not claims about a real-validator distribution.

Trial validation: 524 IR/codegen nextest tests passed, including 15 reviewed new
Core/UPLC and malformed-table snapshots. Root and isolated strict Clippy and
formatting passed; all 23 accepted baseline cases still matched. The original
trial changed neither production assembly nor the permanent baseline.

Adoption measurements reproduce 14 improvements and 26 unchanged cases, with
no CPU, memory or Flat-size regressions. The frozen control and accepted cleanup
start from the same normalized Core. The permanent 23-row baseline now records
the Boolean-helper and cold constant-prefix improvements above; other rows,
O0 programs, results and logs remain unchanged. The explicit baseline check passes.

Adoption validation: 528 focused IR/codegen tests and all 3,746 workspace tests
passed. Reviewed 58 updated snapshots and one new runtime-conditional snapshot;
the original O0 sections and recorded outcomes are unchanged. Root and isolated
strict Clippy and formatting pass. Prior experiment executables were removed
after recording these results; semantic snapshots and budget regression checks remain.

**Direct native-constructor folding (28 September 2026), accepted.**
`known_case::reduce_constr` folds only a direct `Constr` subject under `CaseKind::Tag`.
It checks the same local table contract as lowering: no default, only consecutive
unique tags starting at zero, in any source order. An absent tag or selected
field/binder arity mismatch remains unchanged. Under- and overapplication have
UPLC behavior that cannot be replaced with a simple sequence of field bindings.

With globally unique, well-scoped binders, the pass replaces the selected case
with strict field lets in left-to-right order and the selected body, preserving
the case result type view. Ignored fields still evaluate; unselected branches do
not. Bottom-up traversal also folds nested direct cases. The accepted pass runs after
freshening/static lifting and before the single ANF normalization: ANF otherwise
binds the constructor and changes the subject into a variable. No constructor
fact propagation or call-chain reconstruction is included. Adoption adds this step
after unused-parameter removal and before ANF in the shared accepted snapshot and
performance pipelines. Boolean and constructor folding share `known_case.rs` with
separate entry points because their pipeline placements differ. Unused-binding removal and recursive reachability
likewise share `dead_code.rs`; their cleanup order and algorithms are unchanged.

Temporary measurements compare otherwise identical accepted pipelines with and
without this pre-ANF step. Across 24 direct-constructor cases (0/1/2/3/4/8 fields,
used/ignored fields, pure/traced fields) and 11 existing source workloads, all
results and logs match. All 24 direct cases improve CPU, memory and Flat size;
all 11 source workloads are unchanged. Savings range from 80,000 to 336,000 CPU,
500 to 2,100 memory, and 5 to 28 Flat bytes. These are fixture results, not a
real-validator distribution. Temporary measurement code is removed after recording
results; snapshots retain the semantic evidence.

Validation: 18 reviewed snapshots cover field order, ignored failure, expanded
wildcards, nullary/nested cases, returned functions/delays, cold construction,
non-direct subjects, absent tags, arity mismatches and malformed tables. All
3,754 workspace nextest tests pass with no skips, alongside strict Clippy and
formatting. All 23 accepted performance baseline cases remain unchanged.

Adoption adds the pre-ANF constructor pass to both accepted pipelines. The
23-row performance baseline is numerically unchanged, including corrected V3
vesting inputs; only pipeline settings/provenance change.

Adoption validation: 467 codegen nextest tests pass. All 3,754 workspace unit and
integration tests pass using `cargo test --workspace` after full nextest discovery
stalled twice. The additional doctest phase was stopped after unit/integration
coverage completed; it is not part of nextest coverage. Strict Clippy and
formatting pass. Nine reviewed snapshots change
only accepted optimized output; original Core/O0 UPLC and isolated-pass evidence,
including results and logs, are byte-for-byte unchanged. The explicit 23-row
budget check passes.

**Let-bound constructor folding (accepted).** Source
`decision_tree::compile` binds the subject before matching, so direct-subject
folding alone misses this normal source shape. `reduce_bound_constr` tracks
lexical constructor bindings and their variable aliases by globally unique name.
It selects only complete, consecutive native tag tables with a matching tag and
exact field arity, using the same table checks as direct-constructor folding.

For constructors with eligible matches, non-variable fields are named once at
the original construction site. This includes lambda/delay atoms: copying them
would duplicate their internal binder IDs. Naming literals also leaves the
existing constant propagation size policy in charge. Matching branches receive
aliases to those stable field references. The original constructor stays until
ordinary cleanup proves it unused, so escaping values and repeated matches work.
Strict fields, including ignored fields, stay evaluated once in source order,
even when the matched case sits in a cold branch.

The pass runs on ANF after ordinary cleanup, followed by the existing cleanup
loop. Selecting a branch can expose leading lets, which existing beta binding
splicing flattens; ANF is not rerun. Repeat folding plus cleanup to a fixed point:
folding an outer case can expose aliases that make an inner case known. This extension is now accepted in both the shared snapshot pipeline and the
performance pipeline, after ordinary ANF cleanup. `simplify_bound_constr` owns
the fold/cleanup fixed point. Isolated-pass snapshots retain the trial evidence.

Temporary measurements compare accepted optimization against accepted plus this
trial and cleanup, with identical lowerer sharing settings. Of 85 cases:

- 60 generated cases cover 0/1/2/4/8 fields, 1/2/4 matches, pure/traced fields,
  and retained/escaping constructor values. All improve CPU and memory.
- Two actual Nash examples improve: one `case One 42` saves 112,000 CPU,
  700 memory and 16 Flat bytes (22 to 6); two matches on `Two 20 22` save
  304,000 CPU, 1,900 memory and 27 bytes (37 to 10).
- All 23 existing workloads, including V3 vesting inputs, are unchanged.

Across the 62 improved cases, CPU savings range from 48,000 to 816,000 and memory
savings from 300 to 5,100. Serialized size improves in 60 cases by up to 40 bytes;
the other 25 cases are unchanged. No measured metric regresses; all results and
ordered logs match. These are fixture results, not a real-validator distribution.
Temporary measurement code is removed after recording these findings.

Validation: 18 new reviewed snapshots cover aliases, repeated matches, escaping
values, strict traced/failing fields before cold matches, function/delay fields
and captures, nested matches exposed by cleanup, nullary/reordered cases, absent
tags, empty tables, arity mismatches and malformed tables. All 473 codegen tests
pass using the compiled test binary after focused nextest discovery stalled.
Strict workspace Clippy, formatting and the unchanged 23-row accepted performance
baseline pass. A full `cargo nextest run --workspace` was attempted, but stopped
after two `nash-driver` rustc processes remained idle for over two minutes without
diagnostics; this trial does not claim a completed full-workspace test run.

Adoption validation: all 475 codegen unit/integration tests pass through the
compiled test binaries, including both V3 vesting tests. Strict Clippy and
formatting pass. Forty-one reviewed snapshots change accepted optimized output;
original Core/O0 UPLC, isolated-pass evidence, results and logs remain unchanged.
All 23 performance rows and source inputs remain identical; baseline settings and
provenance record the added pass. Full-workspace nextest and Cargo attempts were
stopped after idle compiler stalls in driver/language-server targets, so full
workspace test completion is not claimed.

**Known native-list folding (accepted 28 September 2026).**
`reduce_list` handles literal `ProtoList` subjects and let-bound literals or
exactly saturated `MkCons` values, following variable aliases. It validates the
whole branch table: unique `Nil` with zero binders and `Cons` with two binders.
The selected arm receives head/tail bindings, or selection uses the default or
an explicit error when that arm is absent. Malformed tables remain unchanged.

`MkCons` stays strict at its original position even when folding removes its last
use. Besides evaluating head then tail, the builtin checks that the head is a
constant, the tail is a list, and their runtime element types match. Core type
annotations alone do not prove those checks; removing the construction belongs
to a separately justified builtin reduction. Partial/oversaturated `MkCons` and
other unknown subjects do not establish list facts.

Matched bound lists share stable head/tail bindings across their cases. This
avoids copying lambda/delay operands and repeated literal-tail serialization.
Literal tails retain the original element metadata. The original list stays if
it escapes. `simplify_list` repeats folding and the accepted cleanup to expose
nested tail matches; ANF is not rerun. The keep decision adds this entry point to the shared accepted snapshot and
performance pipelines, replacing the constructor-only cleanup entry point.
Production assembly remains O0.

Temporary measurements compare accepted optimization with accepted plus this
trial using identical lowerer sharing and V3 budgets. Across 86 cases (42
construction/head-use combinations, 18 literal-tail cases, three Nash examples,
and the existing 23 workloads), results and ordered logs match. CPU and memory
improve in 63 cases, with 23 unchanged and no regressions. Savings range from
32,000 to 544,000 CPU and 200 to 3,400 memory. Size improves in 57 cases by up to
43 bytes, is unchanged in 23, and increases by 11–200 bytes in six cases that
retain both the original 32/128-element literal list and its derived tail.
Sharing derived fields reduced the initial worst growth from 804 to 200 bytes.
These are fixture measurements, not a real-validator distribution.

The actual Nash empty-list example saves 32,000 CPU, 200 memory and 6 bytes;
nonempty head selection saves 64,000 CPU, 400 memory and 5 bytes; tail selection
saves 64,000 CPU, 400 memory and 6 bytes. The existing 23 workloads are unchanged.
Temporary measurement code is removed after recording the findings.

Validation: all 486 codegen library tests pass, including 30 new list snapshots;
existing snapshots are unchanged. Formatting and strict all-target/all-feature
Clippy pass. The separate 23-case performance baseline check passes. Nextest
stalled during test discovery, so the codegen suite was run with Cargo instead;
full-workspace test completion is not claimed for this trial.

Adoption validation: 486 codegen library tests and two vesting integration tests
pass, with 36 updated snapshots and unchanged unoptimized Core/UPLC sections.
Nextest again stalled at discovery; freshly built test binaries passed directly.
Strict all-target/all-feature Clippy and formatting pass. All 23 performance
rows and sources are unchanged; baseline settings now include list folding.

**Known literal Data folding (accepted 28 September 2026).**
`reduce_data` selects `DataI`, `DataB`, `DataList`, `DataMap` or `DataConstr`
for direct `Constant::Data` subjects and let-bound literals/aliases. It validates
all branches first (unique Data tests and exactly one payload binder). Missing
arms use the default or an explicit error. Unknown subjects and malformed tables
remain untouched. This is shape selection, not builtin wrap/unwrap cancellation.

Payloads exactly match runtime unwraps: native integer/bytes, a list of Data,
a list of native Data/Data pairs, or one native `(integer tag, list Data)` pair.
Nested children stay Data; map order and duplicate keys, empty container metadata,
arbitrary integers and full-width constructor tags are preserved. Matched bound
literals share one derived payload at their original binding. Literal payloads
are values, so creating them cannot move traces/failures. `simplify_data` repeats
this rule with accepted list/constructor cleanup without another ANF pass.

The isolated experiment compares accepted optimization with accepted plus this
trial, using the same sharing and V3 budgets. Across 125 cases (102 targeted
shape/size/use/escape combinations plus 23 existing workloads), results and logs
match. CPU and memory improve in 102 cases and are unchanged in 23; no regressions.
Savings range from 578,517–2,172,476 CPU and 2,964–10,956 memory. Size improves in
57 cases by 4–89 bytes, is unchanged in 23, and grows in 45 by 1–1,210 bytes.
Of those increases, 25 retain the original Data and its native payload (maximum
1,210 bytes); 20 discard the original (maximum 743 bytes). Native lists/maps can
encode larger than CBOR Data, and accepted short-byte propagation can duplicate
small payloads. These measurements are synthetic fixtures, not a real-validator
distribution. No size heuristic or constant propagation exception is added.

The 23 existing workloads remain unchanged. Source `I 42` currently emits an
`IData` call, which this literal-only trial intentionally does not recognize.
Builtin-produced Data shapes require a separate strictness/check-preserving trial;
wrap/unwrap cancellation remains Chunk 8. The keep decision adds `simplify_data`
to the shared accepted snapshot and performance pipelines, replacing the list-only
cleanup entry point. Normal build assembly remains O0. Temporary performance
code is removed after recording the results.

Validation: 497 codegen library tests pass, including 59 new Data snapshots;
existing snapshots are unchanged. Tests cover all payload shapes and metadata,
missing/default branches, shared aliases, strict traces/failures, returned and
capturing closures/delays, nested Data/list folding, unknown/runtime-invalid
subjects, full-width tags/integers and malformed unselected branches. Snapshot
output precedes independent scope, ANF, type-view, equivalence and idempotence
checks. Formatting and strict all-target/all-feature Clippy pass. Nextest stalled
at discovery; the freshly built binary passed directly. Full-workspace test
completion is not claimed for this isolated trial.

Adoption validation: 498 codegen and 69 IR tests pass, with 50 updated snapshots
and unchanged unoptimized Core/UPLC sections. Formatting, strict workspace Clippy
and the separate performance check pass. All 23 performance rows and source inputs
are unchanged; baseline settings now include Data folding. Nextest stalled during
discovery, so the freshly built binaries were run directly.

**IData/BData producer case folding (28 September 2026), accepted.**
`reduce_data_wrappers` recognizes exactly saturated `IData`/`BData` let bindings
and aliases in hygienic ANF. Both literal operands (`iData 42`) and variable
operands (`iData x`) qualify. It selects the known Data arm and binds its payload
to the original operand; missing shapes use the default or error. All branch
shapes and binder counts must be valid, including unselected arms.

The original builtin remains strict in its original position, even when its
result becomes unused: invalid runtime operands must still fail. Non-variable
operands are named once before the producer, preventing duplicated lambda/delay
binders. Traces, failures, intervening effects and suspended matches retain their
evaluation order. Partial, overapplied, traced and unrelated producers and
unknown Data parameters do not establish wrapper facts. `simplify_data_wrappers`
repeats producer folding with accepted cleanup without another ANF pass. The user
accepted the measured size tradeoffs without another heuristic. The shared accepted
snapshot and performance pipelines now use `simplify_data_wrappers`; normal build
assembly remains O0 pending Chunk 11.

The separate experiment compares accepted optimization against accepted plus the
trial with identical lowering/sharing. All 191 cases have matching results and
logs: 168 targeted integer/byte literal and runtime-parameter combinations plus
23 existing source workloads. CPU and memory improve in 172 cases and remain
unchanged in 19; neither regresses. Savings range from 115,119–2,124,476 CPU and
64–10,656 memory. Size improves in 160 cases by 16–85 bytes, is unchanged in 19,
and grows in 12 by 12–182 bytes. All size increases involve used short byte
literal payloads duplicated by accepted constant propagation; they occur both
with and without the original Data escaping. Runtime-parameter cases have no
size regressions. These are synthetic measurements, not a real-validator sample.

Four existing source workloads improve: `dataMatch` saves 579,119 CPU, 2,964
memory and 30 bytes; `dataMiss` saves 510,375 CPU, 2,632 memory and 29 bytes;
`validationPass` saves 579,119 CPU, 2,964 memory and 27 bytes; `validationFail`
saves 115,119 CPU, 64 memory and 27 bytes. The other 19 are unchanged. The
23-case accepted baseline check passes. Temporary measurement source is removed
after recording results; performance experiments remain outside normal tests.

Validation: 509 codegen and 69 IR library tests pass, including 50 new snapshots
in the existing `known_case::data::wrappers` module. Existing snapshots are
unchanged. Snapshot output precedes independent scope, type-view, ANF, semantic
and log equivalence, and idempotence checks. Malformed tables retain their lowering
errors. Formatting, diff checks and strict workspace Clippy pass. Nextest stalled
at discovery; Cargo test passed after restarting a stalled compiler invocation.
The full workspace test suite was not rerun for this isolated trial.

Adoption validation: 509 codegen and 69 IR tests pass. The 34 updated snapshots
retain identical unoptimized Core/UPLC and isolated-pass evidence. The reviewed
23-case baseline incorporates only the four workload improvements listed above;
source inputs, results, logs and all O0 measurements are unchanged. Nextest stalled
during discovery, so the freshly built binaries were run directly. Both formatter
checks, workspace and isolated-runner strict Clippy, and the explicit performance
baseline check pass.

**ListData/MapData producer case folding (28 September 2026), accepted.**
`reduce_data_wrappers` extends the scalar-wrapper algorithm to exactly
saturated `ListData` and `MapData` bindings. `DataList` receives the original
native Data list; `DataMap` receives the original native list of Data pairs.
The producer remains strict, including when no branch uses the payload, so wrong
operand types and incorrect empty-list element metadata still fail. Successful
MapData construction preserves entry order and duplicate keys. Alias handling,
non-variable operand sharing, full branch-table validation and defaults use the
same code as the accepted scalar rule. Adoption removes the temporary collection
switch and separate trial entry points; `simplify_data_wrappers` now handles all
four wrappers with accepted cleanup without another ANF pass. Both accepted
pipelines pick up the rule through that existing entry point. Normal build assembly
remains O0 pending Chunk 11 configuration decisions.

The separate experiment compares accepted optimization against accepted plus
this trial with identical sharing and V3 budgets. All 215 cases match results and
logs. The 192 targeted cases cover lists/maps of lengths 0, 1, 4 and 16, one/two/four
matches, used/ignored payloads, escaping/nonescaping original Data and literal/runtime
parameter operands. Every targeted case improves CPU, memory and size; savings
range from 478,375–2,145,232 CPU, 2,432–10,656 memory and 18–85 bytes. All 23 existing
source workloads are unchanged, and their accepted baseline check passes. These
are synthetic coverage cases, not a representative real-validator distribution.
Temporary experiment source is removed after recording the results.

Validation: nextest passes all 582 tests across the codegen/IR binaries on retry
(7.06 seconds); the first invocation stalled at discovery. There are 78 new
snapshots covering both wrappers in the existing module. Four existing snapshots
update isolated evidence and generated binder IDs; accepted comparison output is
otherwise unchanged. Cases cover literal/runtime operands, empty/nonempty and
runtime-built collections, wrong list/pair metadata even on empty or unused
payloads, duplicate map keys and order, aliases, repeated matches, strict effects,
returned/capturing functions, defaults, saturation and malformed unselected arms.
Scope, ANF, type-view, equivalence/log and idempotence checks follow snapshots.
Formatting, strict workspace Clippy, temporary-runner Clippy and the separate
accepted performance check pass. Full-workspace tests were not rerun for this trial.

Adoption validation: 511 codegen and 69 IR library tests pass; 64 snapshots update
only accepted optimized output, preserving identical O0 and isolated-pass evidence.
All 23 performance rows and source inputs remain unchanged; baseline settings now
name the adopted collection wrappers. Formatting, strict workspace/runner Clippy
and the explicit performance check pass. Nextest stalled at discovery; the freshly
built library binaries passed directly. The temporary collection switch and trial
entry points are removed.

**ConstrData producer shape folding (28 September 2026), accepted.**
`reduce_constr_data` recognizes exactly saturated, let-bound `ConstrData` producers
and aliases in hygienic ANF. It validates the full Data branch table, selects
`DataConstr`, and uses the original default or error when that arm is absent.
The original producer remains strict at its existing position, preserving tag
and field evaluation, runtime type checks and tag-range behavior. Partial,
overapplied, traced and unknown producers do not establish facts.

Core has no general constructor for the dynamic native `(int, list Data)` pair
that `DataConstr` binds. Therefore this step removes `chooseData` dispatch but
retains `UnConstrData(scrutinee)` at the original case site when the selected
payload is used. Unused payloads and defaults need no extraction, matching ordinary
lowering. This does not substitute tag/fields directly or change pair representation.
`simplify_constr_data` composes the rule with accepted cleanup without another
ANF pass. Both shared snapshot and performance pipelines now use this entry point.
Normal build assembly remains O0 pending Chunk 11 configuration decisions.

Across 215 accepted-versus-trial comparisons, results and logs match; 193 improve
CPU, memory and size and 22 are unchanged, with no regressions. The 192 targeted
cases cover tags 0, 59 and `u64::MAX`, zero/four fields, one/three matches, literal
or runtime arguments, escaping/nonescaping Data and ignored/pair/tag/fields payload
uses. Savings range from 366,375–1,307,125 CPU, 1,732–6,496 memory and 16–51 bytes.
One existing source workload, `Workloads.decoding`, improves by 366,375 CPU, 1,732
memory and 19 bytes; the other 22 are unchanged. These are synthetic coverage
measurements. The accepted baseline is unchanged, and temporary runner source is
removed after recording the results.

Validation: 518 codegen, 69 IR and two vesting tests pass (589 total) in the
freshly built binaries after nextest stalled at discovery. There are 39 new
snapshots and one updated isolated producer example; existing accepted-pipeline
Core/UPLC output is unchanged. Tests cover operand order/failure, wrong types,
empty/nonempty fields, alias/repeated/captured pair uses, cold producers, defaults,
partial/overapplied/traced producers and malformed tables. Scope, type-view, ANF,
semantic/log equivalence and idempotence checks follow executable snapshots.
Formatting, strict workspace/temporary-runner Clippy and the separate accepted
performance baseline check pass. Full-workspace tests were not rerun for this trial.

Adoption validation: all 589 codegen/IR/vesting tests pass in fresh binaries
following a nextest discovery stall. The 34 updated snapshots retain identical
unoptimized Core/UPLC and isolated-pass evidence. The reviewed performance baseline
incorporates only the `decoding` improvement above; all source inputs, O0 metrics,
results and logs are unchanged. Formatting, strict workspace/runner Clippy and
the explicit baseline check pass.

At the time of this trial, the evaluator panicked for negative tags and tags
above `u64::MAX`; the unbounded folding integration now returns evaluation errors.
Those two fixtures render before/after Core and UPLC and independently check retained
construction without evaluating that existing panic path. In-range boundary tags
are evaluated normally. This is case folding, not permission to remove producers
through cancellation (Chunk 8).

**Native integer/byte literal cases (29 September 2026), accepted.**
`reduce_literals` selects a matching branch or default for literal subjects,
inside the existing cleanup fixed point. The entire table must have tests of the
correct kind, no duplicate tests and no binders. Invalid tables and missing
matches without defaults remain unchanged. Effectful/unknown subjects are not
folded by this rule; strict surrounding bindings retain their evaluation.
The rule preserves the enclosing result type and selected branch delay boundaries.

These Core kinds currently have no source codegen producer: source literal
patterns call their selected conversion and equality traits. Those calls are
not bypassed. Hand-built Core snapshots cover hits, misses, defaults, errors,
empty tables, unknown/effectful subjects, delayed results and malformed tables.
An isolated 80-case experiment covered integer/byte tables of 0, 1, 3, 8 and 32
branches, early/middle/late hits and misses, with/without defaults. 64 cases
improved CPU, memory and size; 16 missing-match cases were unchanged. Maximum
savings were 4,794,656 CPU, 19,532 memory and 374 Flat bytes. Results/logs matched;
the temporary runner was removed. This completes the retained Chunk 7 scope.

**Future late UPLC application packing, separate from Core folding.**
Measure replacing a chain such as `f a b c` with native
`case (constr 0 [a, b, c]) [f]` after reductions. Three or more applications is a
proposed starting point, not an established cutoff. Core known-constructor
folding must run earlier so it does not undo this late representation choice.
Packing must preserve evaluation of the function itself before its arguments:
`f` must already be a value, or be evaluated and bound before constructing them.
It must also preserve application stages: constructing every argument first can
move later traces/failures before an earlier function-body execution. Only pack
when intermediate applications are proven not to execute a body (for example,
a known lambda with enough parameters), or retain the original evaluation stages.
Compare CPU, memory and serialized size before adopting any threshold.

Fold cases on known native constructors, booleans, integers, bytes, lists and Data
shapes using their actual branch tests, binders and defaults. Keep strict subject
and constructor-field evaluation in the original order. An ignored failing field
must still fail. Preserve out-of-range/malformed-case errors; do not assume every
hand-built Core case has a matching branch.

**Known native-constructor fields (29 September 2026), accepted.**
The existing bound-constructor reducer now replaces `Field` of a known tag-zero
constructor with the selected field reference. It follows aliases and requires
an exact field count and an in-range index: other tags still fail, too few fields
still return a partial selector, and extra fields still apply the selected value.
Fields are named at the original construction site, preserving strict evaluation
and sharing lambda/delay values without duplicating binders. Escaping records
retain their construction; ordinary cleanup removes unused safe constructions.
Nested accesses become eligible through the existing cleanup fixed point.

The accepted test/performance pipeline includes this rule; normal build integration
remains Chunk 11. Snapshot fixtures cover every position at arities 1, 3 and 8,
aliases, repeated access, escaping records, ordered traces and ignored failures,
nested records, returned functions, delay boundaries and malformed runtime shapes.
The temporary 144-case experiment covered literal, runtime and computed fields,
one/three uses, and escaping/non-escaping records. All 144 improved: CPU by
96,000–800,000, memory by 600–5,000 and Flat size by 5–46 bytes. The existing
23-case baseline was unchanged. Results and logs matched in every case; the
temporary runner was removed after measurement.

Tests: selected/default branches, empty/nonempty lists, each Data shape, field
ordering, ignored failing/traced fields, returned functions/delays, and Big/little
wildcard fixtures. Cover known conditions produced by `Logic` helpers.

**Done when:** retained rewrites have before/after Core and UPLC snapshots,
equivalence tests, measurements and a keep decision.

## Chunk 8 — Representation and force/delay cleanup

**Direct force/delay cancellation (27 September 2026), accepted.**
`nash_ir::force_delay::reduce` replaces only a syntactic `Force(Delay(body))`
with the body at that evaluation point, retaining the outer type view. Bottom-up
traversal cancels nested pairs without substitution, fresh binders, duplication
or hoisting. The inverse `Delay(Force(x))` is not included. Invalid standalone
forces are untouched. This standalone trial runs after accepted Core cleanup;
existing cleanup can flatten newly exposed nested lets without another ANF pass.

The completed `force_delay` performance experiment compared accepted pre-ANF
parameter removal against cancellation alone and cancellation followed by cleanup.
Across 39 cases, the single-call all-unused hot helper saves 32,000 CPU, 200 memory
and one Flat byte, exactly removing the regression from early parameter removal.
Its cold counterpart saves one byte with unchanged CPU/memory. The remaining
37 cases are unchanged; no measured regression. A second cleanup adds no further
savings in these fixtures. Shared delayed helpers remain shared and keep their
per-call force: this pass does not duplicate their bodies.

Semantic snapshots cover literal, trace, failure, nesting, selected/cold branches,
repeated execution, strict ordering, invalid forces, suspended invalid forces and
exposed let bodies. The keep decision adds cancellation to the accepted cleanup loop before beta
cleanup, so newly exposed lets are flattened and subsequent iterations catch
pairs exposed by single-use substitution. Production assembly remains O0.
Validation: 519 IR/codegen nextest tests passed with 10 reviewed new snapshots.
Root and isolated strict Clippy and formatting passed. All 23 accepted baseline
cases still match. Adoption validation also passed all 519 tests without snapshot
changes, strict Clippy and formatting. The 23 baseline rows remain identical;
only pipeline settings and revision metadata changed.

**Direct integer inverse cancellation (29 September 2026), accepted in O1.**
`inverse::reduce` cancels a directly nested `unIData (iData x)` when successful
evaluation of `x` is structurally known to produce a native integer: an integer
literal, a saturated `unIData`, or traces around those expressions. It keeps
`x` at the original evaluation point and preserves the result type view.
Type annotations alone are not proof: unknown variables, forged annotations,
wrong-kind values, partial applications and the reverse direction remain intact.
The retained `unIData` can still fail; cancellation does not remove its check.

The rule handles adjacent calls before ANF in O1; let-bound producers and broader
integer-result proofs remain subsequent work. Snapshots cover successful/failed decoding,
traces, nested pairs, cold branches, unknown arguments, forged metadata,
partial applications and both reverse-direction outcomes. The temporary
16-case isolated benchmark used negative/zero/positive/large integer values,
literal/decoded operands and traced/untraced paths. Every case saved 100,043 CPU,
464 memory and 3–4 Flat bytes with identical results/logs. The runner was removed.

**Representation cancellation expansion (3 October 2026), accepted under the user's zero-regression condition.**
`inverse::reduce` now handles `unBData(bData x)`, `unListData(listData x)`,
`unMapData(mapData x)` and `decodeUtf8(encodeUtf8 x)`, alongside the integer rule.
The corresponding reverse rules require the exact Data variant or valid UTF-8.
Evidence comes from actual constants, saturated known producers, traces and
lexical let bindings, never Core type annotations. List/map constants must have
both the correct runtime tags and valid element/pair payloads. Failures inside
retained operands still occur at the original evaluation point.

Let-bound cancellation reuses operand variables and retains the original strict
producer binding. It does not duplicate literals or computations, or introduce
bindings. Unary bound cancellation still requires shape evidence: the broader
check-preserving rule was semantically valid but grew five malformed-input
fixtures by one Flat byte, so that rule was narrowed before acceptance.

Constructor Data has two operands and is handled separately. Projection through
a bound `constrData tag fields` returns the existing tag/fields variable while
retaining the validating construction. Reconstruction from `fstPair p` and
`sndPair p` reuses the Data variable only when both refer to the same bound
`unConstrData` result. It retains the decoder binding. Tag-range, field-shape,
variant checks and evaluation of unused fields remain in place. Arbitrary direct
constructor round trips and unknown-shape unary conversions remain unchanged.

The pass now runs only within the post-ANF `small_inline::simplify` cleanup
loop, before beta cleanup. The latter catches bindings exposed by ANF/inlining;
ANF is not repeated. The pass requires globally unique, well-scoped binders.

Comparison against previous O1 at `5eaa7564e6b21b4815be93dd3c4c53aebb66edf7`
used the explicit runner with Plutus V3/PV11, UPLC 1.1.0 and the bundled V3 cost
model. All 108 rows preserve results/logs: 47 improve, 61 are unchanged, and none
increase CPU, memory or raw Flat bytes. The original 23 baseline rows are exactly
unchanged; 85 representation fixtures now form permanent explicit regressions.
Representative savings relative to previous O1:

| Fixture | CPU saved | Memory saved | Flat bytes saved |
| --- | ---: | ---: | ---: |
| Direct byte round trip | 143,325 | 764 | 6 |
| Direct Data-list round trip | 171,785 | 764 | 6 |
| Direct Data-map round trip | 204,869 | 764 | 6 |
| Direct UTF-8 round trip | 247,879 | 712 | 6 |
| Bound constructor reconstruction | 539,887 | 1,664 | 12 |
| Reconstruction of known constructor Data | 642,038 | 2,196 | 18 |

Semantic coverage includes both directions, traces, failures, cold branches,
aliased/captured/escaping bindings, partial/extra application, wrong runtime tags,
forged metadata and malformed constructor inputs. Inconsistent raw list/map
payloads receive noninterference checks without executing runtime panic paths.
Validation: all 3,827 workspace nextest tests pass; the final cancellation
fixtures also compare the complete O1 pipeline against O0. Root and isolated
performance-workspace strict Clippy, formatting and the explicit 108-row baseline
check pass. The retained Chunk 8 scope is complete; further shape inference or
conversion families require their own evidence and measurements.

Cancel `force (delay x)` and valid inverse builtin pairs such as
`unIData (iData x)`. Establish preconditions per direction and representation;
`iData (unIData d)` is not an unconditional replacement for arbitrary Data.
Preserve shape-check failures, traces and strictness. Simplify administrative
applications/aliases only under the ANF and application-staging contract.

List and map Eq selection is already library work, not an optimizer rewrite:
Big-element lists use structural Eq through `listData`; Little-element lists use
selected element Eq. `Primitive.map` is a Storable pair-list alias: Big/Big Eq
uses `mapData`, while mixed/Little maps use selected element Eq. Big `Map` has
structural Eq. Do not recognize Eq impl names to replace arbitrary user behavior,
or introduce an Ord/Show change. Reuse selected-Lift and custom-Eq trace tests.

**Done when:** each accepted cancellation has explicit preconditions, malformed
input tests and measured output; no trait-selection magic is added.

## Chunk 9 — Constant builtin evaluation

**Accepted and integrated (5 October 2026).** Production O1 now runs unbudgeted
constant folding after the post-ANF cleanup/known-case loops, repeating folding
and cleanup to stability without another ANF pass. The callback keeps evaluation
policy in codegen and IR independent of codegen. There is no total attempt or
iteration cap: stop only when folding and cleanup both leave Core unchanged.
The earlier 128-attempt cap was an assistant-added restriction, not an approved
keep decision, and was removed on 5 October 2026. The per-call CPU/memory, payload byte/node/depth, allowlist and Flat growth
gates were also removed. All pure, representable successful calls fold; invalid
constants and unsafe large negative Data literals remain runtime calls. O0 and explicit comptime policy are unchanged.

The user accepted the two shared-prefix size tradeoffs. The initial integration baseline
had 148 O0/O1 cases, including the 40 constant-folding fixtures. Relative to the
previous 108-row O1 baseline, 26 improve and 82 are unchanged; all results/logs
match. See [the report](../docs/research/constant-fold-trial.md) for historical
trial measurements and current integration details. The temporary constant-trial
command and public trial entrypoint were removed.

Use a callback supplied by codegen around the direct unbudgeted builtin evaluator;
keep `nash-ir` independent of codegen. Evaluate saturated, pure constant-argument
calls using current V3 semantics. Runtime errors leave the original expression;
they must not become compile errors. Enforce type and serialization correctness,
without imposing resource or literal-size policy. Explicit user `comptime`
retains its own evaluation semantics.

Tests cover arithmetic, cryptography, byte/string/Data/container operations,
nonzero/zero division, malformed constants, large inputs, growing outputs,
large indices/tags, and folding and signature removal to an unchanged tree.

**Done when:** pure representable successful calls fold until unchanged;
effectful, unrepresentable and failing computations retain runtime behavior.

## Chunk 10 — Single-field native pair projection

**Accepted and integrated (5 October 2026).** Keep only the restriction whose
introduced projection is removed by constructor inverse cleanup. It runs inside
`simplify_constr_data`'s existing loop before constructor/wrapper cleanup; producer
bindings remain strict and ANF is not repeated. The general projection rule and
trial CLI were removed. The baseline now includes 178 O0/O1 cases, adding the 30
pair fixtures. Against the previous 148-case O1 baseline, source `decoding`
improves from 94 to 45 bytes, 1,681,280 to 878,161 CPU and 8,756 to 4,392 memory;
the other 147 cases are unchanged. Results/logs match throughout.

The language also exposes explicit `.fst`/`.snd` fields/accessors for known pair
types, lowering directly to the builtins. These do not change pattern lowering.
See [historical pair trial and integration](../docs/research/pair-projection-trial.md).

Compare `CaseKind::Pair` with `fstPair`/`sndPair` when exactly one branch binder
is used, for a valid one-branch case without a default:

```text
case p of pair a _ -> body a  => let a = fstPair p in body a
case p of pair _ b -> body b  => let b = sndPair p in body b
```

A direct field return can become the projection itself. Keep subject evaluation
exactly once and the projection strict at the original case point, including a
field captured by a returned lambda/delay. Keep accurate field types. Both fields
used retains the case. Neither used is outside this rewrite: preserve necessary
evaluation/shape checks. Do not drop strict fields of known pair constructions.

Applies to native builtin pairs, including the tag/fields pair from `unConstrData`,
not arbitrary two-field ADTs. Test malformed input, traced/failing subjects,
ignored failing fields, direct return and larger bodies. Measure CPU, memory and
Flat size, standalone and with builtin-force caching, then review a deterministic
rule. No assumption that projection wins, and no per-program tuning engine.

**Done when:** keep or discard is decided from measurements; O0 stays unchanged.

## Chunk 11 — Composition, convergence and final configuration

**Complete (5 October 2026), retained scope.**

The final composition review adds five regression tests: a mixed cleanup pipeline
with phase/type/hygiene evidence, same-node-count reassociation exposing further
cleanup, a reachable recursive cycle rendered without evaluation, cleanup-exposed
signature removal, and a 130-call fixture that reaches a folding fixed point.
Independent builders produce deterministic Core and closed UPLC. Accepted cleanup
reaches an unchanged-pointer fixed point; no node-count stopping rule is used.

Static lifting and unused-parameter removal now repeat with constant folding and cleanup until all
leave Core unchanged. There are no optimization iteration or resource cutoffs.
The regression with a parameter used only in a constant dead branch now verifies
equal closed output across two optimizer invocations. Further regressions cover
static parameters exposed by branch cleanup or recursive pruning and all-static
oversaturated recursion. The shared executable fixture harness checks second-run
closed-code equality after each semantic snapshot. ANF still runs once. See
[the convergence follow-up](../docs/research/optimizer-convergence.md).
The mode decision retains O0/O1, default O1, and the existing project/CLI precedence.

Initial composition validation: all 3,857 workspace nextest tests pass; the five composition
checks also pass independently. Strict workspace Clippy, formatting and whitespace
checks pass. All 194 explicit performance cases match the accepted baseline, and
Cargo metadata still excludes the performance package from the root workspace.
The retained Plan 08 scope is complete; deferred work below remains deferred.

**O1 integration (29 September 2026), accepted scope.**
Production `nash-codegen::optimizer` owns the accepted Core pipeline; snapshots
and performance measurements reuse it. Build/test accept `-O0`/`-O1` and
`--optimize 0|1`; application/package config accepts integer `optimize: 0|1`.
The build/test default is O1. Workspace members own these settings, as with trace/target
settings. CLI overrides each owning project's level. No O2 is defined.
O1 preserves enabled traces and their order; trace generation remains independent.
Representation cancellation and bound-constructor folding run only in the
cleanup loops after the single ANF normalization.
Post-recursion work is freshening and optimized lowering with builtin/constant
sharing, never a second normalization. Explicit comptime execution remains O0.
Chunks 9–10 and the final convergence review are complete.
Default-O1 validation exposed deep-tree stack overflows; assembly traversals now use
heap work lists, with no stack enlargement; UPLC term printing is also iterative. Remaining depth risks and
follow-up probes are recorded in [the compiler stack audit](../docs/research/compiler-stack-audit.md).

Compose only the accepted passes. Establish their actual order from interactions:
inlining exposes dead code and known cases, sharing can conflict with inlining,
and recursion rewriting creates cleanup opportunities. Reuse the same accepted
passes in cleanup only if their input contracts allow nested post-rewrite Core.
Do not repeat ANF-dependent passes or whole-program sharing after recursion rewriting.

Detect actual structural progress or accurate rewrite reports, not equal node
counts. Keep generated names deterministic. Test same-size rewrites, cycles,
cleanup idempotence, the joint signature/folding fixed point, and hygiene after every pass. Check ANF only
in the main optimization phase, before recursion rewriting.

Retain named phase sections for raw Core, ANF, optimized recursive Core, rewritten
Core and lowered output (plus any separately accepted later cleanup). Keep each optimization's before/after pair in
one snapshot at the representation it transforms. Differentially evaluate baseline
and optimized programs, including selected traits, Logic laziness and Big/little case fixtures.
Every executable codegen snapshot must expose the original Core and O0 UPLC,
then accepted optimized Core and optimized UPLC, using the shared test pipeline
in `crates/nash-codegen/tests/support/optimizer.rs`. Thus accepting a pass updates
the full source and hand-built fixture corpus, not only its dedicated examples.
Source fixtures also retain their Nash text in the snapshot description and
capture evaluation results/traces where applicable. Hand-built Core/UPLC pass
fixtures capture their actual IR inputs, not invented Nash or Rust source text.
Omit Rust expression metadata from pass snapshots. Automated tests call compiler,
optimizer and evaluator Rust APIs directly; do not launch the Nash CLI binary.
CLI behavior is tested manually by the user. Keep independent type, hygiene,
semantic-equivalence and fixed-point checks after snapshot assertions.
Retain isolated-pass and intermediate-phase evidence after this common comparison.
Metadata and invalid-Core diagnostics remain focused; close open fixtures explicitly
for rendering, and never execute deliberately divergent fixtures.
Keep O0 snapshots; never mass-replace them with optimized ones. Run ordinary
semantic checks separately from the explicit performance regression command.

Final configuration is one optimized mode, O1, alongside explicit O0. Existing
CLI/configuration validation accepts only 0 and 1, project settings belong to the
owning workspace member, and command-line overrides take precedence. Defaults,
trace policy, trait/Logic/Big/little semantics and both lowering modes retain their
existing source, driver and configuration tests.

**Done when:** the accepted combination is semantically equivalent, convergent,
measured and reviewed; final configuration is decided and documented. Mark
SPEC.md complete only when the retained scope is implemented and validated.

## Chunk 12 — Application fusion, native packing and late binding cleanup

These are O1 rules. They preserve enabled traces, success/failure and termination;
none requires silent O2. The user requested adoption after the first measured
fusion/packing case and explicitly assigned this work to Plan 08.

- Adjacent Core application stages fuse inside `single_use::inline`, within the
  existing cleanup fixed point. Only a sole direct use qualifies; operands remain
  atomic and intermediate type views must agree. Arguments containing the binder,
  escaping uses and intervening computations cannot fuse.
- `lower::lower_optimized` is the common O1 entry point for production, shared
  snapshots, source tests and performance checks. It lowers with builtin/constant
  sharing, then calls `uplc_optimizer::optimize`. O0 and isolated sharing APIs
  remain available unchanged.
- Late cleanup removes identity applications without dropping argument evaluation,
  substitutes single-use values, removes unused value bindings and cancels direct
  force/delay pairs. Forced builtins and applied prefixes remain computations, so
  their sharing is retained. Substitution rejects potential name capture, including
  shadowed names. Recursive self-application is never unfolded by multi-use beta
  substitution.
- Native packing replaces eligible application spines with `Case(Constr(0, args),
  [function])`, requiring both function and arguments to be UPLC values. Three
  arguments is the first arity where two native nodes replace more Apply nodes.
  This is a cost-derived rule, not an optimizer execution limit. Existing packed
  prefixes can extend; an effectful later argument retains its outer Apply.
- Cleanup and packing reach a pointer fixed point without rebuilding Apply nodes
  from packs. Neither repeats ANF nor re-encodes recursion. Named inputs must be
  closed and well-scoped.

The isolated baseline now includes four additional source cases: the original
staged call, traces between call stages, an effectful later argument, and failure
before that argument. Unit snapshots also cover shadowing, strict unused bindings,
forced references, recursive divergence (rendered only), and four-argument packing.
Full O1 Flat code is compared after one, two and three runs for all performance
fixtures; normal fixture checks cover Core and late-pass fixed points separately.

**Complete.** All 3,876 workspace nextest tests pass, as do root and isolated
performance-workspace strict Clippy and formatting checks. All 199 explicit
performance cases match the reviewed baseline. Every changed snapshot preserves
its original O0 content. The application report records the size tradeoffs.
Performance checks remain outside normal Cargo tests.

## Boundaries and deferred work

- Integer dispatch is explicit in [Plan 11](11-macros-comptime.md), via an AST/Core
  operation. Ordinary integer literal case remains equality-based. Do not add
  density/max-index thresholds, automatic dispatch selection, guards or table
  filling. Optimizing a known explicit dispatch must preserve its failure and
  branch behavior.
- Explicit Big constructor tags are a separate representation feature; they are
  not permission to renumber tags or treat wildcards as arbitrary runtime tags.
- Source decoder fusion is outside this plan. It needs its own design/review;
  do not sneak in recognition of stdlib function names during folding.
- Native boolean case lowering already exists. Remove no delays on the assumption
  that it still lowers through eager `ifThenElse`; test the actual current backend.
- Cost-model changes require deliberate review/rebaselining of performance tests,
  not silent acceptance. Record rejected candidates as well as retained ones.

## References

Use these as references for individual reviewed rules, not an implementation to
copy wholesale:

- Aiken `crates/uplc/src/optimize.rs`: pass composition and fixed points.
- Aiken `optimize/shrinker.rs`: occurrence analysis, inlining, force caching,
  currying, safe builtin evaluation, inverse conversions and common scopes.
- Aiken `optimize/interner.rs`: binder hygiene.
- Elm `elm/compiler/src/Optimize/Expression.hs`: traversal organization for a
  different target.

### Vesting baseline ABI correction (28 September 2026)

The vesting source fixtures and runnable examples now receive one V3 ScriptContext,
with the redeemer in the context and datum in SpendingScript. Only VestingParam's
minimum-lock parameter precedes the context and is applied off-chain. TxInfo now
uses V3 field order, a validity interval in POSIX milliseconds, and a signatory
list. Previous vesting measurements used a synthetic three-argument convention;
they are historical optimizer experiments, not measurements of this corrected
V3 entry point. The regenerated baseline preserves all 12 vesting outcomes/logs in both pipelines
and leaves the other 11 rows unchanged. Unoptimized and optimized execution
equivalence checks remain required.
