# Codegen — Core IR and lowering to UPLC

This document specifies the back end: the `Core` intermediate representation,
the passes that turn a type-checked Can AST into `Core`, the Core -> Core
optimizations, and the final lowering to a UPLC `Term`. Decisions here follow
[overview.md](overview.md); the runtime layout of each type is specified in
[representation.md](representation.md) and the `Data` type in
[data.md](data.md). Implementation plans: `plans/07-codegen.md` and
`plans/08-optimizer.md`.

## Purpose

Aiken generates UPLC from a stack of intermediate forms (`AirTree` ->
`Vec<Air>` -> `Term<Name>`) with types (`Rc<Type>`) attached to many nodes,
performs monomorphization by rewriting the tree in place, and defaults every
user type to `Data`. Nash does three things differently:

1. **One tree IR.** `Core` is a specialized, explicitly-typed lambda
   calculus. Every pass is `Core -> Core` until the last one, which is
   `Core -> Term<Name>`. There is no linearized instruction stream.
2. **Explicit representations.** Binders carry a `Ty` tagged `Big`,
   `Const` or `Term` (see [kinds.md](kinds.md)); representation-independent
   parameters may instead be `Erased`. Codegen demands concrete types only
   where an operation needs them. Haskell 98 kinds have already been checked.
3. **No `Data` by default.** A little type is never represented as `Data`.
   Conversions between representations are explicit Nash functions using
   typed UPLC builtins and Data patterns. The optimizer may simplify inverse
   builtin pairs when it preserves shape checks and evaluation behavior.

## The Core IR

Every expression is `Core { ty: Ty, kind: CoreKind }`. The operation shapes below
are `CoreKind` variants; the result type is a required field on every node.
Builders take explicit result types for applications, builtins, constructors,
fields, cases, forces and errors. They derive the types of literals, lambdas,
delays, lets and traces from their inputs. A zero-parameter lambda has its body's
type because lowering emits the body directly.

Codegen retains solved runtime-specialized expression types, including nominal
constructor and field identities. Representation-independent specializations
may still deliberately use `Erased`; it never means missing metadata. Explicit
coercions can give a runtime shape a different source type. Tree rewrites preserve
the established node type. ANF reads `node.ty` directly: there is no optional type
table, presence check or `MissingType` path for Core.

```
Core ::= Var(name)
       | Lit(constant)                          -- a nash-plutus Constant, incl. Data
       | Lam(params, body)                      -- n-ary, curried on lowering
       | App(func, args)                        -- n-ary
       | Let(binder, value, body)               -- non-recursive, strict
       | LetRec(rec_binders, body)              -- functions only
       | Case(kind, scrutinee, branches, default)
       | Constr(tag, fields)                    -- UPLC `constr`
       | Field(record, index)                   -- projection out of a `constr`
       | Builtin(fn, args)                      -- saturated or partial builtin call
       | Trace(msg, body)
       | Error
       | Delay(body) | Force(body)
```

Every binder is `Binder { name: Name, ty: Ty }` where `Name { text, unique }`
is globally unique after the hygiene pass (see `plans/08-optimizer.md`,
chunk 1). `Ty` exposes the representation needed by lowering. An opaque
parameter that is only passed through has `Ty::Erased`:

```rust
pub enum Ty<'a> {
    Erased,              // opaque pass-through value; not a Plutus constant type
    Big(&'a BigTy<'a>),     // Int, Bytes, Data, List t, Map k v, Big ADT, Big record
    Const(&'a ConstTy<'a>), // int, bytes, string, bool, unit, list t, pair a b, array t, bls_*, value
    Term(&'a TermTy<'a>),   // little ADT, tuple, little record, function
    Runtime(&'a RuntimeTy<'a>), // compiler-created delays, workers and dispatch packets
}
```

`Ty::repr()` returns no representation for `Erased`. Constant construction
must require concrete layout metadata instead of assuming `Data`. `BigTy` records enough
structure to construct and destruct its Data layout (constructor count and
field types); `TermTy` records constructor arities so `Case` and `Field` can
be lowered; `ConstTy` maps one-to-one onto nash-plutus `typ::Type` so
literals can be built (`crates/nash-plutus/src/typ.rs`).

`RuntimeTy::Delay(t)` describes a delayed computation. `SelfFunction(t)` names the
recursive worker equation `self = self -> t`, avoiding a fake ordinary function
type for self-application. Mutual recursion retains a table of parameter/result
types: `Dispatcher` consumes a `Request`, each constructed `Packet` records its
tag, and `Results` describes the possible branch results. Known-tag calls retain
the selected arm's result type, including an intermediate function returned by
an overapplied call. `Constr` describes a raw UPLC constructor without inventing
a nominal source type. These are internal metadata, not new Nash source types.

### Node semantics

| Node | Meaning | Lowers to |
|---|---|---|
| `Var` | local or top-level variable | `Term::Var` |
| `Lit` | a UPLC constant | `Term::Constant` |
| `Lam(ps, b)` | n-ary function | nested `Term::Lambda` |
| `App(f, as)` | n-ary application | nested `Term::Apply` |
| `Let(b, v, e)` | strict binding | `(\b -> e) v` |
| `Let(b : unit, v, e)` with unused `b` | strict native-unit sequencing, including `case v of () -> e` and `let () = v` | `case v [e]`; subject runs before the sole branch, without a lambda/application pair |
| `LetRec` | recursive function group | self-application, see Recursion |
| `Case(Tag, s, bs, d)` | exhaustive consecutive tags: a `Term` constr binds its fields; a decoded integer tag binds none | `Term::Case` |
| `Case(Bool, s, [t, e], _)` | `if` | `case s [e, t]` (false tag 0, true tag 1) |
| `Case(Int, s, bs, d)` | switch on integer literals | chain of `equalsInteger` + boolean `Term::Case` |
| `Case(Bytes, s, bs, d)` | switch on bytestring literals | chain of `equalsByteString` + boolean `Term::Case` |
| `Case(List, s, [nil, cons], _)` | match on a `Const` list; `cons` binds head and tail | `case s [\head tail -> cons, nil]` |
| `Case(Pair, s, [branch], _)` | destructure a builtin pair | `case s [\first second -> branch]`, including wildcard fields |
| `Case(Data, s, bs, d)` | match on the `Data` tag; five branches `Constr\|Map\|List\|I\|B` | `force (chooseData s (delay constr) (delay map) (delay list) (delay int) (delay bytes))` |
| `Constr(i, fs)` | build a UPLC constr | `Term::Constr` |
| `Field(r, i)` | project field `i` of a constr | `case r [\f0 .. fn -> fi]` |
| `Builtin(f, as)` | call builtin `f`; `as.len() <= f.arity()` | `force^k (builtin f)` applied to `as` |
| `Trace(m, b)` | log `m` then evaluate `b` | `force (trace m (delay b))` |
| `Error` | abort | `Term::Error` |
| `Delay`/`Force` | explicit laziness | `Term::Delay` / `Term::Force` |

