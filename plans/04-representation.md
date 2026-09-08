# Plan 04: Representation types in the front end

## Goal

Make the type checker speak the representation model of
[docs/representation.md](../docs/representation.md):

- (a) remove row polymorphism; records are nominal aliases only,
- (b) remove `Float`, `Char` and Elm's magic supertypes,
- (c) replace Elm's primitive type inventory with the Nash Const/Big
  inventory homed in `nash/core`'s `Builtin` module,
- (d) make record encoding decisions (field order, alias identity)
  available in the canonical AST for codegen.

## Prerequisites

- plans/01 (syntax): record extension syntax removed from the type
  grammar, so `nash_source::Type::Record` has no `ext`; `'a` variables;
  lowercase type names.
- plans/02 (kinds): `nash-ast/src/primitives.rs` (`PRIMITIVES`,
  `builtin_home()`), closed `Kind` and datatype `context` on aliases.
  Representation is separate; use the Haskell 98 follow-up in
  [02-kind-predicates.md](02-kind-predicates.md).
- plans/03 (traits) is *not* required. Chunk B1 types literals
  monomorphically at `int`/`string`; plans/03 replaces that with `FromInt`
  and friends. If plans/03 lands first, skip the literal part of B1.

## Crates touched

`nash-ast`, `nash-can`, `nash-constrain`, `nash-solve`, `nash-driver`
(tests only).

## Reference

- Elm: `elm/compiler/src/Type/Unify.hs` (`unifyRecord`,
  `unifySharedFields`, `gatherFields`, `unifyFlexSuper`,
  `unifyFlexSuperStructure`, `combineRigidSupers`, `atomMatchesSuper`,
  `unifyRigid`, `unifyAlias`), `Type/Type.hs` (`mkFlexNumber`,
  `nameToFlex`, `nameToRigid`, `unnamedFlexSuper`, `SuperType`),
  `Type/Constrain/Expression.hs` (`constrainRecord`, `constrainUpdate`, the
  `Accessor`/`Access` cases of `constrain`), `Type/Constrain/Pattern.hs`
  (`PRecord`), `Canonicalize/Type.hs` (`TRecord`), `Type/Error.hs`
  (`Extension`, `Super`).
- Aiken: `crates/aiken-lang/src/tipo/expr.rs` `infer_record_access`,
  `infer_known_record_access`, `infer_field_access`, `infer_record_update`
  (nominal field access resolved from the record's known type), and
  `crates/aiken-lang/src/gen_uplc/builder.rs` `known_data_to_type`,
  `unknown_data_to_type`, `convert_type_to_data` for the Big/little
  boundary.
- Nash: `crates/nash-solve/src/unify.rs:578-737`,
  `crates/nash-constrain/src/type_.rs`, `crates/nash-constrain/src/expression.rs:142-211,672-770`,
  `crates/nash-constrain/src/pattern.rs:129-162`,
  `crates/nash-solve/src/solve.rs:474-491,587-608`,
  `crates/nash-solve/src/annotation.rs:114-148,322-357`,
  `crates/nash-can/src/types.rs:89-101`, `crates/nash-can/src/expression.rs:198,591-597`.

## Decisions

**Record literal rule**: `{ x = e1, y = e2 }` is resolved at
canonicalization by its *field-name set*. Exactly one record alias in scope
(unqualified or qualified) must have exactly that set; zero is
`RecordLiteralNoAlias`, more than one is `RecordLiteralAmbiguous`. The
literal canonicalizes to `Expr::Record { alias, annotation, fields }` where
`annotation` is the alias's record-constructor type (already built by
`make_record_ctor`, `environment.rs:92`) and `fields` are in declaration
order. Typing is then the constructor call's typing. This needs no
inference-time search and no new solver machinery.

