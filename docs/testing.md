# Testing

Tests live in a `tests` block at the end of a module. `nash test` compiles
every `test` and `prop` into a standalone UPLC program, runs it on the CEK
machine in `nash-plutus`, and reports the result with budgets, traces, labels
and, for properties, a shrunk counterexample.

The design follows Aiken: the PRNG is a Plutus value that the generator
threads through on-chain code, the runner only sees the sequence of random
choices, and shrinking is choice-sequence shrinking in Rust (MiniThesis). Nash
adds a `fuzzer 'a` monad (constructor `Fuzzer`) so generators are written
with `do`, and a power-assert
`assert` that prints the value of every sub-expression on failure.

## Surface syntax

```elm
module Order exposing (compare, invert)

...

tests
    import Fuzz exposing (int, listOf)

    test "lt is strict" = do
        assert (not (lt 1 1))

    prop "compare is antisymmetric" =
        let
            a via int
            b via int
        in
        do
            label (if a < b then "lt" else "ge")
            assert (compare a b == invert (compare b a))

    prop "division by zero fails" fail =
        let x via int in
        do
            assert (x / 0 == 0)

    test "sort stays within budget" within (cpu 2000000, mem 10000) = do
        assert (sort [3, 1, 2] == [1, 2, 3])
```

Grammar (same notation as [syntax.md](syntax.md)):

```ebnf
module        ::= module_header { import } { declaration } [ tests_block ]

tests_block   ::= "tests" INDENT { import } { test_decl } DEDENT

test_decl     ::= "test" string [ "fail" ] [ within ] "=" do_block
                | "prop" string [ "fail" [ "once" ] ] [ within ] "=" prop_body
within        ::= "within" "(" budget [ "," budget ] ")"
budget        ::= "cpu" integer | "mem" integer

prop_body     ::= "let" INDENT via_binder { via_binder } DEDENT "in" do_block
via_binder    ::= pattern_term "via" expr

do_block      ::= "do" INDENT statement { statement } DEDENT
statement     ::= lower_name "<-" expr
                | "let" INDENT { def } DEDENT
                | expr
```

Rules the parser and canonicalizer enforce:

- The `tests` block is the last thing in the module. Its imports are visible
  only inside the block, in addition to the module's own imports. Everything
  in the enclosing module is in scope, including values it does not expose.
- `test` names and `prop` names are strings, unique within the module.
- `via` binders are allowed only at the top of a `prop` body, and a `prop`
  body must start with them (a `prop` that draws nothing is a parse error;
  use `test`). The left side is a pattern: `(a, b) via tuple2 int int` is
  fine. Irrefutable patterns only; a refutable pattern is the usual
  exhaustiveness error.
- The `let ... in` that holds `via` binders holds nothing else. Ordinary
  `let` follows in the body.
- `within` takes one or two budgets in either order, at most one of each.
- `fail once` is a `prop` modifier; on a `test` it is a parse error.
- `do_block` is the general `do` from [syntax.md](syntax.md). What changes in
  a test body is its desugaring, described below.

## Semantics

### Test body

A test body has type `unit` and is always a `do` block: `= do` for a `test`,
`in do` after the `via` binders for a `prop`. A one-statement body is
`= do` followed by one `assert`. This `do` is a **sequencing block**, not a
monadic `do`: there is no monad, and `Monad.bind` is never involved. The
`do` keyword is reused because the layout is the same; the desugaring is:

```
do { e; rest }        ==>  let () = e in rest
do { x <- e; rest }   ==>  let x = e in rest
do { let d; rest }    ==>  let d in rest
do { e }              ==>  e
```

Every expression statement must have type `unit`. `x <- e` binds the value of
`e` (any type) to `x`. The last statement is the result and must be `unit`.
UPLC is strict, so `let () = e in rest` evaluates `e` before `rest`; the
statements run in order.

Only the `do` in test-body position (right after `=`, or after `in` for a
`prop`) is a sequencing block. A `do` anywhere else, including nested
inside a test body or inside a generator, is the monadic `do` of
[syntax.md](syntax.md). There is no `test` monad.

