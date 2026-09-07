# Plan 08 — Core -> Core optimizer

## Goal

The four optimizations named in [docs/overview.md](../docs/overview.md)
as Core -> Core passes in `crates/nash-ir`, plus the two things they need:
a hygiene pass that makes every `Name` unique so substitution is
capture-free, and a measurement harness that turns "does not regress" into
a failing test.

Passes:

1. Inline single-use `Let`s and small lambdas.
2. Builtin force caching (and constant-argument currying).
3. Dead code elimination and unused-parameter removal.
4. Case-of-known-constructor and constant folding (via the CEK machine),
   including cast cancellation and `Force(Delay)` removal.

Specification: [docs/codegen.md](../docs/codegen.md), section
"6. Optimizations".

## Prerequisites

- Plan 07 through chunk 11 (`assemble` calls `nash_ir::optimize::run`,
  which is the identity until this plan lands) and chunk 12 (the
  `Vesting` baseline).
- `Core::walk` / `Core::map` from plan 07 chunk 8.

## Crates touched

`crates/nash-ir` (all passes), `crates/nash-codegen` (harness, `assemble`
wiring, `Case(Bool)` lowering tweak), `crates/nash-plutus` (nothing new;
`flat::encode` and `Program::eval` are used as they are).

## Reference files

Aiken `crates/uplc/src/optimize.rs`: `optimize_repeatedly`,
`aiken_optimize_and_intern` (the pass order and fixed-point loop).

Aiken `crates/uplc/src/optimize/shrinker.rs`:

- `OccurrenceTracker`, `VarLookup`, `var_occurrences` — occurrence
  counting with delay awareness.
- `lambda_reducer`, `inline_reducer`, `identity_reducer`,
  `substitute_var`, `substitute_single_var`, `is_a_builtin_wrapper`.
- `builtin_force_reducer`, `forceable_wrapped_names`,
  `builtin_curry_reducer`, `CurriedBuiltin`, `BuiltinArgs`,
  `try_curry_builtin`, `can_curry_builtin`, `is_order_agnostic_builtin`.
- `builtin_eval_reducer`, `is_error_safe`, `cast_data_reducer`,
  `force_delay_reducer`, `case_constr_apply_reducer`,
  `convert_arithmetic_ops`, `flip_constants`.
- `Scope`, `ScopePath` — the common-ancestor logic reused for hoisting.

Aiken `crates/uplc/src/optimize/interner.rs` — `CodeGenInterner` (the
uniquifier).

Elm `elm/compiler/src/Optimize/Expression.hs` — Elm's optimizer is a
different target but shows the shape of a tree-walking `Optimize` pass over
`Can.Expr`.

## Conventions

- Every pass has the signature `fn(&Builder<'a>, &'a Core<'a>) -> &'a Core<'a>`
  and is pure: input untouched, output freshly allocated where changed
  (`Core::map` rebuilds only the spine above a change).
- Every pass is an `insta` snapshot test on pretty `Core` (before/after)
  and a budget test on the CEK machine.
- Correctness bar: a pass may only change a program to one with the same
  result, logs, and error behaviour. "Cannot throw" is the one analysis
  every pass shares.

---

## Chunk 1 — Traversals, hygiene, occurrence analysis

**Files**

- `crates/nash-ir/src/core.rs` (`walk`, `map`, `free_vars`)
- `crates/nash-ir/src/uniquify.rs` (new)
- `crates/nash-ir/src/occurrences.rs` (new)
- `crates/nash-ir/src/analysis.rs` (new: `cannot_throw`, `size`)
- `crates/nash-ir/src/lib.rs`

**Change**

Add the shared machinery. `uniquify` renumbers every binder so that no two
binders in the program share a `unique` (codegen already tries; this pass
is the guarantee and runs first and last so a bug in a pass shows up as
an assertion, not as capture). `Occurrences` counts uses of each binder
with the delay/lambda context Aiken's `VarLookup` tracks.

**Code**

```rust
// core.rs
impl<'a> Core<'a> {
    pub fn walk(&self, f: &mut impl FnMut(&Core<'a>)) { ... }
}

/// Rebuild-on-change traversal: `f` returns `Some(new)` to replace a node
/// (children of `new` are not revisited), `None` to recurse into it.
pub fn map<'a>(build: &Builder<'a>, core: &'a Core<'a>, f: &mut impl FnMut(&'a Core<'a>) -> Option<&'a Core<'a>>) -> &'a Core<'a>;

/// Capture-free because every binder is unique (uniquify.rs).
pub fn substitute<'a>(build: &Builder<'a>, core: &'a Core<'a>, name: Name<'a>, with: &'a Core<'a>) -> &'a Core<'a>;
```

