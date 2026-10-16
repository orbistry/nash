# Type system audit before Plan 11

Plan 11 sends macro output through the ordinary canonicalize, constrain, solve,
and nitpick path ([plans/11-macros-comptime.md](../../plans/11-macros-comptime.md),
"Checking"), and `@derive` generates impls. This audit asks which trait, kind,
and tests-block rules hold, which diverge from Elm or Haskell and why, and which
rules no test pins. The audit changed no compiler code, existing docs, or tests.

The short answer: the solver is sound and coherent on everything we probed
except transparent aliases. There, impl selection depends on the order in which
unification met the two sides (H1). Two other findings change what macro output
may contain: a `let` around a multi-parameter trait call can be rejected where
the same expression bound by `case` or a lambda is accepted (H2), and `nash check`
accepts programs that code generation then rejects without a source region (H3).

## 0. Baseline and method

All citations are `file:line` at jj change `sxotkppl`, commit `67bd883d` (empty;
its tree equals `kmnvwzxv` `b692bbfc`). `jj log -r 'main..@'` at that point
lists 27 changes, from `soxtwqun` (`8c1071fc`) to `sxotkppl`. The binary under
test is `target/debug/nash` built at that commit.

Probes and raw notes live outside the repo, in `/tmp/nash-ts-audit/`.

| Path | Content |
|---|---|
| `probes/<prefix>_<nn>_<name>/` | One probe project each: `src/Main.nash`, `check.out`, `test.out`, or `build.out`. Some also hold `Main.hs` + `ghc.out` or `elm/` + `elm.out`. |
| `probe.sh <dir> [check\|test\|build]` | Runs the baseline binary and writes `<dir>/<cmd>.out` with the exit code. |
| `notes/*.md` | Per-slice notes with every rule, property, and finding, in more detail than this report. |
| `oracle-ghc/`, `oracle-elm/` | Differential corpora and regeneration scripts. |
| `mutants/m*/mutants.out/` | cargo-mutants output (`caught.txt`, `missed.txt`, `outcomes.json`). |

Probe prefixes: `ent_` entailment, `gen_` generalization, `kind_` kinds,
`tb_` tests blocks, `term_` termination, `elm_` Elm ledger, `elmo_` Elm oracle,
`ghc_` GHC oracle, `tm_` test map, `v_` re-verification.

Tools: elm 0.19.1 (official macOS binary), GHC 9.10.3 (ghcup), and
cargo-mutants 27.1.0 with `--test-tool nextest -j 2`. Every quoted output was
produced by the baseline binary. A reboot wiped `/tmp` partway through. Every
probe quoted here was regenerated and re-run afterwards. Generator scripts for
the Elm ledger and the test map were also saved to `~/.nash-ts-audit-elm/` and
`~/.nash-ts-audit-test-map/`.

