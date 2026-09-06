# Plan 03: Traits

Goal: implement [docs/traits.md](../docs/traits.md): `trait`/`impl`
declarations, qualified types in inference, impl resolution with evidence,
literal traits with defaulting, `do` desugaring, and the interface plumbing
so a trait defined in one module is usable from another.

Prerequisites:

- [plans/01-syntax.md](01-syntax.md) chunks 2, 8, 9: the parser produces
  trait/impl declarations, `=>` contexts, and `do`. This plan consumes
  these `nash_source` types and does not touch the parser:
  - `Annotation { constraints: &'a [&'a Located<Constraint<'a>>], typ: &'a Located<Type<'a>> }`;
    `Value.annotation` and `Def::Define.annotation` are `Option<&'a Annotation<'a>>`.
  - `Constraint { class: &'a Located<&'a str>, module: Option<&'a str>, args: &'a [&'a Located<Type<'a>>] }`.
  - `Trait { name, params: &'a [&'a TypeParam<'a>], supers: &'a [&'a Located<Constraint<'a>>], methods: &'a [&'a TraitMethod<'a>], attributes }`;
    `TypeParam { name: &'a Located<&'a str>, kind: Option<&'a Located<Kind<'a>>> }`;
    `TraitMethod { name, annotation: &'a Annotation<'a>, default: Option<&'a Located<Def<'a>>> }`
    (a `Def::Define` with `annotation: None`).
  - `Impl { context: &'a [&'a Located<Constraint<'a>>], head: &'a Located<Constraint<'a>>, methods: &'a [&'a Located<Def<'a>>], attributes }`;
    `head.class`/`head.module` name the trait, `head.args` are the instance heads.
  - `Module::{traits, impls}`.
  - `Expr::Do { stmts: &'a [&'a Located<Stmt<'a>>], last: &'a Located<Expr<'a>> }` with
    `Stmt::{Let(defs), Bind { pattern, expr }, Expr(expr)}`.
  - `Type::Var("a")` stores the name without the quote; `Type::VarApp { region, name, args }` is `'f 'a`.
  - Until this plan lands, nash-can answers non-empty `constraints`, `traits`,
    `impls`, and `Do` with `Error::Unsupported { feature, region }`.
    Chunks 2, 3, and 10 delete those gates as they take over each feature.
- [plans/02-kinds.md](02-kinds.md) chunks 1-5: `nash_ast::{BaseKind, KindSet, Kind, KindScheme}`,
  the engine `nash_can::kinds::{Infer, KindEnv, Walker, check_annotation}`,
  `nash_can::Error::KindMismatch { region, context: KindContext, expected, actual }`,
  and the pre-seeded little types `int`, `string`, `bytes` used by defaulting.
  Chunk 3 (impl head kinds) and chunk 9 (kind predicates on value schemes)
  depend on it, as does chunk 2 trait-kind inference; chunks 1 and 4-8 do not.
- The shared contract with [plans/07-codegen.md](07-codegen.md) chunk 3 defines `nash_solve::solved::{NodeId, SolvedTypes, Instance}`.
  The file does not exist after plan 02; bootstrap it in this plan.
  This plan fills `SolvedTypes::instances` and adds `SolvedTypes::schemes`;
  it does not define a parallel table. The exact contract is in
  [Contract with plans/07](#contract-with-plans07-codegenmd) below.

Crates touched: `nash-ast`, `nash-can`, `nash-constrain`, `nash-solve`,
`nash-driver`, plus `core/` (Nash source) in chunk 12.

Elm references: `elm/compiler/src/Type/Solve.hs` (`solve` on `CLet`,
`generalize`, `introduce`, `makeCopy`, `makeCopyHelp`, `restore`,
`srcTypeToVariable`), `Type/Unify.hs` (`merge`, `fresh`, `unifyStructure`),
`Type/Type.hs` (`toAnnotation`, `getVarNames`), `Type/Constrain/Expression.hs`
(`constrainDef`, `recDefsHelp`), `Canonicalize/Environment/Local.hs`
(`addVars`, `addTypes`), `Canonicalize/Module.hs` (`canonicalize`,
`toNodeOne`), `Elm/Interface.hs` (`fromModule`). Aiken has no traits;
`aiken-lang/src/tipo/infer.rs` is not a reference here.

Conventions: every new type follows the arena style already in the crates
(`&'a T`, `&'a [T]`, `Located<T>`, `Option<&'a T>`). Solver-owned data
(descriptors, predicates) lives on the real heap like `Descriptor` today.

## Contract with plans/07-codegen.md

Plan 07 chunk 3 defines `crates/nash-solve/src/solved.rs` with
`NodeId` (the arena address of a `Located<Expr>` / `Located<Pattern>`),
`SolvedTypes { exprs, patterns, instances }`, and `Instance { type_args, evidence }`.
This plan keeps that file, that key, and that struct, with these changes
(plan 07 chunk 3 and chunk 9 adopt them; nothing else in plan 07 moves):

```rust
// nash-ast (moved here from nash-solve so nash-constrain can produce it;
// nash_solve::solved re-exports it)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(usize);
impl NodeId {
    pub fn expr(e: &Located<Expr<'_>>) -> Self { NodeId(e as *const _ as usize) }
    pub fn pattern(p: &Located<Pattern<'_>>) -> Self { NodeId(p as *const _ as usize) }
    /// A definition, addressed by its name node (`Def::{Def, TypedDef}.name`).
    pub fn def(name: &Located<&str>) -> Self { NodeId(name as *const _ as usize) }
}

// nash-solve/src/solved.rs
pub struct SolvedTypes<'a> {
    pub exprs: HashMap<NodeId, &'a Located<CanType<'a>>>,       // plan 07 fills
    pub patterns: HashMap<NodeId, &'a Located<CanType<'a>>>,    // plan 07 fills
    /// Every `VarLocal`-to-a-generalized-def, `VarTopLevel`, `VarForeign`,
    /// `VarOperator`, `VarMethod`, `Binop`, literal, and `<-` node.
    pub instances: HashMap<NodeId, Instance<'a>>,               // this plan fills
    /// Every named definition and generalized destructuring pattern: its scheme.
    pub schemes: HashMap<NodeId, Scheme<'a>>,                   // this plan fills
}

pub struct Instance<'a> {
    /// The scheme's `free_vars`, in `Annotation.free_vars` order, at this use.
    pub type_args: &'a [&'a Located<CanType<'a>>],
    /// One per scheme context predicate, in `Annotation.context` order.
    pub evidence: &'a [nash_ast::Evidence<'a>],
}

pub struct Scheme<'a> {
    /// Free vars, context, type. For a recursive group of untyped
    /// definitions every member gets the same context.
    pub annotation: &'a Annotation<'a>,
    /// `Evidence::Given { binder }` inside the body refers to this def
    /// (the first def of an untyped recursive group).
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

Plan 07's `ImplRef { trait_name, impl_home, head, supers }` becomes
`nash_ast::ImplRef { home, key }` plus the `Evidence` tree from chunk 1:
the instantiated head types move to `Evidence::Impl::type_args` (the impl
head's variables in order, so `Mono::request_method` can instantiate the
method body with them), and `supers` is replaced by `Evidence::Super`
resolved through the impl table. `Mono::request_method` reads
`instance.evidence[i]` as an `Evidence`, substitutes the current
specialization's `Given`s, and dispatches to `Impl` or `ReflexiveLift`
afterwards. The latter lowers `lift` and `lower` to identity without an
impl-table lookup. `MonoKey.evidence` is the
substituted, ground `&[Evidence]`; `Evidence` implements `PartialEq, Eq, Hash`
for that purpose.

---

## Chunk 1: canonical AST for traits

Status: complete. Canonical declarations, contexts, method references and
evidence are present. The original typed annotation is retained. Evidence
identity ignores source locations recursively. Workspace tests, strict
Clippy, formatting and snapshot hygiene pass; 68 snapshots changed only
for empty context/trait/impl fields. Later chunks produce these new nodes.

Files: `crates/nash-ast/src/lib.rs`, plus every constructor of
`Annotation` and `Def::TypedDef` (`crates/nash-can/src/types.rs:18`,
`crates/nash-can/src/environment.rs:92`, `crates/nash-can/src/environment/foreign.rs`,
`crates/nash-can/src/module.rs:240`, `crates/nash-can/src/expression.rs:995`,
`crates/nash-can/src/interface.rs:312`, `crates/nash-solve/src/annotation.rs:16`).

Change: add predicates and contexts, trait and impl declarations, method
variables, the impl key used by every later chunk, and the evidence type
codegen will consume. Nothing produces a non-empty context yet. Derived Debug snapshots gain
empty context, traits, and impls fields; review these structural additions.

Code:

```rust
// nash-ast/src/lib.rs

/// `Tr t1 .. tn`: the claim that `t1..tn` have an impl of `Tr`.
#[derive(Debug)]
pub struct Pred<'a> {
    pub trait_: QualifiedName<'a>,
    pub args: &'a [&'a Located<Type<'a>>],
}

#[derive(Debug)]
pub struct Annotation<'a> {
    pub free_vars: FreeVars<'a>,
    /// Scheme context, in evidence order.
    pub context: &'a [Pred<'a>],
    pub typ: &'a Located<Type<'a>>,
}

pub enum Def<'a> {
    Def { .. },   // unchanged
    TypedDef {
        /// Preserve plan 02's original annotation for kind checking.
        annotation: &'a Located<Type<'a>>,
        name: &'a Located<&'a str>,
        free_vars: FreeVars<'a>,
        context: &'a [Pred<'a>],
        args: &'a [TypedPattern<'a>],
        body: &'a Located<Expr<'a>>,
        typ: &'a Located<Type<'a>>,
    },
}

#[derive(Debug)]
pub struct Module<'a> {
    // ... existing fields ...
    pub traits: &'a [&'a Located<Trait<'a>>],
    pub impls: &'a [&'a Located<Impl<'a>>],
}

#[derive(Debug)]
pub struct Trait<'a> {
    pub name: &'a Located<&'a str>,
    pub parameters: &'a [&'a str],
    /// One kind per parameter, generalized together (plans/02 `KindScheme`).
    pub kind: KindScheme<'a>,
    /// Superclasses; args are `Type::Var` over `parameters`.
    pub supers: &'a [Pred<'a>],
    pub methods: &'a [Method<'a>],
}

#[derive(Debug)]
pub struct Method<'a> {
    pub name: &'a Located<&'a str>,
    /// Full method scheme: trait predicate first, then the method's own context.
    pub annotation: &'a Annotation<'a>,
    /// Default body as a `TypedDef` whose annotation is `annotation`.
    pub default: Option<&'a Def<'a>>,
}

/// One instance head: a constructor over distinct variables, or unit/tuple.
#[derive(Debug)]
pub enum Head<'a> {
    Named {
        reference: QualifiedName<'a>,
        vars: &'a [&'a str],
    },
    Unit,
    Tuple(&'a [&'a str]),
}

#[derive(Debug)]
pub struct Impl<'a> {
    pub trait_: QualifiedName<'a>,
    /// Over the head variables only.
    pub context: &'a [Pred<'a>],
    pub heads: &'a [Located<Head<'a>>],
    /// Each a `TypedDef` whose annotation is the method scheme at the heads.
    pub methods: &'a [&'a Def<'a>],
}

/// The constructor of a head, for impl lookup.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HeadCon<'a> {
    Named(QualifiedName<'a>),
    Unit,
    Tuple(u8),
    /// Function types never have impls; a wanted `Show (a -> b)` fails lookup.
    Fun,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ImplKey<'a> {
    pub trait_: QualifiedName<'a>,
    pub heads: &'a [HeadCon<'a>],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImplRef<'a> {
    pub home: ModuleName<'a>,
    pub key: ImplKey<'a>,
}

/// How one wanted predicate was satisfied. Consumed by codegen
/// (plans/07 chunk 9 keys specializations by ground evidence, hence `Hash`).
#[derive(Debug)]
pub enum Evidence<'a> {
    ReflexiveLift { typ: &'a Located<Type<'a>> },
    Impl {
        impl_: ImplRef<'a>,
        /// The impl head's variables, in head order, at this use.
        type_args: &'a [&'a Located<Type<'a>>],
        /// One per predicate of the impl's context, in order.
        args: &'a [Evidence<'a>],
    },
    /// The `index`-th context predicate of the definition `binder` (a `NodeId::def`).
    Given { binder: NodeId, index: u16 },
    /// The `index`-th superclass of `of`'s trait.
    Super { of: &'a Evidence<'a>, index: u16 },
}
```

Add structural `PartialEq, Eq, Hash` derives to `Type`, `FieldType`,
`AliasArgument`, and `AliasType`. `Located` and `Region` already have them.
Evidence equality and hashing must ignore locations recursively in type
arguments; ordinary derived equality on `Located<Type>` includes regions
and is not a specialization key. `ImplRef` also derives `Hash`.

```rust

pub enum Expr<'a> {
    // ... existing ...
    /// A trait method. Always constrained through `annotation`, like `VarForeign`.
    VarMethod {
        trait_: QualifiedName<'a>,
        method: &'a str,
        annotation: &'a Annotation<'a>,
    },
}

pub enum Export<'a> {
    // ... existing ...
    Trait(&'a str),
}

impl<'a> Head<'a> {
    pub fn con(&self) -> HeadCon<'a> {
        match self {
            Head::Named { reference, .. } => HeadCon::Named(*reference),
            Head::Unit => HeadCon::Unit,
            Head::Tuple(vars) => HeadCon::Tuple(vars.len() as u8),
        }
    }
}
```

Every existing `Annotation { free_vars, typ }` literal gains `context:
&[]`; every `Def::TypedDef` literal gains `context: &[]`; `CanModule`
literals gain `traits: &[], impls: &[]`. Interfaces borrow contexts from the retained build arena. `nash-solve/tests/inference.rs::render_annotation`
renders `(Eq a, Show b) => ` before the type when the context is non-empty
(single predicate without parentheses), so later chunks' snapshots read
like Nash source.

Elm reference: `AST/Canonical.hs` (`Annotation`, `Def`, `Export`).

Tests: evidence keys share identical types at different source locations
and distinguish different types. Existing behavior snapshots gain only empty
trait/context fields.

Done when: workspace compiles, evidence identity tests pass, and all snapshot
changes have been reviewed.

---

## Chunk 2: contexts on annotations and trait declarations in nash-can

Status: complete. Annotation contexts, local declarations and defaults,
method resolution, superclass checks and SCC kind inference are implemented.
Interfaces retain trait schemes and method metadata across arena copies;
qualified and exposed imports respect visibility and diagnose ambiguity.
Acceptance snapshots cover higher-kinded method contexts, independent method
quantifiers, default-body checks, private metadata and retained interface metadata.

Files: `crates/nash-can/src/types.rs`, `crates/nash-can/src/environment.rs`,
`crates/nash-can/src/environment/local.rs`, `crates/nash-can/src/environment/foreign.rs`,
`crates/nash-can/src/module.rs`, `crates/nash-can/src/expression.rs`,
`crates/nash-can/src/error.rs`, `crates/nash-can/src/interface.rs`,
`crates/nash-can/src/lib.rs`, new `crates/nash-can/src/traits.rs`.

Change:

1. `types::to_annotation` takes a `&'a nash_source::Annotation<'a>`
   (`constraints` + `typ`), canonicalizes each constraint (trait lookup,
   arity check, args as types), and errors when a context variable is not
   in the type (`ContextVarNotInType`, Haskell's ambiguity check for
   annotations). The `Error::Unsupported { feature: "constraints" }` gate
   from plan 01 chunk 2 is deleted.
2. `Env` gets a trait namespace and the `Var::Method` variant. Methods are
   values in scope with their method scheme.
3. `local::add_traits` and `traits::canonicalize_traits` build
   `nash_ast::Trait`s: parameter checks, superclass canonicalization,
   method schemes, default bodies as `TypedDef`s.
4. `Interface` exports traits; `foreign.rs` imports them and their methods.
5. `find_var` / `find_var_qual` produce `Expr::VarMethod`.
6. Exports: `Export::Trait`.

Code:

```rust
// environment.rs
pub enum Var<'a> {
    Local(Region),
    TopLevel(Region),
    Foreign(ModuleName<'a>, &'a nash_ast::Annotation<'a>),
    Foreigns(ModuleName<'a>, Vec<ModuleName<'a>>),
    /// A trait method: trait name plus method scheme. Local and imported
    /// methods look the same because both are constrained via the scheme.
    Method(QualifiedName<'a>, &'a nash_ast::Annotation<'a>),
}

#[derive(Clone, Copy, Debug)]
pub struct TraitInfo<'a> {
    pub kind: KindScheme<'a>,
    pub home: ModuleName<'a>,
    pub name: &'a str,
    pub parameters: &'a [&'a str],
    pub supers: &'a [nash_ast::Pred<'a>],
    pub methods: &'a [MethodInfo<'a>],
}

