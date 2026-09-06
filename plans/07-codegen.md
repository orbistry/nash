# Plan 07 — Codegen: `nash-ir` and `nash-codegen`

## Goal

Turn a type-checked module into a UPLC `Program` that runs on the
nash-plutus CEK machine. Two new crates:

- `crates/nash-ir` — the `Core` IR (types, constructors, pretty printer,
  traversal helpers) and, in plan 08, the Core -> Core passes.
- `crates/nash-codegen` — Can AST -> Core (monomorphization, decision
  trees, desugaring, recursion rewrite, casts, traces) and Core -> UPLC
  `Term` (lowering, program assembly, comptime evaluation).

Specification: [docs/codegen.md](../docs/codegen.md),
[docs/data.md](../docs/data.md).

## Prerequisites

- [02-kinds.md](02-kinds.md): every type has a base kind; `Ty::kind()` is derivable
  from the Can type plus the union/alias tables.
- [03-traits.md](03-traits.md): `nash-solve` returns, in addition to
  `nash_can::Annotations`, the solved type of every expression, pattern and
  binder, and the resolved evidence at every variable occurrence
  (`SolvedTypes`; chunk 3 shows the types, the section "Contract with
  plans/07-codegen.md" in 03-traits.md is authoritative).
- [09-validators-build.md](09-validators-build.md): the driver's
  `build_with(db, graph, mode, finish)` solves every module and hands the
  `finish` closure a `Solved { store, modules: Vec<SolvedModule> }`; codegen
  is called from there (chunk 11), never from `compile_module`.
- [01-syntax.md](01-syntax.md): `Expr` has `Trace`, `Fail`, `Todo`,
  `Assert`, `Comptime`, `Do` nodes (chunks 10 and 11 name the variants
  they expect). `nash_ast::Module.kind` (`ModuleKind::{Normal, Validator}`)
  comes from [09-validators-build.md](09-validators-build.md) chunk 1 and
  the canonical `tests` block from [10-testing.md](10-testing.md) chunk 1.
- [04-representation.md](04-representation.md) chunk A5: `nash_ast::Ctor`
  (`crates/nash-ast/src/lib.rs:90`) carries
  `CtorArgs::{Positional(&[&Located<Type>]), Labeled(&[FieldType])}`;
  labeled fields are name-sorted with `FieldType.index` as the wire
  position and `CtorArgs::types()` yields the field types in wire order.
  Labels are compile-time only. nash-can rewrites the labeled pattern and
  construction sugar `Datum { owner, deadline }` to positional form, so
  codegen only ever sees positional patterns and constructor calls.
- Nitpick (exhaustiveness) runs before codegen; decision trees assume
  complete matches.

Chunks 1, 2, 4 and the decision-tree data structures of chunk 5 have no
prerequisite beyond the current workspace and can start now.

## Crates touched

`crates/nash-ir` (new), `crates/nash-codegen` (new),
`crates/nash-plutus` (pretty printer, `Name -> DeBruijn`),
`crates/nash-solve` (chunk 3 contract, owned by
[03-traits.md](03-traits.md)). The driver call site belongs to
[09-validators-build.md](09-validators-build.md).

## Reference files

Aiken (`/Users/kcwhite/work/scm/aiken-lang/aiken`):

- `crates/aiken-lang/src/gen_uplc/air.rs` — node inventory (what an IR
  for this target must express).
- `crates/aiken-lang/src/gen_uplc/tree.rs` — `AirTree`, traversal.
- `crates/aiken-lang/src/gen_uplc/decision_tree.rs` — `Path`, `CaseTest`,
  `DecisionTree`, `TreeGen::build_tree`, `do_build_tree`,
  `highest_occurrence`, `get_hoist_paths`, `hoist_by_path`.
- `crates/aiken-lang/src/gen_uplc/stick_break_set.rs` — `Builtins`,
  `TreeSet::diff_union_builtins`.
- `crates/aiken-lang/src/gen_uplc/builder.rs` —
  `identify_recursive_static_params`, `modify_self_calls`,
  `modify_cyclic_calls`, `known_data_to_type`, `unknown_data_to_type`,
  `softcast_data_to_type_otherwise`, `get_generic_variant_name`,
  `apply_builtin_forces`, `wrap_validator_condition`.
- `crates/aiken-lang/src/gen_uplc.rs` — `generate`, `generate_raw`,
  `handle_decision_tree`, `hoist_functions_to_validator`, `gen_uplc`
  (the `DefineFunc` arms at 4607 and 4674).
- `crates/uplc/src/pretty.rs`, `crates/uplc/src/debruijn.rs` — for the
  nash-plutus additions.

Elm: `elm/compiler/src/Generate/JavaScript/Expression.hs` (how Elm walks
`Can.Expr` to emit code; structure only), `elm/compiler/src/Optimize/`
(`Case.hs`, `DecisionTree.hs` for the Maranget tree Elm uses for JS).

Nash (current code):

- `crates/nash-ast/src/lib.rs` — `Expr`, `Pattern`, `Def`, `Decls`,
  `Union`, `Ctor`, `CtorOpts`, `Type`.
- `crates/nash-solve/src/solve.rs:22` (`run`),
  `crates/nash-solve/src/annotation.rs:16` (`to_annotation`).
- `crates/nash-driver/src/compile.rs:151` (`compile_module`).
- `crates/nash-plutus/src/term.rs`, `constant.rs`, `data.rs`, `typ.rs`,
  `program.rs`, `binder/*.rs`, `builtin/default_function.rs`,
  `arena.rs`.

## Conventions

- Arena: `nash_plutus::arena::Arena` (`crates/nash-plutus/src/arena.rs:8`)
  for everything in `Core` and `Term`, so `Constant`s and `Integer`s can be
  shared. `Arena::from_bump` wraps an existing `Bump` when the driver wants
  one arena per module.
- Names: `Name { text: &'a str, unique: u32 }`. Codegen assigns uniques
  from one counter per program; the hygiene pass in plan 08 re-checks.
- Tests: `insta` snapshots via macros defined in each test module, with
  `indoc!` for multi-line sources, following `crates/nash-can` and
  `crates/nash-solve`.

---

## Chunk 1 — `nash-ir`: Core types and pretty printer

**Files**

- `crates/nash-ir/Cargo.toml` (new)
- `crates/nash-ir/src/lib.rs` (new)
- `crates/nash-ir/src/core.rs` (new)
- `crates/nash-ir/src/ty.rs` (new)
- `crates/nash-ir/src/pretty.rs` (new)
- `crates/nash-ir/src/build.rs` (new)
- `Cargo.toml` (workspace member is picked up by `crates/*`)

**Change**

Define the IR exactly as in docs/codegen.md. No passes yet.

**Code**

`crates/nash-ir/Cargo.toml`:

```toml
[package]
name = "nash-ir"
version = "0.1.0"
edition.workspace = true
description = "Nash Core IR: monomorphized, explicitly-typed lambda calculus"
homepage.workspace = true
repository.workspace = true
license.workspace = true

[dependencies]
nash-ast = { path = "../nash-ast", version = "0.3.1" }
nash-plutus = { path = "../nash-plutus", version = "0.1.0" }
num-bigint.workspace = true

[dev-dependencies]
insta.workspace = true
```

`crates/nash-ir/src/ty.rs`:

```rust
//! Monomorphic types with their base kind exposed. `Ty` is the only type
//! codegen ever looks at; casing of the source name has been resolved.

use nash_ast::QualifiedName;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Big,
    Const,
    Term,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ty<'a> {
    Big(&'a BigTy<'a>),
    Const(&'a ConstTy<'a>),
    Term(&'a TermTy<'a>),
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum ConstTy<'a> {
    Int,
    Bytes,
    String,
    Bool,
    Unit,
    List(Ty<'a>),
    Pair(Ty<'a>, Ty<'a>),
    Array(Ty<'a>),
    BlsG1,
    BlsG2,
    BlsMlr,
    Value,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum BigTy<'a> {
    Int,
    Bytes,
    Data,
    List(Ty<'a>),
    Map(Ty<'a>, Ty<'a>),
    Adt(AdtRef<'a>),
    Record(&'a [Ty<'a>]),
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum TermTy<'a> {
    Adt(AdtRef<'a>),
    Tuple(&'a [Ty<'a>]),
    Record(&'a [Ty<'a>]),
    Fun(&'a [Ty<'a>], Ty<'a>),
}

/// A user ADT instantiated at ground type arguments; constructor field
/// types are looked up through `Adts` so recursive types stay finite.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AdtRef<'a> {
    pub name: QualifiedName<'a>,
    pub args: &'a [Ty<'a>],
}

impl<'a> Ty<'a> {
    pub fn kind(self) -> Kind {
        match self {
            Ty::Big(_) => Kind::Big,
            Ty::Const(_) => Kind::Const,
            Ty::Term(_) => Kind::Term,
        }
    }

    pub fn plutus_type(self, arena: &'a nash_plutus::arena::Arena) -> &'a nash_plutus::typ::Type<'a> {
        use nash_plutus::typ::Type;
        match self {
            Ty::Big(_) => Type::data(arena),
            Ty::Const(c) => match c {
                ConstTy::Int => Type::integer(arena),
                ConstTy::Bytes => Type::byte_string(arena),
                ConstTy::String => Type::string(arena),
                ConstTy::Bool => Type::bool(arena),
                ConstTy::Unit => Type::unit(arena),
                ConstTy::List(t) => Type::list(arena, t.plutus_type(arena)),
                ConstTy::Pair(a, b) => Type::pair(arena, a.plutus_type(arena), b.plutus_type(arena)),
                ConstTy::Array(t) => Type::array(arena, t.plutus_type(arena)),
                ConstTy::BlsG1 => Type::g1(arena),
                ConstTy::BlsG2 => Type::g2(arena),
                ConstTy::BlsMlr => Type::ml_result(arena),
                ConstTy::Value => Type::value(arena),
            },
            Ty::Term(_) => unreachable!("Term-kinded values are never constants"),
        }
    }
}

/// Constructor layouts of every ADT instance mentioned in a program.
pub struct Adts<'a> {
    pub layouts: std::collections::HashMap<AdtRef<'a>, &'a [&'a [Ty<'a>]]>,
}
```

`crates/nash-ir/src/core.rs`:

```rust
//! The Core IR. One tree, monomorphized, every binder typed. See
//! docs/codegen.md for node semantics and lowering.

use nash_plutus::builtin::DefaultFunction;
use nash_plutus::constant::{Constant, Integer};

use crate::ty::Ty;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Name<'a> {
    pub text: &'a str,
    pub unique: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct Binder<'a> {
    pub name: Name<'a>,
    pub ty: Ty<'a>,
}

#[derive(Debug)]
pub enum Core<'a> {
    Var(Name<'a>),
    Lit(&'a Constant<'a>),
    Lam {
        params: &'a [Binder<'a>],
        body: &'a Core<'a>,
    },
    App {
        func: &'a Core<'a>,
        args: &'a [&'a Core<'a>],
    },
    Let {
        binder: Binder<'a>,
        value: &'a Core<'a>,
        body: &'a Core<'a>,
    },
    LetRec {
        binders: &'a [RecBinder<'a>],
        body: &'a Core<'a>,
    },
    Case {
        kind: CaseKind,
        scrutinee: &'a Core<'a>,
        branches: &'a [Branch<'a>],
        default: Option<&'a Core<'a>>,
    },
    Constr {
        tag: u16,
        fields: &'a [&'a Core<'a>],
    },
    Field {
        record: &'a Core<'a>,
        index: u16,
        arity: u16,
    },
    Builtin {
        func: DefaultFunction,
        args: &'a [&'a Core<'a>],
    },
    Cast {
        kind: CastKind,
        from: Ty<'a>,
        to: Ty<'a>,
        arg: &'a Core<'a>,
    },
    Trace {
        message: &'a Core<'a>,
        body: &'a Core<'a>,
    },
    Error,
    Delay(&'a Core<'a>),
    Force(&'a Core<'a>),
}

#[derive(Debug)]
pub struct RecBinder<'a> {
    pub binder: Binder<'a>,
    pub params: &'a [Binder<'a>],
    /// Indices into `params` that every self call passes through unchanged.
    pub static_params: &'a [u16],
    pub body: &'a Core<'a>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaseKind {
    Tag,
    Bool,
    Int,
    Bytes,
    List,
    Data,
}

#[derive(Debug)]
pub struct Branch<'a> {
    pub test: Test<'a>,
    /// Fields bound by the test: constructor fields for `Tag`, `[head, tail]`
    /// for `Cons`, `[tag, fields]` for `DataConstr`, one binder for the
    /// other `Data` shapes, none for literals.
    pub binders: &'a [Binder<'a>],
    pub body: &'a Core<'a>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Test<'a> {
    Tag(u16),
    True,
    False,
    Int(&'a Integer),
    Bytes(&'a [u8]),
    Nil,
    Cons,
    DataConstr,
    DataMap,
    DataList,
    DataI,
    DataB,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CastKind {
    ToData,
    FromDataShallow,
    ValidateData,
    Lift,
    Lower,
}

/// A whole program: top-level bindings in dependency order plus the root.
#[derive(Debug)]
pub struct Module<'a> {
    pub bindings: &'a [(Binder<'a>, &'a Core<'a>)],
    pub root: &'a Core<'a>,
}
```

`crates/nash-ir/src/build.rs` — arena constructors, one per node, in the
style of `Term::apply` / `Term::lambda`:

```rust
pub struct Builder<'a> {
    pub arena: &'a Arena,
    next_unique: Cell<u32>,
}

impl<'a> Builder<'a> {
    pub fn fresh(&self, text: &'a str) -> Name<'a> { ... }
    pub fn var(&self, name: Name<'a>) -> &'a Core<'a> { self.arena.alloc(Core::Var(name)) }
    pub fn int(&self, i: i128) -> &'a Core<'a> { self.arena.alloc(Core::Lit(Constant::integer_from(self.arena, i))) }
    pub fn lam(&self, params: &[Binder<'a>], body: &'a Core<'a>) -> &'a Core<'a> { ... }
    pub fn app(&self, func: &'a Core<'a>, args: &[&'a Core<'a>]) -> &'a Core<'a> { ... }
    pub fn let_(&self, binder: Binder<'a>, value: &'a Core<'a>, body: &'a Core<'a>) -> &'a Core<'a> { ... }
    pub fn builtin(&self, func: DefaultFunction, args: &[&'a Core<'a>]) -> &'a Core<'a> {
        debug_assert!(args.len() <= func.arity());
        ...
    }
    pub fn if_(&self, c: &'a Core<'a>, t: &'a Core<'a>, e: &'a Core<'a>) -> &'a Core<'a> { ... }
    // one per remaining node
}
```

`Arena::alloc` returns `&mut T`; slices go through
`arena.alloc_slice_copy` — add `pub fn alloc_slice_copy<T: Copy>(&self, s: &[T]) -> &[T]`
to `crates/nash-plutus/src/arena.rs` delegating to the inner `Bump`.

`crates/nash-ir/src/pretty.rs` — `pub fn pretty(core: &Core<'_>) -> String`.
Output format (stable, used by every snapshot in this plan):

```
let x#1 : int = 1 in
(\y#2 : int -> addInteger x#1 y#2) 2
```

`case` prints one branch per line with its test and binders:

```
case@Tag s#3 of
  0 a#4 -> a#4
  1 n#5 a#6 -> a#6
```

Types print in Nash surface syntax (`int`, `list Int`, `option int`,
`(int, bytes)`, `int -> int`).

**Elm/Aiken reference**: `air.rs` for the node inventory;
`tree.rs` `AirTree` for which nodes carry types (`Ty` is only on binders
and casts in Nash); Elm `AST/Optimized.hs` for a tree IR with hoisted
decision trees.

**Tests** (`crates/nash-ir/src/pretty.rs`, `mod tests`):

- `pretty_let_app`: hand-built `let x = 1 in (\y -> addInteger x y) 2`.
- `pretty_case_tag`: the two-branch `case@Tag` above.
- `pretty_case_data`: five branches plus default.
- `pretty_letrec_static`: a `LetRec` with `static_params = [0]`.

**Done when**: `cargo test -p nash-ir` passes with four accepted
snapshots; `cargo clippy --all-targets -- -D warnings` is clean.

---

## Chunk 2 — UPLC printer, `Name -> DeBruijn`, and Core -> Term lowering

**Files**

- `crates/nash-plutus/src/pretty.rs` (new), `crates/nash-plutus/src/lib.rs`
- `crates/nash-plutus/src/binder/name.rs` (derive `PartialEq, Eq, Hash, Clone, Copy`; add `pub fn text(&self)`, `pub fn unique(&self)`)
- `crates/nash-plutus/src/debruijn.rs` (new)
- `crates/nash-codegen/Cargo.toml` (new)
- `crates/nash-codegen/src/lib.rs` (new)
- `crates/nash-codegen/src/lower.rs` (new)
- `crates/nash-codegen/src/harness.rs` (new, `#[cfg(test)]`-exported helper)

**Change**

nash-plutus gains a textual printer for `Term<V>` in the standard UPLC
syntax that its own `syn` parser accepts, and a `Term<Name>` ->
`Term<DeBruijn>` conversion. nash-codegen gains the lowering of the node
subset that does not need types beyond `Ty::plutus_type`: `Var`, `Lit`,
`Lam`, `App`, `Let`, `Case(Bool)`, `Builtin`, `Trace`, `Error`, `Delay`,
`Force`, `Constr`, `Field`, `Case(Tag)`. The remaining kinds are added in
the chunks that produce them.

**Code**

`crates/nash-plutus/src/pretty.rs`:

```rust
//! Textual UPLC in the syntax `syn` parses back.

pub trait PrettyVar {
    fn write(&self, w: &mut String);
}

impl PrettyVar for Name<'_> {
    fn write(&self, w: &mut String) {
        write!(w, "{}_{}", self.text, self.unique).unwrap();
    }
}

impl PrettyVar for DeBruijn { fn write(&self, w: &mut String) { write!(w, "i{}", self.index()).unwrap() } }
impl PrettyVar for NamedDeBruijn<'_> { ... }

pub fn program<V: PrettyVar>(program: &Program<'_, V>) -> String
pub fn term<V: PrettyVar>(term: &Term<'_, V>) -> String
```

Layout follows Aiken `crates/uplc/src/pretty.rs`: `(lam x body)`,
`[f a]`, `(delay t)`, `(force t)`, `(con integer 1)`, `(builtin addInteger)`,
`(error)`, `(constr 0 a b)`, `(case s b0 b1)`, with a two-space indent per
nesting level and short terms kept on one line.

`crates/nash-plutus/src/debruijn.rs`:

```rust
//! `Term<Name>` -> `Term<DeBruijn>`. Indices are 1-based distance to the
//! binder, matching `Machine`'s `Env::lookup`.

#[derive(Debug)]
pub struct FreeVariable<'a>(pub &'a Name<'a>);

pub fn to_debruijn<'a>(
    arena: &'a Arena,
    term: &'a Term<'a, Name<'a>>,
) -> Result<&'a Term<'a, DeBruijn>, FreeVariable<'a>> {
    let mut scope: Vec<usize> = Vec::new();
    convert(arena, &mut scope, term)
}

fn convert<'a>(
    arena: &'a Arena,
    scope: &mut Vec<usize>,
    term: &'a Term<'a, Name<'a>>,
) -> Result<&'a Term<'a, DeBruijn>, FreeVariable<'a>> {
    Ok(match term {
        Term::Var(name) => {
            let position = scope
                .iter()
                .rposition(|u| *u == name.unique())
                .ok_or(FreeVariable(name))?;
            Term::var(arena, DeBruijn::new(arena, scope.len() - position))
        }
        Term::Lambda { parameter, body } => {
            scope.push(parameter.unique());
            let body = convert(arena, scope, body)?;
            scope.pop();
            body.lambda(arena, DeBruijn::zero(arena))
        }
        Term::Apply { function, argument } => {
            convert(arena, scope, function)?.apply(arena, convert(arena, scope, argument)?)
        }
        Term::Delay(t) => convert(arena, scope, t)?.delay(arena),
        Term::Force(t) => convert(arena, scope, t)?.force(arena),
        Term::Case { constr, branches } => {
            let constr = convert(arena, scope, constr)?;
            let branches = alloc_try(arena, branches.iter().map(|b| convert(arena, scope, b)))?;
            Term::case(arena, constr, branches)
        }
        Term::Constr { tag, fields } => {
            let fields = alloc_try(arena, fields.iter().map(|f| convert(arena, scope, f)))?;
            Term::constr(arena, *tag, fields)
        }
        Term::Constant(c) => Term::constant(arena, c),
        Term::Builtin(f) => Term::builtin(arena, f),
        Term::Error => Term::error(arena),
    })
}
```

`DeBruijn::new` is `pub` already; `DeBruijn::index` comes from the `Eval`
impl (`crates/nash-plutus/src/binder/debruijn.rs:52`).

`crates/nash-codegen/src/lower.rs`:

```rust
//! Core -> UPLC `Term<Name>`. Structural; every representation decision
//! was made upstream.

use nash_ir::core::{Branch, CaseKind, Core, Name as CoreName, Test};
use nash_plutus::arena::Arena;
use nash_plutus::binder::Name;
use nash_plutus::term::Term;

pub struct Lower<'a> {
    pub arena: &'a Arena,
}

impl<'a> Lower<'a> {
    pub fn term(&self, core: &Core<'a>) -> &'a Term<'a, Name<'a>> {
        match core {
            Core::Var(n) => Term::var(self.arena, self.name(*n)),
            Core::Lit(c) => Term::constant(self.arena, c),
            Core::Lam { params, body } => params
                .iter()
                .rev()
                .fold(self.term(body), |body, p| body.lambda(self.arena, self.name(p.name))),
            Core::App { func, args } => args
                .iter()
                .fold(self.term(func), |f, a| f.apply(self.arena, self.term(a))),
            Core::Let { binder, value, body } => self
                .term(body)
                .lambda(self.arena, self.name(binder.name))
                .apply(self.arena, self.term(value)),
            Core::Builtin { func, args } => {
                let head = (0..func.force_count())
                    .fold(Term::builtin(self.arena, self.arena.alloc(*func)), |t, _| t.force(self.arena));
                args.iter().fold(head, |f, a| f.apply(self.arena, self.term(a)))
            }
            Core::Case { kind: CaseKind::Bool, scrutinee, branches, .. } => {
                let (t, e) = bool_branches(branches);
                Term::if_then_else(self.arena)
                    .force(self.arena)
                    .apply(self.arena, self.term(scrutinee))
                    .apply(self.arena, self.term(t).delay(self.arena))
                    .apply(self.arena, self.term(e).delay(self.arena))
                    .force(self.arena)
            }
            Core::Case { kind: CaseKind::Tag, scrutinee, branches, default: None } => {
                let arms = self.arena.alloc_slice_fill_iter(branches.iter().map(|b| {
                    b.binders
                        .iter()
                        .rev()
                        .fold(self.term(b.body), |body, p| body.lambda(self.arena, self.name(p.name)))
                }));
                Term::case(self.arena, self.term(scrutinee), arms)
            }
            Core::Constr { tag, fields } => Term::constr(
                self.arena,
                *tag as usize,
                self.arena.alloc_slice_fill_iter(fields.iter().map(|f| self.term(f))),
            ),
            Core::Field { record, index, arity } => {
                let params: Vec<CoreName> = (0..*arity).map(|i| self.fresh("f", i)).collect();
                let body = Term::var(self.arena, self.name(params[*index as usize]));
                let selector = params.iter().rev().fold(body, |b, p| b.lambda(self.arena, self.name(*p)));
                Term::case(self.arena, self.term(record), self.arena.alloc_slice_copy(&[selector]))
            }
            Core::Trace { message, body } => Term::trace(self.arena)
                .force(self.arena)
                .apply(self.arena, self.term(message))
                .apply(self.arena, self.term(body).delay(self.arena))
                .force(self.arena),
            Core::Error => Term::error(self.arena),
            Core::Delay(t) => self.term(t).delay(self.arena),
            Core::Force(t) => self.term(t).force(self.arena),
            Core::LetRec { .. } => unreachable!("recursion rewrite runs before lowering"),
            Core::Cast { .. } => unreachable!("casts are lowered in chunk 6"),
            Core::Case { .. } => unreachable!("case kind lowered in a later chunk"),
        }
    }

    fn name(&self, n: CoreName<'a>) -> &'a Name<'a> {
        Name::new(self.arena, n.text, n.unique as usize)
    }
}
```

`Term<V>` in nash-plutus requires `Builtin` to be `&'a DefaultFunction`;
`Term::builtin(arena, arena.alloc(func))` is the idiom
(`crates/nash-plutus/src/term.rs:153`).

`crates/nash-codegen/src/harness.rs` (used by every later test):

```rust
pub struct Evaluated {
    pub uplc: String,
    pub result: String,           // pretty term, or "error: <MachineError Debug>"
    pub logs: Vec<String>,
    pub budget: ExBudget,
}

pub fn eval_core<'a>(arena: &'a Arena, core: &Core<'a>) -> Evaluated {
    let named = Lower { arena }.term(core);
    let uplc = nash_plutus::pretty::term(named);
    let term = nash_plutus::debruijn::to_debruijn(arena, named).expect("closed term");
    let program = Program::new(arena, Version::plutus_v3(arena), term);
    let EvalResult { term, info } = program.eval(arena);
    Evaluated {
        uplc,
        result: match term {
            Ok(t) => nash_plutus::pretty::term(t),
            Err(e) => format!("error: {e:?}"),
        },
        logs: info.logs,
        budget: info.consumed_budget,
    }
}
```

Snapshot format (one snapshot per test, YAML via `insta::assert_snapshot!`
of a `Display` impl on `Evaluated`):

```
--- uplc
(program 1.1.0 [(lam x_1 [[(builtin addInteger) x_1] (con integer 2)]) (con integer 1)])
--- result
(con integer 3)
--- logs
--- budget
cpu: 123456 mem: 789
```

**Elm/Aiken reference**: `crates/uplc/src/pretty.rs` (`Program::to_pretty`),
`crates/uplc/src/debruijn.rs` (`Converter::name_to_debruijn`),
`gen_uplc.rs` `gen_uplc` arms for `Air::Builtin` (`apply_builtin_forces`),
`Air::If`, `Air::Trace`.

**Tests**

- nash-plutus: `pretty_roundtrip_*` — print a hand-built `Term<DeBruijn>`
  and parse it back with `syn::parse_term`; `debruijn_lambda_var`,
  `debruijn_free_variable_errors`, `debruijn_case_constr`.
- nash-codegen (`lower.rs` tests, hand-built Core through `Builder`):
  `lower_let_app` (the snapshot above), `lower_if_true`, `lower_builtin_forced`
  (`headList` gets one `force`), `lower_constr_field` (`Field` of a
  3-constr), `lower_case_tag`, `lower_trace_error` (logs contain the
  message, result is an error).

**Done when**: all nash-plutus tests still pass; six lowering snapshots
accepted; `Evaluated` displays budgets.

---

## Chunk 3 — Can -> Core for the `Const`-only subset

**Files**

- `crates/nash-solve/src/lib.rs`, `crates/nash-solve/src/solved.rs` (new;
  [03-traits.md](03-traits.md) owns the population of this table, this chunk defines it
  and fills `exprs` for the subset from the existing `UnionFind`)
- `crates/nash-codegen/src/ty_of.rs` (new)
- `crates/nash-codegen/src/can_to_core.rs` (new)
- `crates/nash-codegen/src/lib.rs`

**Change**

Define the solver output codegen consumes, the Can type -> `Ty`
conversion, and the expression translator for `Expr::Int`, `Str`,
`Unit`, `VarLocal`, `VarTopLevel`, `Lambda` (variable patterns only),
`Call`, `Let` (simple `Def::Def` with no args or with variable-pattern
args), `If`, `Binop` (resolved to a builtin by chunk 4, until
then only `Builtin.*` calls). Everything else panics with the node name;
each later chunk removes one panic. There is no `Negate` arm:
[03-traits.md](03-traits.md) deletes `Expr::Negate` and canonicalizes `-e`
to a `Num.negate` method call, which chunk 9 resolves like any other
method.

**Code**

`crates/nash-solve/src/solved.rs`:

```rust
//! Per-node results of solving, keyed by node identity. Elm keeps only the
//! top-level annotations; Nash's monomorphizer needs every instantiation.
//! The types below are the contract fixed in plans/03-traits.md
//! ("Contract with plans/07-codegen.md"); that section is authoritative.

use std::collections::HashMap;

use nash_ast::{Annotation, Evidence, Type as CanType};
use nash_region::Located;

pub use nash_ast::NodeId;   // arena address of a `Located<Expr>` / `Located<Pattern>` / def name

pub struct SolvedTypes<'a> {
    pub exprs: HashMap<NodeId, &'a Located<CanType<'a>>>,       // this chunk fills
    pub patterns: HashMap<NodeId, &'a Located<CanType<'a>>>,    // this chunk fills
    /// Every `VarLocal`-to-a-generalized-def, `VarTopLevel`, `VarForeign`,
    /// `VarOperator`, `VarMethod`, `Binop`, literal, and `<-` node.
    pub instances: HashMap<NodeId, Instance<'a>>,               // 03-traits.md fills
    /// Every named definition and generalized destructuring pattern: its scheme.
    pub schemes: HashMap<NodeId, Scheme<'a>>,                   // 03-traits.md fills
}

pub struct Instance<'a> {
    /// The scheme's `free_vars`, in `Annotation.free_vars` order, at this use.
    pub type_args: &'a [&'a Located<CanType<'a>>],
    /// One per scheme context predicate, in `Annotation.context` order.
    pub evidence: &'a [Evidence<'a>],
}

pub struct Scheme<'a> {
    pub annotation: &'a Annotation<'a>,
    /// `Evidence::Given { binder }` inside the body refers to this def.
    pub binder: NodeId,
}
```

A generalized let-destructuring owns one aggregate scheme keyed by
`NodeId::pattern` of its original root pattern. Its type is the full RHS/pattern
type, and its context and quantifier order are shared by all extracted names.
A use of an extracted name instantiates the aggregate type, context and selected
component together, then returns the component type. Its `Instance` contains
all aggregate type arguments, including those absent from that component, and
all aggregate evidence slots. Codegen associates that lexical name with its
root pattern scheme and projection; evidence inside the RHS refers to the
pattern binder. Tuple, record and alias patterns use the same rule. This
preserves polymorphic destructuring without losing qualified constraints.

`nash_ast::Evidence` is
`Impl { impl_: ImplRef { home, key }, type_args, args } | Given { binder, index } | Super { of, index }`
(03-traits.md chunk 1) and derives `PartialEq, Eq, Hash`.

`nash_solve::run` returns `SolvedTypes` alongside `Annotations`; its final
signature is defined in [03-traits.md](03-traits.md) chunk 5 ("Solver
API") and is cited here, not redefined:

```rust
pub fn run<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    constraint: &Constraint<'a>,
    tables: &'a nash_can::Tables<'a>,
    fields: &'a nash_can::FieldTable<'a>,
    mode: nash_can::Mode,
) -> Result<(Annotations<'a>, SolvedTypes<'a>), Vec<Error<'a>>>
```
The constrain pass records `NodeId -> Variable` while it walks
(`crates/nash-constrain/src/expression.rs`), and `run` resolves each
through `to_annotation` after solving. This chunk fills `exprs` and
`patterns`; `instances` and `schemes` are filled by 03-traits.md and are
empty until then (the `Const`-only subset has no instantiations).

`crates/nash-codegen/src/ty_of.rs`:

```rust
//! Can type -> `Ty`. Casing of the head name picks the kind (docs/kinds.md);
//! type variables are looked up in the current monomorphization substitution.

pub struct TyEnv<'a> {
    pub arena: &'a Arena,
    pub subst: HashMap<&'a str, Ty<'a>>,
    pub unions: &'a HashMap<QualifiedName<'a>, &'a Union<'a>>,
    pub aliases: &'a HashMap<QualifiedName<'a>, &'a Alias<'a>>,
    pub adts: &'a mut Adts<'a>,
}

impl<'a> TyEnv<'a> {
    pub fn ty(&mut self, t: &Located<CanType<'a>>) -> Ty<'a> {
        match &t.value {
            CanType::Var(v) => self.subst[v],
            CanType::Lambda { from, to } => { /* collect the spine into Fun(params, ret) */ }
            CanType::Unit => Ty::Const(self.arena.alloc(ConstTy::Unit)),
            CanType::Tuple { first, second, rest } => Ty::Term(self.arena.alloc(TermTy::Tuple(..))),
            CanType::Alias { reference, arguments, target } => self.alias(reference, arguments, target),
            CanType::Named { reference, args } => self.named(*reference, args),
            CanType::Record { .. } => {
                // 04-representation.md A1: no `ext`; a record type only occurs as an alias target.
                unreachable!("record types reach codegen through their alias")
            }
        }
    }

    fn named(&mut self, reference: QualifiedName<'a>, args: &[&Located<CanType<'a>>]) -> Ty<'a> {
        let args = self.arena.alloc_slice_fill_iter(args.iter().map(|a| self.ty(a)));
        // Primitive types live in the core `Builtin` module (04-representation.md C1).
        let primitive = reference.home == nash_ast::primitives::builtin_home();
        match (primitive, reference.name) {
            (true, "int") => Ty::Const(self.arena.alloc(ConstTy::Int)),
            (true, "Int") => Ty::Big(self.arena.alloc(BigTy::Int)),
            (true, "list") => Ty::Const(self.arena.alloc(ConstTy::List(args[0]))),
            (true, "List") => Ty::Big(self.arena.alloc(BigTy::List(args[0]))),
            (true, "Data") => Ty::Big(self.arena.alloc(BigTy::Data)),
            // bytes/Bytes, string, bool, unit, pair, array, Map, bls_*, value
            _ => {
                let adt = AdtRef { name: reference, args };
                self.register(adt);
                if reference.name.starts_with(char::is_uppercase) {
                    Ty::Big(self.arena.alloc(BigTy::Adt(adt)))
                } else {
                    Ty::Term(self.arena.alloc(TermTy::Adt(adt)))
                }
            }
        }
    }

    /// Records the constructor layout of `adt` in `self.adts` (once):
    /// one `&[Ty]` per constructor from `Ctor::args.types()` in wire order.
    fn register(&mut self, adt: AdtRef<'a>) { ... uses self.unions[adt.name].ctors ... }
}
```

Big record aliases become `BigTy::Record(field tys)`, little record aliases
`TermTy::Record(field tys)`; field order is the alias's declaration order
(`FieldType::index`, `crates/nash-ast/src/lib.rs:283`), which the kinds
plan makes canonicalization fill in.

`crates/nash-codegen/src/can_to_core.rs`:

```rust
//! Can AST -> Core. One traversal does monomorphization, trait-method
//! resolution, decision trees and desugaring.

pub struct Gen<'a> {
    pub build: Builder<'a>,
    pub solved: &'a SolvedTypes<'a>,
    pub tys: TyEnv<'a>,
    pub locals: HashMap<&'a str, Name<'a>>,      // Can local name -> unique Core name
    pub ctx: &'a Build<'a>,                      // chunk 9; every solved module of the build
    pub mono: Mono<'a>,                          // chunk 9; a one-entry stub until then
    pub trace: TraceConfig,                      // chunk 10
}

impl<'a> Gen<'a> {
    pub fn expr(&mut self, e: &'a Located<Expr<'a>>) -> &'a Core<'a> {
        match &e.value {
            Expr::Int(i) => self.literal_int(e, *i),
            Expr::Str(s) => self.literal_str(e, s),
            Expr::Unit => self.build.unit(),
            Expr::VarLocal(name) => self.build.var(self.locals[name]),
            Expr::VarTopLevel(q) => self.build.var(self.mono.request(self.ctx, *q, &self.solved.instances[&NodeId::expr(e)], &mut self.tys)),
            Expr::Lambda { parameters, body } => self.lambda(parameters, body),
            Expr::Call { function, arguments } => {
                let func = self.expr(function);
                let args = self.build.arena.alloc_slice_fill_iter(arguments.iter().map(|a| self.expr(a)));
                self.build.app(func, args)
            }
            Expr::If { branches, final_else } => branches.iter().rev().fold(self.expr(final_else), |e, b| {
                self.build.if_(self.expr(b.condition), self.expr(b.then_branch), e)
            }),
            Expr::Let { definition, body } => self.let_(definition, body),
            other => todo!("chunk 4+: {}", node_name(other)),
        }
    }

    fn literal_int(&mut self, e: &'a Located<Expr<'a>>, i: i128) -> &'a Core<'a> {
        match self.ty_at(e) {
            Ty::Const(ConstTy::Int) => self.build.int(i),
            Ty::Big(BigTy::Int) => self.build.lit(Constant::data(self.arena, PlutusData::integer_from(self.arena, i))),
            other => unreachable!("FromInt at {other:?} is a user impl, resolved by chunk 9"),
        }
    }

    fn lambda(&mut self, parameters: &'a [&'a Located<Pattern<'a>>], body: &'a Located<Expr<'a>>) -> &'a Core<'a> {
        let params = self.bind_var_patterns(parameters);   // chunk 5 generalizes to any pattern
        let body = self.expr(body);
        self.build.lam(params, body)
    }

    fn ty_at(&mut self, e: &'a Located<Expr<'a>>) -> Ty<'a> {
        self.tys.ty(self.solved.exprs[&NodeId::expr(e)])
    }
}

pub fn module<'a>(arena: &'a Arena, module: &'a nash_ast::Module<'a>, solved: &'a SolvedTypes<'a>, root: &str) -> nash_ir::core::Module<'a>
```

`module` walks `Decls` in order (`Declare` -> one `Let`-style binding,
`DeclareRec` -> a `LetRec` group, chunk 8), builds the binding for `root`
and returns the `Module`. Until chunk 9 there is no worklist: every
top-level definition is emitted once, at its annotation's type, with type
variables mapped to a placeholder that panics if reached.

**Elm/Aiken reference**: Elm `Generate/JavaScript/Expression.hs`
(`generate`), Aiken `gen_uplc.rs` `build` arms for `TypedExpr::Int`,
`Var`, `Fn`, `Call`, `If`, `Assignment`.

**Tests** (`crates/nash-codegen/src/can_to_core.rs`, macros
`assert_core_snapshot!(src)` and `assert_eval_snapshot!(src)`; the module
is parsed, canonicalized, constrained and solved exactly as
`crates/nash-driver/src/compile.rs:151` does, calling `nash_solve::run`
with the [03-traits.md](03-traits.md) chunk 5 signature quoted above, then
`module` is called with
root `main` and the result is pretty-printed / evaluated with the chunk 2
harness):

```rust
assert_eval_snapshot!("main = 42");
assert_eval_snapshot!("main = Builtin.addInteger 1 2");
assert_eval_snapshot!(r#"
    main =
        let
            add x y = Builtin.addInteger x y
        in
        add 20 22
"#);
assert_eval_snapshot!("main = if Builtin.lessThanInteger 1 2 then 1 else 0");
assert_eval_snapshot!("main = (\\x -> Builtin.multiplyInteger x x) 7");
assert_eval_snapshot!("main = -5");
```

**Done when**: six snapshots accepted with evaluation results `42`, `3`,
`42`, `1`, `49`, `-5`; `Builtin.*` resolves through the chunk 4 table
(land chunk 4 first if the tests need it; the two chunks may be one PR).

---

## Chunk 4 — The `Builtin` module

**Files**

- `crates/nash-codegen/src/builtins.rs` (new)
- `crates/nash-ast/src/primitives/builtins.rs` (the synthetic module's typed
  value table; Builtin has no source file)

**Change**

`Builtin.addInteger` and friends canonicalize as `Expr::VarForeign` with
`reference.home == nash_ast::primitives::builtin_home()`. Codegen maps the
table's symbolic variant name to a
`DefaultFunction` and emits `Core::Builtin` with as many arguments as the
call site supplies (at most `arity()`; extra arguments become an outer
`App`). A bare reference (`Builtin.addInteger` passed as a value) is a
`Builtin` with no arguments, i.e. the forced builtin value.

**Code**

```rust
pub fn by_name(name: &str) -> Option<DefaultFunction> {
    Some(match name {
        "addInteger" => DefaultFunction::AddInteger,
        "subtractInteger" => DefaultFunction::SubtractInteger,
        // ... one line per variant of `crates/nash-plutus/src/builtin/default_function.rs`,
        // camelCase of the variant name; the test below checks the table is total.
        _ => return None,
    })
}

impl<'a> Gen<'a> {
    fn call_builtin(&mut self, func: DefaultFunction, arguments: &'a [&'a Located<Expr<'a>>]) -> &'a Core<'a> {
        let arity = func.arity();
        let (now, later) = arguments.split_at(arguments.len().min(arity));
        let args = self.build.arena.alloc_slice_fill_iter(now.iter().map(|a| self.expr(a)));
        let call = self.build.builtin(func, args);
        if later.is_empty() { call } else {
            let rest = self.build.arena.alloc_slice_fill_iter(later.iter().map(|a| self.expr(a)));
            self.build.app(call, rest)
        }
    }
}
```

`Builtin.nash` declares each builtin with its Nash type, e.g.
`addInteger : int -> int -> int`, `headList : list 'a -> 'a`,
`unConstrData : Data -> pair int (list Data)`, `ifThenElse : bool -> 'a -> 'a -> 'a`
(strict; the `if` syntax is the lazy one), `chooseData : Data -> 'a -> 'a -> 'a -> 'a -> 'a -> 'a`.
The `Builtin` module has no bodies: canonicalization treats it like a
foreign interface whose values are all "builtin" (`nash-can` gets a
`Interface` for it generated from the same table).

**Elm/Aiken reference**: Aiken `crates/aiken-lang/src/builtins.rs`
(`from_default_function`, the typed table), `gen_uplc.rs`
`Air::Builtin` arm and `special_case_builtin` (`builder.rs:1084`; Nash does
not special-case, it uses `if` for laziness).

**Tests**

- `builtins_table_is_total`: iterate over every `DefaultFunction` (add
  `pub const ALL: &[DefaultFunction]` to nash-plutus) and assert `by_name`
  of its camelCase name returns it.
- `assert_eval_snapshot!("main = Builtin.headList [1, 2, 3]")` (needs the
  list literal from chunk 5; keep in chunk 5 if not yet available).
- `assert_eval_snapshot!("main = Builtin.lengthOfByteString #\"cafe\"")`.
- `assert_core_snapshot!("main = Builtin.addInteger 1")` shows a partial
  `Builtin` node.
- `assert_eval_snapshot!("main = (Builtin.addInteger 1) 2")` shows the
  outer `App` and evaluates to `3`.

**Done when**: table total; four snapshots accepted.

---

## Chunk 5 — Little ADTs, tuples, lists, and decision trees

**Files**

- `crates/nash-codegen/src/decision_tree.rs` (new)
- `crates/nash-codegen/src/accessors.rs` (new)
- `crates/nash-codegen/src/can_to_core.rs` (constructors, tuples, lists,
  `Case`, `LetDestruct`, argument patterns)
- `crates/nash-codegen/src/lower.rs` (`Case(Int)`, `Case(Bytes)`,
  `Case(List)`)

**Change**

Port Aiken's decision-tree compiler to produce `Core` for `Term`-kinded and
`Const`-kinded scrutinees. Constructor application becomes `Constr`, tuple
literals `Constr(0)`, list literals `Lit` or `mkCons` chains.

**Code**

`decision_tree.rs`:

```rust
//! Maranget decision trees with hoisted leaves and memoized accessor
//! paths. Port of Aiken's gen_uplc/decision_tree.rs.

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Path {
    Tuple(u16),          // field i of a constr 0
    Constr(u16),         // field i of the matched constructor (Term ADT)
    BigField(u16),       // element i of `sndPair (unConstrData d)`
    RecordField(u16),    // element i of `unListData d` (Big record)
    ListHead(u16),       // headList (tailList^i xs)
    ListTail(u16),       // tailList^i xs
    DataTag,             // fstPair (unConstrData d)
    DataFields,          // sndPair (unConstrData d)
    DataList,            // unListData d
    DataMap,             // unMapData d
    DataI,               // unIData d
    DataB,               // unBData d
    PairFst,
    PairSnd,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaseTest<'a> {
    Tag(u16),
    Int(&'a Integer),
    Bytes(&'a [u8]),
    List(u16),
    ListWithTail(u16),
    Data(DataShape),
    Wild,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataShape { Constr, Map, List, I, B }

#[derive(Clone, Copy, Debug)]
pub struct Assigned<'a> {
    pub path: &'a [Path],
    pub name: Name<'a>,
}

#[derive(Clone, Copy)]
struct RowItem<'a> {
    path: &'a [Path],
    pattern: &'a Located<Pattern<'a>>,
}

struct Row<'a> {
    assigns: Vec<Assigned<'a>>,
    columns: Vec<RowItem<'a>>,
    leaf: LeafId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LeafId(pub u16);

pub enum DecisionTree<'a> {
    Switch {
        path: &'a [Path],
        cases: Vec<(CaseTest<'a>, DecisionTree<'a>)>,
        default: Option<Box<DecisionTree<'a>>>,
    },
    ListSwitch {
        path: &'a [Path],
        cases: Vec<(CaseTest<'a>, DecisionTree<'a>)>,
        tail_cases: Vec<(CaseTest<'a>, DecisionTree<'a>)>,
        default: Option<Box<DecisionTree<'a>>>,
    },
    Leaf(LeafId, Vec<Assigned<'a>>),
    Hoist {
        leaf: LeafId,
        params: Vec<Name<'a>>,     // the pattern variables the body uses
        body: &'a Located<Expr<'a>>,
        then: Box<DecisionTree<'a>>,
    },
}

pub struct TreeGen<'a, 'g> {
    gen: &'g mut Gen<'a>,
    subject_ty: Ty<'a>,
}

impl<'a, 'g> TreeGen<'a, 'g> {
    pub fn build(self, clauses: &'a [CaseBranch<'a>]) -> DecisionTree<'a>;
    fn map_pattern_to_row(&mut self, pattern: &'a Located<Pattern<'a>>, path: Vec<Path>) -> (Vec<Assigned<'a>>, Vec<RowItem<'a>>);
    fn do_build_tree(&mut self, matrix: Vec<Row<'a>>) -> DecisionTree<'a>;
}

fn highest_occurrence(rows: &[Row<'_>], columns: usize) -> Option<usize>;
pub fn ty_by_path<'a>(adts: &Adts<'a>, subject: Ty<'a>, path: &[Path]) -> Ty<'a>;
```

`map_pattern_to_row` on `nash_ast::Pattern` (`crates/nash-ast/src/lib.rs:228`):
`Anything` -> nothing; `Var(n)` -> assign; `Alias { pattern, name }` ->
assign plus recurse; `Unit` -> nothing (irrefutable); `Tuple` ->
recurse into each element with `Path::Tuple(i)`; `Record(fields)` ->
one assign per field with `Path::RecordField(i)` / `Path::Tuple(i)` by
kind; `List(elems)` and `Cons` -> a column with the flattened list pattern
(`Cons` chains are flattened to `[p0, p1 | tail]`); `Constructor(ctor)` ->
a column over the already-positional canonical pattern (nash-can has
rewritten the labeled sugar `Datum { owner, deadline }` to
`Datum owner deadline` in wire order, with `Anything` for omitted labels,
per [04-representation.md](04-representation.md) A5); `Bool` -> a column (`CaseTest::Tag(0|1)` on `Const bool` lowers
to `Case(Bool)`); `Int`, `Str` -> columns with literal tests.

`accessors.rs` (port of `stick_break_set.rs`):

```rust
/// One projection step from a parent value to a child, in `Core` terms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Field(u16, u16),        // Core::Field(record, index, arity)
    HeadList,
    TailList,
    UnConstrData,
    FstPair,
    SndPair,
    UnListData,
    UnMapData,
    UnIData,
    UnBData,
}

pub fn steps(subject: Ty<'a>, adts: &Adts<'a>, path: &[Path]) -> Vec<Step>;

/// Which step chains are already bound in the current scope, and under
/// what name. `bind` returns the lets that still have to be emitted.
pub struct Bound<'a> {
    root: Name<'a>,
    children: Vec<(Step, Name<'a>, Bound<'a>)>,
}

impl<'a> Bound<'a> {
    pub fn resolve(&mut self, build: &Builder<'a>, steps: &[Step]) -> (Name<'a>, Vec<(Binder<'a>, &'a Core<'a>)>);
}
```

`TreeGen::emit(tree, bound) -> &'a Core<'a>` walks the tree: for a `Switch`
it resolves the path to a name (emitting the missing accessor lets around
the `Case`), builds the `Case` of the kind matching `ty_by_path`, and
recurses with a cloned `Bound` per branch; for `Leaf` it emits
`App(Var leaf_fn, assigned values)`; for `Hoist` it emits
`Let(leaf_fn, Lam(params, body), then)`. Leaves used once are inlined by
plan 08 rather than special-cased here.

Constructors in `can_to_core.rs`:

```rust
Expr::VarConstructor { reference, index, .. } => {
    let ty = self.ty_at(e);                       // the constructor's function type or the ADT
    match ty {
        Ty::Term(TermTy::Fun(params, ret)) => self.constructor_fn(*index, params, ret),   // eta-expand
        Ty::Term(TermTy::Adt(_)) => self.build.constr(*index, &[]),
        Ty::Big(_) => todo!("chunk 6"),
        _ => unreachable!(),
    }
}
Expr::Tuple { first, second, rest } => self.build.constr(0, [first, second] ++ rest),
Expr::List(items) => self.list(e, items),
```

A saturated `Call` whose head is a `VarConstructor` builds `Constr`
directly instead of applying the eta-expanded lambda.

`lower.rs` additions:

- `Case(Int)`: fold branches from the default:
  `force (ifThenElse (equalsInteger s (con k)) (delay b_k) (delay rest))`
  with `s` let-bound once by the decision tree.
- `Case(Bytes)`: same with `equalsByteString`.
- `Case(List)`: `force (chooseList s (delay nil) (delay (let h = headList s; t = tailList s in cons)))`.

**Elm/Aiken reference**: `decision_tree.rs` `build_tree` (626),
`do_build_tree` (730), `map_pattern_to_row` (1174), `highest_occurrence`
(1336), `get_hoist_paths` (409), `hoist_by_path` (525);
`stick_break_set.rs` `Builtins::new_from_path`, `TreeSet::diff_union_builtins`,
`Builtins::produce_air`; `gen_uplc.rs` `handle_decision_tree` (2628).
Elm `Optimize/DecisionTree.hs` for the same algorithm over `Can.Pattern`.

**Tests**

```rust
assert_eval_snapshot!(r#"
    type option 'a = None | Some 'a
    main =
        case Some 3 of
            Some x -> x
            None -> 0
"#);
assert_eval_snapshot!(r#"
    main =
        case (1, 2) of
            (a, b) -> Builtin.addInteger a b
"#);
assert_eval_snapshot!(r#"
    main =
        case [1, 2, 3] of
            [] -> 0
            [x] -> x
            x :: y :: _ -> Builtin.addInteger x y
"#);
assert_eval_snapshot!(r#"
    type step 'a = Done 'a | Next int 'a
    main =
        case Next 1 (Done 2) of
            Next n (Done m) -> Builtin.addInteger n m
            Next _ _ -> 1
            Done x -> x
"#);
assert_core_snapshot!(/* the nested case above; shows one accessor let shared by two rows */);
assert_eval_snapshot!(r#"
    main =
        case 5 of
            1 -> 10
            5 -> 50
            _ -> 0
"#);
assert_eval_snapshot!(r#"
    main =
        let (a, b) = (1, 2) in
        Builtin.subtractInteger a b
"#);
```

plus unit tests on `decision_tree.rs` porting Aiken's `thing`..`thing6`
(pretty-printed tree snapshots for the list-with-tail matrices).

**Done when**: seven evaluation snapshots and six tree snapshots accepted;
the `Core` snapshot shows a single `Field` accessor let shared by the two
`Next` rows.

---

## Chunk 6 — Big ADTs, `Data` patterns, and casts

**Files**

- `crates/nash-codegen/src/can_to_core.rs` (Big constructors, `Data`
  constructors, casts)
- `crates/nash-codegen/src/decision_tree.rs` (`Path::BigField`,
  `Path::Data*`, `CaseTest::Data`)
- `crates/nash-codegen/src/checkers.rs` (new)
- `crates/nash-codegen/src/lower.rs` (`Case(Data)`, `Cast`)

**Change**

A Big constructor application becomes `Builtin(ConstrData, [Lit tag, fields])`
where `fields` is a `mkCons` chain of the (already `Data`) field values
onto `Lit(ProtoList(data, []))`. Matching a Big ADT emits
`Let p = unConstrData s` and a `Case(Int)` on `fstPair p`; fields come from
`sndPair p` through the memoized accessors. `Data` constructors and
patterns follow docs/data.md. `toData`/`fromData`/`validateData`/`lift`/
`lower` arrive as trait-method calls resolved (chunk 9) to the stdlib's
impls; those impl bodies are the intrinsic `Cast` nodes, which this chunk
introduces through a small set of intrinsics the stdlib can name:
`Builtin.castToData`, `Builtin.castFromDataShallow`, `Builtin.castValidateData`,
`Builtin.castLift`, `Builtin.castLower`, each typed `'a -> 'b` and only
usable inside `core/`.

**Code**

`checkers.rs`:

```rust
//! One `fromData#T` / `validateData#T` function per Big type, generated on
//! demand and hoisted to the program's top-level bindings.

pub struct Checkers<'a> {
    shallow: HashMap<Ty<'a>, Name<'a>>,
    full: HashMap<Ty<'a>, Name<'a>>,
    pub bindings: Vec<RecBinder<'a>>,   // recursive types need LetRec
}

impl<'a> Checkers<'a> {
    pub fn shallow(&mut self, gen: &mut Gen<'a>, ty: Ty<'a>) -> Name<'a>;
    pub fn full(&mut self, gen: &mut Gen<'a>, ty: Ty<'a>) -> Name<'a>;

    fn full_body(&mut self, gen: &mut Gen<'a>, ty: Ty<'a>, d: Binder<'a>) -> &'a Core<'a> {
        let b = &gen.build;
        match ty {
            Ty::Big(BigTy::Data) => b.var(d.name),
            Ty::Big(BigTy::Int) => choose_data(b, d, DataShape::I, b.var(d.name), gen.compiler_fail("validateData: expected I")),
            Ty::Big(BigTy::Bytes) => choose_data(b, d, DataShape::B, ..),
            Ty::Big(BigTy::List(elem)) => {
                let check_elem = self.full(gen, *elem);
                // let xs = unListData d in validate#each check_elem xs ; d
                ...
            }
            Ty::Big(BigTy::Map(k, v)) => ...,
            Ty::Big(BigTy::Adt(adt)) => {
                // let p = unConstrData d ; case@Int (fstPair p) of tag_i -> check fields_i, arity; _ -> fail
                let layouts = gen.tys.adts.layouts[&adt];
                ...
            }
            Ty::Big(BigTy::Record(fields)) => ...,   // unListData, one check per field, nullList at the end
            _ => unreachable!("only Big types have checkers"),
        }
    }
}

fn choose_data<'a>(b: &Builder<'a>, d: Binder<'a>, want: DataShape, ok: &'a Core<'a>, otherwise: &'a Core<'a>) -> &'a Core<'a> {
    let branch = |shape| if shape == want { ok } else { otherwise };
    b.case_data(b.var(d.name), [branch(Constr), branch(Map), branch(List), branch(I), branch(B)])
}
```

`validate#each` is one shared recursive helper
(`\check xs -> if nullList xs then () else (check (headList xs); each check (tailList xs))`)
emitted as a `LetRec` binding through chunk 8.