Anything not observed is marked [INFERENCE]. Docs are treated as claims. Where a
doc and the code disagree, the code is the fact and the doc is listed in
[section 8](#8-doc-claims-the-code-contradicts).

Notation: `Γ ⊢ e : τ | W` means `e` has type `τ` and produces wanted predicates
`W`. `G ⊢ P` means the givens `G` entail `P`. THIH is Jones, "Typing Haskell in
Haskell". H98 is the Haskell 98 Report.

## 1. Rules

Each rule lists its Nash implementation, the Elm or THIH counterpart, the tests
that exercise it, and whether the doc agrees. "Untested" means no test in
`crates/*/tests`, `crates/*/src/**/tests.rs`, or the snapshot directories
exercises the rule. [Section 5](#5-test-adequacy) adds the mutation evidence,
which is stricter.

### 1.1 Kinds

| # | Rule | Nash | Elm / THIH / H98 | Tests | Doc |
|---|---|---|---|---|---|
| K1 | `κ ::= Type \| κ → κ \| var`. First-order unification with occurs check. `apply(h, args)` unifies `h ~ arg → res` with a fresh `res` per argument. | `crates/nash-can/src/kinds.rs:56-114` | H98 §4.6. THIH takes kinds as given (§3), no inference. Elm has none. | `crates/nash-can/src/kinds.rs:138-215` (`self_application_fails_occurs_check`, `indirect_cycle_fails_occurs_check`, ...), `crates/nash-can/tests/kinds.rs:178 infinite_kind`, `:608`, `:634` | `docs/kinds.md:42-51` agrees |
| K2 | `⟦'a⟧ = κ_a` shared within one declaration or annotation. `⟦T τ̄⟧ = apply(kind(T), ⟦τ̄⟧)`. Function, tuple, and record components must be `Type`. Alias bodies must be `Type`. | `kinds.rs:568-651`, alias body `kinds.rs:1562-1563` | H98 §4.6. GHC accepts non-`Type` synonym bodies. | `crates/nash-can/tests/kinds.rs:219`, `:224`, `:231`, `:372` | `docs/kinds.md:44` agrees. `:42` ("Kind inference is Haskell 98") overstates, see L2. |
| K3 | Unions and aliases of one module form SCCs (`scc.rs`, Elm's `stronglyConnComp`). Each group shares its constructor kinds. Leftover kind variables default to `Type` after the whole group is walked. | `kinds.rs:1466-1584`, `default_and_zonk` `kinds.rs:117-130`, `crates/nash-can/src/scc.rs:22-126` | H98 §4.6 (per dependency group). | `crates/nash-can/tests/kinds.rs:126`, `:214`, `:641`, `:777`; `scc.rs` itself untested | `docs/kinds.md:50-51`, `:183` agree |
| K4 | Trait parameter kinds: one variable per parameter shared by superclasses and every method signature. Trait SCCs run after all data kinds are closed. Marker traits default to `Type`. | `kinds.rs:1835-1947`, order `crates/nash-can/src/module.rs:86-88` then `crates/nash-can/src/traits.rs:169` | H98 §4.6 (classes and data share one analysis; Nash splits it, equivalent because data cannot mention user traits). | `crates/nash-can/tests/kind_predicates.rs:189`, `crates/nash-can/tests/impls.rs:975`, `:995` | `docs/kinds.md:264-270` agrees |
| K5 | Impl heads: each head is kinded and unified with the closed trait parameter kind. Context predicates share the head checker. | `kinds.rs:1949-1989`, called from `crates/nash-can/src/impls.rs:234` | H98 §4.3.2 | `crates/nash-can/tests/impls.rs:995 impl_head_kind_mismatch` | `docs/traits.md:158` agrees |
| K6 | Annotations: one fresh checker per annotation; no defaulting here. | `kinds.rs:1793-1833` | none | `crates/nash-can/tests/kinds.rs:315-372`, `crates/nash-solve/tests/representation_predicates.rs:110` | `docs/kinds.md:233-236` agrees |
| K7 | Solver kind contracts: after solving, kinds are rebuilt from the union-find graph. At generalization, `freeze` defaults every quantified variable's kind to `Type`, so there is no kind polymorphism. | `crates/nash-solve/src/kind_check.rs:127-198`, `:278-375`; calls `crates/nash-solve/src/solve.rs:3006-3036` | H98 (no kind polymorphism) | `representation_predicates.rs:110`, `:116`, `:122`; `inference.rs:3730`, `:3745`, `:3838`. `Check::predicate` (`kind_check.rs:228`) is pinned by nothing (section 5). | `plans/02-kind-predicates.md:49-51` agrees |

### 1.2 Datatype contexts

| # | Rule | Nash | Counterpart | Tests | Doc |
|---|---|---|---|---|---|
| D1 | Positional requirement: a field of an uppercase union or record alias requires `Big`. An uppercase non-record alias body requires `Big`; a lowercase one requires `Little`. Lowercase union fields require nothing. | `kinds.rs:1365-1415`, applied at `kinds.rs:1617-1629` | none (H98 datatype contexts are written, §4.2.1) | `crates/nash-can/tests/kinds.rs:156`, `:160`, `:170`, `:174`; `kind_predicates.rs:92` | `docs/kinds.md:83-99`, `:185-194` agree |
| D2 | Formation: `typ(T args)` instantiates `T`'s context at the supplied arguments, keeping only predicates whose variables are all supplied. A variable head gives `Apply h args`, which reduces when `h` is known, including partially applied aliases. | `kinds.rs:777-858`, `crates/nash-can/src/types.rs:537-607` | none | `kinds.rs:1119`, `:1165-1220`; `kind_predicates.rs:110`, `:219`; `crates/nash-can/tests/kinds.rs:756` | `docs/kinds.md:168-179` agrees |
| D3 | Closure with `IrregularRecursion`: a worklist over (reference, predicate). For a reference `B(args)` and a predicate `p` of `B` containing a variable application, every parameter of `p` must map to a bare parameter, else `IrregularRecursion`. No round cap. | `kinds.rs:954-1091`, rule `kinds.rs:1002-1013` | none (GHC accepts nested datatypes) | `kinds.rs:1236`, `:1266`, `:1292`; `kind_predicates.rs:129`, `:135`, `:219`, `:229` | `docs/kinds.md:195-222` agrees, including the admission at `:219-221` that contexts can grow large (M7) |
| D4 | Instantiation sites: annotations, impl heads, specialized impl methods, and inference. In the solver, a wanted `Apply{head,args}` reduces when the head is known and waits otherwise. `Apply` takes no evidence slot. | `kinds.rs:1813-1832`, `kinds.rs:1990-2015`, `impls.rs:541`; `solve.rs:1951-1974`, `:2049-2071`, `:1737`; `crates/nash-solve/src/representation.rs:117-138` | none | `kind_predicates.rs:110`, `:272-284`; `representation_predicates.rs:78`, `:94`; `crates/nash-solve/tests/evidence.rs:286`. The `formed_at` arms for alias, partial alias, tuple, and record (`solve.rs:2746-2777`) are pinned by nothing. | `docs/kinds.md:229-257` agrees except the error-chain example at `:247-249` (not reproduced) |

### 1.3 Impl table formation

| # | Rule | Nash | Counterpart | Tests | Doc |
|---|---|---|---|---|---|
| I1 | Head shape: trait arity matches. Outer head is a named constructor, unit, or tuple; nested patterns and repeated variables are allowed. A bare variable head is allowed only in the trait's own module. Function, record, and variable-application heads are rejected. Context variables must occur in the heads. | `crates/nash-can/src/impls.rs:162-194`, `:217-233`, `:370-467` | H98 §4.3.2 (distinct variables). Nash is roughly FlexibleInstances. THIH `addInst` has no shape check. | `crates/nash-can/tests/impls.rs:109`, `:710`, `:1046` | `docs/traits.md:127-169` agrees |
| I2 | Orphan: legal if the trait is local, or an outer head constructor is local, or (in `nash/base`) the head is unit or a tuple. An alias defined in the module counts as a local constructor. | `impls.rs:195-216` | none in H98 (GHC warns) | `impls.rs:71`, `:653`, `:1046`; `crates/nash-driver/src/compile.rs:923` | `docs/traits.md:218-223` agrees literally. It does not say an alias of a foreign type counts (H1). |
| I3 | Overlap: two heads overlap if they unify after independent freshening and, per variable, the intersection of the representation classes stays non-empty. Aliases are compared by name. Every impl of the trait from every build interface is checked. | `crates/nash-ast/src/head.rs:467-582`, `impls.rs:105-143`, `head_repr` `impls.rs:544-553` | THIH `overlap p q = defined (mguPred p q)`; H98 §4.3.2 forbids overlap | `impls.rs:38`, `:548`, `:862`, `:1019`, `:1068`, `:1096`; `kind_predicates.rs:203` | `docs/traits.md:225-242` agrees |
| I4 | Big Eq ban: a user `impl Eq h` is rejected when `h` is Big, an alias whose subject is Big, or a bare variable whose class admits Big. User impls of the representation traits are rejected. | `impls.rs:180`, `:250-284` | none | `crates/nash-can/tests/structural_eq.rs:9`, `kind_predicates.rs:213` | `docs/traits.md:180-197` agrees |
| I5 | Reflexive-Lift overlap ban: an explicit `Lift` impl whose two heads can be equal is rejected. | `impls.rs:285-302`, `can_equal` `head.rs:479-486` | none | `impls.rs:132`, `:189`, `:225`, `:256` | `docs/traits.md:285-293` agrees |
| I6 | Superclass check at the impl: for each superclass `S` of `T`, `S h̄` must follow from the impl context closure by `by_given` and `by_instance` (not `by_coverage`). A predicate already on the active chain is `Cycle`. One budget of 16,384 steps per impl, depth below 256. | `crates/nash-can/src/entailment.rs:212-449`, called at `impls.rs:99-101` | THIH `bySuper`, `byInst`, `entail`; H98 §4.3.2 | `impls.rs:370`, `:420`, `:446`, `:471`, `:495`, `:522`, `:1150`; `crates/nash-can/tests/traits.rs:50` | `docs/traits.md:170-176`, `:279-302` agree |

### 1.4 Entailment

| # | Rule | Nash | Counterpart | Tests | Doc |
|---|---|---|---|---|---|
| E1 | `by_given`: `P` holds if a given in the nearest frame is the same predicate (compared without unification), or a superclass path from a given reaches it. The closure is built breadth first, explicit givens first. The solver waits while an outer flexible variable could still change the match. | `solve.rs:1823-1910`, `:2427-2458`, `:2043-2048`; `crates/nash-solve/src/preds.rs:284-398` (`same_args`) | THIH `bySuper`, `entail` | `inference.rs:1139`, `:1152`, `:1184`, `:1323`, `:1365`, `:5812`; `solve.rs:4462` | `docs/traits.md:259-260` agrees |
| E2 | `by_instance`: exactly one impl matches and its context becomes sub-wanteds. Matching inspects an argument only where a head tests it: a constructor pattern, a repeated variable, or a variable with a representation class. An unrestricted head variable binds the argument without inspecting it, so an owned blanket impl `impl Name 'a` is selected for a flexible or rigid argument (`v_03_owned_blanket_open_arg`: `f x = name x` infers `f : 'a -> string`, no context). Arguments need decided heads only at tested positions, not everywhere. A second match is a compiler `panic!`. Inference uses `SelectPolicy::Defer` (any undecided candidate defers). Superclass entailment and ground evidence use `Exact` (a selected candidate wins over an undecided one). | `head.rs:191-309` (unrestricted variables `head.rs:207-223`, panic at `head.rs:291`), `crates/nash-solve/src/resolve.rs:348-396` (a `Selected` result is accepted at `:385-392` regardless of argument heads), `solve.rs:2316-2395` | THIH `byInst` | `head.rs:679 selection_outcomes_by_policy`, `head.rs:712-722` (`should_panic`), `solve.rs:4701`, many `inference.rs` tests. Blanket selection for an open argument: untested. | `docs/traits.md:261-263` says "every argument of `P` has a head constructor", which the code does not require (section 8) |
| E3 | `by_coverage`: when selection defers because a rigid variable sits at a representation-tested head position (a subject), first require a given of the same trait with the subject as an argument. Then every cell (Big, Const, Term per subject, as the givens admit) must select an impl whose context holds in that cell. Evidence is `Specialize{trait, args}`; codegen selects per use. One 16,384-step budget. Inference only. | `solve.rs:2220-2293`, `:2474-2709`; `resolve.rs:294-334`, `:414-455`; `head.rs:311-442` | none (GHC rejects such programs) | `inference.rs:5542-5730` (9 `coverage_*` tests), `head.rs:859`, fixture `crates/nash-driver/tests/fixtures/base-traits/Coverage.nash` via `crates/nash-driver/tests/base_traits.rs:128` | `docs/traits.md:264-283` agrees |
| E4 | Reflexive Lift: for the exact core `Lift` (package `nash/base`, module `Lift`), `Lift a b` holds when `a` and `b` are nominally equal. It never unifies or narrows. It is also an improvement candidate. | `solve.rs:2152-2168`, `crates/nash-solve/src/evidence.rs:304-319`, `:380-401`, `entailment.rs:336-351`, `resolve.rs:704-720` | none | `inference.rs:3563`, `:3659`, `:5834`; `crates/nash-solve/tests/evidence.rs:379` | `docs/traits.md:285-289` agrees |
| E5 | Structural Big Eq: a wanted `Eq t` is `StructuralEq` when `Big t` is given or `repr(t) = Big`. Enabled when the core `Eq` trait exists. | `solve.rs:1989-2009`, `:2149-2168`, `:2622-2640`; `evidence.rs:291-303`; `entailment.rs:322-335`; `crates/nash-can/src/environment.rs:121` | none | `inference.rs:4068`, `representation_predicates.rs:202`, `structural_eq.rs:9` | `docs/traits.md:180-187` agrees |
| E6 | Relational improvement (traits of arity 2 or more): probe givens, then impls, then reflexive Lift, without changing the graph. If exactly one candidate is ready, commit its equalities between existing variables. A deferred competitor blocks. No new constructor is invented. | `resolve.rs:456-728`, `solve.rs:2189-2219`, given candidates force `Deferred` at `resolve.rs:357-365` | none in THIH; nearest is functional dependencies | `inference.rs:5761`, `:5790`, `:5812` | `docs/traits.md:418-442` agrees |
| E7 | Out-of-solver resolve, used by codegen for `Super`, `Specialize`, and uses without an instance, and by power-assert: ground predicate, decided formation requirements, `repr_of` for representation traits, structural Eq, reflexive Lift by `agree`, else `select_impl(Exact)`. Work list with 16,384 steps and depth below 256. Returns `Result<Option<Evidence>, Error>`. | `evidence.rs:29-377`; callers `crates/nash-codegen/src/evidence.rs:192-246`, `crates/nash-codegen/src/build.rs:542-561`, `crates/nash-codegen/src/can_to_core/methods.rs:229-238`, `crates/nash-codegen/src/assertion.rs:55-59` | THIH `byInst` over ground predicates | `crates/nash-solve/tests/evidence.rs:67-651` (8 tests), `crates/nash-codegen/src/evidence/tests.rs:80-449`. Its failure paths are mostly unpinned (section 5). | `docs/traits.md:556-608` disagrees on the signature and on which evidence it returns (section 8) |

### 1.5 Constraint generation and generalization

| # | Rule | Nash | Elm / THIH | Tests | Doc |
|---|---|---|---|---|---|
| G1 | Unannotated `let f = e`: solve the header at rank r+1, classify wanteds, `generalize`, check ambiguity and default, retain the rest, bind `f : ∀v̄. W_ret ⇒ τ`. | `crates/nash-constrain/src/expression.rs:1129-1168`; `solve.rs:486-643` | Elm `Type/Constrain/Expression.hs:528-565`, `Type/Solve.hs:157-194`; THIH `tiImpls`, `split`, `quantify` | `inference.rs:2111`, `:2127`, `:1226` | `docs/traits.md:374-411` agrees on behavior, not on order (G6) |
| G2 | Annotated definition: rigids are the annotation's free variables minus enclosing rigids (so nested annotations share names with the enclosing one). The context is the given set. Wanteds over rigids must be entailed, else `MissingConstraint`. No context is inferred. | `expression.rs:1170-1265`; `solve.rs:1865-1910`, `:2294-2315`, `:3142-3182`, `:585-596` | Elm `Expression.hs:555-557`, `:566-598`; THIH `tiExpl` | `inference.rs:1139`, `:1167`, `:1184`, `:2600`. Scoped type variables as a rule: untested. | `docs/traits.md:244-253` agrees. Scoping is undocumented (M5). |
| G3 | Recursive groups: typed members are declared first and are polymorphic at recursive calls. Untyped members are monomorphic in the group and share one binder and one context. Unlike Elm, a later untyped member's pattern state does not re-introduce earlier members' variables. | `expression.rs:1287-1459` (Elm difference at `:1321-1324`); `solve.rs:3045-3140`, quantifier ownership `solve.rs:608-617`, `:3703` | Elm `Expression.hs:601-686`; THIH `tiBindGroup` | `inference.rs:1072`, `:1415`, `:1308`, `:2551`; `solve.rs:4816`, `:5104`. The difference from Elm is untested. | `docs/traits.md:485-487` says "follow Elm", which is wrong for annotated self-calls (section 8) |
| G4 | Destructuring let: one aggregate scheme keyed by the root pattern. A use of any extracted name instantiates every aggregate variable and context slot. | `expression.rs:954-1009`; `solve.rs:609-631`, `:3198-3205` | Elm `Expression.hs:510-521`; under the H98 MR pattern bindings are not generalized over constrained variables | `inference.rs:2141`. Class-constrained aggregates: untested. | `docs/traits.md:545-554` agrees |
| G5 | Wanted production: a use instantiates the scheme and pushes one wanted per context element. Literals are uses of `fromInt` and friends. Integer, string, and bytes patterns also want `Eq`. Impl contexts become sub-wanteds tagged with their parent. | `solve.rs:3207-3230`, `:2362-2395` | THIH `tiExpr` | `inference.rs:807`, `:1840` | `docs/traits.md:354-372` agrees |
| G6 | Classification, in code order. In `solve_header`, before `generalize`: discharge by given; defer if there is no binder or an outer flexible variable occurs under givens; then representation traits, structural Eq and reflexive Lift, impl selection, improvement, coverage; then `MissingConstraint` for rigid arguments of annotated bodies. After `generalize`: ambiguity and defaulting, then `retain_wanted` retains predicates with a generalized variable and defers the rest. | `solve_header` `solve.rs:673`, `resolve_wanted` `solve.rs:2011-2402`, `generalize` `solve.rs:533`, `:2835-2878`, defaulting loop `solve.rs:567-582`, `retain_wanted` `solve.rs:3506-3531` | THIH `split` (after `reduce`) | `inference.rs:1253`, `:1323`, `:1365`; `solve.rs:4536`, `:4612` | `docs/traits.md:376-392` describes classification after `generalize`. The code resolves before it (section 8). |
| G7 | Context reduction: sort by creation order, keep the first exact duplicate, drop a predicate entailed by another retained predicate's superclass closure (compared without unification), assign slots to survivors. Removed predicates get `Given` or `Super` evidence. Contexts are not reduced to head-normal form. | `solve.rs:3533-3642`, `:1823-1863`; `preds.rs:129-158` | THIH `reduce`, `simplify`, `toHnfs`; H98 §4.5.2 requires head-normal form | `inference.rs:1276`, `:1152`; `solve.rs:4462`; `preds.rs:541` | `docs/traits.md:317-318`, `:394-399` agree. `:381-383` reads as if an impl context always replaces the predicate (L7). |
| G8 | Ambiguity: roots are the types of all definitions in the group. If no member is annotated, a multi-parameter predicate extends reachability through an argument whose variables are non-empty, all reachable, and all generalized. A generalized variable in a wanted that is not reachable is ambiguous. | `solve.rs:3313-3470`, closure `solve.rs:3334-3370` (`rank == NO_RANK` at `:3359`) | THIH `ambiguities`; H98 §4.3.4 | `inference.rs:958`, `:994`, `:1007-1037`, `:5790`, `:241`. A captured determining argument: untested (H2). | `docs/traits.md:444-457` agrees |
| G9 | Literal defaulting: for an ambiguous flexible variable, if its wanteds hold exactly one distinct core literal trait whose sole argument is that variable, unify it with `int`, `string`, `bytes`, `bool`, or `unit` and retry. Runs at every generalizing `Let`. The default is not checked against the variable's other predicates first. | `solve.rs:3398-3439` (guard `:3411-3432`), `crates/nash-constrain/src/type_.rs:349` | H98 §4.3.4; THIH `defaultSubst`, `candidates` | `inference.rs:807`, `:994`, `:1685`, `:1810`, `:5282`. User-class plus literal and representation-constrained defaults: untested. | `docs/traits.md:458-481` agrees on the mechanism. Its "outcome is the same as Haskell" is only partly true (M1). |
| G10 | Evidence construction: `Repr`, `StructuralEq`, `ReflexiveLift`, `Given{binder,index}`, `Super{binder,index,path}`, `Impl{type_args,args}`, `Specialize{trait,args}`. | `solve.rs:1706-1791`; `crates/nash-ast/src/lib.rs:628-659` | THIH has no evidence | `solve.rs:4462`, `:4536`, `:4701`, `:4816`; `crates/nash-codegen/src/evidence/tests.rs`; runtime probes `gen_60`, `gen_61` (13 of 13 pass) | `docs/traits.md:304-312` lists five of the seven variants (section 8) |
| G11 | Growing evidence: a graph over `(binder, slot)`. A local call adds edges from the callee slot to the caller's `Given` and `Super` slots, marked if the path crosses `Impl` or `Specialize`. A marked edge on a cycle is `PolymorphicRecursion`. `Repr`, `Apply`, `StructuralEq`, and `ReflexiveLift` add no edges. | `solve.rs:1346-1423`, `:1442-1464` | none (THIH requires a signature; GHC lets the dictionary grow) | `inference.rs:1051`, `:1072`, `:1105`, `:1123`, `:5715`, `:338`; `crates/nash-codegen/src/build/tests.rs:308` | `docs/traits.md:489-509` agrees except its example (section 8) |

### 1.6 Budgets

All trait searches share the constants `WORK_LIMIT = 16_384` and
`DEPTH_LIMIT = 256` (`crates/nash-ast/src/head.rs:13-14`). They share the
numbers, not the unit or the cost model.

| # | Unit counted | Nash | Exhaustion | Tests |
|---|---|---|---|---|
| B1 | Inference depth: length of the `Sub` chain, error at `>= 256` | `solve.rs:2030`, `:2323`, `:2381` | `ImplResolutionLimit` at the use | `inference.rs:1386` |
| B2 | Inference work: one counter per originating wanted (`predicates.root(id)`), never reset. Not per definition boundary. | `solve.rs:178`, `:2316-2331`; `preds.rs:262-267` | same | `solve.rs:4374` |
| B3 | A fresh budget per `select`, `given_candidates`, and `improvement` call. It pays for matching every impl of the trait. | `resolve.rs:358-360`, `:376-384`, `:668` | `ImplResolutionLimit` | `head.rs:868-930` |
| B4 | A fresh budget per `cover`, shared by subjects, cells, and nested proofs | `solve.rs:2227`, `:2485-2487`, `:2567-2570` | `CoverageProblem::Limit` | `inference.rs:5665` |
| B5 | Superclass entailment: one budget per impl, shared by the given closure and all requirements | `entailment.rs:93`, `:212-229`, `:352-356`, `:417-421` | `MissingSuperclass` (reason `Limit`) | `impls.rs:446` |
| B6 | Overlap: one budget per inserted impl across all earlier impls of the trait | `impls.rs:115-130` | `ImplPatternLimit` | `impls.rs:51-66` |
| B7 | Ground resolve: one budget per `resolve` call, charged for substitution | `evidence.rs:84-142` | `Failure::Limit` | `crates/nash-solve/tests/evidence.rs:512` |
| B8 | Codegen grounding: one budget per `ground` call, charged for the whole type argument at each node. Depth check is `> 256`, not `>= 256`. | `crates/nash-codegen/src/evidence.rs:56-129`, `:262`, `:279-282`, `:316-331` | "evidence normalization exceeded its work or depth limit", no region | `crates/nash-codegen/src/evidence/tests.rs:297` |
| B9 | Codegen specialization: 1,024 bodies per build, type depth `> 256` | `crates/nash-codegen/src/build.rs:26-27`, `:655-660`, `:844` | "specialization exceeded the 1024-body or 256-level type limit", no region | `crates/nash-codegen/src/build/tests.rs:693` |

Nash has no syntactic termination condition for impls (no Paterson or coverage
condition). This is deliberate (`plans/02-kind-rewrite-audit.md:94-95`). Every
search has a budget, and an exhausted budget never counts as a proof (checked at
every site above).

### 1.7 Tests blocks

| # | Rule | Nash | Counterpart | Tests | Doc |
|---|---|---|---|---|---|
| T1 | Name environment: the tests environment is the module environment plus default test imports plus the block's imports, with every local declaration re-inserted on top. Test imports never enter the body environment. | `crates/nash-can/src/module.rs:123-158`, `crates/nash-can/src/defaults.rs:106-124` | Elm `Canonicalize/Environment/Local.hs:47-52` (local over foreign) | values only: `crates/nash-can/tests/testing.rs:60 test_imports_are_scoped`. Types, constructors, traits, operators, qualified names, shadowing, collisions: untested. | `docs/testing.md:69-71` agrees for names |
| T2 | Field visibility: `Constraint::Record` and `Constraint::Field` carry a syntactic `tests` flag. The solver uses `test_fields` when it is set. | `crates/nash-constrain/src/expression.rs:26-47`, `crates/nash-constrain/src/type_.rs:68-85`, `crates/nash-solve/src/solve.rs:1203-1207`, `crates/nash-can/src/environment.rs:57-107` | none | `crates/nash-solve/tests/testing.rs:190-210`. Case, lambda, and typed-local patterns: untested (probes `tb_41`, `tb_42` pass). | changeset `field-constraint-test-scope.md` agrees |
| T3 | Impls and traits are global: the table is every build interface plus the module, regardless of imports. | `impls.rs:29-103`, `crates/nash-driver/src/compile.rs:248-298` | H98 §5.4 (instances are always exported) | untested for tests blocks | `docs/traits.md:240-242` agrees. `docs/testing.md:69` overstates the scoping. |
| T4 | The interface never reads `tests`. | `crates/nash-can/src/interface.rs:103-139`, build strip `compile.rs:423-425` | none | `crates/nash-solve/tests/testing.rs:66` | `docs/testing.md:536` agrees |
| T5 | Power-assert `Show`: for each captured operand, substitute the specialization into its solved type and call `evidence::resolve(Show τ)` with no givens. Anything other than `Ok(Some(_))` prints `?`. | `assertion.rs:34-84` (fallback `:55-59`), `build.rs:513-541` | none | `crates/nash-codegen/tests/integration.rs:206`, `:241`, `:289`, `:344`. No test compares it with in-solver resolution. | `docs/testing.md:229-232` says `?` means only a missing impl (section 8) |

## 2. Properties

| Property | Holds? | Evidence | Code |
|---|---|---|---|
| Coherence: one use site always gets the same evidence | **Fails for transparent aliases.** Holds otherwise. | `ent_05_alias_eq_user`: `a == b` is True and `b == a` is False at identical types. `tb_23_alias_unify_order`: `show` gives `"SHOW-IDS"` or `"[1]"` depending on which `if` arm comes first. `ent_02_alias_order` (legal impls on `map` and `list`). Held in `ent_01`, `ent_11`, `ent_12`, `ent_13`, `ent_18`, `ent_20`, `ent_22`, `ent_23`, `ent_26`, `ent_27` (section 6). | `crates/nash-solve/src/unify.rs:441-568`, `resolve.rs:72-81` |
| Coherence: inference selection equals codegen re-selection | Holds for `Specialize` and `Super` at ground types. Fails on limits (H3). | `ent_27` (7 wrapper shapes), `ent_18` (identical evidence, different cells, 10 vs 20). `term_21_deep_build_200`, `ent_06_depth_*`. | `build.rs:629-654`, `crates/nash-codegen/src/demand.rs:157-213` |
| Coherence: no user impl replaces structural Big Eq | Holds on every route tried | `ent_04a` to `ent_04q`: Big ADT, alias, partial alias, record alias, nested ADT, blanket, tuple, list of Big, lowercase alias of Big | `impls.rs:250-284` |
| Coherence: the three selectors agree | Holds, apart from designed differences and budget units | entailment has no coverage (`ent_07a`); only entailment reports `Cycle` (`term_14`); budgets differ (B1 to B8) | `head.rs:268-309` |
| Principality, single-parameter | Holds. Inferred contexts are not in head-normal form. | `gen_02_flexctx`: `g : Foo (list 'a) => 'a -> int`. `ghc_02`: `boxIt : Observe (Box 'a) => 'a -> unit` where GHC infers `Observe a`. Equivalent under a fixed global impl table. | `solve.rs:3506-3642`, `head.rs:252-309` |
| Principality, multi-parameter | Partial | `gen_40_relational`: `g5 : Lift (map 'k 'v1) (Map 'k1 'v) => Map 'k1 'v -> list 'k` keeps an unimproved hidden variable when the candidate is not ready. | `resolve.rs:656-728` |
| Order independence and let-abstraction | **Fails** (H2) | `gen_41_captured_mptc`, `gen_43_order_dependence`, `v_02_mptc_binding_forms` | `solve.rs:3356-3360` |
| Termination | Holds by budget. No run diverged or overflowed the stack; a 20,000-level type stops at depth 256. | `term_80_deep_stress_20000`; table in section 2.1 | B1 to B9 |
| Ambiguity and literal defaulting vs H98 §4.3.4 | Partial (M1) | Agrees on `show 1` and on two literal classes. Differs on user classes (`gen_52`) and on unchecked candidates (`gen_55`). | `solve.rs:3398-3439` |
| Early defaulting at inner lets gives the Haskell outcome | Holds for single-parameter literal classes. Breaks only through H2. | `gen_56_literal_schemes`, `gen_50_defaulting` | `solve.rs:567-582` |
| Let-sharing after specialization (no MR) | **Holds.** Measured in section 2.2. One strictness difference: an unused polymorphic local with evidence is never evaluated. | `gen_30`, `gen_31`, `gen_32`, `gen_33` | `build.rs:503-512` |
| Polymorphic recursion with growing evidence rejected | Holds for dictionary evidence. Growth that changes only representation or layout passes `check` and fails in codegen. | rejected: `gen_10`, `gen_11`, `gen_12`, `gen_13`, `gen_16`, `gen_20`, `gen_21`, `term_62`. Accepted correctly: `gen_14`, `gen_18`, `gen_19`. Escapes to codegen: `gen_17`, `term_60`, `term_61`. | `solve.rs:1346-1423` |
| Kinds vs H98 §4.6 | Holds. Exceptions are by design: alias bodies (L2) and `IrregularRecursion` (L1). | `kind_01` to `kind_07`, `kind_17`, `kind_20`. A differential fuzz of 762 random datatype groups against GHC `-XHaskell98` found no disagreement (`fuzz/fuzz.py`). GHC oracle: 61 of 62 kind programs agree. | `kinds.rs:1525-1584`, `:1835-1947` |
| Datatype-context closure terminates | Holds. Not polynomially bounded (M7). | `kind_05_blowup_n8_k7`: 35.7 s, 4.4 GB, accepted | `kinds.rs:991-1049` |
| Tests blocks: test imports do not leak into the body | Holds for names | `tb_01` to `tb_07` reject; `tb_08` accepts in tests; collisions error (`tb_10`, `tb_12`, `tb_14`) | `module.rs:123-159` |
| Tests blocks: test-only impls cannot change body selection | Holds only through the orphan rule. Impls are global. | `tb_33b`: a module that never imports `Aux` still uses `Aux`'s `Show box`. `tb_40`: the overlapping impl a test module could add is an orphan. | `impls.rs:29-103`, `:195-216` |
| Power-assert `Show` agrees with in-solver `Show` | Partial. Agrees for user impls, base types, Big `Int`, defaulted literals, and phantoms an impl matches. Differs in three cases. | open `Show 'a`: assert prints `?`, the solver rejects (`tb_20`, `tb_21`); polymorphic helper: assert prints values, the solver demands `Show 'a` (`tb_24`, `tb_25`); limit: assert prints `?`, explicit `show` fails codegen (`tb_31_*`) | `assertion.rs:55-59`, `evidence.rs:245-377` |

### 2.1 Termination: Nash budgets against the Paterson conditions

"Paterson" means GHC's instance termination rules (GHC 9.10.3 User's Guide,
"Instance termination rules"). "GHC +U" is GHC with `UndecidableInstances`.
Each probe directory holds `ghc.out` and, where relevant, `ghc_paterson.out`.

| Probe | Paterson | GHC | Nash | Note |
|---|---|---|---|---|
| `term_01_expanding_ctx` `C (box (box 'a)) => C (box 'a)` | reject | reject, +U stack overflow | reject at the use | both stop by a limit |
| `term_10_paterson_reject_terminates` | reject | +U accept | accept | Nash accepts a terminating non-Paterson set |
| `term_11_blanket_nonterminating` `C (box 'a) => C 'a` | reject | reject | reject at the use | the region is the use, not the impl |
| `term_12_self_context`, `term_13_mutual_blanket_cycle` | reject | +U accept (recursive dictionary) | reject (limit) | no coinductive resolution (L10) |
| `term_14_super_cycle`, `term_14b_super_self_cycle` | reject | +U accept | reject, reason `Cycle` | the only place Nash reports a cycle |
| `term_16_declare_only_expanding` (never used) | reject | +U accept | **accept** | non-terminating impls are admitted (M4) |
| `term_17_recursive_dictionary` `(C 'a, C (List (Rose 'a))) => C (Rose 'a)` | reject | +U accept | reject (limit) | productive cycle (L10) |
| `term_20_deep_check_{200..255}` | accept | reject (reduction depth 200) | accept | GHC's default limit is lower |
| `term_20_deep_check_{256,300}` | accept | accept with `-freduction-depth=0` | reject (limit) | boundary is exact: 255 accepted, 256 rejected |
| `term_21_deep_build_{178..255}` | accept | n/a | `check` accepts, `build` rejects | H3 |
| `term_30_wide_check_{14,20,100}` (two traits, `(C 'a, D 'a) => C (wrap 'a)`) | accept | accept | reject (limit) | exponential tree charged per node; GHC shares dictionaries |
| `term_31_wide_build_12` | accept | n/a | `check` accepts, `build` rejects | H3 |
| `term_40_manyctx_125` (125 context predicates plus a superclass) | accept | accept | reject, "requirements keep expanding" | quadratic entailment cost (M4) |
| `term_41_super_deep_43` `impl S (wrap^43 int)` | accept | accept | reject | cubic entailment cost (M4); the same type is accepted at a use up to depth 255 |
| `term_50_sameDef_uses_{1,3}` | accept | n/a | accept | the budget is per root wanted, not per definition |
| `term_51_manyfn_1022` (1,022 reachable monomorphic functions) | n/a | n/a | `check` accepts, `build` rejects | B9 |
| `term_52_manyimpl_16386` | accept | accept | reject the 16,386th impl, "impl pattern is too large" | B6 |
| `term_52_manyimpl_16385_use` | accept | n/a | impls accepted, every use rejected | B3 |
| `term_60`, `term_61` (type-growing recursion) | n/a | accept | `check` accepts, `build` rejects | G11 does not see layout growth |

### 2.2 Let-sharing on the CEK machine

Workload `count 1000`, a literal-polymorphic sum
`(Eq 'a, FromInt 'a, Num 'a) => 'a -> 'a`, in probe `gen_30_let_sharing`.
Numbers are the `[mem, cpu]` that `nash test` reports.

| Variant | -O 0 | -O 1 | -O 2 |
|---|---|---|---|
| one use | 2,614.9K / 672,523.0K | 3,811.6K / 863,995.0K | 3,811.2K / 863,931.0K |
| polymorphic local `big`, `big + big` | 2,615.6K / 672,736.2K | 3,812.3K / 864,208.2K | 3,811.9K / 864,144.2K |
| monomorphic local, lambda-shared, top-level polymorphic, top-level monomorphic | identical to the polymorphic local | identical | identical |

The second use costs 0.2K cpu, so codegen evaluates the local once. With
runtime `Given` evidence (`gen_31`), the polymorphic and monomorphic versions
cost the same at `int` and at `Int`. A top-level polymorphic constant used at
two evidence vectors costs twice (`gen_33`: 8,385.3K / 1,875,664.4K at -O 1),
which matches `docs/traits.md:479-481`. The one semantic difference is
strictness: `let big = inc (fail "boom") in k` passes when `big` is polymorphic
and unused, and fails when it is annotated monomorphic (`gen_32`). That is
documented in `docs/codegen.md:837-844`, not in `docs/traits.md`.

The -O 0 baseline without `count` (1,028.4K cpu) costs more than the one-use
test (672.5K), so compare only within a column.

## 3. Elm divergence ledger

Every row has a probe in `probes/elm_<nn>_<name>/` with `check.out` (Nash) and
`elm.out` (Elm, human and `--report=json`). Classes: **match**, **ID**
(intentional and documented), **IU** (intentional, undocumented), **acc**
(accidental). Elm cites are under `elm/compiler/src/`.

| # | Construct | Elm | Nash | Class | Elm result | Nash result | Explanation |
|---|---|---|---|---|---|---|---|
| L01 | let polymorphism | `Type/Solve.hs:157-194` | `solve.rs:486-643` | match | accept | accept | same generalization |
| L02 | lambda-bound variable is monomorphic | `Solve.hs:157-194` | `solve.rs:486-534` | match | reject at `g "s"` | reject at the annotation | literals are `FromInt 'a`, so the clash surfaces later (L15) |
| L03 | let value generalization (`xs = []`) | `Solve.hs:157-194` | `solve.rs:486-643` | match | accept | accept | no value restriction in either |
| L04 | captured outer variable is not generalized | `Solve.hs:332-349` | `solve.rs:3881-3913` | match | accept, reject | accept, reject | `elm_04`, `elm_04c` |
| L04b | same with overloaded literals | none | none | ID | rejects the 4-tuple | accept | `x` gets a `FromInt` plus `FromString` context [INFERENCE: scheme not printed] |
| L05 | annotation too general | `Reporting/Error/Type.hs:521` | `unify.rs:307-319` | match | mismatch plus hint | `expected 'b, found 'a` plus hint | shorter text |
| L06 | rigid against structure | `Unify.hs:246-277` | `unify.rs:307-319` | ID | `String` vs `a` | `missing_constraint FromString 'a` | string literal is overloaded |
| L07 | annotation variable escapes its scope | `Solve.hs:188`, `:197-210` | `solve.rs:536-552` | IU | **compiler crash** `a [rank = 2]` at `Solve.hs:206:15` | `annotation_variable_escapes` | Nash turns the crash into a diagnostic; its help text assumes an enclosing annotation |
| L08 | nested annotation reuses an outer name | `Type/Constrain/Expression.hs:555-557` | `expression.rs:1241-1265` | IU (inherited) | accept, accept | accept, accept | implicit scoped type variables (M5) |
| L09 | occurs check in a let | `Solve.hs:255-265` | `solve.rs:2806-2829` | match | `a -> ∞` | `'a -> ∞` | identical rendering |
| L10 | occurs check on a lambda variable | `Solve.hs:155`, `:194` | `solve.rs:483`, `:640` | match | `∞ -> a` | `∞ -> 'a` | |
| L11 | occurs check through a tuple | `Type/Occurs.hs:64-73` | `crates/nash-solve/src/occurs.rs:46-53` | match | `a -> ( a, ∞ )` | `'a -> ( 'a, ∞ )` | |
| L12 | untyped mutual recursion is monomorphic in the group | `Expression.hs:606-673` | `expression.rs:1287-1459` | match | 5 errors with duplicates | 1 error | Nash poisons dependents |
| L13 | untyped polymorphic recursion | same | same | match | infinite type | infinite type | |
| L13b | annotated polymorphic recursion | `Expression.hs:665-673`, `Solve.hs:590-592` | `solve.rs:608-617`, `:3703` | ID (`docs/traits.md:487`) | reject | accept | quantifier ownership; "follow Elm" in the doc is wrong (section 8) |
| L13c | annotated mutual recursion with growth | same | same | match | accept | accept | Elm accepts because the callee's `CLet` is not active |
| L14 | two-member untyped group captures an outer variable | `Expression.hs:622-623`, `Solve.hs:412-417` | `expression.rs:1321-1324` | IU (Nash fixes an Elm unsoundness) | **accept** (`z` used at `Int` and `String`) | reject, `expected int, found string` | `elm_14e_group_regen_two`; also `elm_14`, `14b`, `14d`, `14g`, `14i`; single-member control `14f` rejected by both. Untested in Nash (L13). |
| L14h | same with a typed member | `Expression.hs:665-673` | `solve.rs:536-552` | IU | compiler crash `k [rank = 2]` | `annotation_variable_escapes` | as L07 |
| L15 | destructuring let | `Expression.hs:510-521` | `expression.rs:954-1011` | match | accept | accept | |
| L16 | foreign polymorphic value | `Solve.hs:113-126`, `:501-562` | `solve.rs:351-371`, `:2903-2986` | match | accept | accept | |
| L17 | top-level inferred polymorphism | `Solve.hs:80-81` | `solve.rs:316-319` | match | accept | accept | |
| L18 | record alias identity | `Unify.hs:552-558` | `unify.rs:451`, `:489-496` | ID (`docs/syntax.md:343`) | accept | `expected Q, found P` | nominal records |
| L18b | record literal with two same-field aliases | `Unify.hs:613-653` | canonicalization | ID | accept | `record_literal_ambiguous` | |
| L19 | field access on an unknown receiver | `Expression.hs:113-136` | `solve.rs:1085-1260` | ID (`docs/overview.md:74`) | accept | `ambiguous_record_access` | no rows |
| L19b | the same accessor applied to known aliases | same | same | ID | accept | `ambiguous_record_access` | field access is not retried after generalization |
| L20 | missing field | `Reporting/Error/Type.hs` | `crates/nash-report/src/type_/records.rs` | match | "maybe z should be x?" | "Did you mean x?" | |
| L21 | record update with a missing field | same | same | match | as L20 | as L20 | |
| L22 | field access resolved after the call fixes the receiver | none | `solve.rs:272-312`, `:1044-1060` | IU | accept | accept | |
| L22b | same under let generalization | none | `solve.rs:1262-1300` | ID (`inference.rs:4195`) | accept | `ambiguous_record_access` | |
| L23 | dependent definitions after an annotated error | `Solve.hs:246-249` | `solve.rs:553-607` | match | 2 errors | 2 errors | |
| L24 | inferred context that no Base type satisfies | supertypes | `solve.rs:3506-3646` | ID | one error at `bad = 1 + "s"` | no error at `bad`; `FromString int` at each use | Elm has no open classes; in Nash a user type can satisfy the context (`v_04`, M8) |
| L25 | two independent errors | `Solve.hs:246-249` | `solve.rs:240-257` | match | 2 | 2 | |
| L26 | annotation arity | canonicalization | nash-can | match | BAD TYPE ANNOTATION | `annotation_too_short` | |
| L27 | `if` branch mismatch | `Reporting/Error/Type.hs:647` | `crates/nash-report/src/type_.rs:299-399` | ID in part | prose plus `String.toInt` hint | one line plus labels | `docs/diagnostics.md:11-14` claims near-verbatim prose (section 8) |
| L28 | `case` branch mismatch | same | same | same | same | same | |
| L29 | list element mismatch | same | same | same | prose plus two hints | labels "previous list element", "list element 3" | |
| L30 | call argument mismatch | same | same | same | "left to right" hint | "argument 2 of `add2`" | |
| L31 | too many arguments | same | solver error | match | "expects 2 arguments, but it got 3" | "expects 2 arguments, but received 3" | |
| L32 | missing `Just` | same | `type_.rs:430-480` | match | `Maybe Int` vs `Int` | `option int` vs `int` | |
| L33 | alias to a variable, rigid first | `Unify.hs:246-277` | `unify.rs:307-319` | acc (inherited) | reject | reject | `f : 'b -> Id 'b; f x = x` is valid and rejected by both (L12) |
| L33b | same, rigid second | `Unify.hs:453-454` | `unify.rs:466-467` | match | accept | accept | the asymmetry |
| L34 | transparent structural alias | `Unify.hs:459-475` | `unify.rs:469-497` | match | accept | accept | |
| L35 | tuple arity above 3 | canonicalization | `Tuple1` | IU | reject | accept | n-ary tuples |
| L36 | pattern mismatch in `case` | `Reporting/Error/Type.hs:184-269` | `type_.rs:196-235` | match | prose | one line | |
| L37 | literal pattern against its type | same | same | ID | pattern mismatch | `missing_impl FromString int` | |
| L38 | argument pattern, then a body mismatch | same | same | match | body mismatch | body mismatch | |
| L39 | more than 8 nested generalizing lets | `Solve.hs:161-166` | `solve.rs:487-493` | match | accept | accept | 14 levels; also `tm_02_deep_let` at 24 |
| L40 | `number` annotation | `Solve.hs:504-509` | none | ID (`docs/overview.md:72`) | accept, mismatch | accept, `missing_impl Num string` | |
| L41 | `comparable` | `Unify.hs:283-418` | none | ID | mismatch | `missing_impl Ord ('a -> 'a)` | |
| L42 | `appendable` inferred | `Unify.hs:317-322` | none | ID | accept | accept, `Semigroup` context | |
| L43 | cascade through an inferred failure | `Unify.hs:201-204`, `:28-37` | `unify.rs:50-60` | match | 1 error | 1 error | |
| L44 | cascade through a let in the body | same | same | match | 1 | 1 | |
| L45 | annotated body error keeps callers | `Solve.hs:157-194` | `solve.rs:553-557` | match (order differs) | 2 errors, `h` first | 2 errors, `f` first | `crates/nash-report/src/lib.rs:176-208` re-sorts by region (L14) |
| L46 | `n = 1` generalized | `Solve.hs:504-509` | none | ID (`docs/traits.md:479`) | accept | accept | no MR |
| L47 | typed and untyped members in one group | `Expression.hs:606-673` | `expression.rs:1287-1459` | match | accept | accept | |
| L48 | defaulting `show 1` | none | `solve.rs:3313-3471` | ID | n/a | accept | |
| L49 | inferred context on a local function | supertypes | `solve.rs:3506-3646` | ID | accept | accept | |
| L50 | alias through generalization | `Solve.hs:400-402` | `solve.rs:3957-3963` | match | accept | accept | |
| L51 | unit | `Type/Type.hs` `Unit1` | `App1 unit` | IU | accept | accept | `unit` is an ordinary little type |
| L52 | higher-kinded application | none | `unify.rs:574-618` | ID (`docs/kinds.md`) | n/a | accept | |
| L53 | ambiguity `x = [] == []` | none | `solve.rs:3313-3471` | ID | accept | `ambiguous_type`, `Eq (list 'a)`, `Storable 'a` | |
| L54 | kind error | none | `kind_check.rs` | ID | n/a | `kind_mismatch` | |
| L55 | inferred definition with no Base instance | none | `solve.rs:3506-3646` | ID | reject | accept | same as L24 |
| L57 | independent sibling after a failed definition | `Unify.hs:201-204` | `crates/nash-solve/src/recovery.rs` | match | only the first error | only the first error | why Elm suppresses it: not diagnosed |

Summary of the 64 rows: 32 match, 24 intentional and documented (L27 to L30
only in part), 7 intentional and undocumented, and 1 accidental (L33, inherited
from Elm). Elm is not a valid oracle in L07, L14, and L14h, where it crashes or
accepts an unsound program.

## 4. Differential oracles

### 4.1 GHC on the trait and higher-kinded subset

The corpus is every insta snapshot whose program uses traits, impls, contexts,
higher-kinded variables, or kind errors: 221 snapshots, split into 236 Haskell
programs. 246 snapshots were skipped, each with a recorded reason
(representation-only 104, records 53, builtin primitives 21, plain HM 49,
`test` blocks 15, and smaller groups). Each program ran as
`ghc -fno-code -ddump-types -XHaskell2010` with the minimal extensions GHC asked
for, plus `NoMonomorphismRestriction`, `PolyKinds`, and, when GHC asked,
`UndecidableInstances` variants. Results: `oracle-ghc/_results.json`; method and
full table: `notes/oracle-ghc.md`.

| Result | Programs |
|---|---|
| both accept | 66 |
| both accept, GHC needs a flag (14 `NoMonomorphismRestriction`, 2 `UndecidableInstances`) | 16 |
| both reject | 115 |
| GHC rejects only for lack of `UndecidableInstances` | 1 |
| mismatch | 38 |

All 38 mismatches are intentional and documented. None is a Nash bug or a
translation artifact.

| Group | Count | Explanation |
|---|---|---|
| Eager global impl-table checks | 17 | Nash checks overlap, orphans, head shape, compiler-owned `Lift` and `Eq`, missing methods, and cyclic superclasses when the impl is declared (`docs/traits.md:130-140`, `:180-185`, `:218-240`, `:285-299`). GHC accepts the declarations and fails only at a use, or never. |
| Literal defaulting at every let, no MR | 8 | `docs/traits.md:458-481`. GHC does not default `FromInt`-style classes. |
| Enclosing givens win over impls; relational improvement without fundeps | 5 | `docs/traits.md:406-408`, `:418-442`. GHC rejects the context-only variable as ambiguous; a control with a declared fundep is accepted. |
| Growing-evidence polymorphic recursion | 3 | `docs/traits.md:489-509`. GHC builds dictionaries at run time. |
| Nominal aliases, partially applied aliases | 2 | `docs/traits.md:143-148`. GHC expands synonyms and requires saturation. |
| Compiler-owned structural Big Eq | 2 | `docs/traits.md:180-197`. |
| Local-shadowing ban | 1 | `docs/overview.md:27-28`. |

Inferred types: every accept/accept pair has the same scheme after dropping
representation predicates and `Apply`, except one. Nash keeps
`Observe (list a)` where GHC reduces it to `Observe a`
(`inferred_context_preserves_constructed_arguments_and_recursive_groups`).
Probe `ghc_02` shows the same retention for an uppercase user `Box`, so it is not
tied to the list representation split (L7). Kinds: 61 of 62 kind programs agree.
The one difference is Nash's partially applied aliases. With `-XPolyKinds`, GHC
would accept 4 kind errors that Nash rejects.

The audit slices added their own GHC comparisons: kinds (`kind_*/ghc.out`, plus
the 762-group fuzz), termination (section 2.1), generalization
(`gen_41`, `gen_43`, `gen_44`, `gen_50`), and entailment (`ent_05/ghc.out`:
GHC rejects `instance Tag (Same Int)` next to `instance Tag Int` as a duplicate
instance, because H98 §4.2.2 expands synonyms before instance lookup).

### 4.2 Elm on the shared subset

Of 284 `nash-solve` snapshots, 114 are in the Elm-shared subset; 4 three-tuple
reductions bring the hand-run set to 118. 170 snapshots were skipped (108 use
traits, 15 labeled constructor fields, 12 representation words, 10 higher kinds,
7 builtin `pair`, and smaller groups). Real-Base `nash check` gave the snapshot's
verdict and diagnostic codes for all 114.

| Verdict (Nash / Elm) | Rows |
|---|---|
| accept / accept | 40 |
| reject / reject | 51 |
| reject / accept | 17 |
| accept / reject | 10 |

Classes: 49 match, 61 intentional and documented (literal overloading 20,
nominal records 14, representation predicates 13, no supertypes 8, n-ary tuples
4, recovery policy 2), 1 intentional and undocumented (a pattern error anchored
at the inner `True` instead of the whole tuple pattern), 4 translation
artifacts, 3 Elm defects. No accidental Nash bug.

A type-directed generator and mutator (`oracle-elm/gen.py`,
`oracle-elm/fuzzrun.py`) produced 1,333 programs in the strict common subset
(no literals, records, or native lists). 898 were accepted by both, 426
rejected by both, and 9 accepted by Nash and rejected by Elm. All 9 are
annotated polymorphic recursion (L13b). For the 743 accepted programs whose
values `elm repl` could print, every top-level type matches. On the 426
both-reject programs, the error line matches in 423.

Elm defects found along the way, so Elm is not an oracle there:

- Elm accepts `coerce : a -> b; coerce x = second x` through an unsound mutually
  recursive top-level group (`elmo_01_mutual_group_unsound_elm`). Nash rejects it.
- Elm crashes (`Type/Solve.hs:206`) on a local annotation that captures an outer
  lambda parameter. Nash reports `annotation_variable_escapes`.
- `elm make` without `main` skips the value-cycle check. `nash check` does the
  same and `nash build` rejects (`elmo_04`, `elmo_05`).

One under-report is by design: Nash suppresses errors at uses of a failed
definition even when the error is certain (`elmo_02`), per `docs/diagnostics.md`.

## 5. Test adequacy

### 5.1 Mutation testing

cargo-mutants mutated the entailment, generalization, and kind-check code and
ran the `nash-ast`, `nash-solve`, `nash-can`, and `nash-driver` tests (933 tests).
Command and filters: `notes/test-map.md` section 3.1. `unify.rs`, `annotation.rs`,
`occurs.rs`, `recovery.rs`, and the record machinery were not mutated.

| File | Mutants | Caught | Missed | Timeout | Unviable | Missed of viable |
|---|---|---|---|---|---|---|
| `crates/nash-ast/src/head.rs` | 108 | 36 | 3 | 0 | 69 | 7% |
| `crates/nash-can/src/entailment.rs` | 46 | 27 | 12 | 0 | 7 | 30% |
| `crates/nash-can/src/impls.rs` | 65 | 44 | 3 | 1 | 17 | 6% |
| `crates/nash-solve/src/evidence.rs` | 82 | 31 | 41 | 0 | 10 | 56% |
| `crates/nash-solve/src/kind_check.rs` | 26 | 13 | 3 | 0 | 10 | 18% |
| `crates/nash-solve/src/preds.rs` | 84 | 47 | 25 | 0 | 12 | 34% |
| `crates/nash-solve/src/representation.rs` | 25 | 15 | 2 | 0 | 8 | 11% |
| `crates/nash-solve/src/resolve.rs` | 145 | 63 | 16 | 1 | 65 | 20% |
| `crates/nash-solve/src/solve.rs` (selected functions) | 264 | 161 | 57 | 6 | 40 | 25% |
| **total** | 845 | 437 | 162 | 8 | 238 | 27% |

After a mutant times out, cargo-mutants leaves its temporary tree broken and
reports every later mutant as unviable. Runs `m1_solve` and `m1b_solve` are
therefore discarded; `m1c_solve` excludes the two hanging mutants
(`solve.rs:489:34`, `solve.rs:571:32`). The 162 survivors were not triaged for
equivalence.

Fully pinned (0 missed): `head.rs` `matches`, `select_impl`, `overlaps`,
`can_equal`, `unifiable`; `impls.rs` `canonicalize`, `insert_impl`; `solve.rs`
`generalize`, `retain_wanted`, `growing_evidence`,
`contradictory_representations`, `instantiate_binding`, `enter_givens`,
`solve_from_givens`, `given_big`. `check_ambiguity` has one survivor
(`solve.rs:3349`).

Survivors by rule, with the representative mutations (full list:
`mutants/*/mutants.out/missed.txt`):

| Rule | Missed | Representative survivors |
|---|---|---|
| E5 structural Big Eq, E4 reflexive Lift (rule guards) | 17 | `entailment.rs:323`, `:338`, `:339` `&&` to `\|\|`; `solve.rs:2622-2635`; `solve.rs:3562-3570` |
| B1 to B8 budgets | 54 | `entailment.rs:96`; `evidence.rs:88-140` (`substitution_work`: 20 of 23 missed); `solve.rs:2323`; `preds.rs:254-256` |
| E3 coverage `cover` and `prove` | 5 | `head.rs:370`; `resolve.rs:311`, `:320`; limit propagation `solve.rs:2544`. Counted by function, `prove` misses 18 of its 21 viable mutants (Eq, Lift, and `Apply` inside a cell); most of those are counted in the E4, E5, and budget rows. |
| G7 and E1 predicate identity and context reduction | 32 (22 of 45 in `same_args`) | function, tuple arity, alias, partial alias, applied-variable arms (`preds.rs:303`, `:341`); `reduce_context` misses 10 of 23 |
| E7 ground resolve | 10 | failure paths for representation traits and open variables (`head.rs:144`, `evidence.rs:150`). Counted by file, `evidence.rs` misses 41 of 72; most are in the budget row. |
| E6 relational improvement | 6 | `resolve.rs:485` (`Probe::occurs` to `Ok(false)`), `:563-576`, `:620`, `:701`, `:705` |
| G6 classification | 15 | `has_outer_flex` arms `resolve.rs:22-37`; `solve.rs:469`, `:491`, `:556`, `:602`, `:612` |
| D4 formation | 6 | `representation.rs:34`, `:44`; `formed_at` `solve.rs:2746-2777` |
| K7 solver kind check | 4 | `kind_check.rs:228` `Check::predicate` replaced by `Ok(())`; `kind_check.rs:74` |
| G10 evidence | 4 | `solve.rs:1584`, `:1590` (delete match arms in `finish`) |
| E2 by_instance | 2 | `head.rs:165`, `resolve.rs:173` |
| I1 head shape | 3 | `impls.rs:381` (duplicates the guard at `impls.rs:458`) |
| G8 ambiguity | 2 | `solve.rs:3349`; plus 3 timeouts, one of which (`solve.rs:571`, delete `!`) makes the defaulting retry loop infinite |
| error recovery (not a rule above) | 2 | `solve.rs:1479`, `:1512` (`predicate_stranded`) |

Baseline probes that a future test could pin: `tm_03_super_nonEq_big` and
`tm_04_lift_two_arg_super` (rejected correctly at baseline; the `&&` survivors
would accept them [INFERENCE: mutant not built]), `tm_05_wide_expansion_super`
and `tm_06_wide_expansion_use` (stop with a limit error at baseline).

### 5.2 Rules or cases with no test

By test name, every rule in section 1 has at least 3 tests. The test-to-rule map
(`notes/test-rule-table.md`, 408 of 898 tests mapped) records intent, not
coverage. These rules or cases have no test at all:

- Alias-order selection (H1) and the orphan rule for local aliases of foreign types.
- A captured determining argument in the ambiguity check (H2).
- Agreement between `check` and `build` budgets (H3), and the exact depth boundary (255 accepted, 256 rejected).
- Literal defaulting with a user class or a representation requirement (M1).
- `('a : C)` inside a context predicate (M2).
- Partially applied alias unification (M3).
- Scoped annotation type variables as a rule (M5).
- The recursive-group difference from Elm (`expression.rs:1321-1324`, L13).
- `crates/nash-can/src/scc.rs` (no unit tests).
- Class-constrained destructuring aggregates (G4).
- Tests blocks: types, constructors, traits, operators, qualified imports, shadowing, and collisions (T1); case, lambda, and typed-local field patterns (T2); test-only impls (T3).
- Power-assert `Show` against in-solver `Show` (T5).
- Codegen "method B" for `Specialize` has no codegen unit test, only the driver fixture `Coverage.nash`.
- No property-based or fuzz testing exists in these crates. Most solver tests are insta snapshots.

## 6. Execution checks

Each probe typechecks, runs on the CEK machine with `nash test`, and asserts
values that only one impl can produce. Each has a control that must fail, so a
pass is not vacuous. Outputs are in `<probe>/test.out`.

| Probe | What tells the impls apart | Result |
|---|---|---|
| `ent_01_cover_list` | `Tag (list Big)` returns 10, the little-list impl returns 20 plus a head tag; generic `Tag 'a => list 'a -> int` called at Big and little | entailment and codegen agree; 3 PASS plus control |
| `ent_03_map_eq` | Base `Eq` through generic `eqMap` and `eqList` at little/little and Big/Big | agree; Big/Big map 5,577.0K cpu vs list 8,050.1K |
| `ent_11_cover_map_nested` | three disjoint `map` impls return 1, 2, 3; nested lists; pair list | B/B 1, B/L 3, L/B 2, L/L 2, nested Big 30, nested little 140, pair list 20 |
| `ent_12_given_vs_cover` | a direct given `Tag (list 'a)` against coverage | 10 and 20 in both shapes |
| `ent_13_super_exec` | `P` only through `Super(Given Q 'a)` | 1, 2, 100 as selected |
| `ent_16_relational_doc_examples` | `Map.keys` through `Lift` improvement | 3 PASS |
| `ent_18_same_evidence_different_cells` | identical executable evidence at `Int` and `int`, different cells | 10, 20, 30; the specialization key keeps them apart |
| `ent_20_local_captured_specialize` | local helper capturing a rigid `'a` | 4 PASS |
| `ent_22_super_structural_eq`, `ent_26_structural_neq_super` | `StructuralEq` reached through `Super`; default `neq` | PASS |
| `ent_27_coverage_demand_propagation` | 7 wrapper shapes (chain, local helper, impl method, default method, higher-order, recursion, `Super`) at Big and Const with identical evidence | 14 PASS |
| `gen_60_context_reduction`, `gen_61_slots_nested` | reduced contexts, annotated contexts, nested helpers, mixed recursive group | 13 PASS |
| `ent_02_alias_order`, `ent_05_alias_eq_user`, `ent_23_cover_alias_order`, `ent_25_orphan_via_alias`, `ent_29_base_eq_alias_order_cost`, `tb_23_alias_unify_order` | alias against body | codegen runs the impl that entailment selected, but entailment selects by unification order (H1) |

In every check, codegen ran the impl that entailment selected. The only
divergence the checks expose is upstream of codegen: H1.

## 7. Findings ranked by risk to Plan 11

Each finding has a probe that reproduces it at the baseline. "Block" answers
whether the finding should block Plan 11, and which chunk.

### H1. Impl selection for transparent aliases depends on unification order

Unification treats a non-record alias as its body. Impl selection treats it as
a nominal head. When an alias-typed variable meets a structure-typed variable,
the content that survives is whichever side unification reached first
(`crates/nash-solve/src/unify.rs:441-518`, `:542-568`, the same arm as Elm's
`Type/Unify.hs:444-478`). `resolve.rs:72-81` then reads the alias name if it
survived. The result is a coherence failure: the same expression at the same
types selects a different impl when two operands swap.

Repro `ent_05_alias_eq_user`, with no Base trickery:

```elm
type alias same 'a = 'a

impl Eq (same 'a) where
    eq _ _ = True

mixedAliasFirst : same int -> int -> bool
mixedAliasFirst a b = a == b

mixedPlainFirst : same int -> int -> bool
mixedPlainFirst a b = b == a
```

```text
$ nash test  ent_05_alias_eq_user
  PASS alias first -> alias impl     [mem:     0.2K, cpu:    16.1K]
  PASS plain first -> int impl       [mem:     3.1K, cpu:   532.4K]
```

The tests assert `mixedAliasFirst one two` is True and
`mixedPlainFirst one two` is False. Swapping the operands of `==` changes the
result.

Repro `tb_23_alias_unify_order`, with `type alias ids = list int` and
`impl Show ids`:

```text
  × assert (pick1 True == pick2 True)
            │             │
            │             ?
            │             "[1]"
            ?
            "SHOW-IDS"
```

`pick1 c = show (if c then ids1 else plain)` and `pick2` swaps the arms.

Related evidence:

- `ent_02_alias_order`: two legal impls in the trait's own module,
  `impl Tag (list 'a)` and `impl Tag (map 'k 'v)`. `pick m l` returns 2 and
  `pick l m` returns 1 at identical types. The same split holds in `if` arms and
  list literals.
- `ent_23_cover_alias_order`: the same split through `Specialize`.
- `ent_29_base_eq_alias_order_cost`: Base impls only. `m == l` costs 16.1K cpu
  (one `equalsData`) and `l == m` costs 3,845.2K (elementwise list Eq). The
  results agree, and `v_01_base_show_alias_order` shows Base `Show` renders both
  the same way, so Base alone gives a cost difference, not a value difference.
- `ent_25_orphan_via_alias`: `type alias foos = list Foo; impl Eq foos` is
  accepted in a module that owns neither `Eq` nor `list`, because the orphan
  rule counts a local alias as a local constructor (`impls.rs:195-216`). With H1,
  that module changes the meaning of `xs == ys` by operand order.
- `impl Eq (same 'a)` is accepted in a module that owns neither `Eq` nor any
  head constructor except the alias. It acts as a second `Eq` impl for every
  Little type, chosen whenever the alias spelling survives unification.

GHC rejects `instance Tag (Same Int)` next to `instance Tag Int` as a duplicate
(`ent_05_alias_eq_user/ghc.out`), because H98 §4.2.2 expands synonyms before
instance lookup. Elm has the same unification arm, but alias identity there only
affects error messages. `docs/traits.md:143-156` documents nominal alias
matching but not the order dependence. No test covers it.

Record aliases are not affected: they are nominal in unification too
(`unify.rs:451`, `:466`, `:513`, `:563-566`), so no second spelling of the type
exists.

**Block:** yes for chunk 9, which lets macros emit `impl` declarations, until
the alias-impl rule is decided (open question 1). Not for chunk 11:
`plans/11-macros-comptime.md:321-323` limits Derive to unions and record
aliases and rejects other aliases.

### H2. A let around a multi-parameter call with a captured argument is reported ambiguous

`check_ambiguity` lets a multi-parameter predicate extend reachability only
through an argument whose variables are all generalized
(`solve.rs:3356-3360`, `rank == NO_RANK`). A captured outer variable has an
outer rank, so the hidden variable it would determine is reported as ambiguous
at the inner `let`, before the outer body can fix it.

Repro `v_02_mptc_binding_forms`:

```elm
viaLambda m = (\ks -> ks) (Map.keys m)

viaCase m =
    case Map.keys m of
        ks -> ks

viaLet m =
    let
        ks = Map.keys m
    in
    ks
```

```text
$ nash check  v_02_mptc_binding_forms
  × Cannot determine type `'v` in `ks`.
    ╭─[src/Main.nash:14:9]
exit=1
```

Only `viaLet` fails. `gen_43_order_dependence` shows sibling order matters:
`( let ks = Map.keys m in ks, asMap m )` is rejected, and
`( asMap m, let ks = Map.keys m in ks )` is accepted, because in the second the
improvement already fired. GHC with a functional dependency standing in for
Nash's improvement accepts all of these under default flags,
`-XNoMonomorphismRestriction`, and `-XMonoLocalBinds`
(`gen_41_captured_mptc/ghc/ghc.out`, `gen_43_order_dependence/ghc/ghc.out`).
The rule matches `docs/traits.md:446-450`; the doc does not state the
consequence. No test covers a captured determining argument.

**Block:** yes for any chunk whose expansion let-binds caller syntax (the
guide macros of chunks 5, 7, and 8 are candidates), unless the expander binds
with `case` or a lambda, which this probe shows are accepted. Not for chunks
1 to 4.

### H3. `nash check` accepts programs that code generation rejects, with no region

All searches share `WORK_LIMIT` and `DEPTH_LIMIT`, but codegen charges the whole
type argument at every evidence node (B8), roughly n²/2 for a depth-n chain,
while inference charges about n (B2). Codegen also caps a build at 1,024
specialization bodies (B9), and the growing-evidence check (G11) does not see
growth that changes only layout.

Repro `term_21_deep_build_200`: `impl C 'a => C (wrap 'a)` and one use at 200
nested `Wrap`s.

```text
$ nash check  term_21_deep_build_200
Success! Compiled 37 modules (268 declarations)
exit=0
$ nash build  term_21_deep_build_200
Success! Compiled 37 modules (268 declarations)
Error:   × code generation failed in Main: evidence normalization exceeded its work or depth limit
exit=1
```

Thresholds: `build` fails from depth 178; `check` accepts up to 255. Other
cases: width 12 in `term_31_wide_build_12`; about 100 levels of
`Tag 'a => Tag (list 'a)` in `ent_06_depth_viaGiven_k103` and
`ent_06_depth_viaCover_k100`; 1,022 reachable monomorphic functions in
`term_51_manyfn_1022` ("specialization exceeded the 1024-body or 256-level type
limit"); type-growing recursion in `term_60`, `term_61`, and `gen_17`
(`deep n x = ... deep (n - 1) [x]` with only `Storable 'a`). Power-assert hides
the same limit as `?` (`tb_31_limit_assert_180`). None of these errors names a
source location.