Statements that tests use:

| Statement | Type | Meaning |
|---|---|---|
| `assert e` | `e : bool`, statement `: unit` | Keyword. Fails the test when `e` is `False`. Power-assert, see below. |
| `label s` | `string -> unit` | Stdlib `Test.label`. Records a label for the coverage table. Compiles to a trace with a `\0label\0` prefix. |
| `trace s ()` | `trace s : 'a -> 'a` | Keyword. Emits a trace line, shown under `· with traces`. |
| `fail s` | `: 'a` | Keyword. Fails with a message. |

`assert`, `fail`, `todo` and `trace` are keywords with their own expression
nodes (`Expr::Assert` and friends, [syntax.md](syntax.md)), not functions.
`label` and `assertFailed` are ordinary functions in the stdlib `Test`
module ([stdlib.md](stdlib.md)). Only `assert` gets the compiler treatment
below, and only inside a `tests` block.

### Outcome of a unit test

A `test` runs once. Evaluation either completes (any value, but the body is
typed `unit` so it is `()`) or errors. With no modifier, completion passes.
With `fail`, an error passes and completion fails.

### Outcome of a property

A `prop` runs its body for up to `--max-success` (default 100) drawn inputs.
Each iteration completes or errors.

| Modifier | Iteration counts as counterexample when | Prop passes when |
|---|---|---|
| none | body errors | no counterexample in `n` iterations |
| `fail` | body completes | no counterexample in `n` iterations (every input fails) |
| `fail once` | body errors | a counterexample is found; it is reported as `★ counterexample` |

The first counterexample stops the loop and is shrunk. This is Aiken's
`OnTestFailure` table: `FailImmediately`, `SucceedEventually`,
`SucceedImmediately` (`test_framework.rs:421-446`, `1155-1171`).

### `within (cpu N, mem M)`

The test fails when the consumed budget exceeds a given limit; a missing
limit is unbounded. For a `prop` the
limit applies to every iteration, and the report shows the maximum. Budgets
are measured with the cost model of the configured `plutusVersion` after the
same Core passes `nash build` runs, so they match what ships. `within` and
`fail` compose: a `fail` test that errors *and* stays within budget passes.

The budget check is done by the runner from `EvalResult.info.consumed_budget`;
the program is evaluated with the machine maximum budget
(`ExBudget::max()`), never with `N`/`M` as the limit, so a failure is reported
with the real consumption.

### Traces

Every evaluation collects the trace log. The runner splits it:

- lines starting with `\0label\0` are labels;
- lines starting with `\0assert\0` are power-assert payloads;
- everything else is shown verbatim under `· with traces`.

Only failing tests show traces by default. `--trace-level` follows the same
values as `nash build` (default `verbose` for tests).

## Power-assert

`assert e` with `e : bool`. The compiler rewrites the call so that, when `e`
is `False`, the value of every sub-expression of `e` whose type has a `Show`
impl is traced, then evaluation fails. The runner lays the values out under
the source text.

For

```elm
assert (compare a b == invert (compare b a))
```

with `a = 1`, `b = 0` and an `invert` that leaves `LT` unchanged, the report is

```
FAIL compare is antisymmetric [after 4 tests]
× counterexample
│ a = 1
│ b = 0
× assert (compare a b == invert (compare b a))
          │       │ │    │       │       │ │
          │       │ │    │       │       │ 1
          │       │ │    │       │       0
          │       │ │    │       LT
          │       │ │    LT
          │       │ 0
          │       1
          GT
  Order.nash:31:13
```

Rules:

- **Captured sub-expressions.** Variables, calls, operator applications,
  field accesses, negations, and `if`/`case`/`let` expressions as a whole.
  Not captured: literals, lambdas, nullary constructors, and the `assert`
  argument itself (its value is `False`).
- **Strict positions only.** Sub-expressions under a lazy position are not
  captured: the right operand of `&&` and `||`, the branches of `if` and
  `case`, and lambda bodies. Capturing them would change which expressions
  get evaluated.