```rust
// uniquify.rs
//! Assign a fresh `unique` to every binder, in one pass, so substitution
//! never captures. Port of Aiken's CodeGenInterner in spirit; Nash names
//! are already `text + unique`, so this only renumbers.

pub fn uniquify<'a>(build: &Builder<'a>, core: &'a Core<'a>) -> &'a Core<'a>;

/// Panics if two binders share a unique or a variable is unbound.
pub fn check_hygiene(core: &Core<'_>);
```

```rust
// occurrences.rs
#[derive(Clone, Copy, Debug, Default)]
pub struct Occurrence {
    pub count: u32,
    /// Some use sits under a `Lam` or `Delay` relative to the binder, so the
    /// bound value may be evaluated zero or many times there.
    pub under_lambda: bool,
}

pub struct Occurrences<'a> {
    map: HashMap<Name<'a>, Occurrence>,
}

impl<'a> Occurrences<'a> {
    pub fn of(core: &Core<'a>) -> Self;
    pub fn get(&self, name: Name<'a>) -> Occurrence { self.map.get(&name).copied().unwrap_or_default() }
}
```

```rust
// analysis.rs
/// Evaluating this term can neither fail nor loop nor log.
pub fn cannot_throw(core: &Core<'_>) -> bool {
    match core {
        Core::Var(_) | Core::Lit(_) | Core::Lam { .. } | Core::Delay(_) => true,
        Core::Builtin { func, args } => args.len() < func.arity() && args.iter().all(|a| cannot_throw(a)),
        Core::Constr { fields, .. } => fields.iter().all(|f| cannot_throw(f)),
        Core::Cast { kind: CastKind::ToData, arg, .. } => cannot_throw(arg),
        _ => false,
    }
}

/// Approximate flat-encoded size in bytes; the inliner's currency.
pub fn size(core: &Core<'_>) -> usize {
    let mut n = 0;
    core.walk(&mut |c| n += match c {
        Core::Var(_) => 1,
        Core::Lit(k) => literal_size(k),
        Core::Lam { params, .. } => params.len(),
        Core::App { args, .. } => args.len(),
        Core::Let { .. } => 2,
        Core::Builtin { func, args } => 1 + func.force_count() + args.len(),
        Core::Case { branches, .. } => 1 + branches.len(),
        Core::Constr { fields, .. } => 1 + fields.len(),
        Core::Field { arity, .. } => 1 + *arity as usize,
        Core::Trace { .. } => 3,
        Core::Cast { .. } | Core::Delay(_) | Core::Force(_) | Core::Error => 1,
        Core::LetRec { .. } => unreachable!("rewritten before optimization"),
    });
    n
}
```

**Aiken reference**: `OccurrenceTracker::new`, `var_occurrences`,
`VarLookup::delay_if_found`, `substitute_var`, `CodeGenInterner`.

**Tests**

- `uniquify_renumbers_shadowing`: `let x = 1 in let x = x in x` gets two
  uniques; `check_hygiene` passes after, panics on a hand-built duplicate.
- `occurrences_under_lambda`: `let x = 1 in \y -> x` reports
  `under_lambda`.
- `cannot_throw_partial_builtin`: `headList` with zero args is safe, with
  one is not.
- `size_monotone`: `size(App(f, [a]))` > `size(f)`.

**Done when**: unit tests pass; `assemble` calls `uniquify` then
`check_hygiene` at the start and end of `optimize::run`.

---

## Chunk 2 — Measurement harness

**Files**

- `crates/nash-codegen/src/harness.rs` (`Measure`, `assert_budget!`)
- `crates/nash-codegen/tests/budgets/*.toml` (new, committed baselines)
- `crates/nash-codegen/tests/budgets.rs` (new)

**Change**

Every program used as a benchmark is measured three ways and compared to
a committed baseline. A regression fails the test; an improvement fails
too until the baseline is updated (so improvements are reviewed and
recorded).

**Code**

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Measure {
    pub flat_bytes: usize,
    pub cpu: i64,
    pub mem: i64,
}

