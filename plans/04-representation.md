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

### Integration audit required for completion

Audit the complete Haskell 98 engine replacement against `docs/kinds.md` as
part of this work. Assess each affected API, state field, helper and control
path as if implementing the approved design from scratch. Remove obsolete
engine code, abstractions, metadata, compatibility hooks and callers. Start
with disabled overlap-compatibility callbacks, unused kind-environment/module
arguments and stale regression descriptions, but audit beyond those examples.
Do not retain old interfaces through ignored arguments, unconditional
callbacks, wrappers or adapters.

Cover declaration checking, type inference, trait resolution, specialization,
generalization, evidence, interfaces, diagnostics and tests. Preserve Haskell
98 semantics, representation predicates, terminating datatype-context
inference and Plan 03 behavior. Keep the existing trait-resolution policy and
search limits; do not add Paterson-style restrictions. Preserve later-plan
deferrals and unrelated work.

Replace obsolete tests and descriptions with explicit assertions of the new
semantics, preserving local and imported positive and negative regression
coverage. Mark historical documents clearly. Complete formatting, strict
Clippy, the full test suite, reviewed snapshots and snapshot hygiene, the
real core CLI fixture and focused cross-module acceptance. The final audit
must explain what was removed, what remains and why the approved design needs
it, and any unresolved gaps. Passing tests or finding no old symbol names is
not sufficient evidence. Do not push or publish.

### Integration audit outcome

The review covered declaration kind inference and context closure, annotation
checking, impl head canonicalization, overlap and superclass entailment, method
specialization, constraint generation, value inference and generalization,
predicate resolution and evidence, canonical interfaces, cache fingerprints,
diagnostics and regression descriptions. The review traced producer/consumer
paths and retained-state purposes, with separate read-only reviews of
declarations, inference and interfaces.

Removed in this work:

- The duplicate kind-environment argument to superclass entailment and the
  resolver's duplicate environment field. Entailment reads `Tables.kinds`.
- The impl-variable region map whose locations had no consumer. The ordered
  unique variable list now serves head indexing, context membership and
  collision-safe method substitution. Its recursive pattern helper is private.
- Public exposure of module-internal context-closure worklist inputs, failures
  and references. Declaration-kind results have crate visibility.
- Structural record rows, row merging and extension traversal; standalone
  canonical/inference/error/head variants for unit; their obsolete conversion,
  unification, ranking and diagnostic arms.
- Tests and descriptions that described representation failures as kind
  narrowing, plus obsolete record-polymorphism expectations. Historical kind
  plans remain marked as superseded.

The previously removed overlap compatibility callback remains absent. Overlap
uses structural heads without representation-context compatibility. Its work
budget and the existing trait-resolution policy and limits are unchanged.
No adapter, ignored argument or unconditional compatibility callback replaces it.

Retained mechanisms and their required roles:

| Mechanism | Required role under the approved design |
|---|---|
| H98 kind unification, occurs check and defaulting | Infer constructor application arities and freeze generalized kinds; representations never enter kind unification. |
| `TypeInfo` / `KindEnv` | Supply closed kinds, named parameter substitution, datatype contexts, nominal representations and transparent alias bodies across local and imported references. |
| `Formation` and the SCC worklist | Generate and reduce datatype obligations, defer references in the active declaration group, and close recursive contexts with applied-relevance checks rather than an arbitrary round limit. |
| Representation predicates and internal `Apply` | Express storage obligations independently of kinds and retain formation requirements while an application head is unknown. |
| `representation_subject` and `repr_of` | Normalize an unresolved predicate subject and query a known head's representation, respectively; neither substitutes for formation checking. |
| Solver kind contracts and copied contracts | Preserve H98 defaulting and declared/generalized kinds across instantiation, including discarded expression roots and predicate-only variables. |
| Method specialization and annotation rechecking | Remove the exact owner predicate, avoid variable capture, regenerate formation requirements after substitution and check the specialized body. |
| Predicate store, superclass evidence and solved instance metadata | Preserve trait resolution, representation evidence, captured obligations and downstream specialization. `Evidence::Super` is real superclass evidence, not an Elm magic supertype. |
| Deferred fields and rank boundaries | Wait for nominal receiver identity, share captured parameters, and resolve or reject fields before independent generalization. |
| Full canonical interface metadata and visibility filtering | Retain H98 contexts and twin identities while exposing field projection only through visible constructors. Fingerprints include semantic type/context metadata. |