**Block:** no, but Plan 11 must not assume "check passes, so build passes".
Macros "compile through the current Build" (`plans/11-macros-comptime.md:130-131`),
so a macro program can hit B8 or B9 and the macro author gets no location.

### M1. Literal defaulting ignores the variable's other constraints

G9 commits to `int`, `string`, `bytes`, `bool`, or `unit` when exactly one
literal trait is present (`solve.rs:3411-3432`). It does not check the
candidate against the variable's other predicates, and it has no
standard-class restriction. H98 §4.3.4 defaults only when every class is
standard and tries candidates in order until one satisfies all constraints.

- `gen_52_default_picks_int`: `name 1` with a user trait `Name` and impls for
  `int` and `Int` is accepted and selects the `int` impl
  (`PASS literal with a user class defaults to int`). GHC rejects it as
  ambiguous (`gen_50_defaulting/ghc/ghc.out`).
- `gen_51_default_user_class`: with only `impl Name Int`, `name 1` fails with
  `No impl for Name int`. Adding an `int` impl changes the meaning of existing
  call sites.
- `gen_55_default_vs_repr`: `needBig 1` with `needBig : ('a : Big) -> int`, and
  `show (Map.singleton 1 2)`, both fail with
  ``× Unsatisfied representation constraint `Big int`.`` The file prints this
  error 3 times for 2 uses (twice for the `show` use). `FromInt Int` exists, so
  an `Int` default would succeed.