pub fn measure<'a>(arena: &'a Arena, program: &'a Program<'a, DeBruijn>, args: &[&'a Term<'a, DeBruijn>]) -> Measure {
    let flat_bytes = nash_plutus::flat::encode(program).expect("encodable").len();
    let applied = args.iter().fold(program, |p, a| p.apply(arena, a));
    let EvalResult { info, .. } = applied.eval(arena);
    Measure { flat_bytes, cpu: info.consumed_budget.cpu, mem: info.consumed_budget.mem }
}

/// Compares against `tests/budgets/<name>.toml`. `NASH_UPDATE_BUDGETS=1`
/// rewrites the file instead of failing.
#[macro_export]
macro_rules! assert_budget {
    ($name:literal, $measure:expr) => {
        $crate::harness::check_budget($name, $measure, file!())
    };
}

pub fn check_budget(name: &str, actual: Measure, caller: &str) {
    let path = budgets_dir(caller).join(format!("{name}.toml"));
    if std::env::var_os("NASH_UPDATE_BUDGETS").is_some() {
        std::fs::write(&path, toml::to_string(&actual).unwrap()).unwrap();
        return;
    }
    let baseline: Measure = toml::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("no baseline {name}; run with NASH_UPDATE_BUDGETS=1"))).unwrap();
    assert!(
        actual.flat_bytes <= baseline.flat_bytes && actual.cpu <= baseline.cpu && actual.mem <= baseline.mem,
        "{name} regressed: {actual:?} > baseline {baseline:?}"
    );
    assert!(
        actual == baseline,
        "{name} improved: {actual:?} < baseline {baseline:?}; rerun with NASH_UPDATE_BUDGETS=1 to record it"
    );
}
```

`toml` and `serde` join `nash-codegen`'s dev-dependencies.

Benchmarks (`tests/budgets.rs`), each a fixture in `tests/fixtures/`:

| name | program |
|---|---|
| `vesting_claim` | plan 07 chunk 12, `Claim` path |
| `vesting_cancel` | same, `Cancel` path |
| `list_length_100` | `length` over a 100-element list |
| `sum_static` | `replicate` / `sumTo` from plan 07 chunk 8 |
| `data_match` | the four-clause `Data` match from plan 07 chunk 6 |
| `validate_datum` | `validateData` on a nested record |
| `decoder_datum` | `Data.Decode` example from docs/data.md |

**Aiken reference**: none; Aiken measures in `aiken-project` benchmarks
and acceptance tests, not in unit tests.

**Tests**: the seven benchmarks, recorded once with the identity optimizer
as the baseline.

**Done when**: `cargo test -p nash-codegen --test budgets` passes with
committed baselines; `NASH_UPDATE_BUDGETS=1` rewrites them.

---

## Chunk 3 — Inliner

**Files**

- `crates/nash-ir/src/inline.rs` (new)
- `crates/nash-ir/src/optimize.rs` (new: `run` with the pass list)

**Change**

One pass, three rules, applied bottom-up with `map`:

1. **Value bindings.** `Let x = v in b` where `v` is a `Var`, `Lit`
   (except `string` constants, which stay hoisted), zero-argument
   `Builtin`, or a lambda that is a "builtin wrapper" (`\a b -> Builtin(f, [a, b])`)
   is substituted everywhere. (Aiken `lambda_reducer`.)
2. **Single-use bindings.** `Let x = v in b` with `count == 1` is
   substituted when either the use is not `under_lambda` (the value is
   evaluated exactly once either way) or `cannot_throw(v)` (moving it
   under a lambda can only make it run fewer times). (Aiken
   `inline_reducer`.)
3. **Small lambdas.** `App(Lam(ps, body), args)` and
   `Let f = Lam(ps, body) in b` where `size(body) <= INLINE_LAMBDA_SIZE`
   are beta-reduced at every saturated call site, binding each argument
   with a `Let` (so rules 1–2 decide whether it is substituted). Unused
   bindings are left for chunk 5.

**Code**

```rust
pub const INLINE_LAMBDA_SIZE: usize = 12;