**Field access rule**: `r.x`, `.x`, `{ r | x = e }` and the pattern
`{ x, y }` emit a `Constraint::Field` that the solver resolves once the
record's type variable is bound to a record alias. Unresolved field
constraints are retried when each `Let` finishes and at the end of the
module; any still unresolved is `AmbiguousRecordAccess` ("add a type
annotation"). This is order-independent, unlike Aiken's bidirectional
check, and sound because every generalization point re-checks.

**Nominal unification**: two record aliases with different names never
unify, even with identical fields. Non-record aliases stay transparent, as
in Elm.

**Anonymous record types** `{ x : int }` are only legal as the direct body
of a `type alias`; elsewhere `RecordTypeOutsideAlias`.

**Labeled constructor fields** (`type Datum = Datum { owner : Bytes, deadline : Int }`,
overview.md) are ordinary constructor fields with compile-time labels,
encoded flat. `.field` access on such a type is allowed only when the
union has exactly one constructor; the same `Constraint::Field` resolves it
through a field table of unions that the solver receives from `nash-can`.
Record update `{ x | a = e }` stays alias-only in v1. Record literals `{ a = .. }` resolve to
aliases only. A labeled constructor is built positionally, or by label as
sugar: `Datum { owner = o, deadline = d }` parses as the constructor
applied to a record literal, and `nash-can` rewrites it to the positional
call in wire order when the field set matches the labels exactly. Closed
imported unions hide labels along with constructors.

---

## Chunk A1: Drop record extension from the ASTs and canonicalization

**Files**: `crates/nash-ast/src/lib.rs`, `crates/nash-can/src/types.rs`,
`crates/nash-can/src/module.rs`, `crates/nash-can/src/environment/local.rs`,
`crates/nash-can/src/interface.rs`, `crates/nash-can/src/error.rs`,
`crates/nash-solve/tests/inference.rs` (renderer only).

**Change**:

- `nash_ast::Type::Record { fields, ext }` becomes `Type::Record { fields: &'a [FieldType<'a>] }`.
- `types.rs:89-101`: the `SourceType::Record` arm in `canonicalize_type_value`
  returns `Err(vec![Error::RecordTypeOutsideAlias { region }])`. A new
  `canonicalize_alias_body` handles the record case and delegates the rest
  to `canonicalize_type`; `module.rs:458` calls it.
- `types.rs:314-321` (`collect_free_vars`), `types.rs:378-385`
  (`dealias_help`), `module.rs:596-600` (`collect_type_edges`),
  `module.rs:632-639` (`collect_free_type_vars`), `local.rs:66,102`
  (`ext: None` matches), `interface.rs:330-337` (`copy_type`): drop `ext`.
- `inference.rs:69-80` renderer: no `ext` branch.

**Code**:

```rust
/// Canonicalize a `type alias` body: the only place a record type may appear.
pub fn canonicalize_alias_body<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    typ: &'a Located<SourceType<'a>>,
) -> Result<&'a Located<CanType<'a>>, Vec<Error<'a>>> {
    match &typ.value {
        SourceType::Record { fields } => {
            let field_dict = check_fields(fields)?;
            let fields = accumulate::try_all_alloc(
                bump,
                field_dict
                    .into_iter()
                    .map(|(_, (index, field))| canonicalize_field_type(bump, env, index, field)),
            )?;
            Ok(bump.alloc(Located::at(typ.region, CanType::Record { fields })))
        }
        _ => canonicalize_type(bump, env, typ),
    }
}
```

```rust
    RecordTypeOutsideAlias {
        region: Region,
    },
```

**Elm reference**: `Canonicalize/Type.hs` `canonicalize (Src.TRecord fields ext)`;
the Nash port drops the `ext` argument.

**Tests** (`types.rs`, `module.rs`):

- `annotation_record_ext` is deleted; `record_fields_sorted_by_name` moves
  to an alias-body test `alias_record_fields_sorted_by_name`.
- `record_type_outside_alias_errors`: `f : { x : Int } -> Int`.
- Existing module snapshots re-accepted without `ext`.

**Done when**: workspace compiles with no `ext` anywhere in `nash-can`.

---

## Chunk A2: Nominal records in the constraint language and unifier

**Files**: `crates/nash-constrain/src/type_.rs`,
`crates/nash-constrain/src/instantiate.rs`, `crates/nash-constrain/src/error_type.rs`,
`crates/nash-solve/src/solve.rs`, `crates/nash-solve/src/unify.rs`,
`crates/nash-solve/src/annotation.rs`.

**Change**:

- `Type::RecordN { fields, ext }` becomes `Type::RecordN { fields }`;
  `Type::EmptyRecordN` is removed (`type_.rs:99-104`).
- `FlatType::Record1(BTreeMap<&str, Variable>, Variable)` becomes
  `FlatType::Record1(BTreeMap<&str, Variable>)`; `FlatType::EmptyRecord1`
  removed (`type_.rs:76-77`).
- `instantiate.rs:74-86`: no `ext`.
- `solve.rs:474-491` (`type_to_var`), `solve.rs:587-608`
  (`src_type_to_var`), `solve.rs:785-793` (`make_copy_help`): no `ext`.
- `annotation.rs:114-148` and `annotation.rs:322-357`: records convert
  field-by-field; the `iterated_dealias` of the extension and `union_fields`
  / `union_error_fields` are deleted. `ErrorType::Record { fields }`;
  `Extension` removed (`error_type.rs:55-60`).
- `unify.rs`: `unify_record` is exact-field-set unification;
  `unify_shared_fields`, `gather_fields`, `RecordStructure` deleted; the
  `EmptyRecord1` arms at `unify.rs:503-513` deleted; `unify_alias`
  (`unify.rs:391`) refuses to unify two record aliases with different names.

**Code** (`unify.rs`):

```rust
            (FlatType::Record1(fields1), FlatType::Record1(fields2)) => {
                unify_record(uf, vars, context, fields1, fields2)
            }
```

```rust
// UNIFY RECORDS

/// Nominal records: both sides come from the same alias, so the field sets
/// are equal by construction. Elm's `unifyRecord` split shared/unique
/// fields for row polymorphism; only the shared case survives.
fn unify_record<'a>(
    uf: &mut UnionFind<'a>,
    vars: &mut Vec<Variable>,
    context: &Context<'a>,
    fields1: BTreeMap<&'a str, Variable>,
    fields2: BTreeMap<&'a str, Variable>,
) -> UResult {
    if fields1.len() != fields2.len() || fields1.keys().ne(fields2.keys()) {
        return Err(());
    }
    // Like `unifySharedFields`, keep unifying after a failed field so all
    // field errors surface.
    let mut failed = false;
    for ((_, actual), (_, expected)) in fields1.iter().zip(fields2.iter()) {
        if sub_unify(uf, vars, *actual, *expected).is_err() {
            failed = true;
        }
    }
    if failed {
        Err(())
    } else {
        merge(uf, context, Content::Structure(FlatType::Record1(fields1)))
    }
}
```

```rust
        Content::Alias {
            home: other_home,
            name: other_name,
            args: other_args,
            real: other_real_var,
        } => {
            if name == other_name && home == other_home {
                unify_alias_args(uf, vars, &args, &other_args)?;
                merge(uf, context, Content::Alias { home: other_home, name: other_name, args: other_args, real: other_real_var })
            } else if is_record(uf, real_var) || is_record(uf, other_real_var) {
                // Record aliases are nominal.
                Err(())
            } else {
                sub_unify(uf, vars, real_var, other_real_var)
            }
        }
```

```rust
fn is_record<'a>(uf: &mut UnionFind<'a>, var: Variable) -> bool {
    matches!(uf.get(var).content, Content::Structure(FlatType::Record1(_)))
}
```

The `Content::Structure(_)` arm of `unify_alias` (`unify.rs:439`) also
refuses when `real_var` is a record and the structure is a `Record1`: a
bare record structure only ever comes from an alias, so this is a second
alias reaching the structure through `unify_structure`'s
`Content::Alias { real, .. } => sub_unify(context.first, real)` arm
(`unify.rs:482`). Simplest: in `unify_structure`, when `flat_type` is
`Record1` and the other side is `Content::Alias`, compare alias identity via
the context descriptors before descending; document with one comment.

**Elm reference**: `Type/Unify.hs` `unifyRecord` (line 613),
`unifySharedFields` (648), `gatherFields` (677), `unifyAlias` (444),
`unifyStructure` (508).

**Tests** (`inference.rs`):

- `typed_alias_function` re-accepted.
- `nominal_records_do_not_unify`:

  ```elm
  module Main exposing (..)

  type alias a = { x : int }
  type alias b = { x : int }

  f : a -> b
  f r = r
  ```

  expected error snapshot.
- `alias_args_unify`: `type alias box 'a = { v : 'a }`, `f : box int -> box int`.

**Done when**: `nash-solve` has no `ext`, `EmptyRecord`, `gather_fields`.

---

## Chunk A3: Record literals resolve to an alias at canonicalization

**Files**: `crates/nash-ast/src/lib.rs`, `crates/nash-can/src/environment.rs`,
`crates/nash-can/src/expression.rs`, `crates/nash-can/src/error.rs`,
`crates/nash-constrain/src/expression.rs`, `crates/nash-constrain/src/error.rs`.

**Change**:

- `nash_ast::Expr::Record(&[FieldValue])` becomes:

```rust
    Record {
        alias: QualifiedName<'a>,
        /// The alias's record-constructor type, from `Env::RecordCtor`.
        annotation: &'a Annotation<'a>,
        /// Declaration order (`FieldType.index`), which is the wire order.
        fields: &'a [FieldValue<'a>],
    },
```

- `Env` gets a lookup over its `Ctor::RecordCtor` entries by field set:

```rust
impl<'a> Env<'a> {
    /// Every record alias in scope (unqualified or qualified) whose field
    /// names are exactly `names`, deduplicated by home.
    pub fn find_record_by_fields(
        &self,
        bump: &'a Bump,
        region: Region,
        names: &BTreeSet<&'a str>,
    ) -> Result<RecordCtorInfo<'a>, Vec<Error<'a>>> {
        let mut matches: Vec<RecordCtorInfo<'a>> = Vec::new();
        let candidates = self
            .ctors
            .values()
            .chain(self.q_ctors.values().flat_map(|m| m.values()));
        for info in candidates {
            if let Info::Specific(_, Ctor::RecordCtor { home, alias_name, type_vars, typ, fields }) = info {
                let key = QualifiedName { home: *home, name: alias_name };
                if fields.iter().map(|f| f.field).eq(names.iter().copied())
                    && !matches.iter().any(|m| m.reference == key)
                {
                    matches.push(RecordCtorInfo { reference: key, type_vars, typ, fields });
                }
            }
        }
        match matches.as_slice() {
            [single] => Ok(*single),
            [] => Err(vec![Error::RecordLiteralNoAlias {
                region,
                fields: bump.alloc_slice_fill_iter(names.iter().copied()),
            }]),
            many => Err(vec![Error::RecordLiteralAmbiguous {
                region,
                candidates: bump.alloc_slice_fill_iter(many.iter().map(|m| m.reference)),
            }]),
        }
    }
}
```

  `Ctor::RecordCtor` gains `fields: &'a [nash_ast::FieldType<'a>]` (name
  sorted, from the alias body) so the match needs no dealiasing.

- `expression.rs:591-597` (`SourceExpr::Record` case): after
  `check_fields`, call `find_record_by_fields`, canonicalize values, order
  them by the alias's `FieldType.index`, and build the annotation like the
  `RecordCtor` var case at `expression.rs:372-380`.

- `nash-constrain/expression.rs:672-702` `constrain_record`: the record is
  typed as a call of the constructor annotation:

```rust
fn constrain_record<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    rtv: &Rtv<'a>,
    region: Region,
    alias: QualifiedName<'a>,
    annotation: &'a Annotation<'a>,
    fields: &[FieldValue<'a>],
    expected: Exp<'a>,
) -> Constraint<'a> {
    let mut vars = Vec::with_capacity(fields.len() + 1);
    let mut cons = Vec::with_capacity(fields.len() + 2);
    let mut arg_types = Vec::with_capacity(fields.len());
    for field in fields {
        let var = mk_flex_var(uf);
        let tipe: &'a Type<'a> = bump.alloc(Type::VarN(var));
        vars.push(var);
        arg_types.push(tipe);
        cons.push(constrain(
            bump, uf, rtv, field.value,
            Expected::FromContext(region, Context::RecordField(alias.name, field.field.value), tipe),
        ));
    }
    let result_var = mk_flex_var(uf);
    let result_type: &'a Type<'a> = bump.alloc(Type::VarN(result_var));
    vars.push(result_var);
    let ctor_type = arg_types.iter().rev().fold(result_type, |acc, arg| &*bump.alloc(Type::FunN(arg, acc)));
    cons.push(Constraint::Foreign(region, alias.name, annotation, Expected::NoExpectation(ctor_type)));
    cons.push(Constraint::Equal(region, Category::Record, result_type, expected));
    exists(bump, bump.alloc_slice_fill_iter(vars), c_and(bump, cons))
}
```

  `Context::RecordField(&'a str, &'a str)` is a new `Context` variant
  (`nash-constrain/src/error.rs:56`).

- New `nash-can` errors:

```rust
    RecordLiteralNoAlias {
        region: Region,
        fields: &'a [&'a str],
    },
    RecordLiteralAmbiguous {
        region: Region,
        candidates: &'a [QualifiedName<'a>],
    },
```

**Elm reference**: `Canonicalize/Expression.hs` `Src.Record fields`;
`Type/Constrain/Expression.hs` `constrainRecord` (377) and the
`VarCtor` case that this now resembles.

**Aiken reference**: `tipo/expr.rs` `infer_record_expr` style constructor
resolution (Aiken names the constructor explicitly; Nash resolves by field
set).

**Tests** (`inference.rs` and `module.rs`):

- `record_literal` becomes

  ```elm
  module Main exposing (point, p)

  type alias point = { x : int, y : int }

  p = { x = 1, y = 2 }
  ```

  snapshot `p : point`.
- `record_literal_polymorphic_alias`: `type alias box 'a = { v : 'a }`,
  `b = { v = 1 }` gives `b : box int`.
- `record_literal_no_alias_error` (can error snapshot),
  `record_literal_ambiguous_error` (two aliases, same fields),
  `record_literal_qualified_alias_ok` (alias only in scope via
  `import Geo`; compile test in `nash-driver/src/compile.rs`).
- `record_literal_field_type_error`: `{ x = "s", y = 2 }` against
  `point` gives an error with `Context::RecordField`.

**Done when**: `Expr::Record` always names an alias and typing goes
through the constructor annotation.

---

## Chunk A4: Deferred field constraints for access, update and patterns

**Files**: `crates/nash-constrain/src/type_.rs`,
`crates/nash-constrain/src/expression.rs`, `crates/nash-constrain/src/pattern.rs`,
`crates/nash-constrain/src/error.rs`, `crates/nash-solve/src/solve.rs`,
`crates/nash-solve/src/lib.rs`.

**Change**:

- New constraint:

```rust
    /// `record` must be (or become) a record alias with `field : field_type`.
    Field {
        region: Region,
        context: FieldContext<'a>,
        record: &'a Type<'a>,
        field: &'a str,
        field_type: &'a Type<'a>,
    },
```

```rust
#[derive(Clone, Copy, Debug)]
pub enum FieldContext<'a> {
    Access { record_region: Region, maybe_name: Option<&'a str> },
    Accessor,
    Update { record: &'a str },
    Pattern,
}
```

- `expression.rs:142-161` (`Accessor`): fresh `record_var`, `field_var`;
  constraint is `Field { record: VarN(record_var), field, field_type }` and
  `Equal(FunN(record, field_type), expected)`.
- `expression.rs:163-203` (`Access`): `record_con` with
  `Expected::NoExpectation(record_type)` plus `Field` plus the existing
  `Equal(field_type, expected)`.
- `expression.rs:707-770` (`constrain_update`): one `Field` per updated
  field against `record_var`; the base expression is constrained to
  `record_var`; the result equals `record_var`. The `fields_type` /
  `ext_var` construction goes away.
- `pattern.rs:129-162` (`Record` pattern): one `Field` per name against
  the pattern's expected type var; headers as before; no `ext_var`.
- `solve.rs`: `State` gains `deferred: Vec<Deferred<'a>>`. `solve` on
  `Constraint::Field` calls `try_field`; if the record var is a flex var,
  push to `deferred`. After solving the body of every `Let`
  (`solve.rs:246` region, after `body_con`) and at the end of `run`, call
  `retry_deferred`. Leftovers become `Error::AmbiguousRecordAccess`.

**Code** (`solve.rs`):

```rust
struct Deferred<'a> {
    region: Region,
    context: FieldContext<'a>,
    record: Variable,
    field: &'a str,
    field_type: Variable,
}

impl<'a> Solver<'a> {
    fn solve_field(&mut self, uf: &mut UnionFind<'a>, rank: usize, deferred: Deferred<'a>) {
        match self.try_field(uf, &deferred) {
            FieldOutcome::Resolved => {}
            FieldOutcome::Unknown => self.deferred.push(deferred),
            FieldOutcome::Failed(error) => self.errors.push(error),
        }
    }

    /// Resolve one field constraint against the record variable's current content.
    fn try_field(&mut self, uf: &mut UnionFind<'a>, d: &Deferred<'a>) -> FieldOutcome<'a> {
        let content = uf.get(d.record).content.clone();
        let real = match content {
            Content::FlexVar(_) => return FieldOutcome::Unknown,
            Content::Alias { real, .. } => real,
            Content::Error => return FieldOutcome::Resolved,
            _ => return FieldOutcome::Failed(self.not_a_record(uf, d)),
        };
        match uf.get(real).content.clone() {
            Content::Structure(FlatType::Record1(fields)) => match fields.get(d.field) {
                Some(actual) => match unify::unify(self.bump, uf, *actual, d.field_type) {
                    Answer::Ok(vars) => {
                        self.introduce(uf, rank_of(uf, d.record), &vars);
                        FieldOutcome::Resolved
                    }
                    Answer::Err(vars, t1, t2) => {
                        self.introduce(uf, rank_of(uf, d.record), &vars);
                        FieldOutcome::Failed(Error::FieldMismatch { region: d.region, context: d.context, field: d.field, actual: t1, expected: t2 })
                    }
                },
                None => FieldOutcome::Failed(self.missing_field(uf, d, &fields)),
            },
            Content::Alias { .. } => { /* alias of an alias: loop on real */ self.try_field_real(uf, d, real) }
            _ => FieldOutcome::Failed(self.not_a_record(uf, d)),
        }
    }

    fn retry_deferred(&mut self, uf: &mut UnionFind<'a>) {
        let pending = std::mem::take(&mut self.deferred);
        for d in pending {
            self.solve_field(uf, NO_RANK, d);
        }
    }

    /// End of module: anything still unknown is ambiguous.
    fn finish_deferred(&mut self, uf: &mut UnionFind<'a>) {
        self.retry_deferred(uf);
        for d in std::mem::take(&mut self.deferred) {
            let record = annotation::to_error_type(self.bump, uf, d.record);
            self.errors.push(Error::AmbiguousRecordAccess { region: d.region, context: d.context, field: d.field, record });
        }
    }
}
```

Errors (`nash-constrain/src/error.rs`):

```rust
    FieldMismatch { region: Region, context: FieldContext<'a>, field: &'a str, actual: &'a ErrorType<'a>, expected: &'a ErrorType<'a> },
    MissingField { region: Region, context: FieldContext<'a>, field: &'a str, record: &'a ErrorType<'a>, available: &'a [&'a str] },
    NotARecord { region: Region, context: FieldContext<'a>, field: &'a str, record: &'a ErrorType<'a> },
    /// Record update on a labeled constructor type (alias records only in v1).
    UpdateNotRecord { region: Region, record: &'a ErrorType<'a> },
    AmbiguousRecordAccess { region: Region, context: FieldContext<'a>, field: &'a str, record: &'a ErrorType<'a> },
```

This chunk does not change the `run` signature: the alias arm of
`try_field` needs only the union-find. The `fields` parameter of the final
signature (plans/03 chunk 5 "Solver API", quoted in chunk A5) is passed
as `&FieldTable::default()` until A5 fills it.

Retrying at `Let` exit matters for generalization: a field constraint on a
variable about to be generalized must be resolved or reported before the
variable is copied, otherwise the instantiated copies would never see it.
After `retry_deferred`, any deferred entry whose record var has the rank
being generalized is reported as `AmbiguousRecordAccess` right there.

**Elm reference**: `Type/Constrain/Expression.hs` `constrain` for
`Can.Accessor` and `Can.Access`, `constrainUpdate` (403);
`Type/Constrain/Pattern.hs` `PRecord` (91); `Type/Solve.hs` `solve` for
`CEqual`/`CLet` (structure of the new arm).

**Aiken reference**: `tipo/expr.rs` `infer_field_access` (1083),
`infer_record_access` (1304), `infer_known_record_access` (1316),
`infer_record_update` (808): the same "record type must be known" rule,
without deferral.

**Tests** (`inference.rs`):

- `record_access` becomes `record_access_needs_known_type`
  (`getX r = r.x`, error `AmbiguousRecordAccess`).
- `record_access_with_annotation`:

  ```elm
  type alias point = { x : int, y : int }

  getX : point -> int
  getX p = p.x
  ```

- `record_access_resolved_later`: the field access is constrained before
  the call that fixes the record type, so only deferral can solve it:

  ```elm
  g : point -> int
  g p = p.x

  f p = ( p.x, g p )
  ```

  expected `f : point -> ( int, int )`.
- `record_accessor_function` becomes `record_accessor_needs_known_type`
  (`getName = .name`, error) and `record_accessor_applied`
  (`names = List.map .x [ p ]` once `List` is available; otherwise
  `n = .x p` with `p : point`).
- `record_update` becomes `record_update_with_annotation`; add
  `record_update_unknown_field_error`, `record_update_needs_known_type`.
- `record_pattern_arg`: `f : point -> int`, `f { x, y } = x`.
- `missing_field_error`: `p.z` on `point`.
- `not_a_record_error`: `1 .x` style: `f : int -> int`, `f n = n.x`.

**Done when**: every record operation goes through `Constraint::Field`;
no `RecordN` with extension anywhere.

---

## Chunk A5: Labeled constructor fields

**Files**: `crates/nash-ast/src/lib.rs`, `crates/nash-can/src/module.rs`,
`crates/nash-can/src/environment.rs`, `crates/nash-can/src/interface.rs`,
`crates/nash-can/src/pattern.rs`, `crates/nash-can/src/expression.rs`,
`crates/nash-constrain/src/pattern.rs`, `crates/nash-solve/src/solve.rs`,
`crates/nash-solve/src/lib.rs`, `crates/nash-driver/src/compile.rs`.

**Prerequisite**: plans/01 `nash_source::Ctor { name, arguments: CtorArgs }`
with `CtorArgs::{Positional(&[&Located<Type>]), Labeled(&[&FieldType])}`.

**Change**:

- `nash_ast::Ctor.arguments: &'a [&'a Located<Type<'a>>]` becomes
  `arguments: CtorArgs<'a>`:

```rust
#[derive(Clone, Copy, Debug)]
pub enum CtorArgs<'a> {
    Positional(&'a [&'a Located<Type<'a>>]),
    /// Name-sorted like alias records; `FieldType.index` is the wire position.
    Labeled(&'a [FieldType<'a>]),
}

impl<'a> CtorArgs<'a> {
    /// Field types in declaration (wire) order.
    pub fn types(&self, bump: &'a Bump) -> &'a [&'a Located<Type<'a>>] {
        match self {
            CtorArgs::Positional(types) => types,
            CtorArgs::Labeled(fields) => {
                let mut ordered: Vec<&FieldType<'a>> = fields.iter().collect();
                ordered.sort_by_key(|f| f.index);
                bump.alloc_slice_fill_iter(ordered.into_iter().map(|f| f.typ))
            }
        }
    }
}
```

  Every reader of `Ctor.arguments` (`environment.rs:65`
  `Ctor::Union.arguments`, `interface.rs:366-373` `copy_ctor`,
  `pattern.rs` constructor patterns, `expression.rs` constructor
  annotations, `nash-constrain/pattern.rs:326`) goes through `types()`;
  `canonicalize_ctors` (`module.rs:371`) maps `CtorArgs::Labeled` through
  `check_fields` (duplicate labels are `DuplicateField`) and
  `canonicalize_field_type`.

- Field table for the solver. `nash-can` builds it from local unions and
  every `InterfaceUnion` in scope whose constructors are visible:

```rust
/// Labeled single-constructor unions reachable by `.field`, for the solver.
pub struct FieldTable<'a> {
    unions: BTreeMap<QualifiedName<'a>, LabeledUnion<'a>>,
}

#[derive(Clone, Copy)]
pub struct LabeledUnion<'a> {
    pub parameters: &'a [&'a str],
    pub fields: &'a [FieldType<'a>],
}

impl<'a> FieldTable<'a> {
    pub fn get(&self, name: QualifiedName<'a>) -> Option<LabeledUnion<'a>> {
        self.unions.get(&name).copied()
    }
}
```

  `nash_can::canonicalize` returns it in `CanResult.fields`, and the
  solver receives it through the `fields` parameter of the final
  `nash_solve::run` signature, defined once in plans/03 chunk 5 "Solver
  API" and quoted here for the call sites, not redefined:

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

  Call sites that land before a parameter exists pass
  `&Tables::default()`, `&FieldTable::default()` and `Mode::Strict`. This
  chunk fills in `fields`; `tables` and `mode` stay at their defaults.

- `try_field` (chunk A4) gains the union arm:

```rust
            Content::Structure(FlatType::App1(home, name, args)) => {
                let Some(union) = self.fields.get(QualifiedName { home, name }) else {
                    return FieldOutcome::Failed(self.not_a_record(uf, d));
                };
                let Some(field) = union.fields.iter().find(|f| f.field == d.field) else {
                    return FieldOutcome::Failed(self.missing_field(uf, d, union.fields));
                };
                let substitution: BTreeMap<&'a str, Variable> =
                    union.parameters.iter().copied().zip(args.iter().copied()).collect();
                let rank = uf.get(d.record).rank;
                let field_var = self.src_type_to_var(uf, rank, &substitution, field.typ);
                match unify::unify(self.bump, uf, field_var, d.field_type) { .. }
            }
```

  `src_type_to_var` (`solve.rs:553`) already instantiates a canonical type
  against a name-to-variable map, which is exactly the parameter
  substitution needed.

- Multi-constructor labeled unions are simply absent from the table, so
  `.field` on them reports `NotARecord` with a hint to use `case`.
  Record update is alias-only: the `App1` arm returns
  `FieldOutcome::Failed(Error::UpdateNotRecord { .. })` when
  `d.context` is `FieldContext::Update`, before consulting the table.

- Construction by label. `Datum { owner = o, deadline = d }` reaches
  `nash-can` as `SourceExpr::Call { function: Var(Datum), arguments: [Record(..)] }`.
  In `canonicalize_expr` (`expression.rs`), the `Call` case checks for the
  shape "constructor variable applied to exactly one record literal" before
  the general path. When the constructor resolves to `Ctor::Union` with
  `CtorArgs::Labeled(fields)`, the literal's field set is compared with the
  labels and the call is rewritten to `Expr::Call` with the values in wire
  order; otherwise the general path runs (a positional constructor applied
  to an alias record literal stays a one-argument call).

```rust
/// `Ctor { a = x, b = y }` on a labeled constructor: reorder to wire order.
fn canonicalize_labeled_construction<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    region: Region,
    ctor_expr: &'a Located<CanExpr<'a>>,
    labels: &'a [FieldType<'a>],
    literal: &'a [&'a nash_source::FieldAssign<'a>],
    free_locals: &mut FreeLocals<'a>,
    warnings: &mut Vec<Warning<'a>>,
) -> Result<CanExpr<'a>, Vec<Error<'a>>> {
    let given = check_fields(literal)?; // name -> (index in literal, assign)
    let mut errors = Vec::new();
    for label in labels {
        if !given.contains_key(label.field) {
            errors.push(Error::LabeledCtorMissingField { region, ctor: ctor_name(ctor_expr), field: label.field });
        }
    }
    for (name, (_, assign)) in &given {
        if !labels.iter().any(|l| l.field == *name) {
            errors.push(Error::LabeledCtorExtraField { region: assign.field.region, ctor: ctor_name(ctor_expr), field: name });
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let mut ordered: Vec<&FieldType<'a>> = labels.iter().collect();
    ordered.sort_by_key(|l| l.index);
    let arguments = accumulate::try_all_alloc_ref(
        bump,
        ordered.iter().map(|label| {
            let (_, assign) = given[label.field];
            canonicalize_expr(bump, env, assign.value, free_locals, warnings)
        }),
    )?;
    Ok(CanExpr::Call { function: ctor_expr, arguments })
}
```

  New `nash-can` errors: `LabeledCtorMissingField { region, ctor, field }`,
  `LabeledCtorExtraField { region, ctor, field }`. Applying a labeled
  constructor to a record literal whose alias happens to exist is still
  the sugar: the labels win, because a constructor field of an alias
  record type would need parentheses (`Datum ({ .. })`) to be positional.

- Matching by label. `Datum { owner, deadline }` reaches `nash-can` as
  `SourcePattern::Ctor { name, args: [Record(names)] }` (`pattern.rs:191`).
  In the `Ctor` case of `canonicalize_pattern`, before the arity check
  (`pattern.rs:235`), a single record-pattern argument on a constructor with
  `CtorArgs::Labeled(fields)` is rewritten to a positional pattern: one
  sub-pattern per label in wire order, `Pattern::Var(name)` for each named
  label and `Pattern::Anything` for the rest. Names must be a subset of the
  labels; an unknown name is `LabeledCtorUnknownField`. Duplicate names are
  already `DuplicatePattern` from `verify`. A record pattern argument on a
  positional constructor stays a one-argument positional pattern (the
  field must then be an alias record).

```rust
/// `Ctor { a, b }` on a labeled constructor: expand to wire-order sub-patterns.
fn expand_labeled_pattern<'a>(
    bump: &'a Bump,
    region: Region,
    ctor: &'a str,
    labels: &'a [FieldType<'a>],
    names: &'a [&'a Located<&'a str>],
) -> Result<Vec<&'a Located<SourcePattern<'a>>>, Vec<Error<'a>>> {
    let unknown: Vec<Error<'a>> = names
        .iter()
        .filter(|n| !labels.iter().any(|l| l.field == n.value))
        .map(|n| Error::LabeledCtorUnknownField { region: n.region, ctor, field: n.value })
        .collect();
    if !unknown.is_empty() {
        return Err(unknown);
    }
    let mut ordered: Vec<&FieldType<'a>> = labels.iter().collect();
    ordered.sort_by_key(|l| l.index);
    Ok(ordered
        .into_iter()
        .map(|label| match names.iter().find(|n| n.value == label.field) {
            Some(name) => &*bump.alloc(Located::at(name.region, SourcePattern::Var(name.value))),
            None => &*bump.alloc(Located::at(region, SourcePattern::Anything)),
        })
        .collect())
}
```

  The expansion produces source patterns so the existing constructor path
  (`canonicalize_pattern` on `Ctor`, `PatternCtorArg` construction, the
  arity check) runs unchanged on the result. New error:
  `LabeledCtorUnknownField { region, ctor, field }`.

- Encapsulation: `InterfaceUnion::to_public` (`interface.rs:92`) already
  strips constructors for closed exports, and the field table is built
  from visible constructors only, so labels are hidden with them. No extra
  code; one test.

**Elm reference**: none (Elm has no labeled constructor fields).
**Aiken reference**: `crates/aiken-lang/src/tipo/expr.rs`
`infer_known_record_access` (1316): field lookup on a data type's single
constructor with the type's arguments substituted; `crates/aiken-lang/src/ast.rs`
`RecordConstructor`/`RecordConstructorArg` for the labeled AST shape.

**Tests** (`inference.rs`, `module.rs`):

- `labeled_ctor_access`:

  ```elm
  type Datum = Datum { owner : Bytes, deadline : Int }

  deadline : Datum -> Int
  deadline d = d.deadline
  ```

- `labeled_ctor_polymorphic_access`: `type box 'a = Box { v : 'a }`,
  `get : box int -> int`, `get b = b.v`.
- `labeled_ctor_update_error`: `{ d | deadline = 0 }` on `Datum` gives
  `UpdateNotRecord`.
- `labeled_ctor_multi_access_error`: two constructors, `.field` gives
  `NotARecord`.
- `labeled_ctor_positional_construction`: `Datum owner deadline` typed by
  the existing constructor annotation.
- `labeled_ctor_construction_by_label` (module snapshot):
  `Datum { deadline = 1, owner = o }` canonicalizes to a call with
  `owner` first.
- `labeled_ctor_missing_field_error`, `labeled_ctor_extra_field_error`
  (can error snapshots).
- `positional_ctor_applied_to_record_literal_is_a_call`: `Box { x = 1 }`
  where `Box` has one positional field of an alias record type.
- `labeled_ctor_pattern_by_label` (module snapshot):
  `case d of Datum { deadline } -> deadline` expands to
  `Datum _ deadline`.
- `labeled_ctor_pattern_unknown_field_error`: `Datum { amount } -> ..`.
- `positional_ctor_record_pattern_stays_positional`: `Box { x } -> x`
  where `Box` wraps an alias record.
- `module.rs`: `ctor_duplicate_label_error`, `labeled_ctor_fields_in_wire_order`
  (snapshot shows `index`).
- `nash-driver`: imported open union with labels supports `.field`; an
  imported closed union (`Datum` without `(..)`) does not.

**Done when**: labeled and positional constructors share every code path
except canonicalization and the field table.

---

## Chunk B1: Remove `Float`, `Char` and supertypes

**Files**: `crates/nash-constrain/src/type_.rs`,
`crates/nash-constrain/src/error_type.rs`, `crates/nash-constrain/src/expression.rs`,
`crates/nash-constrain/src/pattern.rs`, `crates/nash-solve/src/unify.rs`,
`crates/nash-solve/src/solve.rs`, `crates/nash-solve/src/annotation.rs`,
`crates/nash-solve/src/occurs.rs`.

**Change**:

- `type_.rs`: delete `SuperType` (136-141), `Content::FlexSuper` and
  `Content::RigidSuper` (122,124), `mk_flex_number` (248),
  `unnamed_flex_super` (256), `to_super` (280), `float()` (218),
  `char_home()` (201). `name_to_flex`/`name_to_rigid` (262-276) become
  plain `FlexVar(Some(name))` / `RigidVar(name)`.
- `error_type.rs`: delete `Super`, `ErrorType::FlexSuper`,
  `ErrorType::RigidSuper`, `is_float`, `is_char`.
- `unify.rs`: delete `unify_flex_super` (222), `combine_rigid_supers`
  (281), `atom_matches_super` (288), `is_number` (304),
  `unify_flex_super_structure` (308), `comparable_occurs_check` (364),
  `unify_comparable_recursive` (372). `unify_rigid` loses its
  `maybe_super` parameter and its `FlexSuper` arm; `unify_flex` (170) and
  `unify_structure` (476) lose their `FlexSuper` arms; `actually_unify`
  (143) loses two arms; `unify_alias` (412) loses the `FlexSuper` pattern.
- `solve.rs:535-537` (`src_type_to_variable`): always `FlexVar(Some(name))`.
  `solve.rs:720-731` (`make_copy_help`): `RigidVar` copies to
  `FlexVar(Some(name))`, no super case. `solve.rs:875`: drop the arm.
- `annotation.rs`: delete the `FlexSuper`/`RigidSuper` arms (53-69,
  238-256, 506-525), `super_to_super` (280), `fresh_super_name` (423) and
  the `numbers`/`comparables`/`appendables`/`comp_appends` counters in
  `NameState`.
- `expression.rs:60-72` (`Int` literal): `Constraint::Equal(region, Category::Int, type_::int(), expected)`.
  `expression.rs:76-92` (`Negate`): the sub-expression and result are
  `type_::int()`. `Category::Number` is renamed `Category::Int`. plans/03
  replaces both with `FromInt`/`Num` predicates.
- `pattern.rs:164-174`: `type_::int()` (unchanged in shape).

**Code** (`unify.rs`, after the change):

```rust
fn unify_rigid<'a>(uf: &mut UnionFind<'a>, context: &Context<'a>) -> UResult {
    let content = context.first_desc.content.clone();
    match &context.second_desc.content {
        Content::FlexVar(_) => merge(uf, context, content),
        Content::RigidVar(_) | Content::Alias { .. } | Content::Structure(_) => Err(()),
        Content::Error => merge(uf, context, Content::Error),
    }
}
```

**Elm reference**: `Type/Unify.hs` `unifyFlexSuper` (283),
`combineRigidSupers` (338), `atomMatchesSuper` (345),
`unifyFlexSuperStructure` (370), `unifyRigid` (246); `Type/Type.hs`
`mkFlexNumber`, `nameToFlex`, `toSuper` (via `Name.isNumberType`);
`Type/Error.hs` `Super`.

**Tests** (`inference.rs`): `int_literal` now `main : int`;
`list_of_numbers` gives `list int` after chunk C1 (until then `List int`);
`number_cannot_be_string` re-accepted; new `negate_is_int`;
`no_number_variable_names`: `f : number -> number` is an ordinary rigid
variable named `number` (snapshot shows no special treatment).

**Done when**: `grep -r "Super" crates/nash-constrain crates/nash-solve`
is empty and all snapshots are re-accepted.

---

## Chunk C1: The builtin type inventory

**Files**: `crates/nash-constrain/src/type_.rs`,
`crates/nash-constrain/src/error_type.rs`, `crates/nash-constrain/src/expression.rs`,
`crates/nash-constrain/src/pattern.rs`, `crates/nash-ast/src/primitives.rs`,
`crates/nash-can/src/environment/foreign.rs`, `crates/nash-can/src/environment/local.rs`,
`crates/nash-can/src/module.rs`, `crates/nash-driver/src/compile.rs`,
`crates/nash-solve/tests/inference.rs`.

**Change**:

- `type_.rs:175-240`: replace `basics()`, `list_home()`, `string_home()`
  with `nash_ast::primitives::builtin_home()` (the table from plans/02
  chunk 3). Then:

```rust
// PRIMITIVE TYPES (docs/representation.md)

const fn prim<'a>(name: &'a str) -> Type<'a> {
    Type::AppN { home: builtin_home(), name, args: &[] }
}

pub const fn int<'a>() -> Type<'a> { prim("int") }
pub const fn bytes<'a>() -> Type<'a> { prim("bytes") }
pub const fn string<'a>() -> Type<'a> { prim("string") }
pub const fn bool<'a>() -> Type<'a> { prim("bool") }
pub const fn unit<'a>() -> Type<'a> { prim("unit") }
pub const fn data<'a>() -> Type<'a> { prim("Data") }
pub const fn big_int<'a>() -> Type<'a> { prim("Int") }
pub const fn big_bytes<'a>() -> Type<'a> { prim("Bytes") }

pub fn list<'a>(bump: &'a Bump, elem: &'a Type<'a>) -> Type<'a> {
    Type::AppN { home: builtin_home(), name: "list", args: bump.alloc_slice_copy(&[elem]) }
}

pub fn big_list<'a>(bump: &'a Bump, elem: &'a Type<'a>) -> Type<'a> {
    Type::AppN { home: builtin_home(), name: "List", args: bump.alloc_slice_copy(&[elem]) }
}
```

- `error_type.rs:71-89`: `is_int`, `is_string`, `is_list` compare against
  `builtin_home()` and the lowercase names; `is_big_int`, `is_data` added
  for future hints.
- `expression.rs` list literal (`constrain_list`) and `pattern.rs` list
  patterns build `type_::list`; `if` uses `type_::bool()`; `Unit`
  expressions and patterns use `type_::unit()` instead of `Type::UnitN`.
  `()` in a type is `unit` (plans/01), so `CanType::Unit`, `Type::UnitN`
  and `FlatType::Unit1` are deleted and `nash-can` canonicalizes the
  source unit type to `Type::Named` on `Builtin.unit`.
- `nash-can`: the implicit prelude. `foreign::create_initial_env`
  (`foreign.rs`) adds every `PRIMITIVES` entry to `env.types` unqualified
  and under the `Builtin` prefix, homed at `builtin_home()`, before user
  imports; user declarations still shadow them (`insert_local_type`).
  `local.rs:78` keys `Ctor::Bool` on `builtin_home()` + `bool` and
  `foreign.rs:43` on `builtin_home()` + `list`; the `Basics`/`List`
  special cases go away. `bool`'s `True`/`False` and `unit`'s `()` are
  seeded as constructors of the primitive `bool`/`unit` unions
  (`InterfaceUnion { ctors: [False, True] }`), so `Pattern::Bool` and the
  `Ctor::Bool` path keep working unchanged.
- `compile.rs`: nothing; the prelude comes from `create_initial_env`.
- `inference.rs`: tests stop declaring `type Int = Int`; snapshots show
  `int`, `string`, `list int`, `bool`.

**Elm reference**: `Type/Type.hs` primitive section (`int`, `float`,
`string`, `char`, `bool`, `never`); `Canonicalize/Environment/Foreign.hs`
`createInitialEnv` (Elm's default imports come from `Elm/Compiler/Imports.hs`).

**Tests** (`inference.rs`):

- `int_literal` (`main : int`), `string_literal` (`string`),
  `list_of_numbers` (`list int`), `if_condition_must_be_bool` (error
  mentions `Builtin.bool`), `unit_value` (`unit`).
- `big_types_in_scope`: `f : Int -> Data`, `f = Builtin.toData` is not
  available yet; use `f : List Int -> List Int`, `f x = x`.
- `user_type_shadows_builtin`: `type int = Mine`, `x = Mine` gives
  `x : int` homed at `Main`.
- `module.rs`: `prelude_types_resolve_unqualified` and
  `prelude_types_resolve_qualified` (`Builtin.list`).

**Done when**: no reference to `Basics`, `String`, `Char` or `List` homes
remains outside test fixtures.

---

## Chunk D1: Record encoding facts in the canonical AST

**Files**: `crates/nash-ast/src/lib.rs`, `crates/nash-can/src/expression.rs`,
`crates/nash-can/src/pattern.rs`.

**Change**: codegen needs, for every record operation, the alias and the
field's wire position. After A3 every `Expr::Record` names its alias and
lists fields in declaration order. For access, update and patterns the
alias is only known after solving, so codegen looks it up from the solved
type; what the canonical AST must guarantee is that positions are
recoverable from the alias alone:

- `nash_ast::Alias` gets helpers:

```rust
impl<'a> Alias<'a> {
    /// The record fields in declaration (wire) order, if this alias is a record.
    pub fn record_fields(&self) -> Option<Vec<&'a FieldType<'a>>> {
        match &self.typ.value {
            Type::Record { fields } => {
                let mut ordered: Vec<&FieldType<'a>> = fields.iter().collect();
                ordered.sort_by_key(|f| f.index);
                Some(ordered)
            }
            _ => None,
        }
    }
}
```

Representation lookup uses the existing representation metadata and alias
body rules in `nash-can::kinds`, not the alias's `Kind`. Record aliases use
casing (`Big` or `Term`); transparent aliases substitute their bodies before
lookup. Do not add a second representation classifier to `Alias`.

- `nash_ast::Union` gets the matching helper for labeled single
  constructors, returning the fields in wire order:

```rust
impl<'a> Union<'a> {
    pub fn labeled_fields(&self) -> Option<Vec<&'a FieldType<'a>>> {
        match self.ctors {
            [ctor] => match ctor.arguments {
                CtorArgs::Labeled(fields) => {
                    let mut ordered: Vec<&FieldType<'a>> = fields.iter().collect();
                    ordered.sort_by_key(|f| f.index);
                    Some(ordered)
                }
                CtorArgs::Positional(_) => None,
            },
            _ => None,
        }
    }
}
```

- `expression.rs` record canonicalization (A3) documents that `fields`
  are index-ordered, and `Expr::Update.fields` and `Pattern::Record`
  stay name-ordered (positions come from the alias or union at codegen).
- `FieldType.index` is documented in `nash-ast` as "position in the
  declaration, and therefore in the `Data.List` / `constr 0` encoding".
- `interface.rs` needs no change: `InterfaceAlias.typ` carries the fields
  with indices.

**Tests**: `alias_record_fields_in_wire_order` (nash-ast unit test with a
hand-built alias `{ z, a }` giving `[z, a]`), `record_literal_fields_in_wire_order`
(module snapshot for `{ y = 2, x = 1 }` against `point` shows `x` first).

**Done when**: codegen can compute every record position from
`Alias::record_fields` and `Expr::Record.fields` order alone.

---

## Chunk E1: Changeset and SPEC

```markdown
---
cargo/nash-ast: minor
cargo/nash-can: minor
cargo/nash-constrain: minor
cargo/nash-solve: minor
cargo/nash-driver: patch
---

Nominal records (row polymorphism removed), record literals resolved by
field set, deferred field constraints, removal of Float/Char/supertypes,
and the Const/Big builtin type inventory homed in nash/core Builtin.
```

Grammar lives only in docs/syntax.md (plans/01 owns it). SPEC.md: tick the
record and builtin-inventory boxes under Type Inference and point to
docs/representation.md for the rules.

**Done when**: `cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`,
`cargo insta test --unreferenced delete` pass.

---

## Open questions

- **Accessor functions** (`.x`) are only usable where the record type is
  fixed before generalization. If that is too restrictive in practice, the
  alternative is a `HasField` predicate carried on schemes (plans/03), which
  this design can grow into: `Constraint::Field` already has the shape of a
  predicate.
- **Record literal resolution by field set** means two aliases with the
  same fields in one module make every literal of that shape ambiguous.
  The escape hatch is the constructor function (`point 1 2`), which Elm
  already provides for record aliases. An explicit `point { x = 1, y = 2 }`
  form would need syntax.
