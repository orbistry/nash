# Plan 10 — `tests` blocks, power-assert, property testing, `nash test`

## Goal

A module can end with a `tests` block of `test` and `prop` declarations.
`nash test` compiles each one into a standalone UPLC program, runs it on the
CEK machine, shrinks property counterexamples with choice-sequence shrinking,
and reports budgets, labels, traces and power-assert layouts.

Spec: [docs/testing.md](../docs/testing.md).

## Prerequisites

- plans/01 (syntax): parser support for `tests`, `test`/`prop`, modifiers,
  `via` binders and `do` blocks. The source AST types this plan consumes are
  listed in Chunk 1; if plans/01 chose different names, rename here.
- plans/03 (traits): `Show`, `Functor`/`Applicative`/`Monad` traits, evidence
  resolution (`nash_solve::Evidence`), and monadic `do` desugaring.
- [plans/07-codegen.md](07-codegen.md): the `SolvedTypes` / `NodeId`
  contract (Chunk 3: per-node solved types and instances; the driver gets
  them from `nash_solve::run(bump, uf, constraint, tables, fields, mode) ->
  Result<(Annotations, SolvedTypes), Vec<Error>>`, plans/03 Chunk 5
  "Solver API"), the `Build<'a>` codegen context
  (Chunk 9), and `program::assemble` / `Compiled` (Chunk 11, which
  compiles each test as a separate program; Chunk 4 below extends it with
  the `draw`/`run` pair for props).
- plans/09 (validators): `CompileMode::Test`, `Solved`, `build_with`.
- `core/` stdlib (plans/12): `Fuzz` module with `Prng`, `fuzzer`, `choice`;
  `Test` with `label` and `assertFailed` (stdlib.md). `assert`, `fail`,
  `todo` and `trace` are keywords with their own `Expr` nodes (plans/01
  Chunk 4), not stdlib functions.

## Crates touched

`nash-source` (consume), `nash-ast`, `nash-can`, `nash-constrain`,
`nash-codegen`, `nash-plutus` (small), `nash-driver`, `nash-cli`, and the
new `nash-test`.

## Elm / Aiken references

- Aiken `crates/aiken-lang/src/test_framework.rs`: `Test`, `UnitTest::run`
  (224-265), `PropertyTest::run` / `run_n_times` / `run_once` (359-491),
  `Prng` (640-792), `Counterexample::consider` / `simplify` /
  `binary_search_replace` / `replace` (806-1048), `Cache` (1061-1122),
  `TestResult` (1131-1201).
- Aiken `crates/aiken-lang/src/ast.rs` `OnTestFailure` (261-265).
- Aiken `crates/aiken-project/src/lib.rs` `run_runnables` (1146-1196) for the
  rayon loop; `crates/aiken-project/src/telemetry/terminal.rs` `fmt_test`
  (367-697) for the report layout.
- Aiken `crates/aiken/src/cmd/check.rs` (59-90) for the `--seed`,
  `--max-success`, `--match`, `--exact-match` flags.
- Elm `compiler/src/Canonicalize/Expression.hs` `canonicalize` for how new
  expression forms are added; `Type/Constrain/Expression.hs` for constraint
  shapes.

---

## Chunk 1 — AST types and canonicalization of the `tests` block

**Files**

- `crates/nash-source/src/lib.rs` (consume; plans/01)
- `crates/nash-ast/src/lib.rs`
- `crates/nash-can/src/module.rs`
- `crates/nash-can/src/tests.rs` (new)
- `crates/nash-can/src/error.rs`

**Source AST (from plans/01, `crates/nash-source/src/lib.rs`)**

```rust
// nash-source, exactly as plans/01 defines them
pub struct Tests<'a> {
    pub imports: &'a [&'a Import<'a>],
    pub tests: &'a [&'a Located<Test<'a>>],
}

pub struct Test<'a> {
    pub name: &'a Located<&'a str>,
    pub expect: Expect,
    pub budget: Option<Budget>,
    pub body: TestBody<'a>,
}

pub enum TestBody<'a> {
    Unit(&'a Block<'a>),
    Prop { binders: &'a [&'a Located<ViaBinder<'a>>], body: &'a Block<'a> },
}

/// A `do` block: statements then a final expression.
pub struct Block<'a> {
    pub stmts: &'a [&'a Located<Stmt<'a>>],
    pub last: &'a Located<Expr<'a>>,
}

pub struct ViaBinder<'a> {
    pub pattern: &'a Located<Pattern<'a>>,
    pub fuzzer: &'a Located<Expr<'a>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expect { Pass, Fail, FailOnce }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Budget { Cpu(i128), Mem(i128), Both { cpu: i128, mem: i128 } }

// general `do`, plans/01:
pub enum Expr<'a> {
    // ...
    Do(&'a Block<'a>),
}

pub enum Stmt<'a> {
    /// `let` defs with no `in`; scope is the rest of the block.
    Let(&'a [&'a Located<Def<'a>>]),
    Bind { pattern: &'a Located<Pattern<'a>>, expr: &'a Located<Expr<'a>> },
    Expr(&'a Located<Expr<'a>>),
}
```

The parser already rejects `fail once` on a `test` (`Test::OnceOnUnitTest`),
a `prop` without `via` binders, and a `do` whose last statement is not an
expression (`Do(LastNotExpr)`), so canonicalization only checks what the
parser cannot: duplicate names and irrefutable `via` patterns.

**Canonical AST (this chunk)**

```rust
// crates/nash-ast/src/lib.rs
pub use nash_source::{Budget, Expect};

pub struct Module<'a> {
    // ... fields from plans/09 ...
    pub tests: &'a [&'a Located<Test<'a>>],
}

pub struct Test<'a> {
    pub name: &'a Located<&'a str>,
    pub expect: Expect,
    pub budget: Option<Budget>,
    /// Empty for a unit test, non-empty for a prop.
    pub binders: &'a [Via<'a>],
    /// Already desugared: sequencing `do` became nested `Let`/`LetDestruct`.
    pub body: &'a Located<Expr<'a>>,
}

impl Test<'_> {
    pub fn is_prop(&self) -> bool {
        !self.binders.is_empty()
    }
}

pub struct Via<'a> {
    pub pattern: &'a Located<Pattern<'a>>,
    /// Source text of the pattern, for the counterexample report.
    pub text: &'a str,
    pub generator: &'a Located<Expr<'a>>,
}
```

No new `Expr` variant is needed for test bodies: the sequencing `do`
desugars to existing `Let` and `LetDestruct` nodes
(`crates/nash-ast/src/lib.rs:165-177`). The monadic `do` from plans/03
desugars to `Call`s of `Monad.bind`.

**Canonicalization** (`crates/nash-can/src/tests.rs`)

Runs after `canonicalize_decls` in `canonicalize`
(`crates/nash-can/src/module.rs:73`) with an environment that is the module
environment extended by the block's imports:

```rust
pub fn canonicalize_tests<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    interfaces: Option<&'a BTreeMap<&'a str, Interface<'a>>>,
    tests: &'a SourceTests<'a>,
    warnings: &mut Vec<Warning<'a>>,
) -> Result<&'a [&'a Located<Test<'a>>], Vec<Error<'a>>> {
    let env = environment::foreign::extend_with_imports(bump, env, interfaces, tests.imports)?;
    let mut seen: BTreeMap<&str, Region> = BTreeMap::new();
    let mut out = BumpVec::with_capacity_in(tests.tests.len(), bump);
    let mut errors = Vec::new();

    for decl in tests.tests {
        if let Some(first) = seen.insert(decl.value.name.value, decl.region) {
            errors.push(Error::DuplicateTest { name: decl.value.name.value, first, second: decl.region });
            continue;
        }
        match canonicalize_test(bump, &env, decl, warnings) {
            Ok(test) => out.push(&*bump.alloc(Located::at(decl.region, test))),
            Err(errs) => errors.extend(errs),
        }
    }
    if errors.is_empty() { Ok(out.into_bump_slice()) } else { Err(errors) }
}

fn canonicalize_test<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    src: &'a str,
    decl: &'a Located<SourceTest<'a>>,
    warnings: &mut Vec<Warning<'a>>,
) -> Result<Test<'a>, Vec<Error<'a>>> {
    let d = &decl.value;
    let (source_binders, block): (&[_], &Block<'a>) = match &d.body {
        TestBody::Unit(block) => (&[], block),
        TestBody::Prop { binders, body } => (binders, body),
    };

    let mut binders = BumpVec::with_capacity_in(source_binders.len(), bump);
    let mut body_env = env.clone();
    for b in source_binders {
        let generator = expression::canonicalize(bump, env, b.value.fuzzer, warnings)?;
        let pattern = pattern::canonicalize(bump, &body_env, b.value.pattern)?;
        body_env.add_pattern_locals(bump, pattern)?;
        binders.push(Via { pattern, text: &src[b.value.pattern.region.byte_range(src)], generator });
    }

    let body = desugar_sequence(bump, &body_env, block.stmts, block.last, warnings)?;

    Ok(Test {
        name: d.name,
        expect: d.expect,
        budget: d.budget,
        binders: binders.into_bump_slice(),
        body,
    })
}

/// `do { e; rest }` => `let () = e in rest`; `do { x <- e; rest }` => `let x = e in rest`.
fn desugar_sequence<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    stmts: &'a [&'a Located<Stmt<'a>>],
    last_expr: &'a Located<SourceExpr<'a>>,
    warnings: &mut Vec<Warning<'a>>,
) -> Result<&'a Located<Expr<'a>>, Vec<Error<'a>>> {
    let mut env = env.clone();
    let mut prefix: Vec<(&'a Located<Stmt<'a>>, Bound<'a>)> = Vec::with_capacity(stmts.len());
    for stmt in stmts {
        let bound = match &stmt.value {
            Stmt::Expr(e) => Bound::Unit(expression::canonicalize(bump, &env, e, warnings)?),
            Stmt::Bind { pattern, expr } => {
                let value = expression::canonicalize(bump, &env, expr, warnings)?;
                let pattern = pattern::canonicalize(bump, &env, pattern)?;
                env.add_pattern_locals(bump, pattern)?;
                Bound::Pat(pattern, value)
            }
            Stmt::Let(defs) => {
                let defs = expression::canonicalize_let_defs(bump, &mut env, defs, warnings)?;
                Bound::Let(defs)
            }
        };
        prefix.push((stmt, bound));
    }
    let mut body = expression::canonicalize(bump, &env, last_expr, warnings)?;
    for (stmt, bound) in prefix.into_iter().rev() {
        let region = Region::merge(stmt.region, body.region);
        let expr = match bound {
            Bound::Unit(value) => Expr::LetDestruct {
                pattern: bump.alloc(Located::at(stmt.region, Pattern::Unit)),
                value,
                body,
            },
            Bound::Pat(pattern, value) => match &pattern.value {
                Pattern::Var(name) => Expr::Let {
                    definition: bump.alloc(Def::Def { name: bump.alloc(Located::at(pattern.region, *name)), args: &[], body: value }),
                    body,
                },
                _ => Expr::LetDestruct { pattern, value, body },
            },
            Bound::Let(defs) => expression::wrap_let(bump, defs, body),
        };
        body = bump.alloc(Located::at(region, expr));
    }
    Ok(body)
}
```

`Pattern::Unit` exists (`crates/nash-ast/src/lib.rs:234`), so `let () = e`
types `e : unit` through ordinary pattern unification. A `via` pattern or a
`<-` pattern that is refutable is reported by exhaustiveness (plans/05) on
the generated `LetDestruct`, the same as a refutable `let` pattern.

New `Error` variant: `DuplicateTest { name, first, second }`. Prose (for
plans/06): title `DUPLICATE TEST`, "This module has two tests named
\"lt is strict\". Test names must be unique within a module so `nash test
--match` can pick one."

**Elm reference**: `Canonicalize/Module.hs` `canonicalize` (phase order),
`Canonicalize/Expression.hs` `canonicalize` for `Let`, and `Canonicalize/Environment/Foreign.hs`
`createInitialEnv` which `extend_with_imports` generalises.

**Tests** (`crates/nash-can` snapshot macros `assert_module_snapshot!` /
`assert_module_error_snapshot!`):

- `tests_unit_test`: module with `tests\n    test "one" = do\n        assert True`.
- `tests_prop_with_two_binders`: the overview `compare is antisymmetric`.
- `tests_do_desugars_to_lets`: a body `do { x <- 1; assert (x == 1) }`
  shows `Let { Def x, body: assert .. }`; `do { label "a"; assert True }`
  shows `LetDestruct { Unit, label "a", assert True }`.
- `tests_via_pattern`: `let (a, b) via tuple2 int int in ...` binds both.
- `tests_error_duplicate_name`.
- `tests_block_imports_are_scoped`: a block-only import used in the module
  body is `NotFoundVar`.

**Done when** snapshots accepted; `Module.tests` is empty for modules
without a block; validator builds still strip it (plans/09 Chunk 6).

---

## Chunk 2 — Constraints for tests

**Files**

- `crates/nash-constrain/src/module.rs`
- `crates/nash-constrain/src/tests.rs` (new)

**Change**

Each test contributes a constraint to the module constraint, after the
declarations, in a scope where every `via` binder has a fresh type variable.

```rust
// crates/nash-constrain/src/tests.rs
pub fn constrain_tests<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    rtv: &RigidTypeVars<'a>,
    tests: &'a [&'a Located<Test<'a>>],
    finish: &'a Constraint<'a>,
) -> &'a Constraint<'a> {
    tests.iter().rev().fold(finish, |rest, test| {
        let t = &test.value;
        let mut headers: BTreeMap<&'a str, Located<&'a Type<'a>>> = BTreeMap::new();
        let mut cons = Vec::with_capacity(t.binders.len() + 1);
        for via in t.binders {
            let elem = fresh_var(uf);
            let fuzzer = Type::app(bump, FUZZ_FUZZER, &[elem]);
            cons.push(expression::constrain(bump, uf, rtv, via.generator, Expected::NoExpectation(fuzzer)));
            // Same as a `let` pattern: the pattern is constrained against the element type
            // and contributes its variables as headers (Elm `Pattern.add` in `constrainLet`).
            let state = pattern::add(bump, uf, rtv, via.pattern, Expected::PatternNoExpectation(elem), pattern::State::empty());
            headers.extend(state.headers);
            cons.extend(state.constraints);
        }
        let body = expression::constrain(
            bump, uf, rtv, t.body,
            Expected::FromContext(t.body.region, Context::TestBody { name: t.name.value }, Type::unit(bump)),
        );
        cons.push(body);
        let vars = headers.values().map(|h| h.value).collect_in(bump);
        Constraint::let_(bump, vars, headers, Constraint::and(bump, cons), rest)
    })
}
```

`FUZZ_FUZZER` is the canonical name `Fuzz.fuzzer`; `t.name.value` is the
test name. `Context::TestBody`
gives the "the body of a test must be `unit`" prose:

```
The body of test "lt is strict" is a `bool`, but every test body must be
`unit`. Wrap it in `assert`?
```

**Elm reference**: `Type/Constrain/Module.hs` `constrainDecls` (the fold
shape), `Type/Constrain/Expression.hs` `constrainLet` (headers and
`CLet`).

**Tests** (`crates/nash-constrain` snapshots):

- `test_body_bool_is_error`: `test "x" = do\n    1 == 1` → mismatch with
  context `TestBody`.
- `prop_binder_takes_element_type`: `prop "p" = let x via int in do\n    assert (x + 1 > x)`
  solves.
- `prop_generator_not_a_fuzzer`: `let x via 1 in do ...` → mismatch against
  `fuzzer 'a`.

**Done when** `nash check` on a module with tests type checks the tests and
reports body-type errors.

---

## Chunk 3 — Power-assert rewrite

**Files**

- `crates/nash-codegen/src/assert.rs` (new)
- `crates/nash-codegen/src/lib.rs`

**Where**

A Can → Can pass over each test body, run in `nash-codegen` after solving,
before lowering to Core. It needs the per-expression types
(`nash_solve::SolvedTypes::exprs`, keyed by `NodeId`, plans/07 Chunk 3) and
`nash_solve::evidence::resolve(bump, tables, pred) -> Result<Evidence, MissingImpl>`
(plans/03 Chunk 6; `Tables` is the impl table plus traits, `CanPred` a
predicate over ground canonical types) to resolve `Show` at a
sub-expression's ground type. It does not
run for validator bodies.

**Transformation**

For each `Expr::Assert(e)` node (the `assert` keyword, plans/01 Chunk 4)
found in a test body (in any position, including inside the sequencing
`let`s):

