# Plan 15 — Silent O2 and application regrouping

## Status and confirmed contract

**Initial silent O2 implemented (5 October 2026); broader motion remains planned.** Application fusion,
values-only native packing and late UPLC binding cleanup belong to
[Plan 08](08-optimizer.md), as confirmed by the user. This plan retains O2 silence
and the broader grouping/motion investigations.

User decisions:

- O2 automatically runs silently: no user or compiler traces in emitted scripts.
- O2 discards trace message computations even if they fail or diverge, as
  explicitly confirmed by the user. The value returned by a trace still evaluates.
  Unrelated failure or divergence must not move under an unused lambda.
- Rewrites that preserve the full O1 contract, including traces, belong in O1.
- Optimization continues until unchanged. Do not introduce attempt, execution,
  payload, depth, node, growth or iteration cutoffs without a separate decision.

Keep the current O1 default. O2 is an explicit build/test/project setting when
used. Explicit compact/verbose tracing or compiler traces with O2 are errors.
Merge command overrides before applying test defaults; O2 tests remain silent.
Do not rewrite saved configuration. O0 and O1 retain their trace policy.

Compare O2 with O0 under the same silent source semantics. Source trace syntax
already omits its message computation in silent mode. Direct `Builtin.Trace` is
strict under O0/O1. O2 explicitly removes its message expression, including
failure or divergence, for direct and aliased calls. Preserve evaluation of its
returned value. Separately evaluated expressions outside the trace remain strict.
A first-class trace value becomes a correctly typed no-log function. Calls through
unknown function parameters remain ordinary strict caller work; no lazy calling
convention is introduced. Erasure precedes O1 inlining.

## Current source findings

- `nash-ir/src/anf.rs` preserves application stages: it evaluates an earlier
  application before a later non-atomic argument. An earlier application may
  execute a body, emit a trace, fail or diverge.
- `nash-codegen/src/lower.rs` lowers Core applications left-associatively. Flattening
  an application representation is different from evaluating all arguments early.
- `single_use::inline` deliberately keeps computed function operands bound. It
  does not generally turn `let p = f a in p b` into `f a b`.
- `analysis::safe_to_discard` is not a proof that a term can move or be duplicated.
- Source `user_trace` obeys silent configuration, but direct `Builtin.Trace`
  bypasses it. Tests also enable compiler traces separately in the driver.
- `assemble_core_with_options` accepts prebuilt Core without a `TraceConfig`.
  O2 silence therefore needs enforcement beyond CLI configuration.

These observations describe the pre-extension source. Plan 08 now implements
adjacent fusion, values-only native packing and late UPLC cleanup. The remaining
O2 entry-point and broader motion observations still apply.

## Aiken reference and semantic classification

Reference checkout: `aiken-lang/aiken`, commit
`bd0e4e30c6d8f51221bd9aabf22cfeb794d72884`.

| Candidate | Classification and proof required |
| --- | --- |
| Adjacent application fusion: `let p = f a in p b` to `f a b` | O1 candidate when `p` has only that use, operands are atomic, and the same application stages remain in the same order. Preserve type views and hygiene. |
| Adjacent independent binding grouping: `let x=A in let y=B in C` to `(lambda x y -> C) A B` | O1 candidate when `x` is not free in `B` and A-before-B evaluation is unchanged. Measure whether it helps directly or only enables a later encoding. |
| Grouping across a later computation using known arity | O1 candidate only when actual lambda/builtin shape proves each crossed partial application returns a value without executing a body. A function type alone is not this proof. |
| Dependency-based reordering of nonadjacent bindings | Dependencies alone are insufficient. Require a motion proof covering traces, failures and divergence; after trace erasure, O2 still needs the failure/termination proof. |
| Moving bindings or traces under unapplied lambdas | An effectful or nonterminating RHS cannot move just because its value is used later. Trace-only work disappears in O2; arbitrary strict work still needs a proof. |
| Packing applications into native `Case`/`Constr` | Candidate for O1 or O2 depending on the proven evaluation-order preconditions. Use the actual target's native term support. Do not treat this as cosmetic application flattening. |

Aiken's `crates/uplc/src/optimize/shrinker.rs` contains:

- `split_body_lambda` (around line 1879): groups bindings by variable dependencies,
  may reorder independent groups, and moves unmatched lambdas outside rebuilt
  bindings. It has no general effect/termination gate. In particular,
  `(lambda x -> lambda y -> B) A` can become
  `lambda y -> (lambda x -> B) A`, delaying A until y is supplied. That unrestricted
  rule violates this plan's contract when A fails or diverges.
- `case_constr_apply_reducer` (around line 2058): rewrites application spines to a
  native constructor/case encoding and merges more arguments into existing packs.
  Ordinary application evaluates F, A, applies F to A, then evaluates B; a packed
  constructor evaluates its fields before the branch function is run. Safe cases
  need an explicit proof of which computations are crossed.
- `afterwards` applies these rewrites without a silent-mode gate. Its group-size
  heuristics and argument-count threshold are not Nash policy.