**Block:** no. Macros that generate literals in Big positions (Validate, Data
fields) need explicit types until this changes.

### M2. `('a : C)` inside a context predicate is silently dropped

`crates/nash-can/src/types.rs:24-25` adds `repr_predicates` for the annotation's
type only, and `impls.rs:217-221` for the impl heads only. A representation
annotation inside a context predicate produces nothing.

```text
$ nash check  ent_07j_ctx_annotation_big_use      # g : Q ('a : Big) => 'a -> int; useg = g one, one : int
Success! Compiled 37 modules (270 declarations)
exit=0
$ nash check  ent_07k_annotation_in_type_honoured  # same annotation in the type
  × Unsatisfied representation constraint `Big int`.
exit=1
```

`ent_07c`: `impl Q ('a : Big) => Q (list 'a)` does not give `Big 'a` to the
superclass proof. `docs/kinds.md:119-121` calls the form "sugar for that
context entry" with no restriction on position. **Block:** no; macro output must
not rely on the form until it is either honored or rejected.

### M3. A partially applied alias is nominal in unification

Saturated aliases unify with their bodies. A partially applied alias head does
not (`unify.rs:323-364`, `:374-438`), although `Apply listAlias int` and
`Apply list int` reduce the same way (D2).

```text
$ nash check  kind_03_partial_alias_nominal_rev
  × Type mismatch: expected `wrap listAlias int`, found `wrap list int`.
  × Type mismatch: expected `wrap list int`, found `wrap listAlias int`.
exit=1
```