`Case` branch binders come from the node, not from nested lambdas: a `Tag`
branch is `(tag, &[Binder], body)`, a `List` cons branch binds `(head, tail)`,
a `Data` `Constr` branch binds the decoded `pair int (list Data)`. An explicit
pair pattern adds a nested `Case(Pair, ...)` that binds its tag and fields. Keeping the
binders in the node lets the decision-tree compiler and the optimizer treat
them uniformly without pattern-matching on lambda shapes.

`Builtin` carries the `DefaultFunction` from
`crates/nash-plutus/src/builtin/default_function.rs`; arity and force count
come from `DefaultFunction::arity()` and `DefaultFunction::force_count()`.
A `Builtin` with fewer arguments than its arity is a partial application and
lowers to the same term; a `Builtin` with zero arguments is the (forced)
builtin value itself. `Builtin` never has more arguments than its arity: the
front end wraps excess arguments in an outer `App`.

### Data conversions

The real UPLC builtin inventory preserves nominal primitive types:
`iData : int -> Int`, `unIData : Int -> int`, `bData : bytes -> Bytes`,
and `unBData : Bytes -> bytes`. Collection constructors and destructors
similarly preserve their Big element types. Existing `Data` constructors
and patterns expose universal Data shapes.

`Primitive.coerce : 'a -> 'b` is a separate compiler intrinsic, not a real
Plutus builtin. It accepts any value types independently, including
functions, without representation constraints. Applied coercions lower to
runtime identity with no validation, traversal, or representation change;
the first-class intrinsic behaves as an identity function. It does not add
an entry to the real builtin inventory.

`FromData.fromData` is implemented as this unchecked identity through an ordinary
blanket impl for every Big type, without validation constraints. It does not check
even the outer Data shape; malformed data fails only if a later operation
needs that shape. `Validate.validate` is a required source method on a separate opt-in trait.
Int and Bytes validation matches the Data shape then coerces the original
value; List and Map retain recursive source validation. `ToData.toData` also
uses `Primitive.coerce`: its ordinary blanket impl covers every Big type
and preserves the runtime value without reconstruction or traversal. There are no generated
validation checkers. Ordinary identity remains a Nash function; `fail` is
language syntax, not an entry in the builtin table.

## Pipeline

```
Can AST + solved types + trait evidence
   │ 1. monomorphization worklist         (Can -> Core, one instance per key)
   │ 2. trait methods -> impl bodies      (folded into 1)
   │ 3. pattern matching -> decision trees
   │ 4. desugar do / records / tuples / lists   (folded into 1 and 3)
   ▼
Core with LetRec
   │ 5. hygiene + static lifting + ANF + main optimization (Plan 08; optimized path only)
   │ 6. recursion rewrite                 (LetRec -> self-application/dispatch)
   │ 7. freshen generated binders         (O1; no second ANF pass)
   ▼
Core
   │ 8. Core -> Term<Name> -> Term<DeBruijn> -> Program
   ▼
UPLC
```

Phases 1–4 run in one traversal in `nash-codegen` (`Can -> Core`), phase 6
is a Core pass in `nash-codegen`, phases 5 and 7 are planned in `nash-ir`, phase 8 in
`nash-codegen`.

### 1. Monomorphization worklist

Input: the Can `Module`s of the build (`crates/nash-ast/src/lib.rs`), the
solved annotation per top-level value (`nash_can::Annotations`, produced by
`nash_solve::run` in `crates/nash-solve/src/solve.rs:22`), and the per-use
instantiation types and evidence that [plans/03-traits.md](../plans/03-traits.md) adds to the solver
output (each `Expr::VarTopLevel` / `VarForeign` / `VarOperator` /
`Binop` occurrence gets its instantiated ground type and a slice of resolved
impls).

The worklist identifies a specialization by:

1. The definition's module and lexical node identity. Local shadowed names
   must not share an identity. `Scheme.binder` identifies the evidence owner;
   it does not replace the individual definition identity in recursive groups.
2. Its executable trait evidence, after substituting `Given` and resolving
   `Super`. Impl references and prerequisite implementations select method
   bodies. There are no runtime dictionary parameters or runtime impl searches.
3. The runtime layout descriptors demanded by that body and its callees.
   Complete source type arguments remain available as substitution metadata,
   but do not all participate in key equality or hashing.

Layout demand propagates through calls to a fixed point. It includes native
constant construction and representation-dependent builtins. In particular an
empty builtin list requires its complete UPLC element type: `list int` and
`list Int` cannot share the same empty-list constant. A mere `Const`/`Big` tag
is not enough for nested native lists and pairs. Little constructor tags and
arities do not depend on the types of their opaque fields.

`Evidence::Repr` is checked by the solver and erased; its full nominal type
must not automatically create a specialization. `ReflexiveLift` denotes
identity and `StructuralEq` denotes structural Data equality. Their type
payloads likewise distinguish copies only if some separate operation demands
that layout. Impl type arguments are projected onto the selected code's demands.
Source locations never affect specialization identity.

This permits one body for trait-free polymorphic recursion through little
constructors, such as `plain : 'a -> unit; plain x = plain (Some x)`.
Opaque binders use `Ty::Erased`; no runtime operation inspects that marker.
The solver's existing rejection of recursively growing executable evidence
remains in force. It is not a proof that layout specialization terminates:
a recursive call can require successively deeper native constant types.
Codegen must diagnose growing demanded layouts rather than loop, silently
merge incompatible layouts, or introduce runtime type/dictionary arguments.

Starting from a selected root (`main`, a test body, or a `comptime` subterm),
request each required specialization once, substitute its compile-time
metadata, rewrite trait methods to their selected implementation, and follow
reachable callees. Assign deterministic internal names from traversal order;
source spelling alone is not identity. The existing union-find solver remains
unchanged as the inference representation.