pub fn inline<'a>(build: &Builder<'a>, core: &'a Core<'a>) -> &'a Core<'a> {
    let occ = Occurrences::of(core);
    map(build, core, &mut |node| match node {
        Core::Let { binder, value, body } => {
            let o = occ.get(binder.name);
            let value = inline(build, value);
            if is_value_binding(value) || (o.count == 1 && (!o.under_lambda || cannot_throw(value))) {
                Some(inline(build, substitute(build, body, binder.name, value)))
            } else if let Core::Lam { params, body: lam_body } = value && size(lam_body) <= INLINE_LAMBDA_SIZE && all_uses_saturated(body, binder.name, params.len()) {
                Some(inline(build, beta_at_calls(build, body, binder.name, params, lam_body)))
            } else {
                None
            }
        }
        Core::App { func: Core::Lam { params, body }, args } if args.len() == params.len() => {
            Some(params.iter().zip(args.iter()).rev().fold(*body, |b, (p, a)| build.let_(*p, a, b)))
        }
        _ => None,
    })
}

fn is_value_binding(value: &Core<'_>) -> bool {
    match value {
        Core::Var(_) => true,
        Core::Lit(Constant::String(_)) => false,
        Core::Lit(_) => true,
        Core::Builtin { args, .. } => args.is_empty(),
        Core::Lam { params, body } => is_builtin_wrapper(params, body),
        _ => false,
    }
}