The program is `type wrap 'f 'a = Wrap ('f 'a)`,
`type alias listAlias 'a = list 'a`, `v : wrap listAlias int; v = Wrap [1]`.
The saturated control `kind_03_saturated_alias_control` passes. No doc says
unification is nominal for partial aliases. **Block:** no.

### M4. Impl admission has no termination condition, and the superclass check is super-linear

Nash admits any impl whose context variables occur in its heads (`impls.rs:217-233`).
Non-terminating impls are accepted at the declaration and fail only at a use,
with the region at the use.

```text
$ nash check  term_16_declare_only_expanding    # impl C (box (box 'a)) => C (box 'a)
Success! Compiled 37 modules (267 declarations)
exit=0
$ nash check  term_01_expanding_ctx             # same impl, one use: f x = c x
  × Impl resolution for `C` in `c` exceeded the work limit.
exit=1
```

`kind_21_impl_ctx_fix` accepts `impl Eq ('f (fix 'f)) => Eq (fix 'f)`, the
`UndecidableInstances` instance a derived `Eq` for `Fix` would be. The superclass
check (B5) is cubic in type depth and quadratic in context width: it rejects
`impl S (wrap^43 int)` (`term_41_super_deep_43`), which GHC accepts and which
Nash accepts at a use up to depth 255, and it rejects 125 distinct context
predicates with "requirements keep expanding" (`term_40_manyctx_125`) although
nothing expands. **Block:** no. A bad generated impl surfaces in a caller's code.