1. Collect captured sub-expressions of `e` in pre-order, left-to-right,
   skipping lazy positions (Chunk 3 table in docs/testing.md). Each gets an
   index `i`, a fresh local `__a<id>_<i>`, and its region.
2. Build `e'` = `e` with each captured node replaced by `VarLocal(__a…)`.
   Because capture is pre-order, an outer captured node's replacement
   already covers the inner ones: inner captures bind first, then the outer
   captured node is rebuilt from the inner locals, then bound.
3. Result:

```
let __a_0 = sub_0 in ... let __a_n = sub_n in
if e' then () else Test.assertFailed [ msg_i | i with Show ]
where msg_i = appendString "\0assert\0<id>\0<i>\0" (show __a_i)
```

`show` is a `Call` of `Expr::VarMethod { trait_: Show, method: "show",
annotation }` (plans/03 Chunk 1, `nash_ast::Expr::VarMethod`) applied to
the local. The resolved `nash_ast::Evidence` for that occurrence is
registered in `SolvedTypes::instances` under the new node's `NodeId`, which
is how every other method occurrence carries its evidence to codegen.

```rust
// crates/nash-codegen/src/assert.rs
pub struct AssertSite<'a> {
    pub id: u32,
    pub region: Region,           // the argument `e`
    pub captures: Vec<Capture>,   // in payload index order
}

pub struct Capture {
    pub index: u32,
    pub region: Region,
    pub shown: bool,              // has a Show impl
}

pub struct Rewritten<'a> {
    pub body: &'a Located<Expr<'a>>,
    pub sites: Vec<AssertSite<'a>>,
}

pub fn rewrite_asserts<'a>(
    bump: &'a Bump,
    tables: &Tables<'a>,
    solved: &SolvedTypes<'a>,
    body: &'a Located<Expr<'a>>,
) -> Rewritten<'a> {
    let mut rw = Rewriter { bump, tables, solved, sites: Vec::new(), next_id: 0 };
    let body = rw.expr(body);
    Rewritten { body, sites: rw.sites }
}

struct Rewriter<'a, 'e> {
    bump: &'a Bump,
    tables: &'e Tables<'a>,
    solved: &'e SolvedTypes<'a>,
    sites: Vec<AssertSite<'a>>,
    next_id: u32,
}

impl<'a> Rewriter<'a, '_> {
    fn type_of(&self, e: &Located<Expr<'a>>) -> &'a Located<CanType<'a>> {
        self.solved.exprs[&NodeId::expr(e)]
    }
    /// `Show` evidence at the sub-expression's type; `Err(MissingImpl)` means "print `?`".
    fn show_evidence(&self, e: &Located<Expr<'a>>) -> Option<nash_ast::Evidence<'a>> {
        let pred = CanPred { trait_name: SHOW, args: &[self.type_of(e)] };
        nash_solve::evidence::resolve(self.bump, self.tables, &pred).ok()
    }
}

`Tables` and `CanPred` are plans/03 Chunk 6's types, `Evidence` is
`nash_ast::Evidence`; the tables are the ones the driver already holds for
the module after solving. `method_call` builds the `VarMethod` call and
records `(NodeId, Instance { type_args, evidence })` into `SolvedTypes::instances`
(the `SolvedTypes` is mutable during this pass).

impl<'a> Rewriter<'a, '_> {
    fn expr(&mut self, e: &'a Located<Expr<'a>>) -> &'a Located<Expr<'a>> {
        match &e.value {
            Expr::Assert(arg) => self.assert(e.region, arg),
            // every other variant: rebuild with children visited (a mechanical map)
            _ => self.map_children(e),
        }
    }

    fn assert(&mut self, region: Region, arg: &'a Located<Expr<'a>>) -> &'a Located<Expr<'a>> {
        let id = self.next_id;
        self.next_id += 1;
        let mut caps: Vec<(Capture, &'a str, &'a Located<Expr<'a>>)> = Vec::new();
        let cond = self.capture(id, arg, true, &mut caps);

        let messages = caps.iter().filter(|(c, _, _)| c.shown).map(|(c, local, sub)| {
            let show: nash_ast::Evidence<'a> = self.show_evidence(sub).expect("shown implies evidence");
            let prefix = self.bump.alloc_str(&format!("\0assert\0{id}\0{}\0", c.index));
            append_string(self.bump, str_lit(self.bump, prefix), method_call(self.bump, SHOW_SHOW, show, var_local(self.bump, local)))
        }).collect_in(self.bump);

        let failure = call(self.bump, var_foreign(self.bump, TEST_ASSERT_FAILED), &[list(self.bump, messages)]);
        let mut body = if_(self.bump, cond, unit(self.bump), failure);
        for (_, local, sub) in caps.into_iter().rev() {
            body = let_(self.bump, local, sub, body);
        }
        self.sites.push(AssertSite { id, region: arg.region, captures: caps.into_iter().map(|(c, _, _)| c).collect() });
        self.bump.alloc(Located::at(region, body.value))
    }

    /// Rebuild `e` with captured sub-expressions replaced by locals; `strict` says whether `e` itself is in strict position.
    fn capture(
        &mut self,
        id: u32,
        e: &'a Located<Expr<'a>>,
        strict: bool,
        caps: &mut Vec<(Capture, &'a str, &'a Located<Expr<'a>>)>,
    ) -> &'a Located<Expr<'a>> {
        if !strict {
            return e;
        }
        let rebuilt = match &e.value {
            Expr::Call { function, arguments } => {
                let f = self.capture(id, function, true, caps);
                let args = arguments.iter().map(|a| self.capture(id, a, true, caps)).collect_in(self.bump);
                self.alloc(e.region, Expr::Call { function: f, arguments: args })
            }
            Expr::Binop { symbol, reference, annotation, left, right } => {
                let lazy_rhs = matches!(*symbol, "&&" | "||");
                let l = self.capture(id, left, true, caps);
                let r = self.capture(id, right, !lazy_rhs, caps);
                self.alloc(e.region, Expr::Binop { symbol, reference: *reference, annotation, left: l, right: r })
            }
            Expr::Access { record, field } => {
                let r = self.capture(id, record, true, caps);
                self.alloc(e.region, Expr::Access { record: r, field })
            }
            // `if`, `case`, `let`: captured as a whole; scrutinee/condition is strict, branches are not.
            Expr::If { .. } | Expr::Case { .. } | Expr::Let { .. } | Expr::LetRec { .. } | Expr::LetDestruct { .. } => e,
            Expr::VarLocal(_) | Expr::VarTopLevel(_) | Expr::VarForeign { .. } => e,
            // literals, lambdas, nullary constructors, accessors, tuples of literals: never captured
            _ => return e,
        };
        if !is_capturable(&e.value) {
            return rebuilt;
        }
        let index = caps.len() as u32;
        let local = self.bump.alloc_str(&format!("__a{id}_{index}"));
        let shown = self.show_evidence(e).is_some();
        caps.push((Capture { index, region: e.region, shown }, local, rebuilt));
        self.alloc(e.region, Expr::VarLocal(local))
    }
}
```

`is_capturable`: `Var*`, `Call`, `Binop`, `Access`, `If`, `Case`, `Let*`
(`Expr::Negate` no longer exists after plans/03; `-x` is a `Num.negate`
call); not literals, `Lambda`, `VarConstructor` with arity 0, `Accessor`,
`Unit`, `List`/`Tuple`/`Record` literals. The root `arg` itself is not
captured (its value is known to be `False` on failure): `assert` calls
`capture` on the children of `arg` instead of `arg` when `arg` is itself
capturable; simplest is to capture and then drop the last entry whose
region equals `arg.region`.

`Test.assertFailed : list string -> 'a` is stdlib (next to `Test.label`,
stdlib.md):

```elm
assertFailed : list string -> 'a
assertFailed msgs =
    case msgs of
        [] -> Builtin.error ()
        m :: rest -> Builtin.trace m (\() -> assertFailed rest) ()
```

Codegen for `Builtin.trace` delays its second argument (plans/07 Chunk
10), which is what makes the traces fire before the error. The rewrite
replaces the `Expr::Assert` node, so plans/07 Chunk 10's own `Assert`
lowering (`if e then () else fail`) only ever sees validator bodies.

**Aiken reference**: `Assertion` / `TryFrom<TypedExpr> for Assertion`
(`test_framework.rs:1285-1553`) is the equivalent, restricted to binary
operators at the root; the capture rule above generalises it to all strict
sub-expressions.

**Tests** (`crates/nash-codegen` snapshot of the rewritten Can body, printed
with the existing `Debug`):