`lower.rs` additions:

```rust
Core::Case { kind: CaseKind::Data, scrutinee, branches, default } => {
    // force (chooseData s (delay b_constr) (delay b_map) (delay b_list) (delay b_i) (delay b_b))
    // each present branch binds its payload with the matching un*Data builtin around its body;
    // absent shapes use `default`.
}
Core::Cast { kind: CastKind::ToData, arg, .. } => self.term(arg),
Core::Cast { kind: CastKind::FromDataShallow | CastKind::ValidateData, .. } =>
    unreachable!("replaced by a checker call in can_to_core"),
Core::Cast { kind: CastKind::Lift, from, arg, .. } => match from {
    Ty::Big(_) => self.term(arg),                                             // reflexive `Lift 'a 'a`
    Ty::Const(ConstTy::Int) => Term::i_data(a).apply(a, self.term(arg)),
    Ty::Const(ConstTy::Bytes) => Term::b_data(a).apply(a, self.term(arg)),
    Ty::Const(ConstTy::List(elem)) if elem.kind() == Kind::Big => Term::list_data(a).apply(a, self.term(arg)),
    Ty::Const(ConstTy::List(_)) => Term::map_data(a).apply(a, self.term(arg)),   // list (pair Data Data)
    _ => unreachable!("only the intrinsic Lift impls reach a Cast node"),
},
Core::Cast { kind: CastKind::Lower, to, arg, .. } => /* the un* mirror */,
```