### M5. Annotation type variables are implicitly scoped

A nested annotation that names an enclosing annotation's variable shares it
(`expression.rs:1241-1265`, Elm `Expression.hs:555-557`). No doc mentions it.

```elm
f : 'a -> 'a
f x =
    let
        g : 'a -> 'a
        g y = x
    in
    g x
```

`elm_08b_scoped_tyvars_capture` is accepted by Nash and Elm: the inner `'a` is
the outer one. **Block:** no, but the Plan 11 binding-identity contract
(`plans/11-macros-comptime.md:94-99`) covers value binders only. Generated
annotations need fresh type-variable names too.

### M6. The rules macro output exercises most are the least pinned by tests

From section 5: the structural Eq and reflexive Lift guards in superclass proofs
and coverage cells (17 survivors), every work counter (54), coverage `prove`
(18 of 21), predicate identity (22 of 45 in `same_args`), and the ground
resolver's failure paths (41 of 72 in `evidence.rs`). `kind_check.rs:228`
(`Check::predicate`) can be replaced by `Ok(())` without a test failing.
**Block:** no. Tests for `tm_03` to `tm_06` and for each H finding would close
the gaps that matter for `@derive`.

### M7. The datatype-context fixpoint grows exponentially with no cap

One union with 8 parameters and 3 permuting recursive references
(`kind_05_blowup_n8_k7`) is accepted after 35.7 s and 4.4 GB. Smaller shapes:
`n8_k6` 1.95 s and 557 MB, `n9_k6` 6.97 s and 1.16 GB. Contexts grow about
`n^k` (`kinds.rs:991-1049`). `docs/kinds.md:219-221` accepts this.
**Block:** no; it needs a hand-built permuting type.