- **Evaluation order is preserved.** Each captured sub-expression is bound
  once with `let`, in source order, and the original expression uses the
  binding. Nothing is evaluated twice.
- **`Show`.** A captured sub-expression whose type has no `Show` impl (a
  function, a type without `@derive(Show)`) prints `?` in its column. The
  `Show` evidence is resolved by the trait solver like any other constraint;
  the rewrite runs after solving with the sub-expression types known.
- **Cost.** A passing `assert` costs the `let` bindings only. The `show`
  calls sit in the failure branch. Test programs are not size sensitive.
- **Payload.** On failure the program traces one line per captured
  sub-expression with a `Show` impl, `\0assert\0<assert-id>\0<index>\0<shown>`,
  then errors. The compiler records, per test, the table
  `assert-id -> (source region, text, [(index, sub-expression region)])`.
  The runner joins the two. Sub-expressions without `Show` are in the table
  but never in the payload.

`assert e` outside a `tests` block (in a validator) is lowered by codegen as
`if e then () else fail` ([codegen.md](codegen.md)). The rewrite applies only
to test bodies.

## Fuzzers

```elm
-- Fuzz.nash (stdlib)
type Prng = Seeded Bytes (List Int) | Replayed Int (List Int)

type fuzzer 'a = Fuzzer (Prng -> option (Prng, 'a))

impl Functor fuzzer where
    map f (Fuzzer g) = Fuzzer (\prng -> case g prng of
        None -> None
        Some (p, a) -> Some (p, f a))

impl Applicative fuzzer where
    pure a = Fuzzer (\prng -> Some (prng, a))
    ap = ...

impl Monad fuzzer where
    bind (Fuzzer g) k = Fuzzer (\prng -> case g prng of
        None -> None
        Some (p, a) -> case k a of Fuzzer h -> h p)
```

- `Prng` is a **Big** ADT: the runner builds it as `PlutusData` and reads it
  back from the result. `Seeded seed choices` carries a 32-byte seed and the
  choices made so far, newest first. `Replayed remaining choices` carries a
  count and the choices still to replay, next first.
- `fuzzer 'a` is a **little** ADT with one constructor wrapping the
  function. The result tuple is a UPLC `constr 0 [prng, value]` because `pair`
  only takes `Storable` components and `'a` is any kind. The wrapper exists because
  impls attach to nominal types, not to function aliases.
- Choices are non-negative integers, one per primitive draw, as in
  MiniThesis. Aiken uses bytes; Nash uses `Int` so a primitive can draw a
  full-range integer in one choice and shrink it with one binary search.

The single primitive:

```elm
-- Draw an integer in [0, bound].
choice : int -> fuzzer int
choice bound = Fuzzer (\prng -> case prng of
    Seeded seed choices ->
        let n = mod (lower (bytesToInt (blake2b256 seed))) (bound + 1) in
        Some (Seeded (blake2b256 seed) (lift n :: choices), n)
    Replayed 0 _ -> None
    Replayed k (c :: rest) ->
        if lower c <= bound then Some (Replayed (k - 1) rest, lower c) else None
    Replayed _ [] -> None)
```

A replayed sequence that runs out, or replays a value above the requested
bound, yields `None`. That is what makes choice-sequence shrinking sound: any
edit to the sequence either replays to a valid smaller input or is rejected
by the generator itself. Everything else (`int`, `listOf`, `oneOf`, `bytes`,
`map`, `bind`) is built on `choice`, and generators must draw smaller values
from smaller choices for shrinking to produce smaller inputs.

Nullary generators like `int` are values, so `a via int` and
`xs via listOf int` read naturally.

## How the runner drives a property

For every `prop` the compiler emits two programs that share the module code:

```
draw : Prng -> option (Prng, list string)
run  : Prng -> option Prng
```

`draw` applies the `via` generators in order, threading the PRNG, and returns
the next PRNG with the `show` of each drawn value (`"?"` when the type has no
`Show` impl). `run` draws the same values, evaluates the body with them in
scope, and returns the next PRNG. Both take a `Prng` as `Data`.