### 2. Trait method calls

A use of a trait method `Ord.compare` at type `int` with evidence
`Impl { impl_: Ord int, type_args: [], args: [] }` becomes
`Var(compare#Ord#int)`, whose definition is the impl's method body
instantiated at `type_args` (or the trait's default method body with the
impl's evidence substituted). `args` supplies the evidence for the impl's
own context (`impl Eq 'a => Eq (list ('a : Little))`), and a `Super` node (`lt` using
`Eq` through `Ord`'s superclass) resolves to the superclass impl through
the impl table. After this phase there are no dictionaries and no trait
names in `Core`.

### 3. Pattern matching

`Expr::Case`, `Expr::LetDestruct`, multi-clause function arguments and
lambda argument patterns go through the ordered pattern matrix in
`crates/nash-codegen/src/decision_tree.rs`.

- Bind the subject once, strictly. Variable and alias patterns record bindings.
- Select the first non-irrefutable pattern in the first remaining row. Literal
  patterns call the selected trait matcher and preserve source priority.
- For structural patterns, specialize rows for every known constructor signature.
  A wildcard contributes an ignored field pattern for each field of that
  constructor. Compile each specialized matrix recursively.
- Count how many leaves reach each source branch body. A body used once stays
  inline. A body reached multiple times becomes a shared helper: no pattern
  bindings means `delay body` and `force helper`; bindings mean a lambda over
  those bindings, applied at each leaf. These are generated branch helpers,
  not source continuations. Bind them inside the strict subject binding so
  caller captures remain in scope.
- Big field extraction follows the selected path and only extracts demanded
  fields. The accessor-sharing pass also reuses projections within their valid
  scope; it does not move a failing decode out of a lazy branch.

Structural dispatch lowers to the `Case` kind matching the scrutinee's `Ty`:

| Scrutinee representation / type | `Case` kind | Test |
|---|---|---|
| little ADT (`Term`) | `Tag` | UPLC `case` on the constr |
| `bool` | `Bool` | native `case` (false 0, true 1) |
| Source literal patterns | `Bool` on the selected trait matcher result | ordered equality tests; no automatic integer dispatch |
| Internal Core `Int`, `Bytes` cases | `Int`, `Bytes` | equality chain |
| `list 'a` | `List` | native `case` (cons 0, nil 1) |
| Big ADT | `Tag` on the integer decoded by `unConstrData` and pair destructuring | native `case`; no dispatch for single-constructor types |
| `Data` | `Data` | `chooseData` with delayed branches |
| `List 'a` | `List` after `unListData` | native `case` (cons 0, nil 1) |
| Big record | none (irrefutable) | `unListData` and field projection |

Exhaustiveness is checked earlier by `nash-nitpick` (Elm's
`Nitpick/PatternMatches`), so `default` is `None` for a complete match and the
tree never needs a compiler-generated fallthrough. When a match is not
exhaustive the front end has already reported an error.

### 4. Desugaring

Handled inline while building `Core`:

- `do` blocks are `Monad.bind` chains (see [syntax.md](syntax.md)); they
  reach codegen as ordinary calls and resolve through phase 2.
- Records: a Big record literal is `Builtin(ListData, [cons chain])`; a
  little record literal is `Constr(0, fields)`. Field access is
  `Field(r, i)` for little records and
  `Builtin(HeadList, [tail^i (unListData r)])` for Big records. Record update
  binds the base once. Big updates rebuild through the last changed field
  and reuse the unchanged list suffix; little updates rebuild every field.
  `Expr::Accessor` becomes a
  `Lam`.
- Tuples are `Constr(0, ...)` and `Field`.
- `Expr::List` is a `Const` list: a literal of constants is one `Lit`; a
  list with computed elements is a `mkCons` chain onto a `Lit` nil of the
  right element type. A `List 'a` literal (Big) is the `Const` list wrapped
  in `Builtin(ListData)`.
- `Expr::If` with several branches is nested `Case(Bool)`.
- Negation has no node of its own: canonicalization turns `-e` into a
  `Num.negate` method call ([plans/03-traits.md](../plans/03-traits.md)),
  which phase 2 resolves to the impl's body (`subtractInteger 0 e` for
  `int`).
- `Expr::Unit` is `Lit(Unit)`. `Expr::Str` is a `Const` `string` literal
  when the solved type is `string`, and a `bytes` literal when it is `bytes`
  (see literal defaulting in [traits.md](traits.md)).

### 5. Recursion

`Decls::DeclareRec` and `Expr::LetRec` are the only sources of recursion;
canonicalization already computed the SCCs.

**Self recursion** uses self-application, ported from Aiken's
`modify_self_calls` and `identify_recursive_static_params`
(`crates/aiken-lang/src/gen_uplc/builder.rs:256-408`) and the
`FunctionVariants::Recursive` lowering (`gen_uplc.rs:4607`):

1. Walk the body. A parameter is *static* if every self call passes it
   through unchanged and the function is never used other than as the head
   of a call (`calls == usages`). Otherwise it is *non-static*.
2. Rewrite each self call `f a1 .. an` to `(f f) [non-static args]`.
3. Emit
   ```
   f = \static.. -> (\f -> (f f) nonstatic..)          -- outer, all params
                       (\f nonstatic.. -> body')       -- inner, self-applying
   ```
   With no static params the outer wrapper collapses to
   `f = (\f -> f f) (\f nonstatic.. -> body')`, and a function with no
   parameters gets a `Delay`/`Force` pair so the self-application does not
   loop at definition time.

The accepted Plan 08 implementation performs static-parameter lifting separately in
`nash-ir::static_lift::lift`, before ANF. It examines complete self calls, captures
unchanged parameters in the original function wrapper, and creates an explicit
recursive worker taking only the remaining parameters. The wrapper keeps its
original arity, so partial calls from outside the recursive body retain their
behavior. Genuine partial or first-class self uses still prevent lifting, as in
the existing analysis; no ANF call-chain recognition is needed.

If every parameter is static, the worker is a singleton delayed recursive binding:
`letrec worker = delay body in force worker`. Its binder and body have the same
`Delay(result)` type, and each recursive call forces the worker anew. It is not a
memoized result. Final recursion rewriting supports this explicit delayed form;
ordinary recursive values and zero-parameter mutual groups remain unsupported.
The original integrated lifting path remains in O0 for baseline comparison while
the accepted extracted pass runs before ANF in production O1.

**Mutual recursion** uses native UPLC constructor packets and case dispatch:

```text
dispatch = \request -> case request [
    \self a.. -> bodyA',
    \self b.. -> bodyB'
]
A a.. = dispatch (constr 0 [dispatch, a..])
B b.. = dispatch (constr 1 [dispatch, b..])
```

Inside a branch, a saturated recursive call directly invokes
`self (constr target [self, args..])`. Each branch binds its own self and
parameter list, so functions may have different arities. Function values and
partial applications use curried wrappers that build the packet once fully
applied. Over-applied calls apply remaining arguments to the dispatch result.
Only the selected function body executes; lexical captures remain in scope.
Single-function recursion retains its self-application/static-parameter pass.

No Y combinator is ever emitted.

### 6. Optimizations

Plan 08's accepted shared-analysis infrastructure lives in `nash-ir::analysis` and
`nash-ir::hygiene`; assembly enables no new transformation. Occurrence reports
resolve lexical bindings by numeric name ID and record execution boundaries.
Hygiene checks diagnose duplicate IDs and out-of-scope uses; substitution renames
binders in the recipient and each inserted copy to avoid capture. Callers must
still prove that a proposed rewrite preserves evaluation and effects. Structural
size counts nodes/binders, not literal payload or serialized bytes.

The accepted pipeline begins with binder hygiene, static-parameter lifting, then
A-normal form (ANF). Lifting retains recursive `LetRec` workers and captures the
unchanged parameters before ANF can split complete calls into partial ones. Main
optimization runs before recursion rewriting. Normalize once only: generated
recursive code lowers directly, without a second ANF pass or ANF-dependent cleanup.
Freshen binder occurrences again after recursion
rewriting, which can share generated lambda subtrees at several use sites.
Assembly coordinates these phases. Before recursion rewriting, refresh recursive
groups. Discovering additional static parameters after the main optimizations is
a separate future candidate; the initial lifting pass needs no ANF call-chain
recovery. Do not repeatedly unfold recursive calls
or generated self-application. Post-rewrite Core may contain nested applications;
lowering accepts these. Any future cleanup there must explicitly support nested
Core or UPLC. O0 still performs required recursion rewriting.
Non-atomic intermediate operands receive explicit bindings; variables/literals
can stay inline. Existing Core nodes are reused. Main optimization passes preserve ANF and
strict evaluation order, including application staging and trace timing; they
must not move work across case-branch, lambda, or delay boundaries without a
separate semantic justification. O0 remains the unnormalized baseline. See
[Plan 08 chunk 2](../plans/08-optimizer.md#chunk-2--static-parameter-lifting-and-anf-normalization).

The accepted ANF implementation uses `nash-ir::anf::{normalize, is_atom, validate}`. Variables,
literals, nonempty lambdas, delays and bare builtin references are atoms; lambda
and delay bodies still normalize in their own scopes. An empty lambda lowers to
its body and is not a value boundary. General applications retain atomic argument
runs, binding an earlier application stage before lifting a later computation.
Known builtin arguments normalize in order; valid builtin nodes cannot exceed
arity and only execute on saturation. Trace message work precedes emission, while
body work remains inside the trace. Administrative lets reassociate only with
unique binders, and new binders retain the computation's actual result type.
The accepted pipeline is shared by production O1, snapshots and measurements.

Plan 08 rule 1 is accepted in `nash-ir::propagate`: remove variable aliases,
then propagate literals with at most one remaining use. Integers, byte strings
up to 64 bytes inclusive, and BLS constants may also be duplicated at multiple
uses. Count uses after alias removal to retain other shared payloads. Computed, lambda, delay and builtin bindings
stay bound. This pass runs in O1 before recursion rewriting;
production assembly remains unchanged.

Rule 2 is accepted in `nash-ir::beta`: direct lambda applications become
strict parameter bindings, with partial and oversaturated application staging
preserved. Local binding splicing preserves ANF. `beta::simplify` repeats rules
1 and 2 until neither changes Core, using unchanged-pointer preservation rather
than a node-count comparison. This loop currently runs in the test pipeline.
Computed results of oversaturated calls can introduce an extra ANF binding;
measured tradeoffs are recorded in Plan 08.

Rule 3 is accepted in `nash-ir::single_use`. It substitutes ANF values at a
single use and removes immediate computed return bindings (`let x = rhs in x`).
Forced builtin references are excluded: their bindings remain shared even at one
use or in direct returns. Plan 08 requires one shared binding per forced builtin
at the validator's outermost scope, outside its argument lambdas (or the outermost
program scope for other entry points). The optimized lowering entry point now
implements that placement.
Other computed bindings remain at their evaluation points, including computed
function operands. This keeps ANF and effect order without a general effect-motion
analysis. The O1 cleanup loop composes it with accepted rules 1 and 2.

Rule 4 is implemented in `nash-ir::small_inline`. It selects nonrecursive let-bound
identity lambdas and wrappers containing one saturated builtin with only variable
operands or literals already approved for duplication by rule 1. Only fully
applied direct calls are copied; each copy gets fresh parameters and existing
beta reduction preserves strict argument evaluation. Partial and escaping uses
remain shared. Conditional bodies and broader size heuristics are deferred.
`small_inline::simplify` composes rules 1–4, safe dead-binding removal, recursive
reachability, representation/force-delay cancellation and known Boolean/literal
folding to a fixed point in the shared production O1 pipeline.

Chunk 5 force sharing is implemented in `lower::lower_with_builtin_sharing`.
During lowering, references to each builtin requiring forces share one fresh
name, including builtins introduced by Data-case and Trace lowering. The whole
UPLC root is wrapped in bindings of the fully forced builtin values, outside all
validator argument lambdas. Applied arguments and computations stay in place.
Only references surviving lowering are bound; exhaustive case defaults can be
discarded. This also shares across erased type instantiations without changing
Core typing or repeating ANF. Core cleanup runs before these bindings exist.
The accepted pipeline composes this with the constant-prefix sharing below;
O0 uses `lower`; O1 uses `lower_with_constant_sharing`.

The accepted pipeline uses `lower::lower_with_constant_sharing` for Chunk 5
steps 1 and 2. The two-occurrence minimum is accepted, including startup costs.
It shares repeated first literal arguments of known builtins with arity greater than one. It counts surviving UPLC
occurrences, binds closed partial values outside the root, and keeps forced
references outside those bindings. It does not move later arguments or saturated
calls, reorder operands, or share longer prefixes. Its before/after semantic
snapshots and explicit experiment compare with force sharing alone. The explicit
performance baseline includes both sharing steps.

Candidate optimizations in `plans/08-optimizer.md` are reviewed one chunk or
one rewrite at a time. Implement and measure a concrete candidate, then wait for
the user's keep/revise/discard decision before advancing. Small functions used
multiple times are eligible for consideration; code duplication must be measured.

- Inlining, atom/alias propagation and binding cleanup.
- Builtin force sharing and repeated constant partial applications.
- Dead bindings, unreachable recursive members and unused parameters.
- Known-case and field simplification, preserving strict ignored fields.
- Valid inverse representation conversions and force/delay cancellation.
- Bounded CEK evaluation of safe constant builtin calls.
- Single-field native pair case versus `fstPair`/`sndPair`, including measurements
  with and without shared builtin forces.

Each retained pass preserves ANF, results, trace order, failures and termination.
Compose accepted passes to a structural fixed point; equal node counts are not
proof of convergence. Test idempotence and preserve O0 baselines. Each optimization
snapshot contains both before and after at the representation it transforms:
Core for Core passes, UPLC for UPLC passes. Use named sections in one snapshot,
with downstream UPLC and evaluation sections where needed. Integration fixtures
follow the same sectioned format as unit fixtures, keeping Core, UPLC and outcomes
together. Recursion rewriting remains required even for O0.

Permanent performance regression cases and temporary per-chunk experiments use
an isolated, explicit performance runner outside root Cargo test discovery.
Ordinary `cargo test` and `cargo nextest run`, including workspace/all-features
runs, must not execute them. Record CPU, memory and serialized size; tradeoffs
are decided case by case, with no fixed metric priority. Baseline updates are
explicit. Temporary experiments are removed after review unless promoted into
permanent performance coverage. Ordinary semantic tests continue to run normally.

Optimization levels, flags and defaults are deferred until the accepted passes
have been evaluated. No automatic integer-dispatch heuristic or specialized
list/map Eq recognition belongs to this optimizer; those decisions remain with
explicit source operations and library trait implementations.

### 7. Lowering to UPLC

`Core -> Term<Name>` is a direct structural translation using the
constructors in `crates/nash-plutus/src/term.rs` (`Term::lambda`,
`Term::apply`, `Term::constr`, `Term::case`, `Term::builtin`, ...) into a
nash-plutus `Arena`. `Name` is `crates/nash-plutus/src/binder/name.rs`
(`text` + `unique`); the `unique` comes straight from the `Core` `Name`.
A separate step converts `Term<Name>` to `Term<DeBruijn>`
(`crates/nash-plutus/src/binder/debruijn.rs`) for evaluation and flat
encoding; `Program::new(arena, Version::plutus_v3(arena), term)` wraps it.

n-ary `Lam`/`App` become nested unary terms. A `Builtin(f, args)` becomes
`force^{force_count} (builtin f)` applied to the arguments; the force-caching
pass has usually already replaced the head with a variable.

## How each Nash type lowers

| Nash type | Representation | Runtime value | Build | Take apart |
|---|---|---|---|---|
| `int` `bytes` `string` `bool` `unit` | Const | constant | `Lit` | builtins |
| `list 'a` (`'a` Storable) | Const | `list t` constant | `mkCons` / `Lit []` | native `case` (cons 0, nil 1) |
| `pair 'a 'b` (Storable components) | Const | `pair t1 t2` (each Big component becomes `data`; each Const component keeps its builtin type) | `mkPairData` constructs pairs with Big components; `unConstrData` returns `pair int (list Data)` | `fstPair` `sndPair` |
| `array 'a` | Const | `array t` | `listToArray` | `indexArray` `lengthOfArray` |
| `bls_g1` `bls_g2` `bls_mlr` `value` | Const | constant | builtins | builtins |
| `Int` | Big | `data (I n)` | `iData` | `unIData` |
| `Bytes` | Big | `data (B bs)` | `bData` | `unBData` |
| `Data` | Big | `data` | any | `chooseData` with delayed branches |
| `List 'a` | Big | `data (List xs)` | `listData` | `unListData` |
| `Map 'k 'v` | Big | `data (Map kvs)` | `mapData` | `unMapData` |
| Big ADT `type Foo = A .. \| B ..` | Big | `data (Constr i fields)` | `constrData i fields` | `unConstrData`, pair case, tag case, list indexing |
| Big labeled ctor `type Datum = Datum { owner : Bytes, deadline : Int }` | Big | `data (Constr i [owner, deadline])` | `constrData i fields` | same as a Big ADT |
| Big record `type alias Foo = {..}` | Big | `data (List fields)` | `listData` | `unListData`, list indexing |
| little ADT `type foo = ..` | Term | `constr i [fields]` | `Constr` | `Case(Tag)` |
| little labeled ctor `type step = Next { n : int, rest : step }` | Term | `constr i [n, rest]` | `Constr` | `Case(Tag)`, `Field` |
| tuple, little record | Term | `constr 0 [fields]` | `Constr(0)` | `Field`, `Case(Tag)` |
| function | Term | closure | `Lam` | `App` |

`constrData` takes an `int` tag and a `list data` of fields, so a Big ADT
value is `Builtin(ConstrData, [Lit i, fields])` where `fields` is a `Const`
list of the (already Data) field values.

**Labeled constructor fields** (`Datum { owner : Bytes, deadline : Int }`,
Aiken style) are positional fields with names (`CtorArgs::Labeled`,
[representation.md](representation.md)): the labels exist only at
compile time, canonicalization rewrites labeled patterns and constructor
calls to wire-order positional form, and the encoding is flat, `Constr i [owner, deadline]` for a
Big type and `constr i [owner, deadline]` for a little one. There is no
nested record. On a single-constructor type `.owner` access lowers to the
same field extraction a pattern `Datum { owner }` produces, through the
same memoized accessor path. Only `type alias` records lower to a `List`. Elm's `CtorOpts::Enum` and
`CtorOpts::Unbox` (`crates/nash-ast/src/lib.rs:118`) are ignored for Big
types because the Data layout is the on-chain ABI. For little ADTs `Enum`
changes nothing (a nullary constructor is `constr i []`); `Unbox` is not
applied in v1 (see Open questions).

## Case on a Big ADT

```elm
case datum of
    Datum { owner, deadline } -> deadline
```

produces a direct constructor decode and pair destructuring, with no Data
variant check or constructor-tag test:

```
case@Pair (unConstrData datum) of
  Pair tag fields ->
    case@List fields of
      Cons _ rest -> case@List rest of
        Cons deadline _ -> deadline
```

The solved Big ADT type establishes that its representation is constructor
Data. Codegen therefore calls `unConstrData` directly; only matching the
unrestricted `Data` type uses `chooseData`. For multiple constructors, the
extracted integer tag dispatches with native `Term::case`, with all declared
tags represented. Single-constructor types require no tag dispatch. A
single-constructor type with no fields, including Big `Unit`, requires no
destructuring at all; the scrutinee is still evaluated strictly.

Unchecked casts do not add validation obligations to typed pattern matching.
Explicit `Validate` implementations and source matches on `Data` retain their
checks. Native casing directly on `Data.Constr` requires protocol 12 and is
not emitted by this protocol-11-compatible lowering.

Field extraction follows the selected branch and shares the decoded pair,
list tails, and field projections with subsequent accesses.
Only fields referenced by the compiled pattern branch are extracted. Ignored
fields do not get standalone projections; gaps of two or more use `dropList`
and adjacent required fields reuse the tail bound by the preceding list case. Big-list cons patterns reconstruct a Data-encoded
tail only when the branch uses it. A single-constructor pattern whose fields
are all ignored needs no decoding, but its scrutinee still evaluates strictly.

## Case on a little ADT

```elm
case step of
    Done a   -> a
    Next n a -> a
```

lowers to one UPLC `case`:

```
case step [ (\a -> a), (\n a -> a) ]
```

Each branch is a lambda over the constructor's fields in declaration order,
even if the branch ignores them. `Field(r, i)` is `case r [\f0 .. fn -> fi]`
and is what tuple projection and little-record access compile to.

### Wildcard filling and shared branch helpers

For `type choice = Stop | Zero | One int | Two int int`, a match with
`Stop -> explicit` and `_ -> fallback` supplies all four native case slots.
The little branches for `One` and `Two` consume respectively one and two
constructor fields before executing the fallback. This also holds when the
fallback returns a function: constructor fields must not accidentally become
arguments to that returned function.

A repeated fallback without pattern bindings is shared as a delay. With bound
variables, it is shared as a function receiving those variables. Only the
selected branch forces or calls the helper. A native case branch that is
already inline does not need another delay solely for branch laziness.

Big ADTs instead case on the decoded integer tag, whose branches receive no
implicit fields. Wildcard paths do not extract ignored Data fields, even if an
unchecked value lacks those fields. Matching is not validation.

An unknown tag fails when constructor dispatch is required: wildcard slots cover
known constructors only. A wildcard-only match has no dispatch and returns its
body after strictly evaluating the subject, even for an unchecked unknown tag.

Executed regression snapshots live in `build/tests/wildcard_cases.rs` (under
`crates/nash-codegen/src`). Every Big/little pair asserts both results and exact
logs; each snapshot records source, Core, UPLC, result, logs, and budget.

| Fixture suffix (Big and little) | Behavior exercised |
|---|---|
| `mixed_arities_trace_order` | Explicit branch and wildcard arities 0/1/2 all execute; subject then exactly one branch trace |
| `shared_failure_unselected` | Explicit branch does not force the shared failing fallback |
| `shared_failure_zero/one/two` | Each wildcard arity forces the fallback exactly once and fails |
| `calls_user_continuation_only_when_selected` | Caller-supplied function executes only on wildcard paths; an unselected failing continuation never runs |
| `inline_function_result_consumes_fields` | Single-use wildcard stays inline; little branches consume fields before returning the function, while Big branches ignore the payload |
| `returns_captured_function` | Returned function is applied by the caller, not to ignored constructor fields; captures outer value |
| `binds_whole_and_captures_outer` | Whole-subject binding reaches the shared helper correctly on every wildcard path |
| `multiple_bindings_return_function` | Two pattern bindings retain argument order through a shared helper that returns a closure |
| `shared_helper_receives_bound_values` | Each specialized path passes its own pattern-bound value to the shared helper |
| `unknown_tag_fails_dispatch` | Unchecked unknown tag cannot select a wildcard slot; fallback trace does not run |
| `only_does_not_inspect_unknown_tag` | Wildcard-only match evaluates the subject without inspecting its tag |

`big_wildcard_does_not_extract_missing_fields` additionally executes known tags
with missing ignored fields. The little unknown-tag fixtures explicitly coerce
another little ADT's out-of-range constructor; they do not use Data as a native
constructor representation.

## `if` on `bool`

`Case(Bool, c, [t, e])` lowers to native `case c [e, t]`: false selects
branch 0 and true selects branch 1. Native case evaluates only the selected
branch, so no branch delays or forces are needed. Sparse integer and bytestring
literal patterns still use equality tests, dispatching on each boolean result;
arbitrary integer values are not used as branch-array indexes.

## Records

Big record access unwraps with `unListData` and extracts fields through native
list cases. The accessor cache shares the unwrap and case-bound tails across fields
read in one scope. Sharing starts at the first evaluation and does not move
a possibly failing decoder ahead of effects or out of a branch, lambda, or
delay. Pattern-bound constructor fields also seed the accessor cache.
Little record access is `Field`. On a single-constructor
type with labeled fields, `r.x` is not a record access: it lowers to the
constructor's field extraction (a pair case on `unConstrData r`, then list
cases for Big, `Field` for little), exactly as the pattern
`Ctor { x }` does. Record update
`{ r | x = e }` becomes