### M8. An untyped recursive group can export an uncallable member

`bad = 1 + "s"` is accepted (`elm_55_nash_only_unsatisfiable_inferred`) with
the scheme `(Num.Num 'a, Literal.FromInt 'a, Literal.FromString 'a) => 'a`
(printed by `nash docs`). This is not a bug. The context is satisfiable: a
local type with orphan-legal `Num`, `FromInt`, and `FromString` impls makes
`use : num; use = bad` typecheck and run (`v_04_m8_satisfiable`:
`PASS the inferred context of bad is satisfiable`). A use at `int` fails at the
use with ``× No impl for `FromString int`.`` (`elm_24_cascade_inferred_bad`),
as in Haskell without the MR.

The real finding is narrower. In an untyped recursive group, one member can be
uncallable (`gen_54_shared_group_uncallable`: `useQ = q 3` fails with
``× Cannot determine type `'a1` in `useQ`.``), because the shared context
mentions a variable absent from that member's type (`solve.rs:3332-3333`,
`docs/traits.md:453-454`). No caller can fix that variable, so the definition
could be rejected where it is written, as GHC does in its ambiguity check for
inferred types. **Block:** no. Generated helpers should carry annotations.

### Low-risk findings

| # | Finding | Repro | Code |
|---|---|---|---|
| L1 | `IrregularRecursion` rejects programs with finite contexts: `type T 'f 'a = T ('f 'a) (T list 'a)` gives "This recursive use of `Main.T` constructs the parameter 'f". GHC accepts. | `kind_04_irr_closed_head`, `kind_04_irr_closed_arg` | `kinds.rs:1002-1013` |
| L2 | Alias bodies must have kind `Type`: `type alias lst = list` gives "Kind mismatch in the lowercase name of alias `lst`". GHC accepts. | `kind_16_alias_hk_body` | `kinds.rs:1562-1563` |
| L3 | Trait kind errors are not accumulated: two broken traits report one error. | `kind_21_trait_errors_accumulate` | `kinds.rs:1902-1921` |
| L4 | Diagnostic defects: one bad use reported 3 times; declaration closure errors say "in the type annotation"; solver kind errors use the placeholder name "type application". | `kind_10_apply_list_of_option_inferred`, `kind_12_nest_term_arg`, `kind_13_scoped_tyvar_kind` | `kinds.rs:1676`, `kind_check.rs:269-274` |
| L5 | Power-assert prints `?` for `NonGround` and `Limit`, not only for a missing impl, and it prints values in a polymorphic helper where an explicit `show` needs `Show 'a`. | `tb_31_limit_assert_180`, `tb_24_poly_helper`, `tb_25_solver_helper_needs_show` | `assertion.rs:50-59` |
| L6 | `unused_import` fires on a tests import that field visibility needs; removing it breaks the program. | `tb_44_unused_import_but_needed`, `tb_44b_without_import` | `module.rs:184-197` |
| L7 | Inferred contexts are not in head-normal form: `boxIt : Observe (Box 'a) => 'a -> unit` where GHC infers `Observe a`. Equivalent under the global impl table. Cause not traced. | `ghc_02_head_constructor_resolve_user_type`, `gen_02_flexctx` | `solve.rs:3506-3642` |
| L8 | Growing-evidence check ignores layout-only growth; the failure arrives in codegen with no region (also H3). | `gen_17_polyrec_storable`, `term_60`, `term_61` | `solve.rs:1397-1403` |
| L9 | An unused polymorphic local with evidence is never evaluated, so `fail` inside it does not fire; documented in `docs/codegen.md:837-844` only. | `gen_32_let_sharing_scope` | `build.rs:503-512` |
| L10 | No coinductive dictionaries: `(C 'a, C (List (Rose 'a))) => C (Rose 'a)` is rejected; GHC with `UndecidableInstances` accepts. | `term_17_recursive_dictionary`, `term_12`, `term_13` | `entailment.rs:352-356` |
| L11 | The overlap diagnostic draws "first impl in `A`" against the second module's source. | `ent_09a` | `impls.rs:132-138` |
| L12 | Rigid-against-alias asymmetry from Elm rejects `f : 'b -> Id 'b; f x = x`. | `elm_33_alias_to_var_rigid_first` | `unify.rs:307-319` |
| L13 | Nash fixes an Elm unsoundness in untyped recursive groups, with no test guarding it. | `elm_14e_group_regen_two` | `expression.rs:1321-1324` |
| L14 | Error order: `solve.rs:251-255` reverses to match Elm, then `crates/nash-report/src/lib.rs:176-208` re-sorts by region. | `elm_45_cascade_annotated_body_error` | as cited |
| L15 | Superclass, overlap, and resolution budgets depend on unrelated declarations: the 16,386th impl of a trait fails with "impl pattern is too large", and 16,385 impls make every use fail. | `term_52_manyimpl_16386`, `term_52_manyimpl_16385_use` | `impls.rs:115-130`, `resolve.rs:380` |
| L16 | Power-assert capture compile time grows faster than linear with nesting: 0.6 s, 0.8 s, 2.6 s, 5.1 s at depths 10, 20, 30, 40, measured on a loaded machine. | `tb_36_nested_capture_*` | `assertion.rs:34-84` [INFERENCE: cause not profiled] |