- `assert_captures_call_and_vars`: `assert (f x == y)` → captures `f x`, `x`,
  `y`, `f`; four lets; `f` has `shown: false`.
- `assert_skips_lazy_rhs`: `assert (x /= 0 && 10 / x > 1)` → `10 / x`, `x`
  under `&&` not captured.
- `assert_if_captured_whole`: `assert ((if b then 1 else 2) == 1)`.
- `assert_outside_tests_untouched`: a validator body with `assert` is not
  rewritten (the pass is only invoked on test bodies; test that the
  driver does not call it).

**Done when** snapshots accepted and a compiled failing test (Chunk 4)
emits `\0assert\0…` trace lines on the CEK machine.

---

## Chunk 4 — Codegen per test and prop

**Files**

- `crates/nash-codegen/src/tests.rs` (new)
- `crates/nash-codegen/src/lib.rs`

**Change**

For each `Test` produce flat-encoded programs plus the metadata the runner
needs. This replaces the `tests(...)` entry point sketched in plans/07
Chunk 11 (which assumed one program per test with the prop body as a
lambda): a prop yields two programs, `draw` and `run`, both synthesised as
Can expressions in the module's arena and then assembled per root with
plans/07's `assemble` (module bindings built and monomorphized once from
the union of all roots, `reachable(root)` per program).

```rust
// crates/nash-codegen/src/tests.rs
pub struct TestProgram {
    pub module: String,
    pub name: String,
    pub expect: Expect,
    pub budget: Option<Budget>,
    pub region: Region,
    pub programs: Programs,
    pub asserts: Vec<AssertSite>,      // owned copy: regions + capture list
    pub binder_texts: Vec<String>,     // prop only, pattern source text in order
}

pub enum Programs {
    /// `unit`
    Unit { run: Vec<u8> },
    /// `draw : Prng -> option (Prng, list string)`, `run : Prng -> option Prng`
    Prop { draw: Vec<u8>, run: Vec<u8> },
}

pub fn compile_tests<'a>(
    arena: &'a Arena,
    build: &Build<'a>,
    bump: &'a Bump,
    module: &'a nash_ast::Module<'a>,
    solved: &'a SolvedTypes<'a>,
) -> Result<Vec<TestProgram>, Error> {
    let mut roots: Vec<(&'a Located<Expr<'a>>, RootKind)> = Vec::new();
    let mut metas = Vec::new();
    for test in module.tests {
        let t = &test.value;
        let Rewritten { body, sites } = assert::rewrite_asserts(bump, build.tables(), solved, t.body);
        if t.is_prop() {
            roots.push((synth_draw(bump, build, solved, t.binders), RootKind::Draw));
            roots.push((synth_run(bump, t.binders, body), RootKind::Run));
        } else {
            roots.push((body, RootKind::Unit));
        }
        metas.push((t, sites));
    }
    // plans/07 Chunk 11: bindings monomorphized once from the union of roots,
    // then one `assemble` per root over `reachable(root)`.
    let compiled: Vec<Compiled<'a>> = program::assemble_roots(arena, build, module, &roots)?;
    let mut compiled = compiled.into_iter();
    let mut out = Vec::new();
    for (t, sites) in metas {
        let programs = if t.is_prop() {
            let draw = flat::encode(compiled.next().unwrap().program)?;
            let run = flat::encode(compiled.next().unwrap().program)?;
            Programs::Prop { draw, run }
        } else {
            Programs::Unit { run: flat::encode(compiled.next().unwrap().program)? }
        };
        out.push(TestProgram {
            module: module.name.name.to_string(),
            name: t.name.value.to_string(),
            expect: t.expect,
            budget: t.budget,
            region: t.body.region,
            programs,
            asserts: sites.iter().map(AssertSite::to_owned).collect(),
            binder_texts: t.binders.iter().map(|b| b.text.to_string()).collect(),
        });
    }
    Ok(out)
}
```

`assemble_roots` is plans/07's `tests` generalised to a list of roots; the
synthesised roots are Can expressions typed by construction (their types
are registered in `SolvedTypes` by `synth_*` so `can_to_core` finds them). Compiler traces are always on for tests
(`options.compiler_traces = true`), and the trace level defaults to
`Verbose` unless the config says otherwise.

`synth_run` builds, for binders `p1 via g1, …, pn via gn` (patterns):

```elm
\prng0 ->
    case g1 of Fuzzer f1 -> case f1 prng0 of
        None -> None
        Some (prng1, p1) ->
            ...
            case gn of Fuzzer fn -> case fn prng(n-1) of
                None -> None
                Some (prngn, pn) -> let () = body in Some prngn
```

`synth_draw` is the same shape with the leaf
`Some (prngn, [show v1, …, show vn])`, where `vi` is a fresh variable
bound in place of `pi` (`Some (prngn, vi)`), so the whole drawn value is
shown even when the pattern destructures it; a binder without `Show`
evidence contributes the literal `"?"`. Both are built with the Can
constructors used in Chunk 3 (`case_`, `ctor_pattern`, `tuple_pattern`,
`some`, `none`, `list`). `Fuzz.Prng`, `Fuzz.Fuzzer`, `option`'s `Some`/`None`
are canonical names resolved through the stdlib interface.

**Aiken reference**: `Test::from_function_definition`
(`test_framework.rs:114-182`) compiles the property body with the argument
as a parameter and the `via` expression as a separate program; here the
draw is inlined into the body program instead, see docs/testing.md.

**Tests** (`crates/nash-codegen`):

- `unit_test_program_evaluates`: compile `test "t" = do\n    assert True`, decode
  with `nash_plutus::flat::decode`, evaluate, expect `Ok(unit)`.
- `unit_test_failure_traces_assert`: `test "t" = do\n    x <- 1\n    assert (x == 2)`
  evaluates to `Err` with logs `["\0assert\00\00\01"]` (one capture:
  `x`).
- `prop_via_pattern_shows_whole_value`: `let (a, b) via tuple2 int int in`
  with a stub returning `(1, 2)`: `draw` returns `["(1, 2)"]`.
- `prop_draw_returns_shown_values`: with a stub `Fuzz` module whose `int`
  is `Fuzzer (\p -> Some (p, 7))`, `draw` applied to any `Prng` data returns
  `constr 0 [constr 0 [data, ["7"]]]`.
- `prop_run_returns_prng`: same stub, `run` returns `constr 0 [data]`.

**Done when** the four tests pass on the CEK machine.

---

## Chunk 5 — `nash-test`: types, PRNG, evaluation

**Files**

- `crates/nash-test/Cargo.toml` (new: `nash-plutus`, `nash-codegen` (types
  only), `cryptoxide`, `patricia_tree`, `rayon`, `serde`, `serde_json`,
  `thiserror`, `miette`)
- `crates/nash-test/src/lib.rs`, `types.rs`, `prng.rs`, `eval.rs`

**Types**

```rust
// crates/nash-test/src/types.rs
pub use nash_codegen::tests::{AssertSite, Capture, Programs, TestProgram};
pub use nash_config::PlutusVersion;
pub use nash_plutus::machine::ExBudget;
pub use nash_source::{Budget, Expect};

pub struct Config {
    pub seed: u32,
    pub max_success: usize,
    pub plutus_version: PlutusVersion,
    pub jobs: usize,
}

pub struct Outcome {
    pub test: TestProgram,
    pub status: Status,
    pub budget: ExBudget,          // consumed; max over iterations for props
    pub iterations: usize,         // 1 for unit tests
    pub labels: BTreeMap<String, usize>,
    pub traces: Vec<String>,       // non-label, non-assert log lines of the reported run
    pub assert: Option<AssertReport>,
    pub counterexample: Option<Vec<(String, String)>>,   // (binder, shown)
    pub expected_failure: bool,    // `fail once` found its failure
}

pub enum Status {
    Pass,
    Fail(Failure),
}

pub enum Failure {
    /// The body errored (or completed, under `fail`).
    Body,
    BudgetExceeded { limit: Budget, used: ExBudget },
    /// The generator errored or returned `None` on a seeded run.
    Fuzzer { message: String },
    /// A `prop … fail once` found no failing input.
    NoCounterexample,
}

pub struct AssertReport {
    pub site: AssertSite,
    pub values: Vec<(u32, String)>,   // (capture index, shown)
}
```