#[derive(Clone, Copy, Debug)]
pub struct MethodInfo<'a> {
    pub name: &'a str,
    pub annotation: &'a nash_ast::Annotation<'a>,
    pub has_default: bool,
}

pub struct Env<'a> {
    // ... existing ...
    pub traits: Exposed<'a, &'a TraitInfo<'a>>,
    pub q_traits: Qualified<'a, &'a TraitInfo<'a>>,
}

impl<'a> Env<'a> {
    pub fn insert_local_trait(&mut self, info: &'a TraitInfo<'a>) { /* like insert_local_type */ }

    /// Mirrors `find_type` for traits: unqualified or `Module.Trait`.
    pub fn find_trait(
        &self,
        bump: &'a Bump,
        region: Region,
        prefix: Option<&'a str>,
        name: &'a str,
    ) -> Result<&'a TraitInfo<'a>, Vec<Error<'a>>> { .. }
}
```

```rust
// types.rs
pub fn to_annotation<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    annotation: &'a SourceAnnotation<'a>,
) -> Result<&'a Annotation<'a>, Vec<Error<'a>>> {
    let typ = canonicalize_type(bump, env, annotation.typ)?;
    let context = canonicalize_context(bump, env, annotation.constraints)?;
    let mut free_var_set: BTreeSet<&'a str> = BTreeSet::new();
    collect_free_vars(&typ.value, &mut free_var_set);
    check_context_vars(&context, &free_var_set)?;
    Ok(bump.alloc(Annotation {
        free_vars: bump.alloc_slice_fill_iter(free_var_set),
        context,
        typ,
    }))
}

pub fn canonicalize_context<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    constraints: &'a [&'a Located<SourceConstraint<'a>>],
) -> Result<&'a [Pred<'a>], Vec<Error<'a>>> {
    accumulate::try_all_alloc(bump, constraints.iter().map(|c| {
        let info = env.find_trait(bump, c.value.class.region, c.value.module, c.value.class.value)?;
        if c.value.args.len() != info.parameters.len() {
            return Err(vec![Error::TraitArity {
                region: c.region,
                name: info.name,
                expected: info.parameters.len(),
                actual: c.value.args.len(),
            }]);
        }
        Ok(Pred {
            trait_: QualifiedName { home: info.home, name: info.name },
            args: canonicalize_type_arguments(bump, env, c.value.args)?,
        })
    }))
}

/// Every variable in the context must occur in the type.
fn check_context_vars<'a>(context: &[Pred<'a>], free: &BTreeSet<&'a str>) -> Result<(), Vec<Error<'a>>> { .. }
```

```rust
// traits.rs (new)
/// Mirrors the alias/union pipeline in `module.rs`: env entries first,
/// then canonical declarations that need the full env.
pub fn canonicalize_traits<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    traits: &'a [&'a Located<SourceTrait<'a>>],
    warnings: &mut Vec<Warning<'a>>,
) -> Result<&'a [&'a Located<CanTrait<'a>>], Vec<Error<'a>>>

/// `m : Cm => t` inside `trait C => T ps` becomes the scheme
/// `forall ps fv(t). (T ps, Cm) => t`; the trait predicate is index 0.
fn method_scheme<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    trait_: QualifiedName<'a>,
    parameters: &'a [&'a str],
    method: &'a SourceTraitMethod<'a>,
) -> Result<&'a Annotation<'a>, Vec<Error<'a>>> {
    let own = types::to_annotation(bump, env, method.annotation)?;
    let missing = parameters.iter().find(|p| !own.free_vars.contains(p));
    if let Some(param) = missing {
        return Err(vec![Error::MethodMissingParameter {
            region: method.name.region,
            method: method.name.value,
            parameter: param,
        }]);
    }
    let trait_pred = Pred {
        trait_,
        args: bump.alloc_slice_fill_iter(parameters.iter().map(|p| {
            &*bump.alloc(Located::at(Region::zero(), CanType::Var(p)))
        })),
    };
    let context = bump.alloc_slice_fill_iter(
        std::iter::once(trait_pred).chain(own.context.iter().map(|p| Pred { trait_: p.trait_, args: p.args })),
    );
    Ok(bump.alloc(Annotation { free_vars: own.free_vars, context, typ: own.typ }))
}
```

Default bodies (`TraitMethod.default: Option<&Located<Def>>`, a
`Def::Define` with `annotation: None`) reuse `module::to_node_one`'s typed
path: factor the typed half of `to_node_one` (`module.rs:240-325`) into
`pub(crate) fn canonicalize_typed_value(bump, env, name, args, body, annotation: &'a Annotation<'a>, warnings) -> Result<&'a Def<'a>, ..>`
and call it with the method scheme. Defaults may call other methods of the
same trait (`lt a b = compare a b == LT`); methods are in `env.vars` as
`Var::Method` from `add_traits`, which runs before decls.

Trait parameters are `TypeParam { name, kind }`. The trait's kind scheme
comes from plan 02's engine, called after the method schemes exist:

```rust
// nash-can/src/kinds.rs (plans/02 engine; this is the one addition)
/// Infer one kind per trait parameter from the method signatures, honouring
/// user kind annotations on the parameters, then generalize them together.
pub fn infer_trait_scheme<'a>(
    bump: &'a Bump,
    env: &KindEnv<'a>,
    home: ModuleName<'a>,
    params: &[(&'a str, Option<&'a Located<SourceKind<'a>>>)],
    method_sigs: &[&'a Annotation<'a>],
) -> Result<KindScheme<'a>, Vec<Error<'a>>>
```

built on `Walker::infer_type`. Share kind variables only for trait parameters
(or their annotated kinds); freshen other quantified variables independently
for each method. Check superclass and method-context predicate arguments
against the referenced trait schemes in the same inference engine. The
result is stored as `nash_ast::Trait.kind` and `InterfaceTrait.kind`
(borrowing the retained build arena). This runs in
`canonicalize_traits`, which therefore executes after plan 02's
`infer_declarations` so `KindEnv` already holds the module's types.

`local::add_traits` (after `add_union_types`, before `add_vars`):

```rust
pub fn add_traits<'a>(
    bump: &'a Bump,
    env: &mut Env<'a>,
    traits: &'a [&'a Located<SourceTrait<'a>>],
) -> Result<(), Vec<Error<'a>>> {
    dups::detect(traits.iter().map(|t| (t.value.name.value, t.value.name.region)),
        |name, first, second| Error::DuplicateTrait { name, first, second })?;
    // Method names share the value namespace with top-level values;
    // `add_vars` detects the clash via `dups::detect` over both.
    ...
}
```

Trait kind lookup is separate from type kind lookup: the two namespaces
must not overwrite each other. Infer trait kinds by SCCs over all superclass
and method-context dependencies, using provisional shared kinds inside an
SCC and instantiating completed schemes outside it. Generalize each trait
parameter arrow chain together, after its SCC succeeds. Independently reject
cycles in the superclass-only graph; mutually referring method contexts
are not themselves superclass cycles.

Two passes are needed because superclass predicates and method schemes
mention traits: pass 1 inserts `TraitInfo` stubs with empty `supers` and
`methods` so lookups resolve, pass 2 (in `canonicalize_traits`) fills them
and re-inserts. Superclass cycles are detected with `scc` over the trait
graph (local traits plus imported ones; imported ones are already acyclic).

Interface:

```rust
#[derive(Clone, Copy, Debug)]
pub struct InterfaceTrait<'a> {
    /// Private metadata is retained, but never exposed through import scopes.
    pub exported: bool,
    pub name: &'a str,
    pub parameters: &'a [&'a str],
    pub kind: KindScheme<'a>,
    pub supers: &'a [Pred<'a>],
    pub methods: &'a [InterfaceMethod<'a>],
}
#[derive(Clone, Copy, Debug)]
pub struct InterfaceMethod<'a> {
    pub name: &'a str,
    pub annotation: &'a Annotation<'a>,
    pub has_default: bool,
}
pub struct Interface<'a> { /* ... */ pub traits: &'a [InterfaceTrait<'a>] }
```

The `exported` flag is true for `Export::Trait(name)` or `Exports::Everything`.
Retain private traits for kind checking exported contexts. `foreign.rs`
adds exposed traits to `env.traits`/`q_traits` and their methods to
`env.vars` as `Var::Method` (and `q_vars` with the annotation).
`canonicalize_exports` resolves `Exposed::Upper` against trait names too;
`Privacy::Public` on a trait is `ExportOpenTrait` error.

Errors added: `DuplicateTrait`, `DuplicateMethod`, `NotFoundTrait`,
`AmbiguousTrait`, `TraitArity`, `ContextVarNotInType`,
`MethodMissingParameter`, `RecursiveSuperclass`, `ExportOpenTrait`,
`DuplicateTraitParameter`, `SuperclassBadArg` (superclass arg is not a
trait parameter).

Elm reference: `Canonicalize/Environment/Local.hs` (`addTypes`,
`addVars`), `Canonicalize/Type.hs` (`toAnnotation`, `canonicalize`),
`Canonicalize/Module.hs` (`canonicalizeExports`), `Elm/Interface.hs`
(`fromModule`, `toPublicUnion`), `Canonicalize/Environment/Foreign.hs`
(`addExposedValue`).

Acceptance coverage is in `crates/nash-can/tests/traits.rs`:

- `superclass_and_default_method`, `higher_kinded_method_context`,
  `method_quantifiers_have_independent_kinds`;
- `superclass_cycle`,
  `mutually_referencing_method_contexts_are_not_superclass_cycles`;
- `method_requires_each_trait_parameter`, `method_predicate_checks_argument_kind`,
  `methods_share_the_module_value_namespace`;
- `default_body_checks_nested_annotation_kinds`,
  `default_parameter_cannot_shadow_local_method`;
- `imported_trait_methods_survive_source_arena_drop`,
  `private_trait_metadata_does_not_expose_names`,
  `imported_trait_and_method_ambiguity`.

`types.rs::context_tests` covers qualified annotation contexts, context
variables absent from the type and trait arity. The existing module-level
unknown-constraint test now checks `NotFoundTrait` as
`unknown_trait_in_context`.

Done when: a module with traits canonicalizes; methods resolve to
`VarMethod`; exported traits and method contexts remain available through imported interfaces.

---

## Chunk 3: impl declarations and the impl table

Status: in progress. Local impl heads, capture-safe method substitution,
head/context kind checks, method checks, and local orphan/overlap checks
are implemented. Global tables now retain local and interface impls and
private trait metadata, with overlap checks across interfaces. Focused
snapshots cover these paths. Superclass entailment now checks local impls
after the global table is assembled, using givens, superclass closure and
instance contexts with cycle and work limits. The exact core Lift identity
now enables a separate reflexive rule for equal, proven-Big types, with
coherence checks against explicit impls. Solver production of its evidence
remains part of chunks 6 and 9. Partial alias heads now retain unsupplied formal
parameters and normalize known applications without opening their bound
bodies. Unresolved partial aliases remain at the higher-kinded inference
boundary pending chunk 8's delayed alias applications. Canonicalization
and diagnostic acceptance tests pass. Keep the chunk open until its
dependent solver cases in chunks 6, 8 and 9 are verified end to end.

Files: `crates/nash-can/src/traits.rs`, `crates/nash-can/src/environment.rs`,
`crates/nash-can/src/environment/local.rs`, `crates/nash-can/src/environment/foreign.rs`,
`crates/nash-can/src/interface.rs`, `crates/nash-can/src/module.rs`,
`crates/nash-can/src/error.rs`.

Change: canonicalize `impl` declarations (`nash_source::Impl { context,
head, methods, attributes }`, where `head` is a `Constraint` whose `class`
is the trait and whose `args` are the instance heads) into
`nash_ast::Impl`, build the global `ImplTable`, and enforce orphan,
overlap, superclass, kind, and method checks. Impl method bodies become
`TypedDef`s with the substituted method scheme so chunks 4-6 type-check
them with no special cases. Plan 01's `Unsupported { feature: "impls" }`
gate is deleted.

Code:

```rust
// environment.rs
#[derive(Clone, Copy, Debug)]
pub struct ImplInfo<'a> {
    pub home: ModuleName<'a>,
    pub region: Region,
    pub trait_: QualifiedName<'a>,
    pub context: &'a [Pred<'a>],
    pub heads: &'a [Located<Head<'a>>],
    /// Methods this impl defines; the rest use defaults.
    pub methods: &'a [&'a str],
}

pub type ImplTable<'a> = BTreeMap<ImplKey<'a>, &'a ImplInfo<'a>>;