```
let base = r
Constr(0, [Field base 0, e, Field base 2, ..])       -- little
case@List (unListData base) of                    -- Big
  Cons first rest -> case@List rest of
    Cons _ suffix -> listData (mkCons first (mkCons e suffix))
```

## Runtime errors and traces

| Surface | Core | Notes |
|---|---|---|
| `fail "msg"` | `Trace(Lit "msg", Error)` | message subject to trace level |
| `fail` | `Error` | |
| `todo "msg"` | `Trace(Lit "TODO: msg", Error)` | also a compile warning |
| `trace "msg" e` | `Trace(Lit "msg", e)` | |
| `assert c` | `Case(Bool, c, [Lit (), Trace(msg, Error)])` | `msg` is the power-assert rendering built at compile time (see [testing.md](testing.md)) |

Trace levels are a build setting (`--trace-level`, config `traceLevel`;
[cli.md](cli.md)):

- `silent`: every user `Trace(m, b)` becomes `b`; `fail "msg"` becomes
  `Error`.
- `compact`: the message is replaced by `Module:line:col` of the
  originating expression.
- `verbose`: the message is kept verbatim.

In `silent` and `compact` modes, the original message expression is not
evaluated: its function calls, failures, and nested traces do not run. In
`verbose` mode, the message expression is evaluated before the trace body.