Focused acceptance covers local and imported positive/negative higher-kinded
applications, nominal identity, labeled projection, captured parameters,
constructor privacy and alias-only updates. Ordered `Apply` fingerprint coverage
changes only argument order. Unit syntax and qualified builtin unit share one
canonical identity. Existing trait and recursive-context suites remain required
alongside these integration cases.

No unresolved implementation defect was found in the reviewed scope. Later
codegen/monomorphization plans still own physical encoding and erasure of evidence;
this plan supplies their canonical metadata. The driver cache stores summaries
and fingerprints, not serialized canonical ASTs; canonical-interface propagation
is tested through in-memory imports. This review does not claim a persistent AST
serialization round trip or completed later-plan behavior.

## Prerequisites

### Current implementation reconciliation (2026-09-08)

The Haskell 98 kind and trait implementations have landed. The chunk descriptions
below record the reconciled implementation rather than the original Elm-shaped
sketches. In particular:

- Preserve trait-based literals, negation, real trait tables and solver mode.
  B1's magic supertypes are already removed; legitimate superclass evidence
  named `Super` remains. Verify the obsolete variants specifically.
- C1 uses the primitive inventory for unqualified and qualified type lookup.
  Unit is the named `Builtin.unit` throughout canonicalization and inference;
  source expressions and patterns keep their syntax variants.
- A1 must preserve representation annotations and reject nested anonymous
  record types as well as anonymous records in value annotations.
- A2 must use the retained alias body to distinguish a nominal record alias
  from a transparent alias of that record. Preserve partial aliases and
  higher-kinded application unification.
- A4 must retry fields to a fixed point before generalization. Reject
  unresolved constraints owned by definitions before their variables can be
  copied; existential constraint wrappers are not definition boundaries.
  Preserve constraints on captured outer variables until their owning scope.
- A5 must retain the current trait/evidence integration at all solver callers.
  D1 uses existing representation predicates and metadata, never a second
  representation classifier.

A5 uses the existing positional constructor representation with label metadata:
`Ctor.arguments` remains the shared positional type slice, with optional labels
in the same declaration order. `Ctor::labeled_fields` and
`Union::labeled_fields` derive indexed metadata without storing the types twice.
`Tables.fields` holds single-constructor unions from the actual visible
constructor environment; it does not use every interface in the build. This
preserves the current solver API and keeps real trait tables intact. This replaces the originally proposed `CtorArgs` enum and extra solver
parameters.
Parenthesized record literals retain a source `grouped` flag, so they remain
positional arguments even to labeled constructors. This flag is erased during
canonicalization. Twin constructors must also have equal ordered labels.

Progress:

- [x] A1: direct record alias bodies only; canonical extensions removed.
- [x] A2: closed record inference and nominal identity through transparent aliases.
- [x] A3: field-set literal resolution, lowercase alias constructors and wire order.
- [x] A4: deferred fields and empty-record shape checks, with scoped generalization.
- [x] A5: labeled constructors, sugar and visible field metadata.
- [x] B1: obsolete magic supertypes absent; existing trait literals preserved.
- [x] C1: qualified builtin availability and named unit representation.
- [x] D1: record alias and labeled-union wire-order metadata.
- [x] E1: changeset, progress updates and final validation.