/// Tables the solver needs; returned in `CanResult`.
#[derive(Clone, Debug)]
pub struct Tables<'a> {
    pub traits: BTreeMap<QualifiedName<'a>, &'a TraitInfo<'a>>,
    pub impls: ImplTable<'a>,
}
```

`impls::tables` builds `Tables` from **every** interface in `interfaces`
(not only imported ones; see coherence in the doc), then adds local
declarations. `Tables.traits` includes private metadata: the solver needs
superclass info even for traits the module never names, because resolution
can go through them. Source name lookup remains in `Env`.

Head canonicalization:

```rust
// traits.rs
/// Haskell 98 instance heads: `C 'v1 .. 'vn` with distinct vars, `()`, or a tuple of distinct vars.
fn canonicalize_head<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    typ: &'a Located<SourceType<'a>>,
) -> Result<Located<Head<'a>>, Vec<Error<'a>>> {
    let region = typ.region;
    let bad = |reason| Err(vec![Error::BadInstanceHead { region, reason }]);
    match &typ.value {
        SourceType::Type { name, args, .. } | SourceType::TypeQual { name, args, .. } => {
            let reference = types::find_type_reference(bump, env, region, module_of(&typ.value), name)?;
            let vars = distinct_vars(args).ok_or_else(|| bad(BadHead::NotAVariable))?;
            Ok(Located::at(region, Head::Named { reference, vars }))
        }
        SourceType::Unit => Ok(Located::at(region, Head::Unit)),
        SourceType::Tuple { first, second, rest } => {
            let all = [first, second].into_iter().chain(rest.iter().copied());
            Ok(Located::at(region, Head::Tuple(distinct_vars(all)?)))
        }
        SourceType::Var(_) => bad(BadHead::BareVariable),
        SourceType::Lambda { .. } => bad(BadHead::Function),
        SourceType::Record { .. } => bad(BadHead::Record),
        SourceType::VarApp { .. } => bad(BadHead::NotAVariable),
    }
}
```

`find_type_reference` is the existing `find_type` (`types.rs:166`) returning
the `QualifiedName` for a union or alias without applying it (an alias head
is nominal; no dealiasing). It does not call `check_arity` (`types.rs:268`):
a head may be partially applied (`impl Functor List`), which is what plan
02's open question about relaxing `check_arity` refers to; the kind check
below is the arity check for heads.

Kind check of heads, in `canonicalize_impl` once heads are known (uses
plan 02's engine directly, so it lives in nash-can rather than
nash-constrain):

Instantiate the trait scheme once, preserving shared kind variables across
its parameters. For each head, instantiate the named type's scheme and
apply it once per head variable. `Infer::apply` returns a `Result`;
convert application failures to `KindMismatch` at the impl head region,
with `KindContext::ImplHead`. Unify the remaining kind with the matching
trait parameter kind. Unit heads have kind `Const`; tuple heads have kind
`Term`, matching the existing type kind checker. Report
unification failures with the same head context and generalized expected
and actual kinds; do not unwrap either operation.

`Error::KindMismatch` here is `nash_can::Error::KindMismatch` (plan 02
puts all kind errors in nash-can, not nash-constrain).
`KindContext::ImplHead { trait_: QualifiedName<'a>, index: u16 }` already
exists in plan 02's `KindContext` enum, and is rendered by plans/06
chunk 13 alongside the other kind contexts. No new error variant.

`components` splits the scheme's `Kind::Arrow` chain into one kind per
trait parameter (the scheme packs parameters as `k1 -> k2 -> ... -> Term`
placeholder; `infer_trait_scheme` documents the packing). The head
variables' kinds are whatever `apply` yields; impl method schemes carry
them into chunk 9's kind predicates.

Orphan and overlap:

```rust
/// Rust's orphan rule for constructor-only heads: the trait or one head
/// constructor must be defined in this module.
fn check_orphan<'a>(home: ModuleName<'a>, trait_: QualifiedName<'a>, heads: &[Located<Head<'a>>], region: Region) -> Result<(), Vec<Error<'a>>> {
    let local = trait_.home == home
        || heads.iter().any(|h| matches!(h.value, Head::Named { reference, .. } if reference.home == home));
    if local { Ok(()) } else {
        Err(vec![Error::OrphanImpl { region, trait_, heads: heads.iter().map(|h| h.value.con()).collect_in(bump) }])
    }
}

fn impl_key<'a>(bump: &'a Bump, trait_: QualifiedName<'a>, heads: &[Located<Head<'a>>]) -> ImplKey<'a> {
    ImplKey { trait_, heads: bump.alloc_slice_fill_iter(heads.iter().map(|h| h.value.con())) }
}
```

Overlap: `impls::tables` inserts into `Tables.impls`; an existing key is
`Error::OverlappingImpls { key, first, second, first_home, second_home }`.
Both regions include their defining module because either entry can come
from an interface. The same insertion check handles local and imported impls.

Superclass check (after all local impls are in the table so order does not
matter):

```rust
/// Every superclass of the trait must be entailed at the heads by the table
/// plus the impl's own context. Mirrors the solver's `entails` but on
/// canonical types with a substitution instead of unification variables.
fn check_superclasses<'a>(bump, tables: &Tables<'a>, impl_: &ImplInfo<'a>) -> Result<(), Vec<Error<'a>>> {
    let trait_ = tables.traits[&impl_.trait_];
    let subst: BTreeMap<&str, &Located<CanType>> = trait_.parameters.iter().copied()
        .zip(impl_.heads.iter().map(|h| head_to_type(bump, &h.value)))
        .collect();
    for (index, sup) in trait_.supers.iter().enumerate() {
        let wanted = substitute_pred(bump, &subst, sup);
        if !entails_static(bump, tables, impl_.context, &wanted) {
            return Err(vec![Error::MissingSuperclass { region: impl_.region, trait_: impl_.trait_, heads, superclass: wanted, index }]);
        }
    }
    Ok(())
}

/// `by_given` then `by_instance` over canonical types. Args are compared
/// structurally; instance lookup uses the outermost constructor of each arg.
fn entails_static<'a>(bump, tables, given: &[Pred<'a>], wanted: &Pred<'a>) -> bool
```

`entailment.rs` is the only place nash-can reasons about entailment. Contexts
are flexible and may contain nested types or higher-kinded applications.
It uses rigid, nominal terms, flattens type applications, and ignores source
regions when comparing predicates. Given superclass closure is computed
once per impl. Active instance cycles fail; expanding contexts and
structural comparisons share a 16,384-step budget, with a depth limit of
128. `MissingSuperclass` includes the instantiated requirement and a
`Missing`, `Cycle`, or `Limit` reason. No failed or limited search is a proof.

Methods:

```rust
fn canonicalize_impl<'a>(bump, env, kind_env, impl_: &'a Located<SourceImpl<'a>>, warnings) -> Result<&'a Located<CanImpl<'a>>, Vec<Error<'a>>> {
    let head = &impl_.value.head;
    let info = env.find_trait(bump, head.value.class.region, head.value.module, head.value.class.value)?;
    let heads = try_all(head.value.args.iter().map(|h| canonicalize_head(bump, env, h)))?;
    if heads.len() != info.parameters.len() { TraitArity }
    check_head_kinds(bump, kind_env, env.home, info, &heads)?;
    let head_vars: BTreeSet<&str> = vars of all heads (distinct across heads too);
    let context = types::canonicalize_context(bump, env, impl_.value.context)?;
    // context may only mention head vars
    ...
    dups::detect(impl_.value.methods.iter().map(|m| (def_name(m), def_region(m))), DuplicateMethod)?;
    let mut methods = Vec::new();
    for m in impl_.value.methods {
        // plan 01 only parses `Def::Define { annotation: None, .. }` here
        let SourceDef::Define { name, args, body, annotation: None } = &m.value else { unreachable!("impl bodies are unannotated defines") };
        let Some(method) = info.methods.iter().find(|mi| mi.name == name.value) else {
            return Err(vec![Error::UnknownMethod { region: name.region, name: name.value, trait_: info.name, methods: names(info) }]);
        };
        let scheme = instantiate_method_scheme(bump, info, method.annotation, &heads, context);
        // the method body is scoped like a top-level value: head vars are the annotation's free vars
        methods.push(module::canonicalize_typed_value(bump, env, name, args, body, scheme, warnings)?);
    }
    for mi in info.methods { if !mi.has_default && !defined(mi.name) { MissingMethod } }
    Ok(bump.alloc(Located::at(impl_.region, CanImpl { trait_, context, heads, methods })))
}

/// `forall ps fv. (T ps, Cm) => t` at `ps := heads` becomes
/// `forall headvars fv. (Cimpl, Cm[ps := heads]) => t[ps := heads]`.
/// The trait predicate itself is dropped: inside the impl it is satisfied
/// by this very impl (codegen treats a `Given` of index < |Cimpl| as the
/// impl's context and resolves method-to-method calls on the same head
/// through the table).
fn instantiate_method_scheme<'a>(...) -> &'a Annotation<'a>
```

Note on the dropped trait predicate: a method body calling another method
of the same trait at the same head (`lt a b = compare a b == LT` inside
`impl Ord int`) wants `Ord int`, which resolves by instance to this impl.
Nothing special is needed; recursion through the table is finite because
the head is ground.

Impl method bodies with an annotation of their own (`compare : int -> int -> ordering` inside the impl block) are a parse error in plan 01; only definitions are allowed.

`module.rs::canonicalize` order becomes plan 02's order (plans/02 chunk 4,
`kinds::infer_declarations` before `add_ctors`) with the trait phases
inserted after the kinded declarations exist:

```
add_union_types -> check_union_free_vars -> add_traits (stubs)
-> canonicalize_aliases (pre) -> add_vars (values + method names, dup-checked together)
-> canonicalize_unions (pre) -> kinds::infer_declarations -> allocate unions/aliases
-> add_ctors -> canonicalize_traits (schemes + Trait.kind)
-> add_impls (keys, orphan, overlap) -> canonicalize_impls (head kinds, methods)
-> check_superclasses (all) -> check_binops (accepts Var::Method) -> decls -> exports
```

`CanResult` gains `tables: Tables<'a>`. `Interface` gains
`impls: &'a [ImplInfo<'a>]`, retaining every local impl regardless of exports.
The interface and table share the same metadata type and borrow strings, heads and predicates from the retained build arena.

Errors added: `BadInstanceHead { region, reason: BadHead }`,
`OrphanImpl`, `OverlappingImpls`, `MissingSuperclass`, `MissingMethod`,
`UnknownMethod`, `ImplContextVarNotInHead`, `RepeatedHeadVar`.

Elm reference: `Canonicalize/Module.hs::canonicalize` for the phase order;
`Canonicalize/Environment/Dups.hs` for the duplicate detection shape.

Tests (nash-can):

- `impl_simple`: `type Color = Red | Green` + `trait Eq 'a where eq : 'a -> 'a -> Bool` + `impl Eq Color where eq a b = ...`.
- `impl_with_context`: `impl Eq 'a => Eq (List 'a) where ...`.
- `impl_default_method_inherited`: `impl Ord Color where compare = ...` (no `lt`): snapshot shows `methods: ["compare"]`.
- `impl_unapplied_constructor`: `trait Functor 'f where map : ('a -> 'b) -> 'f 'a -> 'f 'b` + `impl Functor List where map f xs = ...` (canonicalizes and kind-checks `List : Big -> Big` against `k1 -> k2`; type checking comes in chunk 8).
- `impl_head_kind_mismatch`: `impl Functor Color` (a `Big` constructor for a `k1 -> k2` parameter) is `KindMismatch`.
- `impl_tuple_head`: `impl (Eq 'a, Eq 'b) => Eq ('a, 'b) where ...` in a module that defines `Eq` (orphan rule satisfied via the trait).
- errors: `impl_orphan` (module imports both trait and type via `Context.interfaces` built in the test), `impl_overlap`, `impl_missing_method`, `impl_unknown_method`, `impl_missing_superclass`, `impl_bad_head_nested` (`impl Eq (List Int)`), `impl_bad_head_repeated` (`trait Foo 'a 'b where ...` + `impl Foo 'a 'a`; the reflexive `Lift 'a 'a` is compiler provided and exempt, see below), `impl_bad_head_bare_var`, `impl_context_var_not_in_head`.
- Reflexive Lift: exact package/module/trait identity activates the compiler
  rule outside `Tables.impls`; no `Head::Var` is introduced. Static entailment
  checks givens first, then already-equal types with a proven Big kind.
  A copied kind inference state verifies that checking the candidate does
  not narrow any shared impl kind variable. Constructor impls that can
  overlap the rule report `ReflexiveLiftOverlap`. Tests cover exact identity,
  Big versus Const, rigid Big versus broad bounds, and same-constructor
  overlap with distinct head variable names. Solver resolution emits
  `Evidence::ReflexiveLift { typ }` in chunks 6/9, after equality and kind
  proof; unresolved candidates remain pending.

Done when: impls canonicalize into `Module.impls`, the table holds local
and imported impls, and all listed errors have snapshots.

---

## Chunk 4: predicates in the constraint language

Status: complete. Default and impl method bodies now pass through the
ordinary typed-definition constraints, with no module value header. Inference
snapshots cover invalid default and specialized impl return types, and module
helper visibility without exporting method bindings. Local and foreign
constraints carry expression identities, including the enclosing Binop node
for operators; a same-region regression checks that these remain distinct.
Annotation contexts now share the definition's rigid variables. Definition
metadata retains every original name node and full function type, including
methods and mixed recursive groups. Descriptors retain predicate IDs through
unification, including merges after recursive descriptor changes and failures.
Later solving chunks consume these contexts, record instances and copy
predicate bodies independently at each use. The inference regression
`recursive_definition_metadata_preserves_names_types_and_given_variables`
checks method headers, binder identity and shared rigid variables directly.

Files: `crates/nash-constrain/src/type_.rs`, `crates/nash-constrain/src/expression.rs`,
`crates/nash-constrain/src/module.rs`, `crates/nash-constrain/src/pattern.rs`,
`crates/nash-constrain/src/error.rs`, `crates/nash-solve/src/solve.rs`
(only to add the new fields to `Descriptor` literals), `crates/nash-solve/src/unify.rs` (same).

Change: the constraint tree carries givens and binders, descriptors carry
predicate ids, and impl/default method bodies are constrained. No solver
behaviour changes yet: `preds` stays empty, `given` is ignored.

Decision: predicates on the `Descriptor`, not a side table. `UnionFind::union`
replaces the winner's descriptor wholesale (`union_find.rs:83`), so a side
table keyed by `Variable` would need re-keying on every union and every
path compression. `unify::merge` explicitly combines and deduplicates the
predicate IDs from the current representatives, since recursive unification
can change them after the context descriptors were captured. Failed
unification also preserves their IDs. Scheme copying preserves IDs on the
original descriptor; chunk 5 must copy predicate bodies and attach the new
IDs to copied variables, rather than sharing the original IDs between uses.
The predicate bodies live in the solver (`Vec<Predicate>` indexed by
`PredId`) because only the solver creates them.

Code:

```rust
// type_.rs
/// Index into the solver's predicate store.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PredId(pub u32);

#[derive(Clone, Debug)]
pub struct Descriptor<'a> {
    pub content: Content<'a>,
    pub rank: usize,
    pub mark: Mark,
    pub copy: Option<Variable>,
    /// Unresolved predicates mentioning this variable.
    pub preds: Vec<PredId>,
}

/// A predicate in the constraint language (given contexts).
#[derive(Clone, Copy, Debug)]
pub struct Pred<'a> {
    pub trait_: QualifiedName<'a>,
    pub args: &'a [&'a Type<'a>],
}

pub enum Constraint<'a> {
    // ... unchanged variants, except that the two instantiating ones carry
    // the node so the solver can key `SolvedTypes::instances`:
    Local(Region, NodeId, &'a str, Expected<'a, &'a Type<'a>>),
    Foreign(Region, NodeId, &'a str, &'a Annotation<'a>, Expected<'a, &'a Type<'a>>),
    Let {
        rigid_vars: &'a [Variable],
        flex_vars: &'a [Variable],
        header: &'a [(&'a str, Located<&'a Type<'a>>)],
        header_con: &'a Constraint<'a>,
        body_con: &'a Constraint<'a>,
        /// The annotation's context over `rigid_vars`; assumed while solving `header_con`.
        given: &'a [Pred<'a>],
        /// The definition (or first definition of a recursive group) this
        /// Let generalizes. `None` for `exists` and lambda argument scopes.
        binder: Option<Binder<'a>>,
        /// Original name nodes and full types for every generalized member.
        /// Lexical headers cannot supply these identities and are empty for methods.
        definitions: &'a [Definition<'a>],
    },
}
```

`make_descriptor`, `exists`, and every `Constraint::Let` literal in
`expression.rs` (lines 287, 661, 860, 889, 893, 927, 931, 1020, 1063, 1076,
1095, 1100, 1104) gain `given: &[], binder: None` except:

- `constrain_def` `Def::Def` (line 889): `binder: Some(name)`.
- `constrain_def` `Def::TypedDef` (line 927): `given: instantiate_context(bump, &new_rtv, context)`, `binder: Some(name)`.
- `constrain_recursive_defs` typed inner Let (line 1076): `given` from that def's context, `binder: Some(name)`.
- `constrain_recursive_defs` outer flex Let (line 1100): `binder: Some(first untyped def's name)` (or `None` when there are none).

`Definition { site: Binder<'a>, typ: &'a Type<'a>, context: Option<&'a [Pred<'a>]> }`
keeps the original scheme identity and full inference type. `Binder::Named`
uses the canonical definition name; `Binder::Pattern` uses the original
destructuring pattern NodeId and a separate located name for diagnostics. Each ordinary definition
and method gets one entry; an untyped recursive group gets one per member in
group order. Typed recursive members retain their entry on the inner Let that
checks their annotation context. A generalized destructuring Let gets one aggregate entry and pattern binder;\nlambda argument and existential scopes have no entries.
The solver must use these entries for `SolvedTypes::schemes`, rather than
reconstructing name nodes from lexical headers or recording only the binder.

```rust
// expression.rs
/// Instantiate an annotation's context over the rigid variables in `rtv`.
fn instantiate_context<'a>(bump: &'a Bump, rtv: &Rtv<'a>, context: &[CanPred<'a>]) -> &'a [Pred<'a>] {
    bump.alloc_slice_fill_iter(context.iter().map(|pred| Pred {
        trait_: pred.trait_,
        args: bump.alloc_slice_fill_iter(pred.args.iter().map(|arg| instantiate::from_src_type(bump, rtv, arg))),
    }))
}
```