**PRNG** (`crates/nash-test/src/prng.rs`), port of `Prng`
(`test_framework.rs:640-792`) on `nash_plutus::data::PlutusData`
(`crates/nash-plutus/src/data.rs:10`):

```rust
use cryptoxide::{blake2b::Blake2b, digest::Digest};
use nash_plutus::{arena::Arena, constant::integer_from, data::PlutusData};

pub type Choice = u64;

#[derive(Debug, Clone)]
pub enum Prng {
    Seeded { seed: [u8; 32], choices: Vec<Choice> },
    Replayed { choices: Vec<Choice> },
}

impl Prng {
    const SEEDED: u64 = 0;
    const REPLAYED: u64 = 1;
    const SOME: usize = 0;
    const NONE: usize = 1;

    pub fn from_seed(seed: u32) -> Prng {
        let mut digest = [0u8; 32];
        let mut ctx = Blake2b::new(32);
        ctx.input(&seed.to_be_bytes());
        ctx.result(&mut digest);
        Prng::Seeded { seed: digest, choices: vec![] }
    }

    pub fn from_choices(choices: &[Choice]) -> Prng {
        Prng::Replayed { choices: choices.to_vec() }
    }

    /// Choices in draw order.
    pub fn choices(&self) -> Vec<Choice> {
        match self {
            Prng::Seeded { choices, .. } => choices.iter().rev().copied().collect(),
            Prng::Replayed { choices } => choices.clone(),
        }
    }

    /// Encode as the stdlib `Prng` Big ADT.
    pub fn to_data<'a>(&self, arena: &'a Arena) -> &'a PlutusData<'a> {
        let ints = |cs: &[Choice]| -> &'a [&'a PlutusData<'a>] {
            arena.alloc(cs.iter().map(|&c| PlutusData::integer_from(arena, c as i128)).collect::<Vec<_>>()).as_slice()
        };
        match self {
            Prng::Seeded { seed, choices } => PlutusData::constr(arena, Self::SEEDED, arena.alloc([
                PlutusData::byte_string(arena, arena.alloc(*seed)),
                // Seeded runs start with an empty choice list; choices accumulate on chain.
                PlutusData::list(arena, ints(&[])),
            ]).as_slice()),
            Prng::Replayed { choices } => PlutusData::constr(arena, Self::REPLAYED, arena.alloc([
                PlutusData::integer_from(arena, choices.len() as i128),
                PlutusData::list(arena, ints(choices)),
            ]).as_slice()),
        }
    }

    /// Decode a `Prng` returned by `draw`/`run`.
    fn from_data(data: &PlutusData<'_>) -> Prng {
        let PlutusData::Constr { tag, fields } = data else { unreachable!("malformed Prng: {data:?}") };
        let ints = |d: &PlutusData<'_>| -> Vec<Choice> {
            let PlutusData::List(items) = d else { unreachable!() };
            items.iter().map(|i| { let PlutusData::Integer(n) = i else { unreachable!() }; n.to_u64().expect("choice fits u64") }).collect()
        };
        match (*tag, fields) {
            (Self::SEEDED, [PlutusData::ByteString(seed), choices]) => Prng::Seeded { seed: seed[..].try_into().expect("32-byte seed"), choices: ints(choices) },
            (Self::REPLAYED, [_, choices]) => Prng::Replayed { choices: ints(choices) },
            _ => unreachable!("malformed Prng: {data:?}"),
        }
    }
}
```

**Evaluation** (`crates/nash-test/src/eval.rs`)

```rust
use nash_plutus::{arena::Arena, binder::DeBruijn, flat, machine::{ExBudget, PlutusVersion as MachineVersion}, program::Program, term::Term};

pub struct Evaluated<'a> {
    pub term: Result<&'a Term<'a, DeBruijn>, String>,
    pub budget: ExBudget,
    pub logs: Vec<String>,
}

/// Decode `flat` into `arena`, apply `arg` if given, evaluate with the maximum budget.
pub fn evaluate<'a>(arena: &'a Arena, version: MachineVersion, flat_bytes: &[u8], arg: Option<&'a Term<'a, DeBruijn>>) -> Evaluated<'a> {
    let program: &Program<'_, DeBruijn> = flat::decode(arena, flat_bytes).expect("compiler produced valid flat");
    let program = match arg { Some(a) => program.apply(arena, a), None => program };
    let result = program.eval_version_budget(arena, version, ExBudget::max());
    Evaluated {
        term: result.term.map_err(|e| e.to_string()),
        budget: result.info.consumed_budget,
        logs: result.info.logs,
    }
}

pub enum Drawn { Some { prng: Prng, shown: Vec<String> }, None }
pub enum Ran   { Some(Prng), None }

pub fn run_draw(arena: &Arena, version: MachineVersion, draw: &[u8], prng: &Prng) -> Result<(Drawn, Vec<String>), String> {
    let arg = Term::data(arena, prng.to_data(arena));
    let ev = evaluate(arena, version, draw, Some(arg));
    let term = ev.term?;
    Ok((decode_drawn(term), ev.logs))
}

pub fn run_body(arena: &Arena, version: MachineVersion, run: &[u8], prng: &Prng) -> (Result<Ran, String>, ExBudget, Vec<String>) {
    let arg = Term::data(arena, prng.to_data(arena));
    let ev = evaluate(arena, version, run, Some(arg));
    (ev.term.map(decode_ran), ev.budget, ev.logs)
}

fn decode_drawn(term: &Term<'_, DeBruijn>) -> Drawn {
    // Some = constr 0 [constr 0 [Constant(Data prng), Constant(list string)]], None = constr 1 []
    match term {
        Term::Constr { tag: Prng::SOME, fields: [tuple] } => match tuple {
            Term::Constr { tag: 0, fields: [Term::Constant(Constant::Data(prng)), Term::Constant(Constant::ProtoList(_, items))] } => Drawn::Some {
                prng: Prng::from_data(prng),
                shown: items.iter().map(|c| { let Constant::String(s) = c else { unreachable!() }; s.to_string() }).collect(),
            },
            _ => unreachable!("malformed draw result: {term:?}"),
        },
        Term::Constr { tag: Prng::NONE, fields: [] } => Drawn::None,
        _ => unreachable!("malformed draw result: {term:?}"),
    }
}
```

Variant names of `Term` and `Constant` follow
`crates/nash-plutus/src/term.rs:9` and `constant.rs:7`; adjust to the
actual enums. `flat::decode` is the existing decoder in
`crates/nash-plutus/src/flat/decode/`; expose a `decode(arena, &[u8]) ->
Result<&Program<DeBruijn>, FlatDecodeError>` if only lower-level functions
are public today.

Log splitting:

```rust
pub struct Logs { pub labels: Vec<String>, pub asserts: Vec<(u32, u32, String)>, pub traces: Vec<String> }

pub fn split_logs(logs: Vec<String>) -> Logs {
    let mut out = Logs { labels: vec![], asserts: vec![], traces: vec![] };
    for line in logs {
        if let Some(rest) = line.strip_prefix("\0label\0") {
            out.labels.push(rest.to_string());
        } else if let Some(rest) = line.strip_prefix("\0assert\0") {
            let mut parts = rest.splitn(3, '\0');
            let id = parts.next().unwrap().parse().unwrap();
            let index = parts.next().unwrap().parse().unwrap();
            out.asserts.push((id, index, parts.next().unwrap_or("").to_string()));
        } else {
            out.traces.push(line);
        }
    }
    out
}
```

**Aiken reference**: `Prng::from_seed`, `from_choices`, `sample`,
`from_result` (`test_framework.rs:676-792`); `PropertyTest::eval`
(`493-499`).

**Tests** (`crates/nash-test`):

- `prng_seed_matches_aiken`: `Prng::from_seed(42)` seed bytes equal
  blake2b-256 of `[0,0,0,42]` (fixed hex vector).
- `prng_data_round_trip`: `from_data(to_data(p)) == p` for both variants.
- `split_logs_separates_kinds`.

**Done when** unit tests pass; `evaluate` runs the Chunk 4 fixture
programs.

---

## Chunk 6 — Shrinker

**Files**

- `crates/nash-test/src/shrink.rs` (new)

A function-by-function port of `Counterexample` and `Cache`
(`test_framework.rs:800-1122`) generic over an oracle, so it is testable
without UPLC.