Explicit `trace`, `fail`, `todo`, and `assert` messages follow this user trace
level. Implicit match failures use compiler traces; explicit codec failures
follow ordinary Nash failure behavior.

Compiler-generated traces (such as "incomplete pattern match") are controlled by a
separate boolean switch, `compilerTraces`, so a user can ship verbose user
traces without the compiler's, or the reverse. In Aiken both are one
`TraceLevel` (`crates/aiken-lang/src/ast.rs:2305`, used in
`gen_uplc.rs:445`).

Trace strings are hoisted: each distinct message becomes one top-level
`Let` of a `string` constant so the program does not repeat it.

Outside test blocks, failed assertions use an assertion-failure message.
The Plan 10 power-assert rewrite captures displayed values only in test bodies;
it does not rewrite validators.

## Validators

For `validator module Foo exposing (main)`, `main`'s arguments become
lambdas in order and its body is the program body. The caller applies
UPLC constants to the program, so an argument of `main` may have representation
`Big` (a `Data` constant, what the ledger passes) or `Const` (any other
UPLC constant, for parameterized scripts and tests). An argument with representation
`Term` (a function, a little ADT, a tuple) is a compile error, "nothing
outside the script can supply this", reported before codegen (see
[validators.md](validators.md) and [plans/09-validators-build.md](../plans/09-validators-build.md)).
No boundary conversion is inserted for either representation: a Big value *is* its
`Data`, and a Const value is the constant itself.
A V3 entry point `main : Data -> unit` lowers to `\ctx -> body`.
Its one context contains the redeemer and spending datum. Pattern matches inside
`body` are what check the shape; a `validate` call is the user's choice.