`instantiate::from_src_type` must accept `CanType::Var` for every free var
of the annotation, which `make_rigids` already guarantees (context vars are
a subset of `free_vars` by chunk 2's `check_context_vars`).

`Expr::VarMethod { method, annotation, .. }` constrains exactly like
`VarForeign`: `Constraint::Foreign(region, NodeId::expr(expr), method, annotation, expected)`.
Every `Constraint::Local`/`Foreign` site in `expression.rs` (`constrain`
lines 34-51, `constrain_binop` line 416) passes `NodeId::expr(expr)`; a
`Binop`'s operator instance is keyed by the `Binop` node itself, exactly as
plan 07 chunk 3 expects.

Module-level:

```rust
// module.rs
pub fn constrain<'a>(bump, uf, module: &CanModule<'a>) -> Constraint<'a> {
    let methods = constrain_methods(bump, uf, module);
    constrain_decls(bump, uf, module.decls, Constraint::And(bump.alloc_slice_fill_iter(
        methods.into_iter().chain(std::iter::once(Constraint::SaveTheEnvironment)),
    )))
}

/// Default bodies and impl methods, checked in the module's environment
/// (after every top-level name is in scope) without adding headers.
fn constrain_methods<'a>(bump, uf, module) -> Vec<Constraint<'a>> {
    let defs = module.traits.iter().flat_map(|t| t.value.methods.iter().filter_map(|m| m.default))
        .chain(module.impls.iter().flat_map(|i| i.value.methods.iter().copied()));
    defs.map(|def| expression::constrain_method(bump, uf, def)).collect()
}
```

```rust
// expression.rs
/// Like `constrain_def` for a `TypedDef` but with no header: the name is
/// not a binding in the module environment.
pub fn constrain_method<'a>(bump, uf, def: &CanDef<'a>) -> Constraint<'a> {
    let CanDef::TypedDef { name, free_vars, context, args, body, typ } = def else {
        unreachable!("methods are canonicalized as TypedDef")
    };
    let (new_rigids, new_rtv) = make_rigids(bump, uf, &Rtv::new(), free_vars);
    let TypedArgs { tipe: _, result_type, state } = constrain_typed_args(bump, uf, &new_rtv, name.value, args, typ);
    let expr_con = constrain(bump, uf, &new_rtv, body, Expected::FromAnnotation(name.value, args.len(), SubContext::TypedBody, result_type));
    Constraint::Let {
        rigid_vars: bump.alloc_slice_fill_iter(new_rigids.iter().map(|(_, v)| *v)),
        flex_vars: &[],
        header: &[],
        header_con: bump.alloc(Constraint::Let {
            rigid_vars: &[], flex_vars: bump.alloc_slice_fill_iter(state.vars),
            header: header_slice(bump, state.headers),
            header_con: bump.alloc(reversed_and(bump, state.rev_cons)),
            body_con: bump.alloc(expr_con),
            given: &[], binder: None,
        }),
        body_con: bump.alloc(Constraint::True),
        given: instantiate_context(bump, &new_rtv, context),
        binder: Some(name),
    }
}
```

Errors (data only; rendering is plan `nash-report`):

```rust
// error.rs
pub enum Error<'a> {
    // ... existing ...
    /// No impl for a ground predicate.
    MissingImpl {
        region: Region,
        /// The overloaded name or literal that wanted it.
        name: &'a str,
        trait_: QualifiedName<'a>,
        args: &'a [&'a ErrorType<'a>],
        /// Heads that do have impls of this trait, for the hint.
        available: &'a [&'a [HeadCon<'a>]],
    },
    /// A predicate on a rigid variable that the annotation does not provide.
    MissingConstraint {
        region: Region,
        name: &'a str,
        trait_: QualifiedName<'a>,
        args: &'a [&'a ErrorType<'a>],
        binder: &'a Located<&'a str>,
    },
    AmbiguousType {
        region: Region,
        binder: Option<&'a Located<&'a str>>,
        var: &'a str,
        preds: &'a [(QualifiedName<'a>, &'a [&'a ErrorType<'a>])],
    },
    PolymorphicRecursion {
        region: Region,
        binder: &'a Located<&'a str>,
        trait_: QualifiedName<'a>,
        args: &'a [&'a ErrorType<'a>],
    },
}
```

Elm reference: `Type/Constrain/Expression.hs::constrainDef` (lines 528-600)
and `recDefsHelp` (606-687) for the Let shapes; `Type/Constrain/Module.hs`
for the module walk.

Tests: existing inference snapshots unchanged. Add in `nash-constrain`
a unit test that `constrain_method` on a synthesized `TypedDef` produces a
`Let` with `binder == Some(name)` and the expected number of givens.

Done when: workspace compiles; `cargo insta test` shows no snapshot changes.

---

## Chunk 5: predicate store, instantiation, generalization, contexts in annotations

Files: `crates/nash-solve/src/solve.rs`, `crates/nash-solve/src/unify.rs`,
`crates/nash-solve/src/annotation.rs`, new `crates/nash-solve/src/preds.rs`,
`crates/nash-solve/src/lib.rs`, `crates/nash-can/src/interface.rs`
(`Annotations` unchanged), `crates/nash-driver/src/compile.rs`,
`crates/nash-solve/tests/inference.rs`.

Change: the solver creates wanted predicates when instantiating schemes
and annotations, merges them through unification, and at every
generalizing Let retains or defers them. Inferred contexts appear in
annotations. Resolution against the impl table is chunk 6; in this chunk a
predicate whose arguments are all headed is retained too (it becomes
`Show (List a)` in the context) so that everything remains observable in
snapshots. Chunk 6 replaces that branch.

Implementation audit (takes precedence over the sketches below):

- Keep each scheme's context explicitly, in evidence order. Discovering it
  from descriptor IDs reachable through the result type loses predicates such
  as `Eq Color => Bool`, constructed arguments such as `Show (List a)`, and
  the shared context required for every untyped recursive member.
- Preserve each wanted predicate's original use node and context slot.
  Retention ownership is separate metadata; retaining a predicate supplies
  `Given` evidence to that original use instead of overwriting its origin.
- Process definition metadata and given frames in all Let branches, including
  zero-quantifier definitions and monomorphic methods. The current fast paths
  cannot bypass scheme processing merely because variable lists are empty.
- Publish annotated recursive schemes when their lexical headers enter scope,
  before checking any group body. Keep untyped recursive uses monomorphic
  until the group generalizes. The later typed body-check Let alone cannot
  supply declared contexts to earlier recursive uses.
- Copy the full explicit context and type with one variable-copy map. Preserve
  predicate attachments when replacing a copied descriptor's content. Reset
  all touched variable and predicate copy markers, including context-only
  variables not reachable through the result type. Record generalized rigid
  variable copies now; the current solver already copies them to flex vars.
- Classify free variables after rank adjustment. Binder-less Lets must defer
  outer obligations; mixed outer/young contexts must retain outer variables
  without freshening them. `NO_RANK` alone does not establish binder ownership.
  A quantified annotation variable that remains at an outer rank is a source
  type error, `AnnotationVariableEscapes`, rather than an internal compiler
  panic. For example, `inner : 'a; inner = x` cannot quantify an outer `x`.
  Do not report this secondary rank error after an existing body type error.
- Reserve variable names across the full type and explicit context before
  assigning fresh names. `to_annotation_with_context` implements this
  conversion; scheme inference must supply its ordered context. Instance type
  arguments must follow the final annotation's `free_vars` order.

Status: annotation conversion accepts explicit contexts and has a regression
snapshot for headed and context-only arguments with a reserved variable name.
Multi-root copying shares a copy map for all roots of one instantiation and
restores all touched originals directly. Its regression covers repeated uses,
generalized rigid variables, context-only roots, and retained outer variables.
Foreign and method annotation instantiation now creates wanted predicates
with shared type variables, original use identity, context position, and rank.
The predicate store attaches each ID once per argument equivalence class.
Unannotated definition boundaries now consume their pending wanteds, retain
generalized or ground contexts explicitly on lexical bindings, and defer
outer-only predicates to the enclosing boundary. Local uses copy the complete
type and context together and create fresh wanteds with their own provenance.
Recursive group members receive the same context. Snapshots cover independent
uses, outer-only and mixed-rank scopes, constructed predicate arguments, and
mutual recursion. `Definition.context` distinguishes declared schemes
(`Some`, including empty) from inferred schemes (`None`).

Declared contexts now remain on lexical bindings, including monomorphic
annotations. `Constraint::Let.declarations` carries typed recursive schemes
to the point where their headers enter scope, before group bodies are checked.
These declarations share the rigid variables and context slice used by their
later body-check definitions. Predicate provenance distinguishes annotation
binders from originating use nodes. Snapshots cover declared constructed
arguments instantiated at two types and an inferred recursive helper that
uses an annotated member's context.

Exact givens now discharge body wanteds in every Let path, including
monomorphic definitions. Frames are scoped to the header/body check and do not
leak into subsequent bindings. Matching follows current representatives,
preserves nominal alias identity, normalizes record extension chains, and
never unifies variables to force a match. Discharged predicates retain their
use origin and record `Solution::Given { binder, index }`.

Given lookup now expands superclass metadata breadth-first from the canonical
tables. Trait parameters are substituted into each superclass requirement;
explicit givens precede projections, and `Solution::Super` preserves the
original context index and path. The regression checks a transitive chain,
selection of a trait's second parameter, and direct-given precedence.

Annotated bodies now report `MissingConstraint` for an unmatched bare rigid
trait argument, preserving the originating use and owning definition. Tests
cover direct calls, impl element constraints, outer captures through local
helpers, and successful superclass givens. The CLI exits with status 1 and
the same diagnostic. Constructor-headed requirements still need impl
resolution; the core reflexive Lift rule still needs its Big kind proof.

Retained predicates now receive `Given` or `Super` solutions at the owning
definition's final context slots without changing their original use/sub
provenance. Inferred contexts remove duplicate and superclass-implied
requirements without unification, keeping surviving predicate creation order.
The shared recursive context uses the first original definition name as binder.
All recursive members' argument/result variables now belong to the group Let;
placing earlier members' roots in later pattern scopes generalized them early
and disconnected recursive argument/result relationships. The evidence
regression exposed and verifies the correction.

Complete missing-constraint classification,
final scheme/instance recording, and the resulting
solver API are not implemented yet.
Definition records now preserve each original name-node identity, solved type,
context and quantified variable identities at its generalization boundary.
This includes local definitions and monomorphic methods through all three Let
paths. Exported annotations use those records. The local-capture regression
checks that `local y = (x, y)` quantifies only `y`, even after the enclosing
definition generalizes `x`; scheme conversion excludes the captured variable
from `free_vars`. Definition records now also retain their evidence-owner
binder separately from their own identity. Early untyped recursive headers
carry original definition identities while their contexts are unfinished;
their uses are queued and receive the final shared context after the whole
group is generalized and reduced. The recursive evidence regression now
checks calls inside the group as well as later instantiations, including an
outer recursive call that stays pending while a local helper is checked. Typed recursive
calls keep their declared contexts and do not receive duplicate slots.
Ordered instance arguments, complete all-use recording, and scoped output naming still
need to be connected before publishing the shared `SolvedTypes` result.
Chunk 6 now resolves ordinary constructor-headed predicates, including impl
contexts, before inference publishes a scheme. Do not mark this chunk complete from the current
inference snapshots alone.

Code:

```rust
// preds.rs
use nash_ast::{ImplRef, QualifiedName};
use nash_constrain::type_::PredId;
use nash_constrain::Variable;
use nash_region::Region;

#[derive(Clone, Debug)]
pub struct Predicate<'a> {
    pub trait_: QualifiedName<'a>,
    pub args: Vec<Variable>,
    pub origin: Origin<'a>,
    /// Instantiated copy during one `make_copy`; reset by `restore`.
    pub copy: Option<PredId>,
    pub solution: Option<Solution<'a>>,
}

#[derive(Clone, Copy, Debug)]
pub enum Origin<'a> {
    /// Wanted by the use `node` of `name` at `region`, for context slot `index`.
    Use { node: NodeId, region: Region, name: &'a str, index: u16 },
    /// Wanted by the context of the impl that solved `parent`.
    Sub { parent: PredId, index: u16 },
    /// Part of the scheme generalized at `binder`; `index` is its slot.
    Retained { binder: NodeId, index: u16 },
}

/// One instantiation of a scheme at a use site (the `type_args` half of
/// `SolvedTypes::instances`; the evidence half comes from predicates).
pub struct Instantiation {
    pub node: NodeId,
    /// The scheme's generalized variables paired with their copies. For a
    /// `Foreign` the "original" is the annotation's free-var name instead.
    pub vars: Vec<(Variable, Variable)>,
    pub def: Option<NodeId>,
}

#[derive(Clone, Debug)]
pub enum Solution<'a> {
    /// `type_vars`: the impl head's variables in head order (chunk 6 fills them).
    Impl { impl_: ImplRef<'a>, type_vars: Vec<Variable>, subs: Vec<PredId> },
    Given { binder: NodeId, index: u16 },
    /// A given followed by a chain of superclass indexes.
    Super { binder: NodeId, index: u16, path: Vec<u16> },
}

#[derive(Default)]
pub struct Store<'a> {
    pub preds: Vec<Predicate<'a>>,
}

impl<'a> Store<'a> {
    pub fn get(&self, id: PredId) -> &Predicate<'a> { &self.preds[id.0 as usize] }
    pub fn get_mut(&mut self, id: PredId) -> &mut Predicate<'a> { &mut self.preds[id.0 as usize] }
    pub fn push(&mut self, pred: Predicate<'a>) -> PredId {
        self.preds.push(pred);
        PredId(self.preds.len() as u32 - 1)
    }
}
```

Solver state:

```rust
// solve.rs
struct Solver<'a> {
    bump: &'a Bump,
    pools: Vec<Vec<Variable>>,
    /// Wanted predicates per rank, parallel to `pools`.
    wanted: Vec<Vec<PredId>>,
    store: Store<'a>,
    /// Given predicates of the enclosing generalizing Lets, innermost last.
    givens: Vec<GivenFrame<'a>>,
    tables: &'a Tables<'a>,
    /// `Strict` reports unresolved predicates; `Lenient` (macro rounds) drops them. See chunk 6.
    mode: Mode,
    /// One entry per generalizing Let with a binder, in solve order.
    schemes: Vec<SchemeVars<'a>>,
    /// One entry per instantiated use site (`Local` on a generalized var, `Foreign`).
    instantiations: Vec<Instantiation>,
    /// Scratch for `make_copy`: (original, copy) pairs of the current instantiation.
    copied: Vec<(Variable, Variable)>,
}

struct GivenFrame<'a> {
    binder: NodeId,
    preds: Vec<(QualifiedName<'a>, Vec<Variable>)>,
}

/// Per generalizing Let: the header variables (one per def of the group),
/// the binder, and the retained predicates. Rendered into `Scheme`s at the end.
struct SchemeVars<'a> { binder: NodeId, headers: Vec<(&'a Located<&'a str>, Variable)>, retained: Vec<PredId> }

// `run`: see "Solver API" at the end of this chunk for the final signature.
```

Registering a wanted predicate:

```rust
impl<'a> Solver<'a> {
    fn add_wanted(&mut self, uf: &mut UnionFind<'a>, rank: usize, trait_: QualifiedName<'a>, args: Vec<Variable>, origin: Origin<'a>) -> PredId {
        let id = self.store.push(Predicate { trait_, args: args.clone(), origin, copy: None, solution: None });
        for arg in args {
            uf.modify(arg, |desc| desc.preds.push(id));
        }
        self.wanted[rank].push(id);
        id
    }

    /// Remove a solved or retained predicate from its argument descriptors.
    fn detach(&mut self, uf: &mut UnionFind<'a>, id: PredId) {
        for arg in self.store.get(id).args.clone() {
            uf.modify(arg, |desc| desc.preds.retain(|p| *p != id));
        }
    }
}
```

Instantiation points (Elm `srcTypeToVariable`, `makeCopy`):

```rust
    // src_type_to_variable: after building `flex_vars`, before returning
    fn src_type_to_variable(&mut self, uf, rank, use_site: UseSite<'a>, annotation: &Annotation<'a>) -> Variable {
        // ... existing flex var creation (name-sorted, like `free_vars`) ...
        self.instantiations.push(Instantiation { node: use_site.node, vars: flex_vars.values().map(|v| (*v, *v)).collect(), def: None });
        for (index, pred) in annotation.context.iter().enumerate() {
            let args = pred.args.iter().map(|arg| self.src_type_to_var(uf, rank, &flex_vars, arg)).collect();
            self.add_wanted(uf, rank, pred.trait_, args, Origin::Use { node: use_site.node, region: use_site.region, name: use_site.name, index: index as u16 });
        }
        self.src_type_to_var(uf, rank, &flex_vars, annotation.typ)
    }

    fn make_copy(&mut self, uf, rank, var, use_site: UseSite<'a>) -> Variable {
        self.copied.clear();
        let copy = self.make_copy_help(uf, rank, use_site, var);
        restore(uf, &mut self.store, var);
        // `copied` holds every (original, copy) pair of generalized variables
        // touched by this instantiation; the def is known from `env` (the
        // solver env maps names to the header variable, and `schemes` maps
        // header variables back to their binder).
        self.instantiations.push(Instantiation { node: use_site.node, vars: std::mem::take(&mut self.copied), def: self.binder_of(var) });
        copy
    }
```

`struct UseSite<'a> { node: NodeId, region: Region, name: &'a str }` is
built from `Constraint::Local`/`Foreign`. `make_copy_help` pushes
`(variable, copy)` onto `self.copied` whenever it copies a `FlexVar`
(chunk 8 adds `RigidVar` too, which cannot occur here since generalized
variables are flex). At the end of `run`, each `Instantiation` becomes an
`Instance`: `type_args` are the copies of the scheme's `free_vars` in
annotation order (for a `Local`, the scheme is `schemes[def].annotation`,
whose free-var names were assigned by `to_annotation` from the same
original variables, so the pairing is by original `Variable`); `evidence`
is filled by chunk 6.

```rust

    fn make_copy_help(&mut self, uf, max_rank, use_site, variable) -> Variable {
        // ... as today up to `uf.set(variable, Descriptor { copy: Some(copy), .. })` ...
        // Retained predicates of a generalized variable become wanteds of the copy.
        for id in desc.preds.clone() {
            let Origin::Retained { index, .. } = self.store.get(id).origin else { continue };
            let new_id = match self.store.get(id).copy {
                Some(new_id) => new_id,
                None => {
                    let placeholder = self.store.push(Predicate { trait_: self.store.get(id).trait_, args: Vec::new(),
                        origin: Origin::Use { node: use_site.node, region: use_site.region, name: use_site.name, index }, copy: None, solution: None });
                    self.store.get_mut(id).copy = Some(placeholder);
                    let args: Vec<Variable> = self.store.get(id).args.clone().into_iter()
                        .map(|arg| self.make_copy_help(uf, max_rank, use_site, arg)).collect();
                    self.store.get_mut(placeholder).args = args;
                    self.wanted[max_rank].push(placeholder);
                    placeholder
                }
            };
            uf.modify(copy, |d| d.preds.push(new_id));
        }
        // ... content copy as today ...
    }
```

`restore` (`solve.rs:926`) also clears `copy` on the predicates of each
restored variable. The placeholder-before-recursion order mirrors Elm's
"link before copying" comment at `solve.rs:696`.

`Constraint::Local(region, node, name, ..)` and
`Constraint::Foreign(region, node, name, annotation, ..)` pass
`UseSite { node, region, name }`.

Unification (`unify.rs::merge`, `fresh`):

```rust
fn merge<'a>(uf, context: &Context<'a>, content: Content<'a>) -> UResult {
    let mut preds = context.first_desc.preds.clone();
    preds.extend(context.second_desc.preds.iter().copied().filter(|p| !preds.contains(p)));
    uf.union(context.first, context.second, Descriptor { content, rank: min, mark: NO_MARK, copy: None, preds });
    Ok(())
}
```

`fresh` creates descriptors with `preds: Vec::new()`. Error descriptors
(`unify.rs:36`) get `preds: Vec::new()` too: a mismatched variable's
predicates are dropped to avoid cascades, exactly like Elm drops content.

Generalization hook (`solve.rs:195-276`, third Let branch):

```rust
                } else {
                    let next_rank = rank + 1;
                    // ... grow pools; also grow `self.wanted` to the same length ...
                    // introduce variables (as today)
                    let locals = /* header at next_rank, as today */;
                    let given_frame = GivenFrame {
                        binder: binder.map(NodeId::def).unwrap_or(NodeId::NONE),
                        preds: given.iter().map(|p| (p.trait_, p.args.iter().map(|a| self.type_to_variable(uf, next_rank, a)).collect())).collect(),
                    };
                    self.givens.push(given_frame);
                    let state1 = self.solve(uf, env, next_rank, state, header_con);
                    let young_mark = state1.mark;
                    let visit_mark = young_mark.next();
                    let final_mark = visit_mark.next();
                    self.generalize(uf, young_mark, visit_mark, next_rank);
                    self.pools[next_rank] = Vec::new();
                    let state1 = self.split(uf, state1, next_rank, *binder, &locals);
                    self.givens.pop();
                    for rigid in rigid_vars.iter() { self.is_generic(uf, *rigid); }
                    // ... rest as today ...
                }
```

The split (this chunk's version; chunk 6 adds resolution, chunk 7 adds
defaulting):

```rust
    /// Classify every wanted predicate of the young rank after generalization.
    fn split(&mut self, uf, mut state: State<'a>, young_rank: usize, binder: Option<&'a Located<&'a str>>, locals: &[(&'a str, Located<Variable>)]) -> State<'a> {
        let mut queue: VecDeque<PredId> = std::mem::take(&mut self.wanted[young_rank]).into();
        let mut retained: Vec<PredId> = Vec::new();
        while let Some(id) = queue.pop_front() {
            match self.classify(uf, id) {
                Class::Young | Class::Ground => retained.push(id),   // chunk 6 sends Ground to by_instance
                Class::Rigid => retained.push(id),                   // chunk 6 sends Rigid to by_given
                Class::Outer(rank) => self.wanted[rank].push(id),
            }
        }
        retained.sort_unstable();
        if let Some(binder) = binder {
            let binder_id = NodeId::def(binder);
            for (index, id) in retained.iter().enumerate() {
                self.store.get_mut(*id).origin = Origin::Retained { binder: binder_id, index: index as u16 };
            }
            self.schemes.push(SchemeVars { binder: binder_id, headers: header_names(locals), retained });
        }
        state
    }

    enum Class { Ground, Rigid, Young, Outer(usize) }

    /// Look at the free variables of the arguments after generalization.
    fn classify(&mut self, uf, id: PredId) -> Class {
        let args = self.store.get(id).args.clone();
        let mut any_young = false;
        let mut any_rigid = false;
        let mut outer: Option<usize> = None;
        let mut all_headed = true;
        for arg in args {
            if head_of(uf, arg).is_none() { all_headed = false; }
            for var in free_vars(uf, arg) {
                let desc = uf.get(var);
                match desc.content {
                    Content::RigidVar(_) => any_rigid = true,
                    Content::FlexVar(_) if desc.rank == NO_RANK => any_young = true,
                    Content::FlexVar(_) => outer = Some(outer.map_or(desc.rank, |r| r.max(desc.rank))),
                    _ => {}
                }
            }
        }
        if all_headed { Class::Ground }
        else if any_rigid && !any_young { Class::Rigid }
        else if any_young { Class::Young }
        else { Class::Outer(outer.expect("a predicate with no variables is headed")) }
    }
```

`head_of(uf, var) -> Option<HeadCon>` reads `Structure(App1(home, name, _))`
as `Named`, `Unit1`/`Tuple1` as `Unit`/`Tuple(n)`, `Fun1` as `Fun`,
`Alias { home, name, .. }` as `Named` (nominal records), `Record1`/`EmptyRecord1`
as known unsupported heads that fail lookup, even when another argument has
an unknown head. They must not be conflated with flexible variables.
`free_vars(uf, var)` walks the structure collecting flex and rigid
variables (a cycle-safe walk using the occurs mark like `variable_to_error_type`).

Annotations with context (`annotation.rs`):

```rust
pub fn to_annotation<'a>(bump, uf, store: &Store<'a>, variable: Variable) -> &'a Annotation<'a> {
    // ... user_names, state, tipe as today ...
    let mut ids = BTreeSet::new();
    collect_retained(uf, store, &mut BTreeSet::new(), variable, &mut ids);
    let context = bump.alloc_slice_fill_iter(ids.into_iter().map(|id| {
        let pred = store.get(id);
        CanPred { trait_: pred.trait_, args: bump.alloc_slice_fill_iter(pred.args.iter().map(|a| variable_to_can_type(bump, uf, &mut state, *a))) }
    }));
    bump.alloc(Annotation { free_vars: .., context, typ: tipe })
}
```

`collect_retained` follows the same traversal as `get_var_names` and takes
`desc.preds` of every visited variable whose origin is `Retained`. Sorting
by `PredId` equals sorting by retained index because `split` assigned
indexes in `PredId` order. `to_error_type` is unchanged (chunk 6 renders
predicates for errors separately).

Use `Constraint::Let.definitions` for original name identities and full types.
Keep lexical headers unchanged. Binder-less scopes need no fabricated name
node or sentinel `NodeId`; only create a given frame when there is an owner.

`run` converts each `SchemeVars` to one `Scheme` per header (calling
`to_annotation` on the header variable, which also names its generalized
variables) and inserts it under `NodeId::def(header name)`; top-level
headers additionally feed `Annotations` as today. It returns
`(annotations, SolvedTypes { exprs, patterns, instances, schemes })` with
`exprs`/`patterns` left for plan 07 and `instances` built from
`self.instantiations` (evidence slices empty until chunk 6). The driver
stores `SolvedTypes`; `from_module` keeps taking `&Annotations`.
### Solver API

The implementation now returns the paired result below. Definition schemes
and local/foreign use instances preserve quantifier and context order,
including empty-context calls and recursive group evidence. Types within an
outer definition's scope share capture names. Unresolved use evidence produces
an error before publication. Literal and do instances remain for their chunks;
expression and pattern type maps remain for Plan 07. The driver currently
consumes annotations only: retaining canonical arenas and solved results is
still required by chunk 11.

```rust
// crates/nash-solve/src/solve.rs
pub fn run<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    constraint: &Constraint<'a>,
    tables: &nash_can::environment::Tables<'a>, // traits + impls from canonicalization
) -> Result<(Annotations<'a>, SolvedTypes<'a>), Vec<Error<'a>>>
```

`Annotations` is unchanged (`nash_can::Annotations`, now with contexts);
`SolvedTypes` is `nash_solve::solved::SolvedTypes` (plans/07 chunk 3,
extended in this plan's contract section); `Error` is
`nash_constrain::error::Error`. The driver passes `&can_result.tables` directly;
the solver borrows these tables for the run. Plan 04 adds its field-table
parameter when labeled constructor fields exist, and Plan 11 adds its
macro-round mode when that behavior exists. Do not add empty field-table or
mode placeholders in this plan merely to freeze a future signature.

Elm reference: `Type/Solve.hs` lines 157-201 (`CLet` generalizing branch),
574-660 (`makeCopy`, `makeCopyHelp`, `restore`), 501-570 (`srcTypeToVariable`);
`Type/Type.hs::toAnnotation`, `getVarNames`; `Type/Unify.hs::merge`.

Tests (`nash-solve/tests/inference.rs`; each defines its own trait since
core does not exist yet, and its own `type Bool = True | False` where
needed):

```rust
#[test]
fn infer_context_from_method_use() {
    assert_inference_snapshot!(r#"
        module Main exposing (..)

        type Bool = True | False

        trait Eq 'a where
            eq : 'a -> 'a -> Bool

        same x y = eq x y
    "#);
}
// same : forall a. Eq a => a -> a -> Bool

#[test]
fn infer_context_two_traits() {
    // show (eq x y) with Show and Eq: `describe : (Eq a, Show b) => ...`
}

#[test]
fn annotated_context_is_kept() {
    // member : Eq 'a => 'a -> List 'a -> Bool
    // member x xs = case xs of [] -> False; y :: ys -> if eq x y then True else member x ys
}

#[test]
fn context_deferred_to_outer_let() {
    // f x = let g y = eq x y in g
    // f : forall a. Eq a => a -> a -> Bool   (the pred on x's variable is deferred out of g)
}

#[test]
fn context_retained_in_local_let() {
    // f = let g y z = eq y z in (g, g)
    // f : forall a b. (Eq a, Eq b) => ( a -> a -> Bool, b -> b -> Bool )
}

#[test]
fn ground_pred_retained_until_chunk_6() {
    // type Color = Red | Green ; c = eq Red Green   ->  c : Eq Color => Bool  (temporary; chunk 6 changes it)
}
```

Done when: the snapshots above match the doc's inference rules
(`ground_pred_retained_until_chunk_6` is deleted in chunk 6).

---

## Chunk 6: resolution, givens, superclasses, evidence

Status: in progress. Definition boundaries resolve known nominal constructor,
unit and tuple heads through the coherent table, check head argument counts,
and instantiate impl contexts with shared variables. Unknown heads wait;
functions and structural records report `MissingImpl`. `Solution::Impl`
retains ordered type variables and child predicate IDs; `Origin::Sub` traces
errors back to the original call. Pending children form inferred contexts.
Givens take precedence. Synthetic scopes wait for surrounding equalities;
captured outer flex variables wait for enclosing givens before impl selection.
The outer-variable walk follows structure ranks before generalization propagates
them to children, and excludes `NO_RANK` generalized variables. A case-branch
regression verifies this against both inference and the CLI.
Tests cover nested applications of the same impl, context reduction, missing
child impls, given precedence through lambdas/local helpers, and expanding
contexts. Resolution reports `ImplResolutionLimit` after 128 levels or 16,384
expansions at a boundary. A real CLI project imports the trait and impls from
another module and successfully checks `keep [[()]]`.
Replacing the element with a type that has no impl reports `MissingImpl` at
the importing module's call and exits with status 1.

Retained-context reduction and evidence ownership are implemented. Tests cover
duplicate uses, transitive paths in either source order, permuted superclass
parameters with distinct variables, and impl-child evidence in recursive groups.

Final evidence publication through NodeId/SolvedTypes and kind-aware reflexive
Lift resolution are implemented and tested, including nested evidence and
rejection of foreign trait identities. The standalone canonical resolver now
returns nested impl evidence and reflexive Lift evidence, rejects open types,
and bounds recursive contexts and structural work. Its acceptance tests use
solved types, including an inferred partial alias in a higher-kinded context.
Final acceptance review remains; no chunk completion is claimed here.

Files: `crates/nash-solve/src/solve.rs`, new `crates/nash-solve/src/resolve.rs`,
`crates/nash-solve/src/preds.rs`, `crates/nash-solve/src/lib.rs`.

Change: `Class::Ground` goes to `by_instance`, `Class::Rigid` to `by_given`,
solutions become `Evidence`, and the two resolution errors are reported.
Superclass reduction of retained contexts.

Code:

```rust
// resolve.rs
impl<'a> Solver<'a> {
    /// Look the predicate up in the impl table; its context becomes sub-wanteds.
    pub(crate) fn by_instance(&mut self, uf: &mut UnionFind<'a>, id: PredId) -> Result<Vec<PredId>, Error<'a>> {
        let pred = self.store.get(id).clone();
        let heads: Vec<HeadCon<'a>> = pred.args.iter().map(|a| head_of(uf, *a).expect("classified Ground")).collect();
        let key = ImplKey { trait_: pred.trait_, heads: self.bump.alloc_slice_copy(&heads) };
        let Some(info) = self.tables.impls.get(&key).copied() else {
            return Err(self.missing_impl(uf, &pred));
        };
        // Head variables map to the actual argument variables.
        let mut subst: BTreeMap<&'a str, Variable> = BTreeMap::new();
        for (head, arg) in info.heads.iter().zip(&pred.args) {
            match (&head.value, actual_args(uf, *arg)) {
                (Head::Named { vars, .. }, args) | (Head::Tuple(vars), args) => subst.extend(vars.iter().copied().zip(args)),
                (Head::Unit, _) => {}
            }
        }
        let rank = self.wanted_rank_of(id);
        let subs: Vec<PredId> = info.context.iter().enumerate().map(|(index, ctx)| {
            let args = ctx.args.iter().map(|a| self.src_type_to_var(uf, rank, &subst, a)).collect();
            self.add_wanted(uf, rank, ctx.trait_, args, Origin::Sub { parent: id, index: index as u16 })
        }).collect();
        // head variables in head order, for `Evidence::Impl::type_args`
        let type_vars: Vec<Variable> = info.heads.iter().flat_map(|h| head_vars(&h.value)).map(|v| subst[v]).collect();
        self.store.get_mut(id).solution = Some(Solution::Impl { impl_: ImplRef { home: info.home, key }, type_vars, subs: subs.clone() });
        self.detach(uf, id);
        Ok(subs)
    }

    /// Search the given frames, innermost first, then their superclass closure.
    pub(crate) fn by_given(&mut self, uf: &mut UnionFind<'a>, id: PredId) -> Result<(), Error<'a>> {
        let pred = self.store.get(id).clone();
        for frame in self.givens.iter().rev() {
            for (index, (trait_, args)) in frame.preds.iter().enumerate() {
                if let Some(path) = self.super_path(uf, *trait_, args, &pred) {
                    let solution = if path.is_empty() {
                        Solution::Given { binder: frame.binder, index: index as u16 }
                    } else {
                        Solution::Super { binder: frame.binder, index: index as u16, path }
                    };
                    // `frame.binder` is a `NodeId`; `Solution::Given/Super` store `NodeId` too.
                    self.store.get_mut(id).solution = Some(solution);
                    self.detach(uf, id);
                    return Ok(());
                }
            }
        }
        Err(self.missing_constraint(uf, &pred))
    }

    /// Breadth-first over superclasses of `(trait_, args)` looking for `wanted`;
    /// returns the index path, empty when the given itself matches.
    fn super_path(&self, uf, trait_: QualifiedName<'a>, args: &[Variable], wanted: &Predicate<'a>) -> Option<Vec<u16>> {
        let mut queue = VecDeque::from([(trait_, args.to_vec(), Vec::new())]);
        while let Some((t, a, path)) = queue.pop_front() {
            if t == wanted.trait_ && a.len() == wanted.args.len() && a.iter().zip(&wanted.args).all(|(x, y)| uf.equivalent(*x, *y)) {
                return Some(path);
            }
            let info = self.tables.traits[&t];
            for (k, sup) in info.supers.iter().enumerate() {
                // superclass args are trait parameters: substitute positionally
                let sup_args = sup.args.iter().map(|arg| match arg.value {
                    CanType::Var(name) => a[info.parameters.iter().position(|p| *p == name).expect("superclass args are parameters")],
                    _ => unreachable!("chunk 2 restricts superclass args to parameters"),
                }).collect();
                let mut p = path.clone(); p.push(k as u16);
                queue.push_back((sup.trait_, sup_args, p));
            }
        }
        None
    }
}
```

`actual_args(uf, var)` returns the argument variables of `App1`, the
`(name, var)` args of `Alias`, the components of `Tuple1`, empty for
`Unit1`. `uf.equivalent` is exact identity: given frames hold rigid
variables, and a wanted on a rigid variable is only satisfiable by the
same rigid variable.

The `split` loop now:

```rust
            match self.classify(uf, id) {
                Class::Ground => match self.by_instance(uf, id) {
                    Ok(subs) => queue.extend(subs),
                    Err(error) => { state = add_error(state, error); self.detach(uf, id); }
                },
                Class::Rigid => if let Err(error) = self.by_given(uf, id) { state = add_error(state, error); self.detach(uf, id); },
                Class::Young => retained.push(id),
                Class::Outer(rank) => self.wanted[rank].push(id),
            }
```

Superclass reduction of `retained`: drop `p` if another retained `q` with
identical args has `p.trait_` in the superclass closure of `q.trait_`; set
`p.solution = Super { .. }` pointing at `q`'s future slot. Do this after
index assignment so `q`'s index is known; `p` gets no index.

Evidence construction at the end of `run`:

```rust
fn to_evidence<'a>(bump, uf, store: &Store<'a>, state: &mut NameState<'a>, id: PredId) -> Evidence<'a> {
    match store.get(id).solution.as_ref().expect("every non-retained predicate is solved before run ends") {
        Solution::Impl { impl_, type_vars, subs } => Evidence::Impl {
            impl_: *impl_,
            type_args: bump.alloc_slice_fill_iter(type_vars.iter().map(|v| variable_to_can_type(bump, uf, state, *v))),
            args: bump.alloc_slice_fill_iter(subs.iter().map(|s| to_evidence(bump, uf, store, state, *s))),
        },
        Solution::Given { binder, index } => Evidence::Given { binder: *binder, index: *index },
        Solution::Super { binder, index, path } => {
            let given = bump.alloc(Evidence::Given { binder: *binder, index: *index });
            path.iter().fold(given, |of, k| bump.alloc(Evidence::Super { of, index: *k })).clone_out()
        }
    }
}
```

`Instance.evidence` for node `n` collects every predicate with
`Origin::Use { node: n, index, .. }` into slot `index`; the slice length is
the scheme's context length (known from the annotation for `Foreign`, from
`schemes[def]` for `Local`), so a missing slot is a solver bug and panics.
`type_args` render through the same `NameState` as the instance's evidence
so variable names agree within one `Instance`. A `Use` predicate that ended
up `Retained` (the use is inside a definition whose scheme now carries it)
also gets `Given` evidence pointing at its own slot: `Predicate` keeps
`use_: Option<(NodeId, u16)>` separate from `origin` so both survive;
adjust the struct in this chunk.

Error construction: `missing_impl` renders args with `to_error_type` and
lists `tables.impls` keys with the same trait for the hint;
`missing_constraint` uses the innermost frame's binder.

Post-solve resolution on ground types (needed by plans/10 power-assert
for `Show` at each subexpression's type, and by plans/11 `@derive`), in a
new `crates/nash-solve/src/evidence.rs`:

```rust
//! Impl resolution for fully ground predicates, outside the solver loop.

/// Failure resolving a canonical predicate outside the inference loop.
#[derive(Debug)]
pub struct Error<'a> {
    pub trait_: QualifiedName<'a>,
    pub args: &'a [&'a Located<CanType<'a>>],
    pub reason: Failure, // MissingImpl | NonGround | Limit
}

/// Resolve `pred` against the impl table. `pred.args` must contain no
/// `Type::Var`; the caller has ground types by construction.
pub fn resolve<'a>(
    bump: &'a Bump,
    tables: &Tables<'a>,
    pred: &CanPred<'a>,
) -> Result<Evidence<'a>, Error<'a>>;
```

The resolver flattens applications while preserving `Named`/`Alias` nominal
identity, and handles `Unit` and `Tuple` heads. Functions and structural records
have no impl heads. Impl variables and original canonical type arguments stay
in head order. The in-solver selector shares `head_vars`; context substitution
uses `nash_can::types::substitute_type`, retaining alias binders.
An exact-core Lift predicate on nominally equal, already-Big arguments produces
`ReflexiveLift`. The kind proof reuses canonical kind inference and permits
inferred record carriers only while their representation stays unrestricted.
Recursive resolution and structural traversal share 16,384 work steps and
depth 128; substitution is charged before allocation. Callers provide
well-kinded canonical types and complete metadata. This API has no givens and
does not create inference variables.

Lenient mode (for plans/11 macro expansion rounds): `nash_solve::run`
takes `nash_can::Mode` (`Strict` | `Lenient`, defined by plans/11 chunk 2
on `nash_can::Context`). Every `Err(error)` arm in `split` goes through
one helper:

```rust
    /// Strict: report. Lenient: forget the predicate; its use site gets no evidence.
    fn unresolved(&mut self, uf, state: State<'a>, id: PredId, error: Error<'a>) -> State<'a> {
        self.detach(uf, id);
        match self.mode {
            Mode::Strict => add_error(state, error),
            Mode::Lenient => state,
        }
    }
```

Chunk 7's ambiguity and polymorphic-recursion errors use the same helper,
so in `Lenient` an ambiguous variable is left unresolved (not defaulted
to an error content) and `Instance.evidence` slots for dropped predicates
are absent; `run` fills such slices only when every slot resolved, and
otherwise omits the `instances` entry. Plans/11 reads `exprs`/`patterns`
only. What plans/11 calls `Error::NoInstance` is `Error::MissingImpl`
here.

Elm reference: none (Elm has no classes). The shape follows "Typing
Haskell in Haskell" `entail`/`byInst`/`bySuper`, adapted to unification
variables.

Tests:

```rust
#[test] fn resolve_ground_impl()            // eq Red Green : Bool, evidence Impl(Eq Color)
#[test] fn resolve_impl_with_context()      // impl Eq 'a => Eq (List 'a); eq [Red] [Green] resolves to Impl(Eq List)[Impl(Eq Color)]
#[test] fn resolve_impl_context_retained()  // f x = eq [x] [x]  ->  f : Eq a => a -> Bool
#[test] fn superclass_from_given()          // trait Eq 'a => Ord 'a; f : Ord 'a => 'a -> 'a -> Bool; f x y = eq x y   (Super evidence)
#[test] fn superclass_reduction()           // g x y = (eq x y, compare x y)  ->  g : Ord a => a -> a -> (Bool, Ordering)
#[test] fn default_method_body_checks()     // trait with `lt a b = eq (compare a b) LT` type-checks against the scheme
#[test] fn impl_method_uses_context()       // impl Eq 'a => Eq (List 'a) where eq xs ys = ... eq x y ...
#[test] fn multi_param_impl()               // trait Lift 'small 'big ...; impl Lift Small Big; lift (Small 1)
#[test] fn error_missing_impl()             // eq Red 1  (no Eq for the literal's type after chunk 7; here: `eq (Red, Green) (Red, Green)` with no tuple impl)
#[test] fn error_missing_constraint()       // same : 'a -> 'a -> Bool; same x y = eq x y
#[test] fn error_missing_impl_in_impl_context() // impl Eq (List 'a) without context, body uses eq on elements -> MissingConstraint
```

Add an `assert_evidence_snapshot!` macro rendering `SolvedTypes::instances`
as `<name>@<region> : [type args] [Impl Eq List [Impl Eq Color]]` lines
(the name and region come from the node's `Located<Expr>`), used by the
first four.

Done when: all listed snapshots exist, `ground_pred_retained_until_chunk_6`
is deleted.

---

## Chunk 7: literal traits, defaulting, ambiguity, polymorphic recursion; remove supertypes

Status: in progress. Declared recursive schemes now preserve quantifier
ownership during body checking. Direct, mutual and nested-helper calls reject
cycles of context-slot dependencies containing impl wrappers; unchanged givens, closed evidence,
unconstrained polymorphic recursion and nonrecursive method calls pass.
Snapshot tests and a real CLI check verify the direct diagnostic.
Nested helpers with closed evidence and separate context slots which reset to
closed evidence pass. Non-literal ambiguity is checked after generalization
and before context reduction, for inferred and annotated definitions. The
check uses all full header types in an untyped recursive group and preserves
outer captures. Diagnostics identify the innermost definition and deduplicate
equal requirements. Defaulting now recognizes the three exact core literal
traits and retries resolution with the same givens and limits. Real interfaces
test explicit literal-method calls, including chained defaults and duplicate
requirements; traits from another package remain ambiguous. Literal expressions
and patterns now produce trait predicates, including canonical bytes nodes.
Focused acceptance checks verify concrete literal impl evidence and ordered
FromX/Eq pattern givens at original nodes. Generalized let-destructuring now owns an aggregate scheme at the original
pattern node. Uses copy its whole type and context, preserving polymorphism
and quantifiers absent from the selected component. Regression tests reject
a destructured literal used as a function and verify complete use-site type
arguments. Workspace tests, strict Clippy and snapshot hygiene pass. A real
three-module CLI workspace checks all three literal syntaxes, a bytes pattern
and destructuring; applying a destructured integer as a function reports
MissingImpl at the extracted-name use. Inference snapshots reflect literal traits.
The SuperType variants, name-prefix rules, specialized unification and error
rendering are removed. Negation is canonically desugared to the core Num method
and retains evidence at its original node. Core Literal and Num source modules
are implemented; the remaining hierarchy belongs to chunk 12.
`two_literal_traits_do_not_choose_an_arbitrary_default` verifies that FromInt
and FromString on the same hidden variable report AmbiguousType with both
requirements, rather than selecting either default.

Files: `crates/nash-constrain/src/type_.rs`, `crates/nash-constrain/src/expression.rs`,
`crates/nash-constrain/src/pattern.rs`, `crates/nash-constrain/src/error_type.rs`,
`crates/nash-solve/src/solve.rs`, `crates/nash-solve/src/resolve.rs`,
`crates/nash-solve/src/unify.rs`, `crates/nash-solve/src/annotation.rs`.

Change:

1. Integer, string and bytes literals are typed through
   `FromInt`/`FromString`/`FromBytes`. Plan 01 already parses bytes in
   expressions and patterns. Add canonical bytes variants and remove both
   canonicalizer gates as part of this conversion.
2. `split` defaults ambiguous variables and reports ambiguity.
3. Cycles of local-call context-slot dependencies containing an `Impl`
   wrapper are `PolymorphicRecursion` errors, including nested helpers.
4. `SuperType`, `Content::FlexSuper`, `Content::RigidSuper`,
   `mk_flex_number`, `to_super`, `unify_flex_super*`, `combine_rigid_supers`,
   `atom_matches_super`, `comparable_occurs_check`, `unify_comparable_recursive`,
   `NameState::fresh_super_name`, `ErrorType::FlexSuper/RigidSuper`, and
   `Super` are deleted. Nash has no `number`/`comparable`/`appendable`.

Code:

```rust
// type_.rs
use nash_ast::primitives::CORE;   // PackageName { author: "nash", project: "core" }, plans/02 chunk 3
pub const fn literal_home<'a>() -> ModuleName<'a> { ModuleName { package: Some(CORE), name: "Literal" } }
pub const fn eq_home<'a>() -> ModuleName<'a> { ModuleName { package: Some(CORE), name: "Eq" } }
pub const fn monad_home<'a>() -> ModuleName<'a> { ModuleName { package: Some(CORE), name: "Monad" } }
pub const fn from_int<'a>() -> QualifiedName<'a> { QualifiedName { home: literal_home(), name: "FromInt" } }
pub const fn from_string<'a>() -> QualifiedName<'a> { .. }
pub const fn from_bytes<'a>() -> QualifiedName<'a> { .. }
pub const fn eq_trait<'a>() -> QualifiedName<'a> { QualifiedName { home: eq_home(), name: "Eq" } }

/// `forall a. Tr a => a`, the scheme of a literal (`traits` lets pattern literals add `Eq a`).
pub fn literal_annotation<'a>(bump: &'a Bump, traits: &[QualifiedName<'a>]) -> &'a Annotation<'a> {
    let var: &'a Located<CanType<'a>> = bump.alloc(Located::at_zero(CanType::Var("a")));
    bump.alloc(Annotation {
        free_vars: bump.alloc_slice_copy(&["a"]),
        context: bump.alloc_slice_fill_iter(traits.iter().map(|t| CanPred { trait_: *t, args: bump.alloc_slice_copy(&[var]) })),
        typ: var,
    })
}

/// The default type for an ambiguous literal variable.
pub fn literal_default<'a>(trait_: QualifiedName<'a>) -> Option<Type<'a>> {
    if trait_ == from_int() { Some(int()) } else if trait_ == from_string() { Some(string()) } else if trait_ == from_bytes() { Some(bytes()) } else { None }
}
```

Default types use the actual `nash/core` `Builtin` identities from Plan 02.
The old `type_::int()` and `type_::string()` still serve pre-trait literal
constraints and must be removed or replaced with that path in this chunk.

```rust
// expression.rs
        CanExpr::Int(_) => Constraint::Foreign(region, node, "fromInt", type_::literal_annotation(bump, &[type_::from_int()]), expected),
        CanExpr::Str(_) => Constraint::Foreign(region, node, "fromString", type_::literal_annotation(bump, &[type_::from_string()]), expected),
```

`Category::Number`/`Category::String` are kept for the error message, so
these two arms wrap the `Foreign` like today's `Int` arm wraps `Equal`:
`exists` is no longer needed because the flex variable is made inside
`src_type_to_variable`. `Negate` uses `Num.negate`: it is `Call(VarMethod
negate, [e])` after nash-can desugars it (add to chunk 10's desugaring list;
until then `Negate` keeps a fresh flex var with a wanted `Num a`, which is
the same code path).

Pattern literals (`pattern.rs` `CanPattern::Int`/`Str`/`Bytes` arms): a fresh flex
var `v`, push `Constraint::Foreign(region, NodeId::pattern(pattern), "literal", literal_annotation(bump, &[from_int(), eq_trait()]), Expected::NoExpectation(VarN(v)))`
and `Constraint::Pattern(region, PCategory::Int, VarN(v), expectation)`.
Use the matching literal trait and category for each syntax form. Emit one
instance per original pattern node, with ordered `[FromX, Eq]` evidence; two
independent instances at the same NodeId would overwrite each other.

Defaulting runs after generalization and resolution, before context reduction.
The implemented `check_ambiguity` / `resolve_wanted` loop is authoritative:

- Collect generalized variables absent from the full definition types; retain
  outer captures and use all members of an untyped recursive group.
- Deduplicate equal requirements. Only a unary, exact core literal predicate
  whose argument is the ambiguous variable supplies a default. Repeated
  requirements for the same literal trait count once.
- When exactly one distinct literal trait supplies a default, unify the flex
  variable with its real Builtin type at an active pool rank, never `NO_RANK`.
- Retry the original wanted IDs with the enclosing givens, source provenance,
  evidence slots and cumulative resolution budget intact. Resolution uses the
  definition's inference rank so captured outer variables remain deferred.
- If any default progresses, retry before diagnosing other hidden variables:
  a newly selected impl may supply their literal requirements.
- Only when no default progresses, report remaining ambiguity at the owning
  definition. Preserve distinct variable names across the diagnostic group.

Polymorphic recursion is checked after resolution and recursive-use backfill,
before publishing `SolvedTypes`. Build a graph whose vertices are
`(Scheme.binder, context_index)`. For each local call, connect each callee slot
to the `Given` or `Super` slots referenced by its evidence. Mark a dependency
when its path crosses `Solution::Impl`. Reject a marked edge when its
source can reach its target; report the original call and context predicate.
Closed impls add no edges. Foreign method uses do not introduce local-call
slot dependencies. This replaces the syntactic recursive-group test, which
missed evidence composition through local helpers and conflated distinct slots.

Snapshot render: `render_annotation` already prints contexts; the
`forall number. number` of `int_literal` becomes `forall a. FromInt a => a`.

Elm reference: `Type/Unify.hs` lines 283-440 (`unifyFlexSuper` through `unifyComparableRecursive`, all deleted),
`Type/Type.hs::nameToFlex` / `fresh super names` (deleted).

Tests (existing `int_literal`, `number_cannot_be_string`, and every
snapshot mentioning `number` are re-accepted after review):

```rust
#[test] fn literal_defaults_to_int()        // main = show 1   with `trait Show`, `impl Show int`, `impl FromInt int` declared in the test (use the real Builtin int and canonicalized core Literal interface)
#[test] fn literal_context_retained()       // one = 1 -> one : FromInt a => a
#[test] fn literal_in_arithmetic()          // f x = add x 1 -> f : (FromInt a, Num a) => a -> a
#[test] fn literal_pattern_wants_eq()       // isZero n = case n of 0 -> True; _ -> False -> (Eq a, FromInt a) => a -> Bool
#[test] fn error_ambiguous_no_literal()     // trait Sized 'a where size : 'a -> Int; trait Monoid 'a where empty : 'a; width = size empty
#[test] fn error_ambiguous_two_literals()   // f = eq 1 "one"  (FromInt and FromString on the same var)
#[test] fn error_polymorphic_recursion()    // nest : Show 'a => Int -> 'a -> String; nest n x = ... nest (n) [x] ...
#[test] fn polymorphic_recursion_closed_is_ok() // wrap : Show 'a => 'a -> String; wrap x = wrap (show x) ... evidence Impl(Show string) has no Given
```

Done when: no `SuperType` machinery remains in Rust sources and all
snapshots reflect literal traits.

---

## Chunk 8: higher-kinded traits (type-variable application)

Status: in progress. Constructor and inferred partial alias applications convert, unify,
generalize and retain use-site evidence. Coverage checks distinct Functor
impls, a qualified Monad bind chain, imported applications, rigid heads,
partial variable heads, cyclic diagnostics, and imported alias impl evidence
with independent prefix arguments. Closed alias templates survive inference
and interfaces; saturation does not capture caller variables, including in
nested filled aliases. Allocating normalization returns variables through
unification; inspection uses allocation-free inference views. Evidence keys
ignore open-versus-filled body representation. The temporary conversion gates
are removed. Real CLI projects verify successful imported partial aliases and
MissingImpl for another alias with the same record shape. Formatting, strict
Clippy, all workspace tests and snapshot hygiene pass.

Acceptance audit found a remaining source-annotation gap: an explicit nested
partial constructor such as `box (pairAlias int)` is rejected by the old exact
arity check in `types::canonicalize_env_type`, before kind checking. The same
ground type can be inferred and resolves correctly through the standalone
resolver. Supporting the explicit annotation remains required; the inferred
case alone does not complete this chunk.

Files: `crates/nash-ast/src/lib.rs`, `crates/nash-can/src/types.rs`,
`crates/nash-constrain/src/type_.rs`, `crates/nash-constrain/src/instantiate.rs`,
`crates/nash-solve/src/{solve.rs,unify.rs,annotation.rs,occurs.rs}`,
`crates/nash-can/src/interface.rs`.

Ownership: plan 02 provides canonical `nash_ast::Type::App { head, args }`,
including substitution. Interfaces now borrow retained canonical types from
the build arena. Keep that representation.
This plan adds solver-side `nash_constrain::Type::AppVarN` and
`FlatType::AppV1` and owns everything that *unifies* or *walks* them: the arms in `unify.rs`, `solve.rs`,
`annotation.rs`, `occurs.rs`, `instantiate.rs`. The impl head kind check
is already done in chunk 3 (it adds `KindContext::ImplHead` to plan 02's
enum).

Change: `'f 'a` in signatures. A variable applied to arguments is a new
type form; unification decomposes it against constructor applications.
Remove plan 02's explicit `UnsupportedApplication` inference boundary once
the solver handles canonical applications end to end.

Partial aliases: canonical `Type::Alias.remaining` is the unsupplied suffix
of the alias's formal parameters. `arguments` followed by `remaining`
preserves declaration order. `Open` bodies bind those formal names;
`Filled { body, typ }` retains the closed template in `body` and the substituted
caller type in `typ`, and always has empty `remaining`. Inference aliases carry
the closed template through copies and serialization, so decomposition of a
saturated alias can recover a partial constructor without reverse substitution.
Canonical substitution consumes known applications into `Named.args` or
`Alias.arguments`; applications whose heads remain variables stay `App`.
The solver must also handle heads that become known only during unification:
retain the nominal alias name, ordered parameters, supplied arguments and
closed body until saturation, and add application decomposition against
aliases alongside `App1`. Never eagerly expand an unsaturated alias body.
Remove the temporary partial-alias checks in both canonical-to-constraint
and canonical-to-solver conversion when this support lands.

Code:

```rust
// nash-ast (plan 02 chunk 1 already added this variant; shown for reference)
pub enum Type<'a> {
    // ...
    App { head: &'a Located<Type<'a>>, args: &'a [&'a Located<Type<'a>>] },
}

// nash-constrain type_.rs
pub enum Type<'a> {
    // ...
    AppVarN(&'a Type<'a>, &'a [&'a Type<'a>]),
}
pub enum FlatType<'a> {
    // ...
    /// `f a b` with `f` a variable; `args` non-empty.
    AppV1(Variable, Vec<Variable>),
}
```

Unification (`unify_structure`, new arms):

```rust
        (FlatType::AppV1(f, xs), FlatType::AppV1(g, ys)) => {
            let (short, long, short_f, long_f) = if xs.len() <= ys.len() { (xs, ys, f, g) } else { (ys, xs, g, f) };
            let extra = long.len() - short.len();
            if extra == 0 { sub_unify(short_f, long_f)?; }
            else {
                let partial = fresh(uf, vars, context, Content::Structure(FlatType::AppV1(long_f, long[..extra].to_vec())));
                sub_unify(uf, vars, short_f, partial)?;
            }
            unify_args(uf, vars, context, short, &long[extra..])
        }
        (FlatType::AppV1(f, xs), FlatType::App1(home, name, ys)) | (App1.., AppV1..) => {
            if xs.len() > ys.len() { return Err(()); }   // kind checking makes this unreachable in practice
            let split = ys.len() - xs.len();
            let partial = fresh(uf, vars, context, Content::Structure(FlatType::App1(home, name, ys[..split].to_vec())));
            sub_unify(uf, vars, f, partial)?;
            unify_args(uf, vars, context, &xs, &ys[split..])
        }
        (FlatType::AppV1(..), _) | (_, FlatType::AppV1(..)) => Err(()),
```

Before matching, `unify_structure` normalizes: if `first` is `AppV1(f, xs)`
and `f`'s content is `Structure(App1(h, n, ys))`, it treats it as
`App1(h, n, ys ++ xs)`; if `f` is `Structure(AppV1(g, ys))`, as
`AppV1(g, ys ++ xs)`. Same for `second`. `unify_flex_super` and friends
are gone (chunk 7), so this is the only structural arm to touch.

`App1` with fewer args than the constructor's arity is now a legal content
(`List` unapplied). `to_annotation`/`to_error_type` render it as `Named`
with the args it has; `render_type` in the tests prints `List`.

Other walkers get an `AppV1` arm: `adjust_rank_content` (max over `f` and
args), `restore_content`, `copy_flat_type`, `occurs`, `get_var_names`,
`term_to_can_type` (produce `App` when `f` is a variable, or fold into
`Named` when `f` resolved to `App1`), `term_to_error_type` (add
`ErrorType::VarApp`), `head_of` (an `AppV1` whose head resolves to `App1`
has that constructor as head; otherwise `None`), `actual_args` (append).
`instantiate::from_src_type` and `src_type_to_var` map `CanType::App`
to `AppVarN`/`AppV1`. `types.rs::canonicalize_type_value` maps
`SourceType::VarApp`. Interfaces retain these canonical nodes in the build arena.

Elm reference: none for `AppV1` (Elm is first-order). `Type/Unify.hs::unifyStructure`
for the arm shape; `unifyArgs` for pairwise unification with continued
unification after a mismatch.

Tests:

```rust
#[test] fn functor_big_list() {
    // trait Functor 'f where map : ('a -> 'b) -> 'f 'a -> 'f 'b
    // impl Functor List where map f xs = case xs of [] -> []; x :: rest -> f x :: map f rest
    // twice f xs = map f (map f xs)
    // twice : forall a f. Functor f => (a -> a) -> f a -> f a
    // ids = map (\x -> x) [Red]   -> ids : List Color, evidence Impl(Functor List)
}
#[test] fn functor_little_list()   // impl Functor list (needs plan 02's `list`; the same test with `type option 'a = None | Some 'a` until then)
#[test] fn functor_option()        // type Option 'a = None | Some 'a; impl Functor Option; both impls in one module, two call sites, two evidences
#[test] fn monad_bind_hkt()        // trait Applicative 'm => Monad 'm where bind : 'm 'a -> ('a -> 'm 'b) -> 'm 'b; chained binds infer `Monad m => m a`
```

Done when: `Functor` snapshots for a Big and a little (or two user)
constructors pass and `AppV1` appears in no snapshot of a fully resolved
type.

---

## Chunk 9: kind predicates on value schemes

Status: done. The call-site regression rejects the invalid
`list (option ())` with `BadKind` at the use of `first`. Annotation
kind checking now generalizes all free-variable kinds under one shared binder
and preserves the annotation's free-variable order. Retaining those signatures
on canonical definitions, methods, specialized impl methods, and constructors
is implemented. Declared signatures also reach constraints and solver binding
records; their roots participate in copying and quantifier discovery. Solver
kind graph inference now has focused coverage for shared application kinds,
type-variable merges, and rigid entailment. Local declared uses and foreign
uses retain instantiated kind roots and are checked before solved output is
published. Uses prove their requirements against enclosing declared signatures,
including captures through nested helpers, without narrowing those signatures.
Inferred signatures now combine body requirements at generalization, retain
captured roots internally, and participate in local instantiation. Declared and
inferred signatures are retained in solved schemes and exported interfaces;
local-wrapper and importing-module regressions verify enforcement. Final kind
validation now combines requirements across each top-level body and nested
definitions after seeding declared promises from outer to inner scopes. A
snapshot and real CLI regression reject conflicting Big and Term requirements
on a body-local variable absent from its declared signature. Generic structural
record calls now preserve an unresolved carrier kind: Any/All uses are accepted,
but narrowing the carrier or equating distinct carriers' kinds requires nominal
record identity from plan 04. Field values are checked independently of row
tails. Focused tests cover transactional rejection and type-variable merges,
and the CLI accepts identity applied to a record. Reflexive Lift now checks
exact core identity, existing type equality, and a non-narrowing Big proof at
the active definition boundary. Solved output retains direct and nested
reflexive evidence, while Const calls can select ordinary impls. Focused
tests use an interface with the actual nash/core Lift identity. A real CLI
workspace with a nash/core package accepts concrete/rigid Big uses and an
explicit Const impl, and rejects an unconstrained declaration. This establishes
the compiler rule, not the shipping core hierarchy (chunk 11).

The acceptance audit also replaced the hand-built imported HKT fixture with a
checked producer, verifying shared arrow-domain/argument roots after import.
The Storable interface fixture now has mutually recursive declared/inferred
definitions and snapshots their successful signatures before rejecting both
invalid imported calls. Inference snapshots display retained base bounds and
arrow kinds. CLI cross-module checks accept these signatures and reject an
invalid recursive wrapper call at its use. Formatting, strict Clippy, the full
test suite, and snapshot hygiene pass.

Files: `crates/nash-ast/src/lib.rs`, `crates/nash-can/src/module.rs`,
`crates/nash-can/src/kinds.rs`, `crates/nash-constrain/src/type_.rs`,
`crates/nash-solve/src/{preds.rs,solve.rs,resolve.rs,annotation.rs}`,
`crates/nash-constrain/src/error.rs`.

Change: plan 02's `check_annotation` returns the kind of every free
variable of a value annotation and leaves enforcement at use sites to this
plan (its open question "Kind predicates on values"). Kind requirements are
instantiated with the value scheme and retained at generalization, under a
shared binder separate from dictionary predicates. `'a : Storable` in
`cons : 'a -> list 'a -> list 'a`
then rejects `cons (Some 1) nil` at the call site, and the compiler
provided reflexive Lift rule must discharge `Big 'a` before producing
`Evidence::ReflexiveLift { typ }`.

Code:

```rust
// nash-ast
#[derive(Debug)]
pub struct Annotation<'a> {
    pub free_vars: FreeVars<'a>,
    pub context: &'a [Pred<'a>],
    /// Kind roots aligned with `free_vars`, under one shared kind binder.
    pub kinds: ValueKinds<'a>,
    pub typ: &'a Located<Type<'a>>,
}

pub struct ValueKinds<'a> {
    pub bounds: &'a [KindSet],
    pub kinds: &'a [&'a Kind<'a>],
}
```

`module.rs` stores `check_annotation`'s result into the `TypedDef`'s
annotation; `to_annotation` in `types.rs` fills
`kinds` with one distinct `ALL` kind variable per free type variable and the kind
pass overwrites it. All roots must be generalized and instantiated together:
in `'f 'a`, the domain of `'f` shares the kind of `'a`. Independent unary
schemes lose this relation. Method schemes and impl method schemes (chunks 2-3) get
their kinds from the trait's `Trait.kind` components and the head kinds.
Check every method scheme after trait kinds are installed, including methods
without defaults. Impl specialization must retain restrictions supplied by the
owning trait when its predicate is removed, and follow type-variable renaming.
Substitute the original shared method signature itself: rechecking a specialized
owner predicate alone creates fresh constructor-kind instantiations and loses
relationships to method-local variables. The canonicalizer regression
`impl_method_retains_owner_kind_restriction` covers this with
`Keep ('f : Big -> Term)` specialized to `option`.

Solver: retain one complete signature with its corresponding type variables.

```rust
pub struct KindSignature<'a> {
    pub kinds: ValueKinds<'a>,
    pub variables: &'a [Variable],
}
```

`Definition`, solver bindings, and use records carry these signatures. They
remain separate from `Predicate`: kind constraints have shared roots and no
dictionary slots. `kinds::State` uses plan 02's `Infer` over type union-find
representatives. It instantiates constructor schemes at the supplied arity,
reconciles merged roots, and reports `BadKind` with the originating use site.
Before generalization, inferred bodies combine all use requirements and retain
their captured roots. Final validation combines body requirements per outer
definition after seeding all declared promises in scope order. A use cannot
narrow a declared bound or introduce equality between independent declared
kind roots. Export serializes the shared signature in final quantifier order.

During trait resolution, active declaration signatures and the current body's
requirements supply a non-narrowing Big proof for reflexive Lift. This check
uses the explicit current binder even during defaulting retries. The rule
requires the exact core trait identity and already-equal type arguments;
otherwise normal given/impl resolution applies. `Solution::ReflexiveLift`
retains the type until evidence serialization, including under impl arguments.

`Scheme.annotation.kinds` therefore tells codegen (plan 07 `TyEnv`) the
kind of every type argument without recomputing it.

Carry the declaration signature and its type variables through `Definition`
in both typed-definition paths, including recursive declarations. Associate
roots through the annotation's `free_vars` order, not the sorted rigid-variable
allocation order. In `instantiate_binding`/`make_scheme_copies`, copy a shared
kind group once per use alongside all quantified type and predicate roots;
captured variables retain their existing restrictions. Respect frozen declared
quantifiers while checking recursive bodies. Reconcile kind roots when type
union-find representatives merge.

Kind claims must bypass trait defaulting, superclass reduction, dictionary
enumeration, and growing-evidence analysis. Recursive context completion must
not mark a kind claim as dictionary `Given`. Serialize shared kind roots from
the solver in `to_scheme_annotation`, preserving correlations in the final
quantifier order. `Tables.kinds` supplies the actual constructor kind schemes.

Elm reference: none.

Tests (inference snapshots; need plan 02's `list` and `option`):

```rust
#[test] fn kind_bound_is_enforced_at_an_inferred_call_site()
#[test] fn inferred_wrapper_preserves_the_callees_kind_requirement()
#[test] fn imported_values_retain_declared_and_inferred_kind_signatures()
#[test] fn imported_higher_kinded_value_preserves_application()
#[test] fn reflexive_lift_retains_big_evidence()
#[test] fn reflexive_lift_neither_narrows_types_nor_uses_foreign_identity()
```

Snapshot notation (plan 02 follows it): `render_annotation` writes a
bound variable as `(a : Storable)` inside the `forall`, an arrow kind as
`(f : Big -> Big)`, and a variable whose bound is `KindSet::ALL` as a bare
`a`. Bounds that are a set of base kinds render with plan 02's set names
(`Storable`, `Little`, `Any`); a single base kind renders as itself. So
`forall (a : Storable) b. a -> list a -> list a` and
`forall (f : Big -> Big) a. Functor f => f a -> f a`.

Done when: the bound, propagation, interface and Lift snapshots pass and
`Annotation.kinds`, including shared higher-kinded roots, round-trips through
interfaces. The tests above cover these requirements.

---

## Chunk 10: `do` desugaring and operator methods

Status: in progress. Do statements now reuse lambda/let canonicalization for
scoping and delayed free-variable tracking. Synthetic bind calls look up the
checked exact nash/core Monad.Monad method independently of local value names
and import aliases. Refutable patterns and unavailable core methods have
specific errors; a bind RHS cannot see its new pattern. A single-expression
block needs no bind method. Canonical snapshots cover mixed statements and
scoping, and inference checks match explicit nested bind calls with distinct
Given/Super evidence sites. Negation now lowers to an ordinary checked core
Num.negate method call. Its generated method node owns evidence, local
`negate` values do not override it, and same-named non-core traits cannot enable
the syntax. Canonical Negate, the fabricated annotation, and the dedicated
constraint/error paths are removed. A real CLI workspace accepts imported
core Num calls with a concrete impl; changing only the package identity rejects
the same prefix expressions with NegateWithoutNum. Formatting, strict Clippy,
the full tests, and snapshot hygiene pass for negation. Operators now accept
local and imported trait methods. Interfaces retain the checked method scheme
and original method identity separately from the operator provider. The
cross-module inference regression covers generic uses, concrete impl evidence,
sections, operator values, and a consumer importing only the operator provider.
All five Main operator nodes and the transitive consumer retain evidence at
their own NodeIds. A three-module CLI project compiles these uses; applying
the same operator to tuples reports MissingImpl for Methods.Select at the
operator expression, with the available unit impl listed.
The real CLI accepts a concrete option block with bind, let, discard, and final
expression statements, and reports RefutableBindPattern at an invalid `<-`
pattern. The superseded Unsupported test was removed. Formatting, strict
Clippy, the workspace tests, and snapshot hygiene pass for this step.

The do pair example currently infers `a : Term` and `m : Term -> Any` under
the shared-domain kind contract; the explicit expansion has the same bound.
This exposes a design question about changing element kinds through an
abstract constructor. Separately, the documented Applicative.apply requires
containers of functions, incompatible with the promised Big List/Storable list
instances. Both contract questions have been raised with the user; the shipping
hierarchy is not validated by the reduced test fixture.

Files: `crates/nash-can/src/expression.rs`, `crates/nash-can/src/environment/local.rs`,
`crates/nash-can/src/error.rs`.

Change: `SourceExpr::Do { stmts, last }` becomes nested `bind` calls;
`Negate` becomes a `Num.negate` call; `check_binops` accepts operators
bound to methods. Plan 01 already guarantees the block ends in an
expression (`Do::LastNotExpr` is a parse error). Any `Unsupported` gate
plan 01 left on `Do` is deleted.

Code:

```rust
// expression.rs
fn canonicalize_do<'a>(bump, env, stmts: &'a [&'a Located<Stmt<'a>>], last: &'a Located<SourceExpr<'a>>, region: Region, free_locals, warnings) -> Result<&'a Located<CanExpr<'a>>, Vec<Error<'a>>> {
    let mut body = canonicalize_expr(bump, env, last, free_locals, warnings)?;   // NOTE: scoping below
    for stmt in stmts.iter().rev() {
        body = match &stmt.value {
            Stmt::Bind { pattern, expr } => {
                pattern::require_irrefutable(pattern).map_err(|_| vec![Error::RefutableBindPattern { region: pattern.region }])?;
                let (can_pattern, bindings) = pattern::verify(bump, env, DuplicatePatternContext::LambdaArgs, pattern)?;
                // rest of the block was canonicalized under `env.add_locals(&bindings)`: see the two-pass note
                let value = canonicalize_expr(bump, env, expr, free_locals, warnings)?;
                let bind = bump.alloc(Located::at(stmt.region, CanExpr::VarMethod { trait_: monad_trait(), method: "bind", annotation: env.method_annotation(monad_trait(), "bind") }));
                let lambda = bump.alloc(Located::at(stmt.region, CanExpr::Lambda { parameters: bump.alloc_slice_copy(&[can_pattern]), body }));
                bump.alloc(Located::at(stmt.region, CanExpr::Call { function: bind, arguments: bump.alloc_slice_copy(&[value, lambda]) }))
            }
            Stmt::Expr(expr) => /* same with `Pattern::Anything` */,
            Stmt::Let(defs) => /* reuse canonicalize_let with `body` as the let body */,
        };
    }
    Ok(body)
}
```

Because each statement scopes the rest of the block, the straightforward
implementation is recursive front-to-back (`canonicalize_do_from(index,
env)`), not a reverse fold; the sketch above shows the produced shape.
`env.method_annotation` searches exposed and qualified trait metadata by the
exact nash/core Monad.Monad identity (not the value namespace, so shadowing
`bind` locally does not change `do`). A sequencing statement without that
method gets `Error::DoWithoutMonad`; a single final expression needs no method.

`Negate(e)` becomes `Call(VarMethod { Num.negate }, [e])` at the same
region; `Expr::Negate` and `Category::Negate`/`Context::Negate` are deleted
from `nash-ast`/`nash-constrain` (Elm keeps them only because `number` is
magic).

`check_binops` runs after trait canonicalization and accepts `Var::Method`
as the operator's function, including exposed imported methods. Canonical
binops retain the qualified backing function and its checked method scheme;
`interface.rs::extract_binops` uses that scheme directly, and only looks up
local ordinary functions in the solver's `annotations`. Imported operator
uses retain their provider separately for import accounting. Trait identity
and evidence continue to refer to the original defining module.

Elm reference: `Canonicalize/Expression.hs::canonicalize` (`Src.Negate`),
`Canonicalize/Environment/Local.hs::addVars`.

Tests (nash-can snapshots, plus one inference snapshot):

- `do_bind_chain`: `f m = do\n    x <- m\n    y <- m\n    pure (x, y)` canonical shape.
- `do_expr_statement`, `do_let_statement`, `do_single_expr`.
- errors: `do_refutable_pattern` (`Some x <- m`).
- inference: `do_infers_monad`: `f m = do { x <- m; y <- m; pure (x, y) }` with core-like `Functor`/`Applicative`/`Monad` declared in the test gives `f : forall m a. Monad m => m a -> m (a, a)`.

Done when: `do` blocks canonicalize and infer `Monad`; `Expr::Negate` is gone.

---

## Chunk 11: driver plumbing and cross-module tests

Status: complete. The driver acceptance tests exercise both trait-owned and
type-owned impls through direct consumers and a transitive wrapper. They build
the real dependency graph from reversed source input and require every module
interface to be present. A diagnostic snapshot checks orphan and same-module
overlap failures at the impl module while the other modules succeed. The real
CLI compiles the four-module fixture, then reports OrphanImpl and OverlappingImpls
for the corresponding invalid additions. Existing tests retain original
definition/use NodeIds and evidence binders across dependent compilation and
verify source fetching preserves dependency order, including fetch errors.
The core hierarchy and implicit core imports remain Chunk 12 work.

Files: `crates/nash-driver/src/compile.rs`, `crates/nash-can/src/interface.rs`,
`crates/nash-can/src/environment/foreign.rs`.

Change: the driver passes tables to the solver, keeps each module's
`SolvedTypes` output for codegen, and imports traits/impls across modules.
Package identity now flows from discovery through `ModuleOrigins` into
`nash_can::Context`. Application modules retain `None`. Repeated discovery of
the same URI and package is deduplicated; conflicting package ownership is
rejected. Logical source URLs remain the graph/cache keys. A real CLI workspace
with `nash/core` Literal and an application verifies explicit literal-method
defaulting; changing the package name leaves the use ambiguous. Canonical nodes now live in the build arena; owned annotations and solved
evidence remain together per module until the build ends. A regression checks
original definition/use NodeIds and evidence binders after a dependent module
compiles. The superseded interface-copy helper is removed; its tests now
exercise imported traits, impl metadata, higher-kinded types and partial aliases
using the retained arena.

Code:

```rust
// compile.rs
/// Everything codegen needs from one compiled module. plans/09 chunk on
/// `build_with` grows this into its `SolvedModule { uri, module,
/// annotations, types, source }`; the fields here are the subset this
/// plan needs and keep those names.
pub struct SolvedModule<'a> {
    pub module: &'a nash_ast::Module<'a>,
    pub annotations: nash_can::Annotations<'a>,
    pub types: nash_solve::SolvedTypes<'a>,
}

fn build_sync(sources) -> BuildResult {
    let store = Bump::new();
    let mut interfaces: BTreeMap<&str, Interface<'_>> = BTreeMap::new();
    let mut solved: BTreeMap<&str, SolvedModule<'_>> = BTreeMap::new();
    for (uri, package, source) in &sources {
        let (output, compiled) = compile_module(uri, package.as_ref(), source, &store, &interfaces);
        if let Some((interface, module)) = compiled {
            interfaces.insert(interface.home.name, interface);
            solved.insert(interface.home.name, module);
        }
        ...
    }
}
```

`compile_module` allocates the module in the build-wide `store` arena
instead of a per-module `Bump` (the per-module arena was only ever dropped
at the end of the function; codegen needs the canonical AST and
`SolvedTypes` to outlive it, and `NodeId`s are arena addresses, so the
nodes must not move). The interface-copy path is deleted; interfaces borrow the
original arena data. Annotations and `SolvedTypes` own heap maps, so keep them
in normally dropped `SolvedModule` values, not directly in a bump allocation
(which would skip their destructors). The canonicalizer's interface lookup
borrow has a separate lifetime from the arena references it returns. Memory grows with the build, which is what
Elm's `Details`/`Artifacts` do too.

`nash_solve::run(&bump, &mut uf, &constraint, &can_result.tables, &can_result.fields, Mode::Strict)`
(the "Solver API" signature in chunk 5).

Elm reference: `Build.hs::compile` (`Compile.compile` returns
`Artifacts` with the canonical module and annotations).

Tests (`compile.rs` tests):

- `trait_impls_resolve_in_direct_and_transitive_consumers`: Methods defines
  Keep and its unit impl; Types defines Token, its Keep impl, and a qualified
  `forward` scheme. Main imports both modules and resolves both impls directly.
  Transitive imports only Types and resolves both through `forward`.
- `driver_reports_orphan_and_overlap_at_the_impl_module`: an impl of an
  imported trait for an imported type fails with OrphanImpl; two local impls
  of the same head fail with OverlappingImpls. Both diagnostics are snapshotted.

Done when: all five behaviors pass and `nash check` on the cross-module project
reports the right errors.

---

## Chunk 12: the core trait hierarchy in `core/`

Status: in progress. The synthetic Builtin interface now exports all 103
specified value schemes: 101 symbolic DefaultFunction variants plus identity
and error. Static canonical type trees preserve the documented signatures;
the existing kind checker derives their shared bounds. The table adds no
runtime dependency to nash-ast. Inference snapshots and real CLI checks cover
unConstrData projections, Storable list elements, Any choice results, and unit
annotations/impls. A function element gets BadKind; unit and () impls overlap.
Formatting, strict Clippy, 1,889 tests, and snapshot hygiene pass for this step.
The real bool and Data constructor metadata is also present. Existing bool
pattern snapshots now use the synthetic interface, with Const kind and
False/True indices 0/1. Canonicalization recognizes only exact core bool;
both local and imported Basics.Bool special cases are removed. Inference and
CLI checks cover bool conditions/patterns and every Data constructor, retaining
the specified little field types. A source-defined Basics.Bool remains an
ordinary union, pattern-only imports count as used, and the CLI rejects I ().
The first real core sources are now in core/src: Eq for int, bytes, string,
bool, unit, list, pair, and Data; Literal for the three little literal types.
Their method bodies use the real builtin interface. The tests/core application
checks those impls, the neq default, recursive list/pair evidence, and literal
defaulting through the CLI in CI. This exposed and fixed list syntax still
using Big List: literals and patterns now use little list with Storable
elements. Existing list/evidence snapshots were corrected, and a function
element is rejected. All compiler-known types are seeded in scope as specified;
literal uses count their core trait imports. Formatting, strict Clippy, 1,893
tests, snapshot hygiene, and the three-module CLI acceptance pass.
Num and Integral now provide the little int methods with the specified
divide/mod versus quotient/remainder builtin mappings. Semigroup and Monoid
provide bytes, string, list, and unit impls. The seven-module CLI acceptance
checks prefix negation, arithmetic, superclass method use, generic append/empty,
and concrete empty values. The CLI rejects Num on bool with MissingImpl.
These checks establish compilation and evidence resolution; executing the
operations still depends on code generation. No Rust crate changed in this
source-only step.
Remaining core modules, Big twin impls/conversions, implicit imports,
and the full hierarchy acceptance example remain unfinished. The two contract
questions recorded in Chunk 10 also remain open.
The promised four-element tuple impls also need canonical expression/pattern
support: the current canonicalizer still reports TupleLargerThanThree for a
four-element tuple expression. The numeric acceptance uses separate pairs;
that does not count as satisfying the tuple prerequisite.

Files: `core/Eq.nash`, `core/Ord.nash`, `core/Show.nash`, `core/Num.nash`,
`core/Integral.nash`, `core/Semigroup.nash`, `core/Monoid.nash`,
`core/Functor.nash`, `core/Applicative.nash`, `core/Monad.nash`,
`core/Data.nash` (ToData/FromData), `core/Lift.nash`, `core/Literal.nash`,
`core/Prelude.nash` (the `infix` table from docs/stdlib.md and the prelude
impls), `crates/nash-ast/src/primitives.rs` (synthetic Builtin prerequisite),
`crates/nash-driver/src/compile.rs` (implicit imports),
`crates/nash-constrain/src/type_.rs` (homes match the files). One module
per trait is the lead's decision; docs/stdlib.md's single `Prelude`
listing is the same declarations split by file.

Change: write the traits from the doc's hierarchy as Nash source with
impls for the little types (`int`, `string`, `bytes`, `bool`, `unit`,
`list`, `pair`), the Big types (`Int`, `Bytes`, `List`, `Map`, `Data`),
tuples, and `option`. Method bodies use `Builtin.*`. The driver adds the
implicit imports (`import Eq exposing (Eq, eq)`, ...) like Elm's
`Imports.defaults`. `Builtin` has no Nash source: docs/stdlib.md specifies
a synthetic interface from the Rust primitive and builtin tables. The current
interface now supplies type constructors and the builtin value schemes needed
by these method bodies, including bool/Data constructor metadata. The specified
twin conversions remain a stdlib prerequisite;
do not fabricate source bindings or count declaration-only modules and test
fixtures as completion. Core impl bodies must use the specified real bindings.
Code generation for those bindings remains Plan 07 work.

Tests: a driver test compiling `core/` plus a `Main.nash` using `==`, `<`,
`+`, `show`, a `do` block over `option`, and literal defaulting, with no
trait declarations in `Main`.

Done when: `nash check` on the example in `docs/overview.md` type-checks
up to the `tests` block.

---

## Open questions

- **Pre-seeded little types.** Plan 02 supplies `int`, `string`, `bytes`
  through the real Builtin interface. Chunk 7's defaulting tests use those
  types and canonicalized literal impls, without stand-ins.
- **Per-module arenas** are settled: plans/09's `build_with` retains a
  `SolvedModule { module, annotations, types: &SolvedTypes, .. }` per
  module in the build-wide arena. Chunk 11 is the first step of that
  shape; `NodeId` being an address is why deep-copying is not an option.
- **`Negate` removal (chunk 10)** changes `Category::Negate` snapshots in
  `nash-constrain`; acceptable because Nash's `-x` is `Num.negate x`.
- **Plan 07 adjustments.** Chunk 3 of plan 07 should take `NodeId` from
  `nash-ast`, drop its own `ImplRef`, and read `Instance.evidence` as
  `&[nash_ast::Evidence]`; chunk 9's `MonoKey.evidence` becomes the
  substituted ground `&[Evidence]` and `request_method` follows
  `Evidence::Impl.type_args`. The `Hash`/`Eq` derives on `nash_ast::Type`
  and `Located` (chunk 1 here) are what make that key work.
- **Kind claim rendering.** `Annotation.kinds` in snapshots is shown as
  `forall a : Storable.` only when the bound is not `ALL`; agree the
  notation with plan 02's `k0 -> Big` rendering before chunk 9.