The value never crosses the program boundary. A drawn value can be of any
kind (an `int`, an `option`, a function), and only `Data` can be handed from
one CEK evaluation to the next. So the body is compiled together with the
draw, and the generator runs again inside `run`. Generation is deterministic
and cheap next to the body, and the happy path costs one evaluation per
iteration.

Loop, seeded with `--seed`:

1. `prng = Prng::from_seed(seed)`. Evaluate `run prng`.
2. `Some p'` with no error: the iteration passed. Collect labels from the
   log. Continue with `p'`.
3. Error: the body failed (or, with `fail`, completion is the failure).
   Evaluate `draw prng` to recover the next PRNG, the choice sequence and
   the shown values. If `draw` itself errors or returns `None`, the generator
   is broken: the prop is reported as `× fuzzer failed unexpectedly` and the
   loop stops.
4. Build a `Counterexample { choices, shown }` and shrink it.

Why not one combined program: the body fails with `error`, which discards the
result, so the next PRNG and the choices would be lost exactly when they are
needed. Why not return a thunk: discharging the CEK closure into a term and
re-applying it works but copies the environment per iteration; two programs
are simpler and mirror Aiken's `sample` / `eval` split
(`test_framework.rs:709-722`, `493-499`).

### Shrinking

A port of Aiken's `Counterexample::simplify` (`test_framework.rs:848-1007`),
itself a port of MiniThesis, with `u64` choices instead of `u8`:

1. Delete chunks of 8, 4, 2, 1 choices from the end, with the extra step of
   decrementing the choice before a deleted chunk (list lengths).
2. Replace chunks of 8, 4, 2 with zeros.
3. Binary-search each choice down toward 0.
4. Sort chunks of 8, 4, 2 ascending.
5. Swap out-of-order neighbours at distance 2 and 1, and redistribute value
   between them with a binary search.
6. Repeat until a full pass makes no change.

A candidate sequence is evaluated with `Prng::from_choices(candidate)`:
`draw` first (error or `None` is `Invalid`), then `run` (error is `Keep`,
completion is `Ignore`; swapped for `fail`). A candidate is accepted when it
is `Keep` and shorter, or equal length and lexicographically smaller
(`consider`, `test_framework.rs:807-826`). Results are memoised in a Patricia
trie keyed on the choice bytes, with the prefix rule from Aiken's `Cache`
(`test_framework.rs:1061-1122`): a non-`Invalid` result for a prefix is the
result for every extension, because the generator did not read past the
prefix.

Shrinking reports `Simplifying counterexample from N choices` and
`Simplified counterexample in Tms after S steps` on stderr while it works.

### Determinism and replay

The seed is a `u32`. The initial PRNG is `Seeded (blake2b256 seed_be_bytes)
[]`, as in Aiken (`Prng::from_seed`, `test_framework.rs:676-692`). Given the
seed, `--max-success`, and the same compiled programs, a run is fully
deterministic, including the shrink. The summary prints the seed; pass it
back with `--seed` to replay. Tests run in parallel but each prop owns its
PRNG chain, so parallelism does not change results.

## Reporting

Per test, one line:

```
PASS lt is strict                    [mem:    1.2K, cpu:   345.1K]
PASS compare is antisymmetric        [after 100 tests]
FAIL division by zero fails          [after   1 test]
```

Budgets on unit tests show the consumed budget. Props show iterations; with
`within` they also show the maximum budget. Failures add, in this order:

1. `× counterexample` (or `★ counterexample` for `fail once`), one
   `pattern = shown` line per `via` binder, the pattern printed as written.
2. Power-assert output for the failing `assert`, when the failure came from
   one.
3. `× budget exceeded` with consumed vs limit, when `within` failed.
4. `· with coverage`: the label table, only for passing props with labels.
5. `· with traces`: remaining trace lines.

The label table shows each label with its share. `--coverage labels`
(default) divides by the total number of labels recorded; `--coverage tests`
divides by the number of iterations. Labels are sorted by count, descending.

### Example output