The compiler leaves the result type free, but the Plutus V3 ledger requires
a unit result. Nash does not convert a `bool` result into validation; `assert`
is the idiom for a condition. (Aiken
wraps the body in `wrap_validator_condition`, `builder.rs:1214`; Nash does
not.)

The program is `Program { version: 1.1.0, term }` for Plutus V3.

## Tests

Each `test` / `prop` in the `tests` block compiles to its own `Program`. The
module's `Core` bindings are built and optimized once (monomorphized from
the union of all test roots), and each test program is assembled from the
bindings reachable from its own body, so shared code is compiled once and
DCE is per program. A `test` compiles to one program of type `unit`:
success is no error. A `prop` compiles to one preparation program following the
runner protocol in [testing.md](testing.md):

```nash
prepare : prng -> option (prng, unit -> unit, unit -> list string)
```

Codegen produces ordinary source-pattern callbacks and calls Nash `Test.both`
and `Test.prepare`. The Base functions own generator sequencing, rejection,
result unpacking, tuple construction, and deferred body/display calls.
Native tuples retain function values and captures.
The runner saves the state before calling the body; it calls the display function
only when needed. Generation is not repeated after a body failure.

## Comptime hook

`comptime e` reaches codegen as a marked subterm. After monomorphization
the subterm is closed (it may reference top-level bindings, which are
included), so it is lowered on its own, evaluated with `Program::eval`, and
the resulting `Term::Constant` becomes a `Lit` in the enclosing `Core`.
Evaluation errors and non-constant results are compile errors. Constant
folding uses the same function on any closed `Builtin` subterm, so the
comptime hook is not a special path. Macros (see [macros.md](macros.md))
use the same CEK machine but not the constant rule: the `Ast` family is
`Term` representation, so the host applies the macro program to a `Term::Constr`
tree and reads the output `Ast` from the result `Value`, never through
`Data`.