```rust
use patricia_tree::PatriciaMap;
use crate::prng::Choice;

#[derive(Debug, Clone, PartialEq)]
pub enum Status<T> {
    Keep(T),
    Ignore,
    Invalid,
}

pub struct Cache<'a, T> {
    db: PatriciaMap<Status<T>>,
    run: Box<dyn Fn(&[Choice]) -> Status<T> + 'a>,
}

fn key(choices: &[Choice]) -> Vec<u8> {
    choices.iter().flat_map(|c| c.to_be_bytes()).collect()
}

impl<'a, T: Clone + PartialEq> Cache<'a, T> {
    pub fn new(run: impl Fn(&[Choice]) -> Status<T> + 'a) -> Self {
        Cache { db: PatriciaMap::new(), run: Box::new(run) }
    }

    pub fn size(&self) -> usize { self.db.len() }

    // test_framework.rs:1092-1121
    pub fn get(&mut self, choices: &[Choice]) -> Status<T> {
        let k = key(choices);
        if let Some((prefix, status)) = self.db.get_longest_common_prefix(&k) {
            let status = status.clone();
            // Big-endian 8-byte keys keep the prefix relation at choice granularity
            // as long as the prefix length is a multiple of 8.
            if prefix.len() % 8 == 0 && (status != Status::Invalid || prefix.len() == k.len()) {
                return status;
            }
        }
        let status = (self.run)(choices);
        if status != Status::Invalid {
            let stale: Vec<_> = self.db.iter_prefix(&k).map(|(key, _)| key).collect();
            for s in stale { self.db.remove(s); }
        }
        self.db.insert(&k, status.clone());
        status
    }
}

pub struct Counterexample<'a, T> {
    pub value: T,
    pub choices: Vec<Choice>,
    pub cache: Cache<'a, T>,
    pub steps: usize,
}

impl<T: Clone + PartialEq> Counterexample<'_, T> {
    // test_framework.rs:807-826
    fn consider(&mut self, choices: &[Choice]) -> bool {
        if choices == self.choices.as_slice() { return true; }
        match self.cache.get(choices) {
            Status::Invalid | Status::Ignore => false,
            Status::Keep(value) => {
                if choices.len() <= self.choices.len() || choices < self.choices.as_slice() {
                    self.value = value;
                    self.choices = choices.to_vec();
                    true
                } else {
                    false
                }
            }
        }
    }

    // test_framework.rs:848-1007, with `eprintln!` events replaced by the `steps` counter
    pub fn simplify(&mut self) {
        loop {
            let prev = self.choices.clone();

            // 1. delete chunks, with the "decrement the choice before" retry
            let mut k = 8;
            while k > 0 {
                let (mut i, mut underflow) = if self.choices.len() < k { (0, true) } else { (self.choices.len() - k, false) };
                while !underflow {
                    if i >= self.choices.len() {
                        (i, underflow) = i.overflowing_sub(1);
                        self.steps += 1;
                        continue;
                    }
                    let j = i + k;
                    let mut choices = [&self.choices[..i], if j < self.choices.len() { &self.choices[j..] } else { &[] }].concat();
                    if !self.consider(&choices) {
                        if i > 0 && choices[i - 1] > 0 {
                            choices[i - 1] -= 1;
                            if self.consider(&choices) { i += 1; }
                        }
                        (i, underflow) = i.overflowing_sub(1);
                    }
                    self.steps += 1;
                }
                k /= 2;
            }

            if !self.choices.is_empty() {
                // 2. zero blocks
                let mut k = 8;
                while k > 1 {
                    let mut i = self.choices.len();
                    while i >= k {
                        self.steps += 1;
                        let ivs = (i - k..i).map(|j| (j, 0)).collect::<Vec<_>>();
                        i -= if self.replace(ivs) { k } else { 1 };
                    }
                    k /= 2;
                }

                // 3. binary search each choice down
                let (mut i, mut underflow) = (self.choices.len() - 1, false);
                while !underflow {
                    self.steps += 1;
                    self.binary_search_replace(0, self.choices[i], |v| vec![(i, v)]);
                    (i, underflow) = i.overflowing_sub(1);
                }

                // 4. sort chunks
                let mut k = 8;
                while k > 1 {
                    let mut i = self.choices.len() - 1;
                    while i >= k {
                        self.steps += 1;
                        let (from, to) = (i - k, i);
                        let mut sorted = self.choices[from..to].to_vec();
                        sorted.sort_unstable();
                        self.replace((from..to).zip(sorted).collect());
                        i -= 1;
                    }
                    k /= 2;
                }

                // 5. swap / redistribute neighbours
                for k in [2, 1] {
                    let mut j = self.choices.len() - 1;
                    while j >= k {
                        let i = j - k;
                        if self.choices[i] > self.choices[j] {
                            self.replace(vec![(i, self.choices[j]), (j, self.choices[i])]);
                        }
                        let (iv, jv) = (self.choices[i], self.choices[j]);
                        if iv > 0 && jv <= Choice::MAX - iv {
                            self.binary_search_replace(0, iv, |v| vec![(i, v), (j, jv + (iv - v))]);
                        }
                        self.steps += 1;
                        j -= 1;
                    }
                }
            }

            if prev == self.choices { break; }
        }
    }

    // test_framework.rs:1011-1032
    fn binary_search_replace(&mut self, lo: Choice, hi: Choice, f: impl Fn(Choice) -> Vec<(usize, Choice)>) -> Choice {
        if self.replace(f(lo)) { return lo; }
        let (mut lo, mut hi) = (lo, hi);
        while lo + 1 < hi {
            let mid = lo + (hi - lo) / 2;
            if self.replace(f(mid)) { hi = mid; } else { lo = mid; }
        }
        hi
    }

    // test_framework.rs:1036-1047
    fn replace(&mut self, ivs: Vec<(usize, Choice)>) -> bool {
        let mut choices = self.choices.clone();
        for (i, v) in ivs {
            if i >= choices.len() { return false; }
            choices[i] = v;
        }
        self.consider(&choices)
    }
}
```

The one deviation from Aiken: `Choice = u64` instead of `u8`, so the
Patricia key is 8 bytes per choice and `get` checks that the matched
prefix ends on a choice boundary.

**Tests** (`crates/nash-test/src/shrink.rs`, no UPLC; the oracle is a
closure that "generates" from choices):

- `cache_prefix_rule`: port of Aiken's `test_cache`
  (`test_framework.rs:1713-1742`) with `u64` choices; same call counts.
- `shrink_single_int`: oracle reads `choices[0]` as `n`, `Keep(n)` when
  `n >= 1000`, `Ignore` otherwise, `Invalid` if empty. Start `[48213]`,
  expect `[1000]`.
- `shrink_list_sum`: oracle reads `len = choices[0]` then `len` elements,
  `Invalid` if too short, `Keep(sum)` if `sum > 100`. Start
  `[5, 10, 90, 20, 5, 1]`, expect `[1, 101]`.
- `shrink_pair_ordering`: `Keep` when `a > b`, start `[9, 3]`, expect
  `[1, 0]`.
- `shrink_is_deterministic`: two runs from the same start give the same
  `choices` and `steps`.

**Done when** all five pass; `shrink_list_sum` exercises the "decrement
before deleted chunk" rule (it cannot reach `[1, 101]` without it).

---

## Chunk 7 — Runner: unit tests, props, labels, `within`, parallelism

**Files**

- `crates/nash-test/src/run.rs` (new)
- `crates/nash-test/src/lib.rs`