```
  Testing Order (src/Order.nash)

  PASS lt is strict                  [mem:    1.2K, cpu:   345.1K]
  FAIL compare is antisymmetric      [after 4 tests]
  × counterexample
  │ a = 1
  │ b = 0
  × assert (compare a b == invert (compare b a))
            │       │ │    │       │       │ │
            │       │ │    │       │       │ 1
            │       │ │    │       │       0
            │       │ │    │       LT
            │       │ │    LT
            │       │ 0
            │       1
            GT
    src/Order.nash:31:13
  PASS division by zero fails        [after 100 tests]
  · with coverage
  | neg  51.0%
  | pos  49.0%
  PASS sort stays within budget      [mem:    9.8K, cpu: 1,912.4K]

  Summary 3 passed, 1 failed, 0 skipped   seed 2894013177   0.81s
```

`Simplifying`/`Simplified` progress lines go to stderr as they happen and are
not part of this block.

### JSON

`nash test --json` writes one document to stdout:

```jsonc
{
  "seed": 2894013177,
  "maxSuccess": 100,
  "tests": [
    {
      "module": "Order",
      "name": "compare is antisymmetric",
      "kind": "prop",
      "status": "fail",
      "iterations": 4,
      "budget": { "cpu": 512000, "mem": 2400 },
      "counterexample": [ { "name": "a", "value": "1" }, { "name": "b", "value": "0" } ],
      "assert": { "file": "src/Order.nash", "line": 31, "column": 13,
                  "source": "compare a b == invert (compare b a)",
                  "values": [ { "column": 0, "value": "GT" }, ... ] },
      "labels": {},
      "traces": []
    }
  ]
}
```

## Parallel execution

Tests are independent. The runner evaluates them on a rayon pool
(`--jobs`). Each worker decodes the flat-encoded program into its own
`nash_plutus::arena::Arena`, so no arena crosses a thread. A prop's
iterations and its shrink run sequentially on one worker; parallelism is
across tests, as in Aiken (`aiken-project/src/lib.rs:1173-1176`).

## Runtime consequences

- `Prng` is `Data`, so `Prng::from_seed` and `Prng::from_choices` are two
  `PlutusData::constr` calls (`crates/nash-plutus/src/data.rs:22`), and the
  returned PRNG is read with `unwrap_constr`.
- `draw` returns a `constr` term: `Some` is `constr 0 [constr 0 [data, list]]`,
  `None` is `constr 1 []`. The stdlib declares `type option 'a = Some 'a |
  None` in that order; the runner depends on the tags.
- Test programs are compiled with the project's `optimize` level and with
  compiler traces on, so pattern-match failures and `todo` sites show up in
  the trace log.

## Interactions

- **Validators.** `nash build` strips the `tests` block before
  canonicalization ([validators.md](validators.md)).
- **Traits.** `Show` for power-assert and counterexamples; `Functor`,
  `Applicative`, `Monad` for `fuzzer`; `@derive(Show)` from
  [macros.md](macros.md).
- **Kinds.** `(Prng, 'a)` is a tuple (kind `Term`) because `pair` requires
  `Storable` components, while `'a` may be `Term`; `list string` is a `Const` list of `Const` strings.
- **Codegen.** Each test program is a standalone UPLC program that inlines
  the module's dependency closure ([codegen.md](codegen.md)).
- **Diagnostics.** Test-shape errors (`via` on a `test`, `once` on a `test`,
  duplicate names) are canonicalization errors rendered by `nash-report`.

## Open questions

- **Choices as `Int`.** This diverges from Aiken's byte choices. It makes
  `choice bound` one draw instead of a byte loop and keeps the shrinker
  identical up to the element type. If interop with Aiken generators matters,
  a byte-based `choice8` can be added without changing the runner.
- **Benchmarks.** Aiken has `bench`; Nash v1 does not. `within` covers the
  regression case.
- **Precondition / discard.** MiniThesis `assume` is not supported (same as
  Aiken). A generator can return `None` to reject, which counts as invalid,
  not as a discarded iteration.