/// `\a b -> Builtin(f, [a, b])` or the same with literal arguments mixed in.
fn is_builtin_wrapper(params: &[Binder<'_>], body: &Core<'_>) -> bool {
    matches!(body, Core::Builtin { args, .. } if args.iter().all(|a| matches!(a, Core::Var(v) if params.iter().any(|p| p.name == *v)) || matches!(a, Core::Lit(_))))
}
```

The size heuristic is the only tunable. `INLINE_LAMBDA_SIZE = 12` is
about one builtin call with three arguments plus a `case`; it is chosen so
that `Field` selectors, `Lift`/`Lower` wrappers and decision-tree leaves
used twice inline, and a checker function does not.

**Aiken reference**: `lambda_reducer` (1753), `inline_reducer` (2316),
`is_a_builtin_wrapper` (2797), `substitute_single_var` (1461),
`identity_reducer` (2252; Nash's rule 1 covers `\x -> x`).

**Tests** (`inline.rs`, `assert_pass_snapshot!(inline, src)` prints
`Core` before and after):

- `inline_single_use_let`: `let x = f 1 in g x` -> `g (f 1)`.
- `keep_single_use_under_lambda`: `let x = f 1 in \y -> x` unchanged.
- `inline_single_use_under_lambda_when_safe`: `let x = 1 in \y -> x` ->
  `\y -> 1`.
- `inline_multi_use_var`: `let x = y in (x, x)` -> `(y, y)`.
- `keep_string_constant`: `let m = "hello" in (trace m 1, trace m 2)`
  unchanged.
- `beta_reduce_small_lambda`: `let sel = \a b c -> b in sel 1 2 3` -> `2`
  (after chunk 5 removes the dead lets).
- `keep_large_lambda`: a `validateData#Datum`-sized lambda used twice is
  not inlined.
- budgets: `vesting_*`, `data_match`, `decoder_datum` must improve;
  update baselines.

**Done when**: snapshots accepted; budgets updated and all `<=` the
chunk 2 baselines (the "improved" assertion documents each change).

---

## Chunk 4 — Builtin force caching and constant currying

**Files**

- `crates/nash-ir/src/builtins.rs` (new)

**Change**

Two rewrites over the whole program, run once (not in the fixed-point
loop):

1. **Force caching.** Every `Builtin { func, args }` with
   `func.force_count() > 0` becomes `App(Var forced_f, args)` where
   `forced_f` is bound once at the program root to `Builtin { func, args: [] }`
   (which lowers to `force^k (builtin f)`). Saves a `force` per call;
   costs one root `Let` per distinct forced builtin. (Aiken
   `builtin_force_reducer` + `run_once_pass`.)
2. **Constant currying.** For builtins where the first argument may be
   a constant that repeats (`equalsInteger 0 _`, `lessThanInteger _ 10`,
   `appendByteString #"" _`, ...) and which are order-agnostic or take the
   constant first, `Builtin(f, [Lit k, x])` occurring at least twice
   becomes `App(Var f_k, [x])` with `f_k = Builtin(f, [Lit k])` bound at
   the lowest common ancestor scope of the uses. (Aiken
   `builtin_curry_reducer`, `CurriedBuiltin`, `is_order_agnostic_builtin`,
   `Scope::common_ancestor`.)

**Code**

```rust
pub fn cache_forces<'a>(build: &Builder<'a>, core: &'a Core<'a>) -> &'a Core<'a> {
    let mut forced: BTreeMap<DefaultFunction, Binder<'a>> = BTreeMap::new();
    let body = map(build, core, &mut |node| match node {
        Core::Builtin { func, args } if func.force_count() > 0 => {
            let b = *forced.entry(*func).or_insert_with(|| build.fresh_binder(forced_name(*func), builtin_ty(*func)));
            Some(if args.is_empty() { build.var(b.name) } else { build.app(build.var(b.name), args) })
        }
        _ => None,
    });
    forced.into_iter().rev().fold(body, |body, (func, b)| build.let_(b, build.builtin(func, &[]), body))
}

pub const CURRY_MIN_USES: usize = 2;

pub fn curry_constants<'a>(build: &Builder<'a>, core: &'a Core<'a>) -> &'a Core<'a>;

fn can_curry(func: DefaultFunction) -> bool {
    matches!(func,
        AddInteger | SubtractInteger | MultiplyInteger | DivideInteger | ModInteger | QuotientInteger | RemainderInteger
        | EqualsInteger | LessThanInteger | LessThanEqualsInteger
        | AppendByteString | EqualsByteString | ConsByteString | LessThanByteString | LessThanEqualsByteString
        | AppendString | EqualsString | EqualsData | ConstrData | MkCons)
}

fn is_order_agnostic(func: DefaultFunction) -> bool {
    matches!(func, AddInteger | MultiplyInteger | EqualsInteger | EqualsByteString | EqualsString | EqualsData)
}
```

`curry_constants` collects `(func, constant)` pairs with their `Scope`
paths (a `Vec<u32>` of child indices from the root, as Aiken's
`ScopePath`), keeps those with `>= CURRY_MIN_USES`, binds each at the
common ancestor, and rewrites the uses. For order-agnostic builtins a
constant in second position is moved first.

**Aiken reference**: `builtin_force_reducer` (1813), `run_once_pass`
(2907), `builtin_curry_reducer` (3084), `BuiltinArgs::args_from_arg_stack`
(657), `CurriedArgs::merge_node_by_path` (821), `flip_constants` (2753).

**Tests**

- `forces_cached_once`: three `headList` calls -> one root binding, three
  `App`s.
- `partial_builtin_becomes_var`: a bare `Builtin(HeadList, [])` becomes the
  cached var.
- `curry_equals_zero`: two `equalsInteger n 0` (constant second,
  order-agnostic) -> one `equalsInteger 0` binding.
- `no_curry_single_use`.
- `curry_scope_is_common_ancestor`: uses in two `case` branches bind above
  the `case`, uses in one branch bind inside it.
- budgets: `list_length_100`, `data_match`, `validate_datum` must improve
  (fewer `force`s).

**Done when**: snapshots accepted; budgets updated.

---

## Chunk 5 — Dead code elimination and unused parameters

**Files**

- `crates/nash-ir/src/dce.rs` (new)

**Change**

1. **Dead lets.** `Let x = v in b` with `count == 0` and `cannot_throw(v)`
   becomes `b`. (A binding that can throw is kept: dropping it would turn
   a failing program into a succeeding one.) Top-level bindings the root
   does not reach are removed the same way, since `assemble` makes them
   `Let`s.
2. **Unused parameters.** For `Let f = Lam(ps, body) in b` where every
   use of `f` in `b` is the head of a saturated `App`, each parameter with
   zero occurrences in `body` is removed from `ps` and from every call
   site, provided every dropped argument `cannot_throw` (otherwise it is
   kept as a `Let _ = arg` at the call site, which rule 1 then keeps or
   drops). A parameter is never removed from a function whose `Lam` is the
   `inner` of a self-application (its first parameter is the function
   itself; it is used), so plan 07's static-param lifting is preserved.

**Code**

```rust
pub fn dce<'a>(build: &Builder<'a>, core: &'a Core<'a>) -> &'a Core<'a> {
    let occ = Occurrences::of(core);
    map(build, core, &mut |node| match node {
        Core::Let { binder, value, body } if occ.get(binder.name).count == 0 && cannot_throw(value) => Some(dce(build, body)),
        Core::Let { binder, value: Core::Lam { params, body: lam }, body } => {
            let unused: Vec<u16> = params.iter().enumerate()
                .filter(|(_, p)| occ.get(p.name).count == 0)
                .map(|(i, _)| i as u16)
                .collect();
            if unused.is_empty() || !all_uses_saturated(body, binder.name, params.len()) { return None; }
            Some(drop_params(build, *binder, params, lam, body, &unused))
        }
        _ => None,
    })
}

fn drop_params<'a>(build: &Builder<'a>, f: Binder<'a>, params: &[Binder<'a>], lam: &'a Core<'a>, body: &'a Core<'a>, unused: &[u16]) -> &'a Core<'a>;
```

**Aiken reference**: `inline_reducer`'s "strip out unused terms that can't
throw" arm (2316), `remove_inlined_ids` (2730). Aiken has no
unused-parameter pass; Nash needs one because decision-tree leaves are
hoisted as lambdas over all their pattern variables.

**Tests**

- `drop_unused_safe_let`: `let x = 1 in 2` -> `2`.
- `keep_unused_throwing_let`: `let x = fail "no" in 2` unchanged.
- `drop_unreachable_top_level`: a module with an unused helper loses it.
- `drop_unused_param`: `let f = \a b -> a in f 1 2` -> `let f = \a -> a in f 1`
  (then chunk 3 inlines).
- `keep_param_when_arg_throws`: `f 1 (fail "x")` keeps a `let _ = fail`.
- `keep_self_param`: the chunk 8 `sumTo` program keeps its self parameter.
- budgets: `decoder_datum`, `vesting_*` must improve.

**Done when**: snapshots accepted; budgets updated.

---

## Chunk 6 — Case-of-known-constructor, constant folding, cast cancellation, Big-list fast paths

**Files**

- `crates/nash-ir/src/fold.rs` (new)
- `crates/nash-ir/src/fastpath.rs` (new): rewrites a monomorphized call of
  core's elementwise `Eq (list 'a)` / `Ord (list 'a)` / `Show (list 'a)`
  method at a ground Big element type into the single-builtin form
  (`equalsData (listData a) (listData b)`, etc.). Keyed on the core impl's
  `ImplRef` plus the ground `MonoKey`; semantics identical because Big
  equality is structural `equalsData` per element. Budget test: `list Int`
  equality of 100 elements must cost one `equalsData` plus two `listData`.
- `crates/nash-codegen/src/comptime.rs` (`eval_closed` reused)
- `crates/nash-codegen/src/lower.rs` (`Case(Bool)` without delay when
  both branches are values)

**Change**

One bottom-up pass with these rules:

| Before | After | Condition |
|---|---|---|
| `Case(Tag, Constr(i, fs), bs)` | `Let b_j = f_j in body_i` | always |
| `Case(Bool, Lit true/false, [t, e])` | `t` / `e` | always |
| `Case(Int, Lit k, bs, d)` | matching branch or `d` | always |
| `Case(Bytes, Lit k, ..)` | same | always |
| `Case(List, Lit [], ..)` / `Lit (x :: xs)` | `nil` / `Let h, t in cons` | always |
| `Case(Data, Lit data, ..)` | the branch for its shape, payload bound to a `Lit` | always |
| `Field(Constr(_, fs), i)` | `f_i` | every other `f_j` `cannot_throw` |
| `Builtin(f, lits)` saturated | `Lit(result)` | `is_error_safe(f, lits)` |
| `Cast(Lower, Cast(Lift, x))` and the reverse | `x` | always |
| `Builtin(UnIData, [Builtin(IData, [x])])` and the other three pairs | `x` | always |
| `Force(Delay(x))` | `x` | always |
| `App(App(f, as), bs)` | `App(f, as ++ bs)` | always |
| `Case(Bool, Builtin(IfThenElse, [c, Lit true, Lit false]), ..)` | `Case(Bool, c, ..)` | always |

Constant folding evaluates the saturated builtin on the CEK machine
through plan 07's `eval_closed` (which needs no bindings for a
literal-only term). `is_error_safe` is ported from Aiken and lists, per
builtin, the argument shapes under which evaluation cannot fail
(division by a non-zero literal, `headList` of a non-empty literal list,
integer arithmetic on integer literals, `iData` on an integer, ...).
Everything not listed is not folded: `fail`s must stay `fail`s at
runtime, not become compile errors.

**Code**

```rust
pub struct Folder<'a, F: FnMut(&'a Core<'a>) -> Option<&'a Constant<'a>>> {
    pub build: &'a Builder<'a>,
    /// Evaluates a closed, error-safe builtin application; `None` when the
    /// evaluator declines (budget, unsupported constant).
    pub eval: F,
}