Not found: no route lets a user impl replace structural Big Eq (`ent_04a` to
`ent_04q`); no input reaches the `select_impl` panic at `head.rs:291`
(`ent_28a`, `ent_28b` give errors) [INFERENCE: not proven unreachable]; no
disagreement between inference and codegen selection at ground types other than
limits; no non-termination in relational improvement (`term_70` to `term_72`).

## 8. Doc claims the code contradicts

| Doc | Claim | Code |
|---|---|---|
| `docs/traits.md:261-263` | `by_instance` needs a head constructor for every argument of `P` | only positions a head tests need one; an unrestricted head variable binds an open argument (`head.rs:207-223`; `v_03_owned_blanket_open_arg`) |
| `docs/traits.md:304-312` | `Evidence` has five variants | seven: also `Repr` and `StructuralEq` (`crates/nash-ast/src/lib.rs:631-659`) |
| `docs/traits.md:564-570` | `resolve` returns `Result<Evidence, Error>` | `Result<Option<Evidence>, Error>` (`evidence.rs:29-34`); `None` for `Apply` |
| `docs/traits.md:584-586` | the result is a closed tree of `Impl` and `ReflexiveLift` | it can also be `StructuralEq` and `Repr` (`evidence.rs:282`, `:300`) |
| `docs/traits.md:409` | 16,384 attempts "per definition boundary" | per root wanted, plus a fresh budget per `select`, `improvement`, and `cover` (B2 to B4; `term_50`) |
| `docs/traits.md:376-392` | classification happens after `generalize` adjusts ranks | resolve, discharge, and defer run in `solve_header` before `generalize`; only retain runs after (G6) |
| `docs/traits.md:381-383` | an impl's context replaces the predicate | `Observe (Box 'a)` stays in the inferred context (L7) |
| `docs/traits.md:499-505` | `nest (n - 1) [x]` with `Show 'a` is rejected as growing evidence | it fails with `No impl for Show (list 'a)` and a missing `Storable 'a` (`gen_10b_polyrec_doc_example`); with `Storable 'a` added, it is a polymorphic-recursion error (`gen_10c`) |
| `docs/traits.md:485` | recursive groups "follow Elm" | annotated self-calls are polymorphic, Elm's are not (L13b); untyped groups do not re-introduce earlier members' variables (L14) |
| `docs/traits.md:473-477` | inner-let defaulting gives the Haskell outcome | not with a user class (M1) or a captured MPTC argument (H2) |
| `docs/testing.md:229-232` | `?` means no `Show` impl; Show is resolved "by the trait solver like any other constraint" | `?` also for `NonGround` and `Limit`; resolution is the post-solve resolver with no givens (T5, L5) |
| `docs/testing.md:69` | tests-block imports are visible only inside the block | true for names; impls and traits are global (T3) |
| `docs/kinds.md:42` | kind inference is Haskell 98 | alias bodies must be `Type` (L2); `IrregularRecursion` rejects H98-valid types (L1) |
| `docs/kinds.md:119-121` | `('a : Storable)` is sugar for the context entry | not inside a context predicate (M2) |
| `docs/kinds.md:135-140` | contradictory representations are checked per variable | also on applications (`yzyxrmxo`), nested annotations (`xktrnzok`), and impl contexts through superclasses (`uuuntvoo`); the doc was not updated |
| `docs/diagnostics.md:11-14` | Elm's prose is kept nearly verbatim | one line plus labels; Elm's paragraphs and hints are dropped (L27 to L30) |

## 9. History

These are the changes that introduced each rule, read from their diffs. Full
per-change tables: `notes/history.md`.

| Topic | Changes (commit) | What the diff did |
|---|---|---|
| Traits core | `mpqxyunm` (`25ee81ef`) to `unkkzyxn` (`9cc6614c`), 2026-09-05 to 09-23 | AST predicates and evidence; trait and impl canonicalization (`vlkpktvx`, `psoyzunr`); global impl tables (`ynzuovqp`); superclass check with cycle and limit (`ovrqutmr`); reflexive Lift (`kmvvylzt`); solver predicates, givens, superclass givens, impl contexts (`qwlwzkzz` to `wxrtrsms`); ambiguity (`yszywxtx`); literal defaulting (`pruywspl`); supertypes removed (`pxnwpwlv`); recursive impl patterns and the shared matcher `head.rs` (`umwystmv`); relational inference (`unkkzyxn`) |
| Haskell 98 kinds | `nxylqotm` (`3495cc5c`, 2026-09-07), `vqkkxpoq`, `zwzxkzlm` | Replaced the subkinding engine with H98 kinds, and in the same commit added representation predicates, datatype contexts, `Apply`, `ImplOfBuiltinTrait`, and head-only coherence. The title names only the kinds. |
| Datatype-context termination | `vpmvsvro` (`80a9e5e8`), then `ppmtxkrs` (`5d7a0112`), 2026-09-07 | Self-application kinds were made coinductive, then inductive two hours later; the tests kept their names and flipped their expectations. |
| Representation classes on impl heads | `umrynmls` (`95d828a2`, 2026-09-23) | `Head::Var` carries a `ReprSet`; disjoint Big and Little blankets allowed; reversed the head-only coherence of `nxylqotm`. |
| Structural Big Eq, split list and map Eq | `pyruooxr` (`ec35b41d`), `vowtnomk` (`4a3ec22f`) | `StructuralEq` evidence and the user-impl ban; Base split list Eq and added native `map` with three disjoint Eq impls. |
| Contradictory representation givens | `yzyxrmxo`, `xktrnzok`, `uuuntvoo` (2026-10-12, 10-13) | Each closes one way to reach the `select_impl` panic; none updated `docs/kinds.md`. |
| Shared selection | `lylnrzzn` (`de1b7097`) | `select_impl` with `Defer` and `Exact`; a second match panics instead of taking the first. |
| `by_coverage` | `nnrqmrqm` (`baadec9e`), `kltlospo`, `onrsxrym`, `usytrzsw` | Coverage cells and `Specialize`; open variables in ground resolve; `Exact` lets a selected impl beat an undecided one; cyclic arguments defer. Codegen "method B" has no codegen unit test. |
| Polymorphic recursion | `klnsvvtl` (`27c09868`), replaced by `yxpzplto` (`4e5f758c`, empty description) | A syntactic check, then the slot-graph rule. |
| Constraint tree | removed by `lssnqtpu` (`0ed0c75a`), restored by `nollpxwz` (`ba457678`) | The restore also fixed alias identity for importers of annotated definitions, under a "refactor" title. |
| Tests blocks | `xnwzwmzv` (`9883ea8e`), `zwkoyyss` (`0ec9311b`), `ltmkrkqv` (`9fa1d0a7`), `rxuzkxps` (`5655782b`) | Syntax; scoped imports and `test_fields`; `nash test` and `assertion.rs` resolving `Show` outside the solver; the `tests` flag on field constraints. |
| Limits | `sqslkmtl` (`438d5070`), `wuvqpusv` (`e58587b6`), `poqnnxur` (`95afd58c`) | One home for the constants, without unifying `>` and `>=`; depth 128 to 256 in two separate constants; a stale codegen test fixed a day later. |

Rules that landed without a test in the same change: the recursive-evidence
slot backfill (`pruvrnur`); the `assert!` tripwire for contradictory givens
(`yzyxrmxo`); codegen `Specialize` selection (`nnrqmrqm`); `reaches_cycle`
(`usytrzsw`, tested only through 3 inference tests); the limit values
(`sqslkmtl`, `wuvqpusv`); and the `show_value` fallback (`ltmkrkqv`).

## 10. Open questions for the owner

1. Should an impl on a non-record alias be an error when an impl for its body
   could exist (the GHC reading), or should selection always expand aliases?
   H1 cannot be closed without this decision.
2. Is "check passes, so build passes" a Plan 11 requirement? If so, the
   codegen budgets (B8, B9) need the inference cost model or a region.
3. Should a captured variable count as determined in the ambiguity check (H2),
   in the way GHC grows the environment's variables through functional
   dependencies?
4. Should literal defaulting try the Big counterpart, or check the candidate
   against the variable's other predicates (M1)?
5. Should `('a : C)` inside a context predicate add `C 'a` or be rejected (M2)?
6. Is `IrregularRecursion` meant to stay, or to stand in for a size bound that
   would also fix M7 and admit L1's programs?
7. Can a dependency package be named `nash/base` and so receive the structural
   Eq and reflexive Lift rules (`crates/nash-can/src/environment.rs:121-128`)?
   Not probed.