## Interactions

- **Representations** ([kinds.md](kinds.md)): `Ty::repr()` decides every
  representation choice; codegen never inspects casing.
- **Traits** ([traits.md](traits.md)): evidence drives phase 2; literal
  traits (`FromInt` ...) resolve to ordinary Nash impl bodies, including
  direct Data builtin calls for Big literals.
- **Data** ([data.md](data.md)): ordinary source codecs and typed Data
  builtin signatures.
- **Nitpick**: exhaustiveness is assumed; decision trees have no
  fallthrough of their own.
- **Testing** ([testing.md](testing.md)): power-assert messages are built by
  the front end and reach codegen as string literals.
- **nash-plutus**: `Term`, `Constant`, `PlutusData`, `DefaultFunction`,
  `Program::eval`, flat encoding.

## Open questions

1. **`Unbox` for little ADTs.** Elm unboxes single-constructor,
   single-field types. Doing the same for a little ADT saves a `constr`
   allocation and a `case` per access. Deferred to after v1.
2. **nash-plutus lacks a `Term` pretty printer and `Name -> DeBruijn`
   conversion.** The syn module only parses. `plans/07-codegen.md` chunk 2
   adds both to nash-plutus (ports of Aiken `crates/uplc/src/pretty.rs` and
   `crates/uplc/src/debruijn.rs`).

## Generalized local values

A generalized binding that needs executable evidence or a concrete native layout
is a compile-time template. Each requested specialization supplies those inputs
and evaluates the resulting value in its lexical scope. An unused template has no
runtime value: codegen does not select an arbitrary literal instance or native
list element type merely to execute it. A generalized local value with no evidence
or layout demand can use an erased instance and retains strict let evaluation;
for example, an unused `let stopped = fail` still fails. Ordinary monomorphic
local bindings also retain strict evaluation.

## Build targets