Aiken's `builder::delayed_trace` emits `force (trace message (delay body))`.
That emits before evaluating the body. There is no separate trace-sinking pass
in the inspected shrinker; the lambda motion can explain traces moving later.

## Execution and review

Follow the existing optimizer experiment workflow: three concrete cases, one at a
time. Show the source-generated Core and before/after UPLC, check behavior, and
measure CPU cost, memory cost and raw Flat bytes in `tools/optimizer-perf`.
Review each candidate before production adoption. Do not implement this entire
plan before reviewing the first candidate. Do not use size growth as an implicit
rejection gate or silently import Aiken's thresholds.

Keep normal tests semantic and structural. Performance experiments and cost
regressions remain outside normal Cargo test discovery. Preserve O0 snapshots.
Reuse compiled fixtures; render snapshots before independent property assertions.

## O1 work moved to Plan 08

Application fusion, values-only native `Case`/`Constr` packing and late UPLC
binding cleanup are implemented under [Plan 08, Chunk 12](08-optimizer.md).
Their source measurements and semantic boundaries are in the
[application report](../docs/research/application-packing-trial.md). They are not
pending O2 features. Continue broader grouping only from those validated rules.

## Chunk 2 — Automatic O2 silence (complete)

Implemented through config, CLI settings resolution, driver, test compilation and
public Core assembly. Source and Core snapshots cover failure erasure, retained
value failure, partial/aliased trace references, and lambda/delay bodies. The
[three-case report](../docs/research/o2-silence-trial.md) compares O1 verbose,
O0/O1 silent and O2 silent. No broader grouping or motion is added.

Completed scope:

- Add O2 numeric parsing/serialization and CLI help for `-O2` / `--optimize 2`.
- Resolve effective silence after project settings, command defaults and overrides.
  Cover `Build::for_tests` and the driver's currently hardcoded compiler tracing.
- Disable source user/compiler trace generation before constructing bodies whose
  trace expressions can be omitted under existing silent syntax semantics.
- At the Core/assembly boundary, handle both `CoreKind::Trace` and direct builtin
  Trace, including saturated, partial, aliased and first-class uses.
- Remove trace message expressions, even failing/diverging ones. Replace bare
  builtin references with typed no-log functions; retain returned-value evaluation,
  remaining applications and type views.
- Check emitted UPLC, including lambda and delay bodies, contains no Trace builtin.

Reject explicit compact/verbose/compiler-trace settings for O2 in build and test.
Test
nested project ownership and direct assembly of hand-built Core. Test successful
values, failing messages, partial calls and unused functions. Do not confuse test
runner diagnostics with script traces. Explicit `comptime` evaluation is a separate
feature; this plan does not change its budget or evaluation policy.

## Chunk 3 — Known-arity grouping, then native application packing

Investigate after the Plan 08 application work is validated. Use actual lambda/builtin arity to
identify intermediate applications that cannot execute a body. Do not weaken
ANF staging globally based on function types.

For native `Case`/`Constr` packing, explicitly identify the function and argument
computations whose order changes. Known values/closure-building stages may make
some cases O1-safe. Erased traces can expose additional eligible cases under O2,
but potential failure/divergence remains a barrier.

For each candidate, compare direct calls, effect-sensitive/excess-argument cases,
and real source workloads. Select Core versus late UPLC placement from the emitted
code and semantic proof. Avoid adding an IR node solely to copy Aiken's machinery.

## Chunk 4 — Lambda and binding movement

Consider dependency grouping and motion only with a sufficient proof that crossed
computations cannot change observable behavior. Partial applications, escaping
functions, unused functions, early failures and recursive divergence are mandatory
counterexamples. Trace erasure alone is not that proof.

Do not add a trace-after-lambda rewrite to O1 if it changes when or how often traces
occur. In O2, erased trace nodes do not need to be moved. Measure a remaining code
shape before proposing a separate pass.

## Chunk 5 — Composition and completion

- All accepted Core passes reach a joint unchanged-pointer fixed point; freshening
  and the initial normalization are not repeated as a substitute for convergence.
- Late UPLC rewrites, if adopted, have their own stable structural contract and do
  not repeatedly expand recursive encodings.
- Check one/two/three optimizer runs produce the same closed code, including cases
  exposed by interactions between grouping, lifting, parameter removal and folding.
- For O1 compare values, failures, traces and termination against O0. For O2 compare
  against silent O0, assert no emitted traces, and preserve failures/termination
  outside discarded trace message computations.
  Render deliberate divergent cases; use isolated bounded observations for runtime
  experiments without introducing optimizer execution limits.
- Review changed performance rows explicitly; preserve result evidence and distinguish
  changes in budget consumption from semantic changes.
- Run formatting, strict Clippy and `cargo nextest run --workspace`; run the explicit
  performance workspace separately. Update changesets, docs and SPEC as chunks land.

**Done when:** O2 is automatically silent at every entry point, each adopted rewrite
has a reviewed semantic classification and measurements, and the complete pipeline
converges without violating the selected mode's contract.