A1–A4 validation: workspace snapshot suite passes with reviewed snapshots and
stale snapshots removed. Cross-module checks cover qualified lowercase names,
record identity and duplicate exposure. Regressions cover empty record patterns,
fixed-point field resolution, captured field rank safety and retained trait
evidence. The nominal-record changeset covers the public API and downstream
publication chain.

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

`nash-source`, `nash-ast`, `nash-parse`, `nash-can`, `nash-constrain`,
`nash-solve`, `nash-driver` and `nash-cli` through the publication chain.

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
- Nash: `nash-can` type/environment/expression canonicalization,
  `nash-constrain` expression/pattern constraints, and `nash-solve`
  unification, field resolution, generalization and annotation conversion.

## Decisions

**Record literal rule**: `{ x = e1, y = e2 }` is resolved at
canonicalization by its *field-name set*. Exactly one record alias in scope
(unqualified or qualified) must have exactly that set; zero is
`RecordLiteralNoAlias`, more than one is `RecordLiteralAmbiguous`. The
literal canonicalizes to `Expr::Record { alias, annotation, fields }` where
`annotation` is the alias's record-constructor type (already built by
`make_record_ctor`) and `fields` are in declaration
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

## Chunk A1: Record alias bodies and closed ASTs

Implemented across source canonicalization, canonical types, constraints and
inference. Anonymous record types are accepted only as direct alias bodies,
including an outer representation annotation. Nested anonymous record types,
record types in value annotations and record bodies hidden inside transparent
alias expressions fail with `RecordTypeOutsideAlias`.

Canonical records and their inference/error forms have no row extension.
Free-variable collection, substitution, kind checking, predicate formation,
interface propagation, copying and diagnostics traverse closed fields only.
Record expression and pattern syntax remains available under nominal rules.
Granular snapshots cover direct aliases, nested rejection and annotation
rejection while representation predicates remain active.

## Chunk A2: Nominal record unification

A direct record alias is nominal. Two different record alias identities cannot
unify even when their field sets and types are identical. Same-identity aliases
unify their parameters. Transparent aliases unwrap without losing the identity
of a nested nominal record, including through parameterized wrappers.

Closed internal record structures compare exact field-name sets and unify
shared field types. Row gathering, merging, extension variables and empty-row
variants are removed. Partial aliases and higher-kinded application unification
remain part of the H98 type language. Regressions cover equal and distinct alias
identities, transparent wrappers, parameter preservation and trait arguments.

## Chunk A3: Nominal record literal resolution

Canonicalization resolves a bare record literal by its exact field-name set
against visible record aliases. Zero candidates produce `RecordLiteralNoAlias`;
multiple distinct identities produce `RecordLiteralAmbiguous`. Multiple import
paths to the same identity count once. Private or non-imported aliases
cannot contribute candidates.

A canonical literal records the selected alias identity, its constructor
annotation and field expressions in declaration order. Record storage may use
name order for lookup, but field indices preserve wire order. Alias constructor
functions offer explicit disambiguation. Qualified lowercase type names and
alias constructors work through the existing environment lookup paths.

Snapshots and imported tests cover ambiguous and missing candidates, duplicate
exposure, nominal identity, explicit constructors, parameterized records and
field order with names deliberately opposed to their declaration order.

## Chunk A4: Deferred record operations

Access, accessor functions, update and bare record patterns emit deferred field
constraints. Empty patterns/updates still emit a receiver-shape requirement.
The solver retries to a fixed point as receiver types become known and before
predicate resolution and definition generalization. A receiver must resolve to
an eligible nominal type; the constraint never infers an anonymous row.

Known aliases resolve their fields with their actual type arguments. Missing
fields, mismatched field values and non-record receivers have distinct errors.
Updates retain the receiver's nominal identity and field types. Labeled union
projection extends this same mechanism in A5, while updates stay alias-only.

Unresolved constraints owned by a definition fail before its scheme can be
copied. A constraint on a captured receiver stays with the outer scope, and its
result cannot generalize independently. Existential wrappers do not create
false definition boundaries. Tests cover deferred resolution order, fixed-point
chains, ambiguous accessors, captured-rank safety, retained trait evidence,
empty patterns, update mismatch and imported identity/visibility.