pub fn fold<'a>(build: &Builder<'a>, eval: &mut impl FnMut(&'a Core<'a>) -> Option<&'a Constant<'a>>, core: &'a Core<'a>) -> &'a Core<'a>;

pub fn is_error_safe(func: DefaultFunction, args: &[&Core<'_>]) -> bool {
    let all_ints = || args.iter().all(|a| matches!(a, Core::Lit(Constant::Integer(_))));
    match func {
        AddInteger | SubtractInteger | MultiplyInteger | EqualsInteger | LessThanInteger | LessThanEqualsInteger | IData => all_ints(),
        DivideInteger | ModInteger | QuotientInteger | RemainderInteger =>
            all_ints() && !matches!(args[1], Core::Lit(Constant::Integer(i)) if i.is_zero()),
        AppendByteString | EqualsByteString | LessThanByteString | LessThanEqualsByteString | LengthOfByteString | BData | Sha2_256 | Sha3_256 | Blake2b_256 | Blake2b_224 | Keccak_256 =>
            args.iter().all(|a| matches!(a, Core::Lit(Constant::ByteString(_)))),
        ConsByteString => matches!(args[0], Core::Lit(Constant::Integer(i)) if (0..=255).contains(i)) && matches!(args[1], Core::Lit(Constant::ByteString(_))),
        AppendString | EqualsString | EncodeUtf8 => args.iter().all(|a| matches!(a, Core::Lit(Constant::String(_)))),
        HeadList | TailList => matches!(args[0], Core::Lit(Constant::ProtoList(_, xs)) if !xs.is_empty()),
        NullList => matches!(args[0], Core::Lit(Constant::ProtoList(..))),
        FstPair | SndPair => matches!(args[0], Core::Lit(Constant::ProtoPair(..))),
        UnIData => matches!(args[0], Core::Lit(Constant::Data(PlutusData::Integer(_)))),
        UnBData => matches!(args[0], Core::Lit(Constant::Data(PlutusData::ByteString(_)))),
        UnListData => matches!(args[0], Core::Lit(Constant::Data(PlutusData::List(_)))),
        UnMapData => matches!(args[0], Core::Lit(Constant::Data(PlutusData::Map(_)))),
        UnConstrData => matches!(args[0], Core::Lit(Constant::Data(PlutusData::Constr { .. }))),
        ConstrData | ListData | MapData | MkCons | MkPairData | EqualsData | SerialiseData => args.iter().all(|a| matches!(a, Core::Lit(_))),
        _ => false,
    }
}
```

The `eval` closure in `assemble` is
`|core| nash_codegen::comptime::eval_closed(arena, &[], core).ok()`.

`lower.rs`: `Case(Bool)` whose two branches both satisfy
`is_value_binding` (chunk 3) lowers to `ifThenElse c t e` without
`delay`/`force`. This is a lowering rule, not a `Core` rewrite, because
`Core` `Case(Bool)` is always lazy by definition.

**Aiken reference**: `builtin_eval_reducer` (2674), `is_error_safe`
(412), `cast_data_reducer` (2522), `force_delay_reducer` (2448),
`case_constr_apply_reducer` (2058), `convert_arithmetic_ops` (2652),
`inline_constr_ops` (2490).

**Tests**

- `case_known_constr`: `case Some 3 of Some x -> x; None -> 0` -> `3`
  after chunks 3+5.
- `case_known_bool`, `case_known_int_default`, `case_known_data`.
- `field_of_constr`: `Field(Constr 0 [a, fail], 0)` is not simplified;
  `Field(Constr 0 [a, b], 0)` is.
- `fold_add`: `addInteger 40 2` -> `Lit 42`.
- `no_fold_div_zero`: `divideInteger 1 0` stays.
- `no_fold_head_nil`: `headList []` stays.
- `cancel_lift_lower`: plan 07 chunk 6's `castLower (castLift 41)` ->
  `Lit 41`.
- `cancel_un_i_data_i_data`.
- `force_delay`.
- `flatten_apps`.
- `if_of_values_is_strict` (lowering snapshot).
- budgets: `sum_static`, `data_match`, `validate_datum`, `decoder_datum`
  must improve.

**Done when**: snapshots accepted; budgets updated.

---

## Chunk 7 — Driver loop and regression gate

**Files**

- `crates/nash-ir/src/optimize.rs`
- `crates/nash-codegen/src/program.rs` (`assemble` wiring)
- `.github/workflows/*.yml` (the budget test runs in CI; it already does
  as part of `cargo test`)

**Change**

Order and fixed point, following `aiken_optimize_and_intern`:

```rust
/// `--optimize 0|1|2` (docs/cli.md, docs/validators.md); `Options.optimize: u8`
/// in plan 07 maps onto it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    /// No Core passes (front-end tests, `Core` snapshots).
    O0,
    /// Inlining, DCE, builtin force caching and currying.
    O1,
    /// `O1` plus case-of-known-constructor and CEK constant folding.
    O2,
}

impl Level {
    pub fn from_flag(n: u8) -> Level {
        match n { 0 => Level::O0, 1 => Level::O1, _ => Level::O2 }
    }
}

pub fn run<'a>(build: &Builder<'a>, eval: &mut impl FnMut(&'a Core<'a>) -> Option<&'a Constant<'a>>, core: &'a Core<'a>, level: Level) -> &'a Core<'a> {
    if level == Level::O0 { return core; }
    let core = uniquify(build, core);
    check_hygiene(core);

    let core = repeat(build, core, |b, c| {
        let c = inline(b, c);
        let c = dce(b, c);
        if level == Level::O2 { fold(b, eval, c) } else { c }
    });
    let core = cache_forces(build, core);
    let core = curry_constants(build, core);
    let core = repeat(build, core, |b, c| dce(b, inline(b, c)));

    check_hygiene(core);
    core
}

/// Apply `pass` until the node count stops changing (Aiken `optimize_repeatedly`).
fn repeat<'a>(build: &Builder<'a>, mut core: &'a Core<'a>, mut pass: impl FnMut(&Builder<'a>, &'a Core<'a>) -> &'a Core<'a>) -> &'a Core<'a> {
    let mut count = size(core);
    loop {
        core = pass(build, core);
        let next = size(core);
        if next == count { return core; }
        count = next;
    }
}
```

`repeat` terminates because every rule either removes nodes or is applied
at most once per node per iteration and the loop stops when the size is
stable; the `assert` in `check_hygiene` catches a pass that duplicates a
binder.

`assemble` passes `Level::from_flag(build.options.optimize)`; the plan 07 test macros
gain a variant `assert_eval_snapshot_unoptimized!` so front-end tests keep
readable output, and every existing `Core` snapshot in plan 07 is
re-accepted once with the optimizer on (their evaluation results must not
change; the test asserts that separately by running both).

**Aiken reference**: `optimize.rs` `aiken_optimize_and_intern` (25),
`optimize_repeatedly` (9), `multi_pass` (2965), `afterwards` (3049).

**Tests**

- `run_is_idempotent`: `run(run(x)) == run(x)` on every fixture (pretty
  `Core` equality).
- `results_unchanged`: every plan 07 evaluation snapshot has the same
  `result` and `logs` with and without the optimizer (a loop over the
  fixtures).
- `tests/budgets.rs`: all seven benchmarks against the final baselines;
  the `Vesting` numbers from plan 07 chunk 12 are the reference point for
  the summary table in the PR.

**Done when**: idempotence and results tests pass; budgets committed;
CI runs `cargo test` including the budget gate.

---

## Open questions

1. **Strictness of dropped arguments.** Chunk 5 keeps arguments that may
   throw. A future strictness analysis could drop more; the conservative
   rule is chosen because a validator that fails must keep failing.
2. **`INLINE_LAMBDA_SIZE` and `CURRY_MIN_USES`** are constants. Making
   them `Options` fields is trivial if a project needs a size/cost
   trade-off knob.
3. **Fusion of `Data.Decode` combinators** (docs/data.md) is not in this
   plan. It would be a fifth pass after chunk 6, recognizing the
   monomorphized stdlib names.
4. **Budget baselines on cost-model changes.** A nash-plutus cost-model
   update shifts every `cpu`/`mem` number; the update procedure is
   `NASH_UPDATE_BUDGETS=1 cargo test` in the same PR.