Only the `core/` impls whose body is one builtin (`int`/`Int`,
`bytes`/`Bytes`, `list 'a`/`List 'a` with Big elements, `Map`) use
`Builtin.castLift` / `Builtin.castLower`. The compiler's `ReflexiveLift`
evidence lowers directly to identity. `Lift (list 'a) (List 'b)` given `Lift 'a 'b` is
written in Nash in `core/` as a map of `lift` followed by `castLift`; when
`'a = 'b` is Big the element map is identity, which plan 08
inlines and folds to the bare `listData`. `Lift option Option`,
`Lift result Result` and `Lift ordering Ordering` are ordinary `core/`
functions (see docs/representation.md's `Lift` table) and never become
`Cast` nodes.

`can_to_core.rs` replaces `Cast(FromDataShallow)` / `Cast(ValidateData)`
at construction time with `App(Var checker, [arg])`, so `lower.rs` never
sees them; the `CastKind` variants stay in `Core` for the optimizer's
cancellation rule and for pretty output.

**Elm/Aiken reference**: `builder.rs` `known_data_to_type` (481),
`unknown_data_to_type` (529), `softcast_data_to_type_otherwise` (589),
`convert_type_to_data` (755), `to_data_builtin` (1045), `undata_builtin`
(1016); `gen_uplc.rs` `expect_type_assign` (2016) for the full-check
generator (Nash generates a named function per type instead of inlining
the check at every site).

**Tests**

```rust
assert_eval_snapshot!(r#"
    type Redeemer = Claim | Cancel
    main = case Cancel of
        Claim -> 0
        Cancel -> 1
"#);
assert_core_snapshot!(r#"
    type Datum = Datum Bytes Int
    main d = case d of
        Datum owner deadline -> deadline
"#);   // shows unConstrData bound once, fields via sndPair + headList (tailList ..)
assert_eval_snapshot!(r#"
    main = case Constr 0 [ I 1, B #"ff" ] of
        Constr 0 [ I n, _ ] -> n
        Constr _ _ -> 100
        I n -> n
        _ -> 200
"#);
assert_eval_snapshot!("main = case List [ I 7 ] of List [ I n ] -> n ; _ -> 0");
assert_eval_snapshot!("main = Builtin.castLower (Builtin.castLift 41 : Int) : int");   // 41, and plan 08 later folds it
assert_eval_snapshot!(r#"
    type Datum = Datum Bytes Int
    main = Builtin.castValidateData (Constr 0 [ B #"aa", I 1 ]) : Datum
"#);   // result is the same Data
assert_eval_snapshot!(r#"
    type Datum = Datum Bytes Int
    main = Builtin.castValidateData (Constr 0 [ I 1, I 1 ]) : Datum
"#);   // error, log "validateData: Datum field 0: expected B"
assert_eval_snapshot!(r#"
    type Datum = Datum Bytes Int
    main = Builtin.castFromDataShallow (Constr 0 [ I 1, I 1 ]) : Datum
"#);   // ok: shallow only checks Constr/tag/arity
```

**Done when**: eight snapshots accepted; `checkers.rs` emits one binding
per distinct type across all uses (asserted by a `Core` snapshot with two
`validateData` calls at the same type).

---

## Chunk 7 — Records

**Files**

- `crates/nash-codegen/src/can_to_core.rs` (`Record`, `Access`,
  `Accessor`, `Update`, record patterns)

**Change**

By the kind of the solved type: Big record literal ->
`Builtin(ListData, [mkCons chain])`; little -> `Constr(0, fields)`.
`Access` -> `Field(r, i, n)` (little) or the accessor chain
`headList (tailList^i (unListData r))` (Big). `Accessor(".x")` -> a
`Lam` around the access. `Update` -> `Let base = ..` then rebuild all
fields in declaration order (`FieldType::index`). Record patterns bind
the named fields through `Path::RecordField` / `Path::Tuple`.

**Code**

```rust
Expr::Record { alias, annotation: _, fields } => {          // 04-representation.md A3: nominal, fields in declaration order
    let ty = self.tys.alias_ty(*alias);
    let values = fields.iter().map(|f| self.expr(f.value));
    match ty.kind() {
        Kind::Term => self.build.constr(0, values),
        Kind::Big => self.build.builtin(ListData, &[self.const_list(Ty::Big(Data), values)]),
        Kind::Const => unreachable!("records are never Const"),
    }
}
Expr::Access { record, field } => {
    let r = self.expr(record);
    let (ty, order) = self.record_layout(record);
    let i = order.iter().position(|f| f == &field.value).unwrap() as u16;
    match ty.kind() {
        Kind::Term => self.build.field(r, i, order.len() as u16),
        Kind::Big => self.big_index(self.build.builtin(UnListData, &[r]), i),
        Kind::Const => unreachable!(),
    }
}
Expr::Update { record, base, fields } => {
    let b = self.bind_tmp("base", self.expr(base));
    // rebuild every field: updated ones from `fields`, the rest as Access on `b`
}
```

`record_layout` reads the alias through `SolvedTypes::exprs`
(`CanType::Alias { reference, target: Filled(Record { fields, .. }) }`).

**Labeled constructor fields.** When the solved type of `record` in
`Expr::Access` is an ADT (`BigTy::Adt` / `TermTy::Adt`) rather than an
alias, the type has exactly one constructor whose `args` is
`CtorArgs::Labeled(fields)` (the type checker rejects `.x` on
multi-constructor types), and `i` is the `FieldType.index` of the field
named `x`. The access lowers to the field extraction the pattern
`Ctor { x }` produces: the decision-tree accessor path `[Path::Constr(i)]`
for little, `[Path::BigField(i)]` for Big, resolved through the same
`Bound` set so a pattern and a later `.x` in one scope share the
extraction. `Expr::Record`
never builds a labeled constructor (that is `Expr::VarConstructor` applied
to arguments, chunks 5 and 6). Record update `{ r | x = e }` is for
`type alias` records only in v1; the type checker rejects it on a labeled
constructor, so `Expr::Update` never sees an ADT type here.

**Elm/Aiken reference**: Aiken `Air::RecordUpdate`, `Air::FieldsExpose`
(`gen_uplc.rs` arms, `list_access_to_uplc` `builder.rs:814`); Elm
`Generate/JavaScript/Expression.hs` `generateRecord`/`generateAccess`.

**Tests**

```rust
assert_eval_snapshot!(r#"
    type alias acc = { total : int, seen : int }
    main = { total = 1, seen = 2 }.seen
"#);
assert_eval_snapshot!(r#"
    type alias acc = { total : int, seen : int }
    main =
        let a = { total = 1, seen = 2 } in
        { a | total = 10 }.total
"#);
assert_eval_snapshot!(r#"
    type alias Datum = { owner : Bytes, deadline : Int }
    main = { owner = #"aa", deadline = 5 }.deadline
"#);   // result (con data (I 5))
assert_eval_snapshot!(r#"
    type alias acc = { total : int, seen : int }
    main = case { total = 1, seen = 2 } of
        { total, seen } -> Builtin.addInteger total seen
"#);
assert_eval_snapshot!(r#"
    type alias acc = { total : int, seen : int }
    main = .seen { total = 1, seen = 2 }
"#);
assert_core_snapshot!(r#"
    type Datum = Datum { owner : Bytes, deadline : Int }
    main d = case d of
        Datum { owner } -> (owner, d.deadline)
"#);   // Big labeled ctor: one unConstrData/sndPair shared by the pattern and the access; no unListData
assert_eval_snapshot!(r#"
    type step = Next { n : int, rest : int }
    main = (Next { n = 1, rest = 2 }).rest
"#);   // little labeled ctor: constr 0 [1, 2] then Field 1
```

**Done when**: seven snapshots accepted; Big record access in the `Core`
snapshot shows one `unListData` shared by two field reads, and the labeled
constructor snapshot shows no `List` wrapping.

---

## Chunk 8 — Recursion

**Files**

- `crates/nash-codegen/src/recursion.rs` (new)
- `crates/nash-codegen/src/can_to_core.rs` (`DeclareRec`, `Expr::LetRec` ->
  `Core::LetRec`)
- `crates/nash-codegen/src/lib.rs` (pipeline: `can_to_core` ->
  `recursion::rewrite` -> optimizer -> `lower`)

**Change**

`DeclareRec` and `Expr::LetRec` produce `Core::LetRec` with
`static_params` computed at construction. `recursion::rewrite` is a
Core -> Core pass that removes every `LetRec` by self-application or a
combined dispatcher, so the optimizer and `lower` never see one.

**Code**

```rust
//! LetRec -> self-application. Port of Aiken's modify_self_calls /
//! identify_recursive_static_params / modify_cyclic_calls.

/// Parameters that every self call passes through unchanged, provided the
/// function only ever appears as the head of a call.
pub fn static_params<'a>(f: Name<'a>, params: &[Binder<'a>], body: &Core<'a>) -> Vec<u16> {
    let mut statics: Vec<u16> = (0..params.len() as u16).collect();
    let mut calls = 0usize;
    let mut uses = 0usize;
    walk(body, &mut |node| match node {
        Core::App { func: Core::Var(g), args } if *g == f => {
            calls += 1;
            uses += 1;
            statics.retain(|&i| matches!(args.get(i as usize), Some(Core::Var(v)) if *v == params[i as usize].name));
        }
        Core::Var(g) if *g == f => uses += 1,
        _ => {}
    });
    if calls == uses { statics } else { Vec::new() }
}

pub fn rewrite<'a>(build: &Builder<'a>, core: &'a Core<'a>) -> &'a Core<'a> {
    map(build, core, &mut |node| match node {
        Core::LetRec { binders: [single], body } => Some(self_apply(build, single, body)),
        Core::LetRec { binders, body } => Some(dispatcher(build, binders, body)),
        _ => None,
    })
}

/// f = \static.. -> (\f -> (f f) nonstatic..) (\f nonstatic.. -> body[f a.. := (f f) nonstatic..])
fn self_apply<'a>(build: &Builder<'a>, rb: &RecBinder<'a>, body: &'a Core<'a>) -> &'a Core<'a> {
    let f = rb.binder;
    let (statics, nonstatics): (Vec<Binder>, Vec<Binder>) = rb.params.iter().enumerate()
        .partition(|(i, _)| rb.static_params.contains(&(*i as u16)));
    let self_call = build.app(build.var(f.name), &[build.var(f.name)]);
    let inner_body = substitute_self_calls(build, rb.body, f.name, &rb.static_params, self_call);
    let inner = build.lam(&[f], build.lam(&nonstatics, inner_body));        // \f nonstatic.. -> body'
    let apply_self = build.app(self_call, &nonstatics.iter().map(|b| build.var(b.name)).collect::<Vec<_>>());
    let knot = build.app(build.lam(&[f], apply_self), &[inner]);            // (\f -> (f f) ns..) inner
    let definition = if statics.is_empty() { knot } else { build.lam(&statics, knot) };
    let definition = if rb.params.is_empty() { build.force(build.delay(definition)) } else { definition };
    build.let_(f, definition, body)
}

/// cycle = \cycle select -> select (\a.. -> bodyA') (\b.. -> bodyB')
/// g x   = cycle cycle (\a b -> b) x
fn dispatcher<'a>(build: &Builder<'a>, binders: &[RecBinder<'a>], body: &'a Core<'a>) -> &'a Core<'a> {
    let cycle = build.fresh_binder("cycle", /* Term Fun */);
    let select = build.fresh_binder("select", ..);
    let names: Vec<Name> = binders.iter().map(|b| b.binder.name).collect();
    let call_of = |i: usize| -> &'a Core<'a> {
        let picks = binders.iter().map(|b| build.fresh_binder(b.binder.name.text, ..)).collect::<Vec<_>>();
        let selector = build.lam(&picks, build.var(picks[i].name));
        build.app(build.var(cycle.name), &[build.var(cycle.name), selector])
    };
    let arms = binders.iter().map(|rb| build.lam(rb.params, substitute_vars(build, rb.body, &names, &call_of)));
    let dispatcher = build.lam(&[cycle, select], build.app(build.var(select.name), &arms.collect::<Vec<_>>()));
    let body = substitute_vars(build, body, &names, &call_of);
    build.let_(cycle, dispatcher, body)
}
```

`walk` and `map` are the two traversal helpers added to
`crates/nash-ir/src/core.rs` in this chunk (`pub fn walk(&self, f: &mut impl FnMut(&Core))`
and `pub fn map<'a>(build, core, f: &mut impl FnMut(&Core<'a>) -> Option<&'a Core<'a>>) -> &'a Core<'a>`,
a rebuild-on-change traversal used by every pass in plan 08).

**Elm/Aiken reference**: `builder.rs` `identify_recursive_static_params`
(256), `modify_self_calls` (310), `modify_cyclic_calls` (410);
`gen_uplc.rs` `FunctionVariants::Recursive` (4607) and `Cyclic` (4674)
arms; `hoist_functions_to_validator` (2974) for the SCC handling that Nash
gets for free from `Decls::DeclareRec`.

**Tests**

```rust
assert_eval_snapshot!(r#"
    length xs =
        case xs of
            [] -> 0
            _ :: rest -> Builtin.addInteger 1 (length rest)
    main = length [1, 2, 3]
"#);
assert_core_snapshot!(r#"
    sumTo acc n =
        if Builtin.equalsInteger n 0 then acc
        else sumTo (Builtin.addInteger acc n) (Builtin.subtractInteger n 1)
    main = sumTo 0 10
"#);   // no statics: (\f -> f f) (\f acc n -> ..)
assert_core_snapshot!(r#"
    replicate x n =
        if Builtin.equalsInteger n 0 then []
        else Builtin.mkCons x (replicate x (Builtin.subtractInteger n 1))
    main = replicate 7 3
"#);   // x is static: \x -> (\f -> (f f) n) (\f n -> ..)
assert_eval_snapshot!(r#"
    isEven n = if Builtin.equalsInteger n 0 then True else isOdd (Builtin.subtractInteger n 1)
    isOdd n = if Builtin.equalsInteger n 0 then False else isEven (Builtin.subtractInteger n 1)
    main = isEven 10
"#);
assert_eval_snapshot!(r#"
    main =
        let
            go n = if Builtin.equalsInteger n 0 then 0 else go (Builtin.subtractInteger n 1)
        in
        go 5
"#);
assert_eval_snapshot!(r#"
    ones = Builtin.mkCons 1 ones   -- no params: Delay/Force wrapper; must not loop at definition
    main = Builtin.headList ones
"#);   // expected: evaluation error (strict), snapshot documents it
```

**Done when**: `static_params` unit tests (four cases: all static, none,
function passed as a value disqualifies, shadowed param) pass; six
snapshots accepted; no `LetRec` reaches `lower`.

---

## Chunk 9 — Monomorphization worklist with trait evidence

**Files**

- `crates/nash-codegen/src/mono.rs` (new)
- `crates/nash-codegen/src/can_to_core.rs` (`VarTopLevel`, `VarForeign`,
  `VarOperator`, `Binop`, literals through `Instance`)
- `crates/nash-codegen/src/lib.rs` (`compile` takes all modules of the
  build, not one)

**Change**

Replace the "emit every top-level once" stub with the worklist. Each
request returns the `Name` of the instance and enqueues its instantiation;
the driver loop drains the queue and appends bindings. Trait-method
occurrences resolve to the impl's method instance using the evidence.

**Code**

```rust
/// The codegen context over every solved module of one build. Constructed
/// once by 09-validators-build.md's `build_validators` inside `build_with`'s
/// `finish` closure.
pub struct Build<'a> {
    pub arena: &'a Arena,
    pub options: Options,
    pub modules: Vec<(&'a nash_ast::Module<'a>, &'a Annotations<'a>, &'a SolvedTypes<'a>)>,
    pub definitions: HashMap<QualifiedName<'a>, &'a Def<'a>>,
    pub impls: HashMap<ImplRef<'a>, &'a Impl<'a>>,                // nash_ast::ImplRef { home, key }, 03-traits.md
    pub unions: HashMap<QualifiedName<'a>, &'a Union<'a>>,
    pub aliases: HashMap<QualifiedName<'a>, &'a Alias<'a>>,
}

pub struct Options {
    pub plutus_version: nash_config::PlutusVersion,
    pub trace_level: TraceLevel,          // chunk 10
    pub compiler_traces: bool,
    /// `--optimize 0|1|2` (docs/cli.md): plan 08 `Level::{O0, O1, O2}`.
    pub optimize: u8,
}

impl<'a> Build<'a> {
    pub fn new(
        arena: &'a Arena,
        modules: impl IntoIterator<Item = (&'a nash_ast::Module<'a>, &'a Annotations<'a>, &'a SolvedTypes<'a>)>,
        options: Options,
    ) -> Self;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MonoKey<'a> {
    pub name: QualifiedName<'a>,
    pub type_args: &'a [Ty<'a>],
    /// Ground: every `Given` substituted, every `Super` resolved, so only
    /// `Evidence::Impl` trees and `ReflexiveLift` leaves remain.
    pub evidence: &'a [Evidence<'a>],
}

pub struct Mono<'a> {
    pub instances: HashMap<MonoKey<'a>, Name<'a>>,
    queue: VecDeque<(MonoKey<'a>, Name<'a>)>,
    pub bindings: Vec<(Binder<'a>, &'a Core<'a>)>,
    /// Evidence of the specialization being built, indexed by `Given.index`,
    /// keyed by the def's `Scheme.binder`.
    givens: HashMap<NodeId, &'a [Evidence<'a>]>,
}

// request_identity(typ, tys) interns a typed one-argument identity binding
// in `bindings`; it does not look up a source definition or create an ImplKey.
// `tys.ty` applies the current specialization's substitution to the evidence type.

impl<'a> Mono<'a> {
    pub fn request(&mut self, build: &Build<'a>, name: QualifiedName<'a>, instance: &Instance<'a>, tys: &mut TyEnv<'a>) -> Name<'a> {
        let key = MonoKey {
            name,
            type_args: tys.arena.alloc_slice_fill_iter(instance.type_args.iter().map(|t| tys.ty(t))),
            evidence: self.ground(build, instance.evidence),
        };
        if let Some(n) = self.instances.get(&key) { return *n; }
        let n = Name { text: variant_name(tys.arena, &key), unique: tys.fresh_unique() };
        self.instances.insert(key, n);
        self.queue.push_back((key, n));
        n
    }

    /// Substitute `Given`s with the current specialization's evidence and
    /// resolve `Super`s through the impl table; the solver guarantees every
    /// remaining leaf is an `Impl` or `ReflexiveLift`.
    fn ground(&self, build: &Build<'a>, evidence: &'a [Evidence<'a>]) -> &'a [Evidence<'a>] {
        build.arena.alloc_slice_fill_iter(evidence.iter().map(|e| self.ground_one(build, e)))
    }

    fn ground_one(&self, build: &Build<'a>, e: &Evidence<'a>) -> Evidence<'a> {
        match e {
            Evidence::ReflexiveLift { typ } => Evidence::ReflexiveLift { typ },
            Evidence::Given { binder, index } => self.givens[binder][*index as usize].clone(),
            Evidence::Super { of, index } => {
                let Evidence::Impl { impl_, .. } = self.ground_one(build, of) else { unreachable!("core Lift has no superclasses") };
                build.impls[&impl_].supers[*index as usize].clone()   // the impl's superclass evidence, itself an `Impl`
            }
            Evidence::Impl { impl_, type_args, args } => Evidence::Impl {
                impl_: *impl_,
                type_args,
                args: self.ground(build, args),
            },
        }
    }

    /// A trait method at ground types: the impl's method body instantiated
    /// at the impl head's `type_args`, with `args` as its givens; or the
    /// trait's default body with the same evidence.
    pub fn request_method(&mut self, build: &Build<'a>, trait_name: QualifiedName<'a>, method: &'a str, instance: &Instance<'a>, tys: &mut TyEnv<'a>) -> Name<'a> {
        let evidence = self.ground(build, instance.evidence);
        if let Evidence::ReflexiveLift { typ } = &evidence[0] {
            let typ = tys.ty(typ);
            return self.request_identity(typ, tys);
        }
        let Evidence::Impl { impl_, type_args, args } = &evidence[0] else { unreachable!("ground method evidence") };
        let imp = build.impls[impl_];
        let def = imp.methods.get(method).copied().unwrap_or_else(|| build.trait_default(trait_name, method));
        let key = MonoKey {
            name: def_name(def),
            type_args: tys.arena.alloc_slice_fill_iter(type_args.iter().map(|t| tys.ty(t))),
            evidence: args,
        };
        self.request_key(key, tys)
    }

    pub fn drain(&mut self, build: &Build<'a>, gen: &mut Gen<'a>) {
        while let Some((key, name)) = self.queue.pop_front() {
            let def = build.definitions[&key.name];
            let scheme = &gen.solved.schemes[&NodeId::def(def_name_node(def))];
            let subst = scheme.annotation.free_vars.iter().copied().zip(key.type_args.iter().copied()).collect();
            let saved = std::mem::replace(&mut gen.tys.subst, subst);
            self.givens.insert(scheme.binder, key.evidence);
            let body = gen.def(def);        // may call `request`, growing the queue
            self.givens.remove(&scheme.binder);
            gen.tys.subst = saved;
            self.bindings.push((Binder { name, ty: gen.def_ty(def) }, body));
        }
    }
}

/// `List.map#int#Int`, `compare#Ord#list#int`; sanitized to a valid UPLC name.
fn variant_name<'a>(arena: &'a Arena, key: &MonoKey<'a>) -> &'a str;
```

`Module::bindings` are ordered by first request, which is a valid
dependency order only if a binding never references an instance requested
after it began; because `drain` finishes a body before pushing it, the
final order is reversed request order and then topologically fixed by a
small pass (`order_bindings`) that sorts by free-variable dependencies
(recursive groups already collapsed by chunk 8).

**Elm/Aiken reference**: `builder.rs` `get_generic_variant_name` (178),
`monomorphize` (201); `gen_uplc.rs` `hoist_functions_to_validator` (2974),
`hoist_function` (3355), `find_function_vars_and_depth` (3663).
Nash instantiates before building rather than after, so there is no
`mono_types` map threaded through a tree traversal.

**Tests**

```rust
assert_core_snapshot!(r#"
    identity x = x
    main = (identity 1, identity #"aa")
"#);   // two bindings: identity#int, identity#bytes
assert_eval_snapshot!(r#"
    trait Add 'a where
        add : 'a -> 'a -> 'a
    impl Add int where
        add = Builtin.addInteger
    twice : Add 'a => 'a -> 'a
    twice x = add x x
    main = twice 21
"#);
assert_core_snapshot!(/* same program: shows `add#Add#int` bound to the builtin and `twice#int` calling it */);
assert_eval_snapshot!(r#"
    trait Eq 'a => Ord 'a where
        compare : 'a -> 'a -> ordering
        lt : 'a -> 'a -> bool
        lt a b = compare a b == LT
    ...
    main = lt 1 2
"#);   // default method + superclass evidence
assert_eval_snapshot!(r#"
    import Utils
    main = Utils.helper 1
"#);   // cross-module instance with Utils in the same build
```

**Done when**: each `(name, type args, evidence)` appears once in the
`Core` snapshot; the five snapshots accepted; `nash-driver` still builds
(it does not call codegen yet).

---

## Chunk 10 — `trace`, `fail`, `todo`, `assert`, and trace levels

**Files**

- `crates/nash-codegen/src/traces.rs` (new)
- `crates/nash-codegen/src/can_to_core.rs` (`Expr::Trace`, `Fail`, `Todo`,
  `Assert` from [01-syntax.md](01-syntax.md))

**Change**

Implement the table in docs/codegen.md "Runtime errors and traces".
Messages are hoisted: `Traces::message(text) -> Name` returns a top-level
`Let` of the string constant, deduplicated by text.

**Code**

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceLevel { Silent, Compact, Verbose }

#[derive(Clone, Copy, Debug)]
pub struct TraceConfig {
    pub user: TraceLevel,
    pub compiler: bool,
}

pub struct Traces<'a> {
    config: TraceConfig,
    messages: HashMap<&'a str, Name<'a>>,
    pub bindings: Vec<(Binder<'a>, &'a Core<'a>)>,
}

impl<'a> Traces<'a> {
    /// A user trace: `Trace(msg, body)`, or `body` when silent.
    pub fn user(&mut self, build: &Builder<'a>, region: Region, module: &str, text: &'a str, body: &'a Core<'a>) -> &'a Core<'a> {
        match self.config.user {
            TraceLevel::Silent => body,
            TraceLevel::Compact => build.trace(build.var(self.message(build, location(module, region))), body),
            TraceLevel::Verbose => build.trace(build.var(self.message(build, text)), body),
        }
    }

    /// A compiler trace (validateData, incomplete match): on or off.
    pub fn compiler(&mut self, build: &Builder<'a>, text: &'a str, body: &'a Core<'a>) -> &'a Core<'a> {
        if self.config.compiler { build.trace(build.var(self.message(build, text)), body) } else { body }
    }

    fn message(&mut self, build: &Builder<'a>, text: &'a str) -> Name<'a> { ... }
}
```

`assert c` becomes `Case(Bool, c, [Lit unit, user_trace(power_assert_text, Error)])`
where the power-assert text is produced by the front end ([10-testing.md](10-testing.md)).

**Elm/Aiken reference**: `gen_uplc.rs` 440–470 (message selection by
`TraceLevel`), `builder.rs` `wrap_validator_condition` (1214; not
ported), `CodeGenSpecialFuncs::insert_new_function` (hoisting of message
thunks).

**Tests** (each program run at all three user levels with compiler traces
on and off; the macro takes a `TraceConfig`):

```rust
assert_eval_snapshot!(verbose, r#"main = trace "hello" 1"#);      // logs: ["hello"], result 1
assert_eval_snapshot!(compact, r#"main = trace "hello" 1"#);      // logs: ["Main:1:8"]
assert_eval_snapshot!(silent,  r#"main = trace "hello" 1"#);      // logs: []
assert_eval_snapshot!(verbose, r#"main = fail "boom""#);           // error, logs ["boom"]
assert_eval_snapshot!(verbose, r#"main = assert (Builtin.equalsInteger 1 2)"#);
assert_core_snapshot!(verbose, r#"main = (trace "x" 1, trace "x" 2)"#);   // one hoisted message
```

**Done when**: eighteen snapshots accepted (six programs x three configs
where relevant); message hoisting shown.

---

## Chunk 11 — Program assembly: validators, comptime, entry point

**Files**

- `crates/nash-codegen/src/program.rs` (new)
- `crates/nash-codegen/src/comptime.rs` (new)
- `crates/nash-codegen/src/error.rs` (new)
- `crates/nash-codegen/src/lib.rs`

**Change**

`program.rs` assembles a `Program<DeBruijn>` from a `Module`: bindings
become a `Let` chain around the root, the optimizer (plan 08; identity
until then) runs, then `lower` and `to_debruijn`. Validator roots are
`main` with its parameters as lambdas. The entry point `validator` is
called by [09-validators-build.md](09-validators-build.md)'s
`build_validators` from inside `build_with`'s `finish` closure, after
every module of the build is solved (a validator inlines its whole
dependency closure); [10-testing.md](10-testing.md) chunk 4 builds test
programs on the same `assemble`. Nothing in `compile_module` changes.

**Code**

```rust
pub struct Compiled<'a> {
    pub program: &'a Program<'a, DeBruijn>,
    pub named: &'a Term<'a, Name<'a>>,       // for `nash build --uplc`
}

pub fn assemble<'a>(arena: &'a Arena, module: &Module<'a>) -> Compiled<'a> {
    let build = Builder::new(arena);
    let body = module.bindings.iter().rev().fold(module.root, |body, (b, v)| build.let_(*b, v, body));
    let body = nash_ir::optimize::run(&build, &mut eval, body, Level::from_flag(options.optimize));   // plan 08; identity until then
    let named = Lower { arena }.term(body);
    let term = nash_plutus::debruijn::to_debruijn(arena, named).expect("assembled programs are closed");
    Compiled { program: Program::new(arena, Version::plutus_v3(arena), term), named }
}

pub fn validator<'a>(arena: &'a Arena, build: &Build<'a>, module: &'a nash_ast::Module<'a>) -> Result<Compiled<'a>, Error>;

/// Bindings reachable from `root`, for callers that assemble several
/// programs from one monomorphized module (tests, comptime).
pub fn reachable<'a>(bindings: &[(Binder<'a>, &'a Core<'a>)], root: &Core<'a>) -> Vec<(Binder<'a>, &'a Core<'a>)>;
```

`assemble` and `reachable` are the shared primitives. Test programs
(`TestProgram`, `compile_tests`, the `draw`/`run` pair per prop) are
defined in [10-testing.md](10-testing.md) chunk 4, which monomorphizes
once from the union of all test roots and calls `assemble` per root with
`Module { bindings: reachable(root), root }`.

`comptime.rs`:

```rust
pub enum ComptimeError<'a> {
    NotClosed(Name<'a>),
    Evaluation(String),
    NotAConstant,
}

/// Evaluate a closed Core term and return its constant.
pub fn eval_closed<'a>(arena: &'a Arena, bindings: &[(Binder<'a>, &'a Core<'a>)], core: &'a Core<'a>) -> Result<&'a Constant<'a>, ComptimeError<'a>> {
    let module = Module { bindings: reachable(bindings, core), root: core };
    let compiled = assemble(arena, &module);
    match compiled.program.eval(arena).term {
        Ok(Term::Constant(c)) => Ok(c),
        Ok(_) => Err(ComptimeError::NotAConstant),
        Err(e) => Err(ComptimeError::Evaluation(format!("{e:?}"))),
    }
}
```

`Expr::Comptime(inner)` in `can_to_core.rs` builds `inner`, calls
`eval_closed`, and emits `Lit`. Plan 08's constant folder calls the same
function on `Builtin` subterms whose arguments are literals.

`error.rs`:

```rust
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum Error {
    #[error("`{builtin}` needs Plutus V3; this build targets {target}")]
    BuiltinUnavailable { module: String, builtin: String, target: String, #[label] region: Region },
    #[error("comptime evaluation failed: {reason}")]
    Comptime { module: String, reason: String, #[label] region: Region },
}
```

`validator` returns `Result<Compiled<'a>, Error>`, and
09-validators-build.md converts the error to a `ModuleResult::Failed` for
the module. The Term-kind `main` argument check is not here: it is
09-validators-build.md's post-solve `MainParameterIsTerm` check, so
`validator` may assume every parameter of `main` is Big or Const.

The call site, for reference (owned by 09-validators-build.md, chunk 6):

```rust
let (result, outputs) = build_with(db, &graph, CompileMode::Build, move |solved| {
    let arena = Arena::new();
    let build = nash_codegen::Build::new(&arena, solved.modules.iter().map(|m| (m.module, m.annotations, m.types)), options);
    solved.modules.iter().filter(|m| m.module.is_validator())
        .map(|m| nash_codegen::validator(&arena, &build, m.module))
        .collect::<Result<Vec<_>, _>>()
}).await;
```

**Elm/Aiken reference**: `gen_uplc.rs` `generate` (286),
`generate_raw` (324), `finalize_with` (397); `test_framework.rs`
`Test::from_function_definition` (114) for how a test becomes a program;
`cast_validator_args` (`builder.rs:1187`; not ported: `main` args are Big
or Const and arrive as the constants the caller applies, and Term-kinded
args were rejected by [09-validators-build.md](09-validators-build.md)'s
check before codegen).

**Tests**

- `assemble_lets_chain`: a `Module` with two bindings pretty-prints as
  nested lets and evaluates.
- `validator_main_args_are_lambdas`: `main d r ctx = assert ...` produces
  a three-lambda program; applying three `Data` constants via
  `Program::apply` evaluates.
- `validator_const_arg`: `main (threshold : int) (d : Data) = assert ...`
  accepts an integer constant then a `Data` constant.
- `reachable_keeps_transitive_bindings`: a module with a helper used only
  through another helper; `reachable(root)` returns both, and drops an
  unrelated third binding (the `tests_share_bindings` test of
  [10-testing.md](10-testing.md) chunk 4 builds on this).
- `comptime_folds_to_constant`: `main = comptime (Builtin.addInteger 40 2)`
  gives `Lit 42` in the `Core` snapshot.
- `comptime_error_reports`: `main = comptime (fail "x")` yields
  `Error::Comptime`.

**Done when**: the entry points compile against the `Build::new` call in
09-validators-build.md chunk 6; `nash build` on a validator module (driven
by that plan) writes a flat file that `syn::parse_program` of the pretty
text round-trips to.

---

## Chunk 12 — End-to-end validator

**Files**

- `crates/nash-codegen/tests/vesting.rs` (new)
- `crates/nash-codegen/tests/fixtures/Vesting.nash` (new; the
  docs/overview.md example, minus the `tests` block)

**Change**

No new code. One integration test that compiles the overview's `Vesting`
validator with the stub `Cardano.Tx` helpers written in Nash inside the
fixture, snapshots the pretty UPLC and the `Core`, and evaluates it four
ways with `Program::apply`:

| datum | redeemer | context | expected |
|---|---|---|---|
| deadline 10 | `Claim` | slot 20, unsigned | success |
| deadline 30 | `Claim` | slot 20 | error, log from `assert` |
| owner `#"aa"` | `Cancel` | signed by `#"aa"` | success |
| owner `#"aa"` | `Cancel` | unsigned | error |

Budget lines are part of the snapshot so plan 08 can show improvements.

A second fixture, `VestingParam.nash`, prepends a `Const` parameter
(`main : int -> Datum -> Redeemer -> Data -> unit`, the minimum lock
period) and is applied to `(con integer 5)` before the three `Data`
constants. A third, `VestingBad.nash`, declares
`main : (Data -> bool) -> Data -> unit` and asserts the build reports
[09-validators-build.md](09-validators-build.md)'s `MainParameterIsTerm`
and never calls `validator` (the fixture runs through `build_with`).

**Done when**: the four evaluations match for both compiling fixtures;
the bad fixture is rejected; the `Vesting` snapshot exists as the
baseline for plan 08's measurement harness.

---

## Open questions

1. **Casts as `Builtin.cast*` intrinsics** (chunk 6) versus compiler
   knowledge of the stdlib impl names. Intrinsics keep the compiler free
   of stdlib name knowledge and are restricted to `core/`.
3. **Order of chunks vs the brief.** Core -> Term lowering is chunk 2
   rather than second to last, because every later chunk's tests run on
   the CEK machine.
4. **`Field` on a Big record** is an accessor chain, not a `Field` node;
   the node is `Term`-only. If the optimizer wants to reason about Big
   field access, add `BigField` then.