Production assembly accepts a ledger target and validates generated UPLC against
the protocol 11 compatibility baseline described in [validators.md](validators.md#target-compatibility).
`assemble_core` remains the default V3 entrypoint; `assemble_core_for_version`
emits UPLC 1.1.0 for V1, V2, and V3 and checks the complete program.
O1 is the build/test default; O0 remains available (required recursion rewriting then ordinary lowering).
`assemble_core_with_options` also supports O1: shared `optimizer::optimize`
performs freshening, static lifting, unused-parameter removal, direct constructor
folding and representation inverse cancellation, then one ANF normalization and accepted
cleanup/known-case passes. Recursion is encoded afterwards, binders are freshened,
and forced-builtin/constant-prefix sharing runs during lowering. O1 preserves
all traces generated by the selected trace settings; it does not strip traces.
Snapshot and performance pipelines reuse this production optimizer. Explicit
`comptime` evaluation retains O0 and its existing budget semantics.
Core traversal, scope-aware rewriting, ANF, recursion encoding, lowering and UPLC
conversion/validation use explicit heap work lists rather than depth-dependent
Rust calls. Assembly does not enlarge the thread stack. See the
[stack audit](research/compiler-stack-audit.md) for remaining frontend, diagnostic
and nested-type risks.

### Field offset extraction

Big record and constructor fields are extracted with native list cases. Each
case binds a head and tail together. Within an evaluation scope, each tail
retains its original list and offset; repeated fields reuse the bound head.
Later reads start from the nearest available preceding tail. A remaining gap
of one uses a list case; gaps of two or more use `dropList`, followed by a case
to bind the selected field. This favors memory for skipped spans while sharing
both outputs for adjacent reads. See [measured costs](research/list-extraction-costs.md).
Constructor pattern bindings participate in the same cache. Pair destructuring
uses native pair cases and shares both components.

For a typed Big record, `unListData` retains the declared field count in
the accessor cache. Every offset below that count is known nonempty from
the record layout, even before any field is read. Adjacent update suffixes
therefore use a list case from an available tail. Unchecked coercion does not
require record access to revalidate this layout.

Explicit source calls to `headList` and `tailList` remain builtin calls unless
an existing case binding already supplies the result. Arbitrary lists have no
declared length: a remaining `dropList 1` keeps its saturating behavior unless
an earlier read or Cons match proves the current tail nonempty. Case prefixes
preserve source evaluation order and stay inside their branch, lambda, trace,
or delay scope.

Big record updates evaluate the base once, rebuild the prefix through the
last changed field, and attach the original suffix. Unchanged suffix fields
are neither extracted nor rebuilt. Replacing every field requires no base
decoder. Updates preserve declaration-order evaluation of replacement
expressions and retained prefix fields, including traces before decoding.
This is not validation: for malformed records obtained through unchecked
coercion, an untouched suffix retains extra fields and is not checked for
missing fields.

Single-constructor labeled types also support record updates. The base evaluates
once, and replacement expressions and retained fields follow declaration order.
Big labeled types reconstruct `constrData 0`; little labeled types reconstruct
native `constr 0`. Alias-record suffix sharing does not change this constructor
encoding.

When a source Data pattern ignores its payload, `chooseData` selects the branch
without calling its unwrapper. A used payload still invokes the corresponding
`un*Data` builtin. This does not remove explicitly written discarded calls in
Nash validation implementations.

Chunk 6 removes unused nonrecursive lets when `analysis::safe_to_discard` proves
the RHS terminates without trace or failure. Repeating cleanup releases dead
captures and aliases while preserving strict effectful arguments exposed by beta
reduction. It runs before recursion rewriting; no second ANF pass is added.
The cleanup loop also runs `dead_code::prune_recursive`,
rooted in continuation references and their transitive member dependencies. It
retains source order and existing metadata. Repeating cleanup releases newly
unused safe captures while preserving effectful initializers. The accepted pre-ANF
`unused_params::reduce` pass removes unused parameters from nonrecursive
let-bound lambdas only when all uses are exact direct calls. It preserves strict
argument evaluation before or after ANF: each non-atomic argument gets a
call-local strict binding in source order, including discarded arguments. It
uses Delay/Force when every parameter is unused. The pass runs after static
lifting and before the single ANF normalization, followed by accepted cleanup.
Partial/staged/escaping/oversaturated uses and recursive signatures are unchanged;
these accepted passes run in production O1.


Chunk 8 adds accepted `force_delay::reduce` cleanup for direct
`force (delay body)` cancellation. The body stays at the force's evaluation point
and keeps its outer type view. It does not cancel `delay (force x)`, move argument
work, or duplicate shared delayed bodies. Existing cleanup can flatten exposed
lets without another ANF pass. Cancellation runs in the accepted fixed-point
loop before beta cleanup.


Plan 08 Chunk 6 also requires recursive unused-parameter removal. This is pending
design and implementation, including self/mutual forwarding dependencies, strict
argument evaluation and consistent worker/static-parameter metadata. The current
accepted nonrecursive pass does not implement that scope.


Chunk 7 includes accepted `known_case::reduce_bool` cleanup. A literal Boolean subject
selects its matching branch or default while retaining the case result type.
Malformed Boolean tables, unmatched cases without defaults and nonliteral
subjects stay unchanged. It runs inside the accepted fixed-point loop after
force/delay cancellation and before beta cleanup. This lets cleanup expose new
literal subjects and remove dead branch helpers, without another ANF pass.
Production O1 includes these accepted passes.

Accepted native-constructor folding (`known_case::reduce_constr`) runs after
static lifting and unused-parameter removal, before the single ANF pass. It
selects a direct `Constr` subject's matching `CaseKind::Tag` branch and replaces
field binders with strict lets in original field order. Ignored fields still
evaluate. Malformed tables, absent tags, arity mismatches and non-direct subjects
stay unchanged. No constructor fact propagation is included.

Representation cancellation (`inverse::reduce`) runs both before ANF and inside
the main cleanup loop. It cancels integer/byte/list/map Data round trips and
UTF-8 round trips only with runtime-shape evidence; reverse conversions require
the precise Data variant or valid UTF-8. Let-bound operands stay at their original
evaluation point and only variables are reused. Constructor Data projections and
reconstruction reuse variables through retained strict producer bindings, keeping
all tag/field/variant checks. Unknown unary shapes stay unchanged. See Plan 08
Chunk 8 for exact retained rules and the zero-regression measurements.