```rust
// crates/nash-test/src/run.rs
use rayon::prelude::*;

pub fn run_all(tests: Vec<TestProgram>, config: &Config) -> Vec<Outcome> {
    let pool = rayon::ThreadPoolBuilder::new().num_threads(config.jobs).build().expect("thread pool");
    pool.install(|| tests.into_par_iter().map(|t| run_one(t, config)).collect())
}

fn run_one(test: TestProgram, config: &Config) -> Outcome {
    let arena = Arena::new();
    let version = machine_version(config.plutus_version);
    match &test.programs {
        Programs::Unit { run } => run_unit(&arena, version, test, run),
        Programs::Prop { draw, run } => run_prop(&arena, version, test, draw, run, config),
    }
}

// test_framework.rs:224-265
fn run_unit(arena: &Arena, version: MachineVersion, test: TestProgram, run: &[u8]) -> Outcome {
    let ev = evaluate(arena, version, run, None);
    let logs = split_logs(ev.logs);
    let errored = ev.term.is_err();
    let body_ok = match test.expect {
        Expect::Pass => !errored,
        Expect::Fail => errored,
        Expect::FailOnce => unreachable!("rejected by the parser"),
    };
    let status = match (body_ok, over_budget(test.budget, ev.budget)) {
        (false, _) => Status::Fail(FailureKind::Body),
        (true, Some(limit)) => Status::Fail(FailureKind::BudgetExceeded { limit, used: ev.budget }),
        (true, None) => Status::Pass,
    };
    Outcome {
        assert: assert_report(&test, &logs.asserts),
        status, budget: ev.budget, iterations: 1,
        labels: count(logs.labels), traces: logs.traces,
        counterexample: None, expected_failure: false, test,
    }
}

fn over_budget(limit: Option<Budget>, used: ExBudget) -> Option<Budget> {
    limit.filter(|l| match *l {
        Budget::Cpu(cpu) => i128::from(used.cpu) > cpu,
        Budget::Mem(mem) => i128::from(used.mem) > mem,
        Budget::Both { cpu, mem } => i128::from(used.cpu) > cpu || i128::from(used.mem) > mem,
    })
}

// test_framework.rs:359-491
fn run_prop(arena: &Arena, version: MachineVersion, test: TestProgram, draw: &[u8], run: &[u8], config: &Config) -> Outcome {
    let mut prng = Prng::from_seed(config.seed);
    let mut labels = BTreeMap::new();
    let mut max_budget = ExBudget::new(0, 0);
    let mut iterations = 0;

    while iterations < config.max_success {
        iterations += 1;
        let (result, budget, logs) = run_body(arena, version, run, &prng);
        max_budget = ExBudget::new(max_budget.mem.max(budget.mem), max_budget.cpu.max(budget.cpu));
        let logs = split_logs(logs);
        for l in logs.labels { *labels.entry(l).or_insert(0) += 1; }

        let errored = result.is_err();
        let is_counterexample = match test.expect {
            Expect::Pass | Expect::FailOnce => errored,
            Expect::Fail => !errored,
        };

        if let Some(limit) = over_budget(test.budget, budget) {
            return finish(test, Status::Fail(FailureKind::BudgetExceeded { limit, used: budget }), max_budget, iterations, labels, logs, None, None);
        }

        if is_counterexample {
            // Recover choices and shown values; a broken generator surfaces here.
            let (drawn, _) = match run_draw(arena, version, draw, &prng) {
                Ok(d) => d,
                Err(message) => return finish(test, Status::Fail(FailureKind::Fuzzer { message }), max_budget, iterations, labels, logs, None, None),
            };
            let Drawn::Some { prng: next, shown } = drawn else {
                return finish(test, Status::Fail(FailureKind::Fuzzer { message: "generator returned None on a seeded run".into() }), max_budget, iterations, labels, logs, None, None);
            };
            let choices = next.choices();
            let (final_shown, final_logs) = shrink(arena, version, &test, draw, run, choices, shown);
            let assert = assert_report(&test, &final_logs.asserts);
            let ce = test.binder_texts.iter().cloned().zip(final_shown).collect();
            return match test.expect {
                Expect::FailOnce => finish_with(test, Status::Pass, true, max_budget, iterations, labels, final_logs, assert, Some(ce)),
                _ => finish_with(test, Status::Fail(FailureKind::Body), false, max_budget, iterations, labels, final_logs, assert, Some(ce)),
            };
        }

        prng = match result {
            Ok(Ran::Some(next)) => next,
            Ok(Ran::None) => return finish(test, Status::Fail(FailureKind::Fuzzer { message: "generator returned None on a seeded run".into() }), max_budget, iterations, labels, logs, None, None),
            Err(_) => unreachable!("errored runs are counterexamples or expected failures"),
        };
    }

    let status = if test.expect == Expect::FailOnce { Status::Fail(FailureKind::NoCounterexample) } else { Status::Pass };
    finish(test, status, max_budget, iterations, labels, Logs::default(), None, None)
}
```

Under `Expect::Fail` an erroring iteration is the expected outcome and
the loop needs the next PRNG, which the error discarded. Recover it with
`run_draw` in that branch (same call as the counterexample path, without
shrinking); the extra evaluation only happens for `fail` props.

Shrink driver, the oracle closure from `run_once`
(`test_framework.rs:452-480`):

```rust
fn shrink(arena: &Arena, version: MachineVersion, test: &TestProgram, draw: &[u8], run: &[u8], choices: Vec<Choice>, shown: Vec<String>) -> (Vec<String>, Logs) {
    let oracle = |choices: &[Choice]| -> Status<(Vec<String>, Vec<String>)> {
        let prng = Prng::from_choices(choices);
        let shown = match run_draw(arena, version, draw, &prng) {
            Err(_) | Ok((Drawn::None, _)) => return Status::Invalid,
            Ok((Drawn::Some { shown, .. }, _)) => shown,
        };
        let (result, _, logs) = run_body(arena, version, run, &prng);
        let errored = result.is_err();
        let failing = match test.expect {
            Expect::Pass | Expect::FailOnce => errored,
            Expect::Fail => !errored,
        };
        if failing { Status::Keep((shown, logs)) } else { Status::Ignore }
    };
    let mut ce = Counterexample { value: (shown, vec![]), choices, cache: Cache::new(oracle), steps: 0 };
    if !ce.choices.is_empty() {
        eprintln!("  Simplifying counterexample from {} choices", ce.choices.len());
        let start = std::time::Instant::now();
        ce.simplify();
        eprintln!("   Simplified counterexample in {:?} after {} steps", start.elapsed(), ce.steps);
    }
    let (shown, logs) = ce.value;
    (shown, split_logs(logs))
}
```

The initial `value` carries the logs of the first failing seeded run only
if the shrinker never improves; to keep it simple, re-run `run_body` once
on the final choices when `steps == 0`.

Assert report:

```rust
fn assert_report(test: &TestProgram, payloads: &[(u32, u32, String)]) -> Option<AssertReport> {
    let (id, _, _) = payloads.first()?;
    let site = test.asserts.iter().find(|s| s.id == *id)?.clone();
    Some(AssertReport { site, values: payloads.iter().filter(|(i, _, _)| i == id).map(|(_, idx, v)| (*idx, v.clone())).collect() })
}
```

Only the first assert id appears in a payload (the program errors after the
first failing `assert`), so `first()` is the failing site.

**Aiken reference**: `PropertyTest::run`, `run_n_times`, `run_once`
(`test_framework.rs:359-491`); `UnitTest::run` (`224-265`);
`run_runnables` (`aiken-project/src/lib.rs:1146-1196`) for rayon.

**Tests** (`crates/nash-test/tests/runner.rs`, using hand-written UPLC
text parsed with `nash_plutus::syn::parse_program` and flat-encoded, so no
Nash compiler is needed):

- `unit_pass`: program `(program 1.1.0 (con unit ()))` → `Pass`.
- `unit_fail`: `(program 1.1.0 (error))` → `Fail(Body)`; with
  `Expect::Fail` → `Pass`.
- `unit_within_exceeded`: a loop that burns budget, `within (cpu 1000, mem 100)` → `BudgetExceeded`.
- `prop_pass`: `run` = `(lam p (constr 0 [p]))`, `draw` returns
  `Some (p, ["x"])` → `Pass` after `max_success` iterations.
- `prop_fail_shrinks`: `draw` returns the first choice as an integer string,
  `run` errors when the first choice `>= 10`; the reported counterexample
  is `"10"`. This UPLC is written by hand with `unConstrData`/
  `unListData`/`headList` on the `Prng` data.
- `labels_are_counted`: `run` traces `"\0label\0a"` → `labels == {a: 100}`.

**Done when** the runner tests pass and `--jobs 1` and `--jobs 8` give
identical outcomes.

---

## Chunk 8 — Reporting: terminal and JSON

**Files**

- `crates/nash-test/src/report/terminal.rs`, `report/json.rs`, `report/mod.rs`

Terminal layout follows docs/testing.md `Example output`; port
`fmt_test` (`terminal.rs:367-697`) with these differences: budgets are
right-aligned with thousands separators (`numfmt` or a 20-line helper),
`× counterexample` prints `name = shown` per binder, and the power-assert
block is drawn from `AssertReport`:

```rust
/// Lay out captured values under the assert's source text.
pub fn render_assert(source: &str, report: &AssertReport) -> String {
    let site = &report.site;
    let text = &source[site.region.byte_range(source)];
    let base_col = site.region.start.column as usize;
    let mut cols: Vec<(usize, String)> = report.values.iter().map(|(idx, v)| {
        let cap = site.captures.iter().find(|c| c.index == *idx).expect("index from payload");
        (cap.region.start.column as usize - base_col, v.clone())
    }).collect();
    for cap in site.captures.iter().filter(|c| !c.shown) {
        cols.push((cap.region.start.column as usize - base_col, "?".to_string()));
    }
    cols.sort_by_key(|(c, _)| *c);

    let mut out = format!("× assert ({text})\n");
    let pipes = |upto: usize, cols: &[(usize, String)]| -> String {
        let mut line = " ".repeat(text.len() + 10);
        for (c, _) in cols.iter().take(upto) { line.replace_range(10 + c..10 + c + 1, "│"); }
        line.trim_end().to_string()
    };
    out.push_str(&pipes(cols.len(), &cols));
    out.push('\n');
    for i in (0..cols.len()).rev() {
        let mut line = pipes(i, &cols);
        let (c, v) = &cols[i];
        let start = 10 + c;
        if line.len() < start { line.push_str(&" ".repeat(start - line.len())); }
        line.truncate(start);
        line.push_str(v);
        out.push_str(&line);
        out.push('\n');
    }
    out
}
```

Columns are measured in characters; `Region` positions are already
line/column (`crates/nash-region/src/lib.rs:67`). Values wider than the
gap to the next column push the next column's line up, which is the
standard power-assert behaviour: each value is on its own line, rightmost
first, so nothing overlaps.

JSON: `serde::Serialize` on `Outcome` with a small DTO; shape in
docs/testing.md.

**Tests**: snapshot `render_assert` for the docs example; snapshot the
full terminal block for a fixed `Vec<Outcome>`.

**Done when** the docs example renders byte-for-byte.

---

## Chunk 9 — Driver and `nash test` command

**Files**

- `crates/nash-driver/src/test.rs` (new)
- `crates/nash-cli/src/cmd/test.rs` (new)
- `crates/nash-cli/src/cmd/mod.rs`

```rust
// crates/nash-driver/src/test.rs
pub fn collect_tests(solved: &Solved<'_>, config: &nash_config::Build, filter: &Filter) -> Result<Vec<TestProgram>, BuildError> {
    let arena = Arena::new();
    // Same `Build` context as `build_validators` (plans/09 Chunk 6), compiler traces on.
    let build = nash_codegen::Build::new(
        &arena,
        solved.modules.iter().map(|m| (m.module, m.annotations, m.types)),
        nash_codegen::Options { plutus_version: config.plutus_version, trace_level: config.trace_level, compiler_traces: true, optimize: config.optimize },
    );
    let mut out = Vec::new();
    for m in &solved.modules {
        if m.module.tests.is_empty() { continue; }
        // Chunk 4's signature; `TestProgram` and `compile_tests` are owned by this plan.
        let programs = nash_codegen::tests::compile_tests(&arena, &build, solved.store, m.module, m.types)
            .map_err(|source| BuildError::Codegen { module: m.module.name.name.to_string(), source })?;
        out.extend(programs.into_iter().filter(|t| filter.matches(&t.module, &t.name)));
    }
    Ok(out)
}

pub struct Filter { pub patterns: Vec<String>, pub exact: bool }

impl Filter {
    /// `Module`, `Module.{name}`, or a substring of either (Aiken's `--match-tests`).
    pub fn matches(&self, module: &str, name: &str) -> bool {
        if self.patterns.is_empty() { return true; }
        self.patterns.iter().any(|p| match p.split_once(".{") {
            Some((m, rest)) => {
                let n = rest.trim_end_matches('}');
                if self.exact { module == m && name == n } else { module.contains(m) && name.contains(n) }
            }
            None => if self.exact { module == p || name == p } else { module.contains(p.as_str()) || name.contains(p.as_str()) },
        })
    }
}
```

```rust
// crates/nash-cli/src/cmd/test.rs
#[derive(clap::Args)]
pub struct Args {
    #[arg(default_value = ".")]
    pub path: PathBuf,
    /// Seed for property tests (random when omitted; printed in the summary)
    #[arg(long)]
    pub seed: Option<u32>,
    /// Iterations per property
    #[arg(long, default_value_t = 100)]
    pub max_success: usize,
    /// Only run tests matching PATTERN (`Module`, `Module.{name}`, or a substring)
    #[arg(short, long = "match")]
    pub matches: Vec<String>,
    /// Match whole strings
    #[arg(long)]
    pub exact: bool,
    #[arg(long, value_enum)]
    pub trace_level: Option<TraceLevelArg>,
    /// Worker threads
    #[arg(long, default_value_t = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1))]
    pub jobs: usize,
    #[arg(long, value_enum, default_value_t = Coverage::Labels)]
    pub coverage: Coverage,
    #[arg(long)]
    pub json: bool,
}

impl Args {
    pub async fn exec(self) -> Result<()> {
        let project = Project::load(&self.path).await.into_diagnostic()?;
        let db = Arc::new(Mutex::new(Database::new(FileSystemSource::new())));
        let modules = project.discover_modules(&*db.lock().await).await.into_diagnostic()?;
        let graph = build_graph(db.clone(), &modules).await.into_diagnostic()?;

        // cli.md: flag, else `traceLevel` from nash.jsonc, else verbose for tests.
        let mut config = project.config.build();
        if let Some(level) = self.trace_level {
            config.trace_level = level.into();
        } else if !project.config.has_trace_level() {
            config.trace_level = TraceLevel::Verbose;
        }
        let filter = Filter { patterns: self.matches.clone(), exact: self.exact };
        let seed = self.seed.unwrap_or_else(rand_seed);

        let (result, tests) = build_with(db, &graph, CompileMode::Test, move |solved| collect_tests(solved, &config, &filter)).await;
        if !result.is_success() {
            report_failures(&result);
            std::process::exit(1);
        }
        let tests = tests.into_diagnostic()?;
        let sources = load_sources(&project, &tests).await?;   // module -> source text, for assert rendering

        let started = std::time::Instant::now();
        let outcomes = nash_test::run_all(tests, &nash_test::Config {
            seed, max_success: self.max_success, plutus_version: config.plutus_version, jobs: self.jobs,
        });

        if self.json {
            println!("{}", nash_test::report::json::render(seed, self.max_success, &outcomes));
        } else {
            eprint!("{}", nash_test::report::terminal::render(&outcomes, &sources, self.coverage, seed, started.elapsed()));
        }
        if outcomes.iter().any(|o| matches!(o.status, nash_test::Status::Fail(_))) {
            std::process::exit(1);
        }
        Ok(())
    }
}

fn rand_seed() -> u32 {
    // No rand dependency: mix the clock through the same hash the PRNG uses.
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let mut out = [0u8; 4];
    let mut h = cryptoxide::blake2b::Blake2b::new(4);
    h.input(&nanos.to_le_bytes());
    h.result(&mut out);
    u32::from_be_bytes(out)
}
```

`Cmd::Test(test::Args)` with alias `t` in `cmd/mod.rs`.

**Aiken reference**: `aiken/src/cmd/check.rs` (59-90) flags;
`aiken-project/src/lib.rs` `check` → `collect_tests` → `run_runnables`.

**Tests**: `crates/nash-cli` clap `debug_assert`; an `examples/order`
project with the docs/testing.md module, run by a CI step:
`nash test examples/order --seed 1` must fail with exit 1 and its stderr
must contain `× counterexample`.

**Done when** `nash test` on `examples/order` produces the transcript in
docs/testing.md up to timing digits.

---

## Ordering and compile state

| Chunk | Depends on |
|---|---|
| 1 | plans/01, plans/09 Chunk 1 |
| 2 | 1, plans/03 |
| 3 | 2, plans/07 `Typed`/`Evidence` |
| 4 | 3, plans/07 `assemble` |
| 5 | nash-plutus only |
| 6 | 5 (types only) |
| 7 | 5, 6, 4 (fixtures) |
| 8 | 7 |
| 9 | 4, 7, 8, plans/09 Chunk 6 |

Chunks 5 and 6 are independent of the compiler and can be built first;
their tests use hand-written UPLC and closures.

## Open questions

- **`fail` props and the PRNG chain.** Every iteration of a `fail` prop
  errors by design, so the next PRNG is recovered with an extra `draw`
  evaluation per iteration. Acceptable; noted so the cost is not a
  surprise.
- **`Region` to byte range.** `render_assert` needs a line/column → byte
  offset helper on the source; `nash-report` (plans/06) likely adds one.