---

## Chunk A5: Labeled constructor fields

Implemented using `Ctor.labels: Option<&[&str]>` parallel to the existing
positional `Ctor.arguments` slice. Labels and argument types retain declaration
order. Canonicalization rejects duplicate labels before building a constructor.
This avoids a second storage path for constructor argument types and preserves
the existing inference, trait and evidence consumers.

`Ctor::labeled_fields` produces indexed `FieldType` entries from those parallel
slices. `Union::labeled_fields` exposes them only for a single labeled
constructor. `Tables.fields` maps exact qualified union identities to parameter
names and field metadata. Canonicalization fills that table from the module's
visible local and qualified constructor environments. A closed export or private
constructor does not contribute a field table, even when its type occurs in an
exported value's annotation.

For a labeled constructor applied to one unparenthesized record literal,
canonicalization requires exactly the constructor's labels and emits an ordinary
positional call in declaration order. Missing and extra labels have dedicated
errors. A positional constructor still accepts an ordinary alias record as its
argument. Parenthesized record literals retain a source `grouped` flag and
bypass labeled construction; canonicalization erases that flag.

A constructor pattern containing one record pattern selects a subset of its
labels. Canonicalization expands it into positional patterns, filling omitted
fields with wildcards and rejecting unknown or duplicate fields. Positional
constructors keep ordinary nested record-pattern behavior. Twin constructors
must have equal label order as well as equal constructor names and arities.

The existing deferred field constraint resolves a labeled single-constructor
union once its nominal type is known. It substitutes the union's actual type
arguments into the selected field type at the active solver rank, then unifies
that type with the projection result. Transparent aliases are followed. Missing
fields fail; multi-constructor and hidden unions do not support projection.
Record updates remain alias-only. No new solver entry point or alternate trait
table is needed.

Coverage includes construction and subset patterns in wire order, missing and
extra labels, duplicate declarations/patterns, positional record arguments,
parenthesized arguments, polymorphic and higher-kinded projection, captured
parameters, empty record patterns, twin label order, imported construction and
projection, and hidden constructor labels.

---

## Chunk B1: Remove Elm primitive and supertype machinery

The completed syntax and trait work had already removed `Float`, `Char`,
`FlexSuper`, `RigidSuper`, `SuperType`, magic number/comparable variable names
and their special unification/diagnostic machinery. This plan verified those
specific obsolete variants and preserves ordinary flexible/rigid variables.

Literal typing, negation, trait defaulting and real trait tables retain Plan 03
semantics. `Evidence::Super` remains necessary superclass evidence. It must not
be confused with Elm's former magic supertypes. The old monomorphic-literal
sketch and a blanket search for the word `Super` are not acceptance criteria.
Existing literal, negation, representation and superclass regressions pass.

---

## Chunk C1: The builtin type inventory

**Files**: `crates/nash-constrain/src/type_.rs`,
`crates/nash-constrain/src/error_type.rs`, `crates/nash-constrain/src/expression.rs`,
`crates/nash-constrain/src/pattern.rs`, `crates/nash-ast/src/primitives.rs`,
`crates/nash-can/src/environment/foreign.rs`, `crates/nash-can/src/environment/local.rs`,
`crates/nash-can/src/module.rs`, `crates/nash-driver/src/compile.rs`,
`crates/nash-solve/tests/inference.rs`.

**Implemented change**:

- All `PRIMITIVES` entries are available unqualified and under `Builtin`
  before user imports. Local declarations shadow only the unqualified name.
  Value imports and core-only intrinsic visibility retain their existing rules.
- `()` in a type canonicalizes to `Type::Named` on `Builtin.unit`.
  Canonical, constraint, union-find, impl-head and diagnostic unit variants
  are removed. Expression and pattern syntax nodes remain and generate
  `type_::unit()` constraints. Core ownership of unit instances is preserved
  by exact builtin identity, as required by `docs/traits.md`.
- List expressions and patterns share `type_::list`; the obsolete
  `list_home` wrapper is removed. Bool retains its existing builtin identity.
  The primitive interface already supplies `True` and `False` constructors;
  `()` is syntax and needs no identifier in the constructor environment.
- Keep trait-based literal typing and its defaulting rules. Do not add unused
  primitive helpers or diagnostic predicates for hypothetical future hints.
  The earlier concrete `int()`/`string()` literal sketch is superseded by the
  completed trait implementation.

**Elm reference**: `Type/Type.hs` primitive section (`int`, `float`,
`string`, `char`, `bool`, `never`); `Canonicalize/Environment/Foreign.hs`
`createInitialEnv` (Elm's default imports come from `Elm/Compiler/Imports.hs`).

**Acceptance**: builtin-qualified lookup without an import, named-unit AST
identity, unit instance syntax, big builtin types, unqualified user shadowing,
literal typing and the complete core fixture pass. Primitive type identities
use `nash/core.Builtin`; real standard-library module identities and imported
fixtures are preserved.

---

## Chunk D1: Record encoding facts in the canonical AST

Implemented in `nash-ast` and canonical record construction:

- `Alias::record_fields` returns direct record fields sorted by their declaration
  index. Transparent aliases keep their existing body-based lookup rules.
- `Ctor::labeled_fields` derives fields in declaration order;
  `Union::labeled_fields` provides them only for a single labeled constructor.
- `Expr::Record` identifies its nominal alias and stores expressions in wire
  order. Constructor label sugar becomes a positional call or pattern in the
  same order. Access, update and bare record patterns retain field names;
  later codegen obtains positions from the solved nominal type's metadata.
- Interface aliases retain indexed field types, and interface unions retain
  constructor labels and positional argument types.

Representation remains the existing predicate/head-based classification in
`nash-can::kinds`. No second classifier or representation kind is introduced.
AST helper tests use deliberately reversed field names to verify wire order,
and reject positional or multi-constructor unions as projection metadata.

---

## Chunk E1: Changeset, progress and validation

The [nominal-records changeset](../.sampo/changesets/nominal-records.md) includes
minor changes for source/AST/parser/canonicalization/constraints/solver and
patch releases for driver/CLI in dependency publication order. `SPEC.md` marks
Plan 04 complete. The representation and syntax docs describe the final rules;
stale kind-error descriptions in later plans are corrected without implementing
those deferred plans.

Final validation of the completed `plan-4` implementation:

- `cargo fmt --all` and the final format check pass.
- `cargo clippy --all-targets --all-features -- -D warnings` passes.
- `cargo insta test --workspace --check --unreferenced delete` passes:
  2,088 tests passed, three ignored; no pending or unreferenced snapshots.
- `cargo run -p nash-cli -- check tests/core` passes: 23 modules,
  215 declarations. `target/debug/nash check core` also passes: 22 modules,
  53 declarations.
- Focused imported labeled-constructor acceptance passes all six tests,
  including positive and negative higher-kinded/captured-parameter cases.
- Changeset ordering is checked against the crates' dependency manifests.

The completed work is split into four commits: nominal records, builtin type
identity, labeled constructors, and final audit cleanup/documentation. The
intermediate trees pass formatting, strict Clippy, workspace tests and the core
fixture. No push or publication is part of this work.

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
  alternative is a future `HasField` predicate carried on schemes, which
  this design can grow into: `Constraint::Field` already has the shape of a
  predicate.
- **Record literal resolution by field set** means two aliases with the
  same fields in one module make every literal of that shape ambiguous.
  The escape hatch is the constructor function (`point 1 2`), which Elm
  already provides for record aliases. An explicit `point { x = 1, y = 2 }`
  form would need syntax.
