# Plan 12: `nash/core` standard library

Goal: write `core/` in Nash per [docs/stdlib.md](../docs/stdlib.md) and
wire it into the compiler: embedded package, default imports, the
synthetic `Builtin` module, the trait modules and twin types, the type
modules, decoders, `Fuzz`, `Test`, `Ast`, `Cardano.*`.

Prerequisites, by chunk (each chunk's Nash must type-check with the
compiler features available when it lands):

| Chunk | Needs |
|---|---|
| 1 skeleton, 2 default imports, 3 `Builtin` | nothing beyond today's pipeline |
| 4 kinds-aware twin types | plans/02 (kinds) |
| 5 trait modules, operators, `Lift`, `ToData` | plans/03 (traits; chunk 12 there is this chunk's file list) |
| 6 type modules, 7 `Data`/`Map` modules | plans/03; `Data` patterns from data.md |
| 8 `Fuzz`, 9 `Test` | plans/03, plans/10 (tests block, sequencing `do`, `Prng` protocol, runner) |
| 10 `Ast`, `Derive` | plans/11 chunk 4 (tags) and chunk 10 |
| 11 `Cardano.*` | chunk 7 |

Module layout is docs/stdlib.md "Layout": one file per trait, one module
per type pair named by the uppercase name, functions on the little twin
only. `Prelude` is the `infix` table, its helper functions, and the tuple
impls. There is no Big `String` and no `Data.List`-style module family.

Crates touched: new `nash-core`, `nash-can`, `nash-driver`, `nash-cli`,
`nash-codegen` (builtin lowering), `core/`.

References:

- Elm: `elm/compiler/src/Elm/Compiler/Imports.hs` (`defaults`),
  `Canonicalize/Environment/Foreign.hs` (`createInitialEnv`),
  `Elm/Kernel.hs` (how Elm binds native code; we bind builtins by table
  instead).
- Aiken: `crates/aiken-lang/src/builtins.rs` (`from_default_function`,
  `prelude`, the builtin type table), `crates/aiken-project/src/lib.rs`
  (stdlib is a normal dependency; we embed instead), Aiken stdlib
  `aiken-lang/stdlib` for API shape (`list`, `option`, `cbor`, `fuzz`).
- Current code: `crates/nash-can/src/environment/foreign.rs:18`
  (`create_initial_env`, the `List` pre-seed at :34),
  `crates/nash-driver/src/project.rs:70` (`discover_modules`),
  `crates/nash-driver/src/source.rs:224` (`OverlaySource`),
  `crates/nash-driver/src/compile.rs:86` (`build_sync` interface map),
  `crates/nash-plutus/src/builtin/default_function.rs`,
  `SPEC.md` "Prelude / default imports (deferred)".

Conventions: type variables `'a`; lowercase bare type names are little,
uppercase Big.

---

## Chunk 1: package skeleton and embedding

**Files**

- `core/nash.jsonc` (new)
- `core/src/Prelude.nash` (new, minimal)
- `crates/nash-core/Cargo.toml`, `build.rs`, `src/lib.rs` (new)
- `crates/nash-driver/Cargo.toml`, `src/project.rs`, `src/source.rs`
- `Cargo.toml` (workspace member is picked up by `crates/*`)

**Change**

Embed `core/src/**/*.nash` into the compiler at build time and make the
driver see those modules as part of every build under the `nash/core`
package. In-repo path dependency was considered and rejected: `nash check`
must work with no config and no network, and one compiler must map to one
core.

**Code**

`core/nash.jsonc`: as in docs/stdlib.md.

`core/src/Prelude.nash` (chunk 1 version, grows later):

```elm
module Prelude exposing (..)

identity : 'a -> 'a
identity x = x

always : 'a -> 'b -> 'a
always x _ = x
```

`crates/nash-core/build.rs`:

```rust
//! Embeds `core/src/**/*.nash` as `(module_name, source)` pairs.
use std::{env, fs, path::{Path, PathBuf}};

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../core/src");
    let mut files = Vec::new();
    collect(&root, &root, &mut files);
    files.sort();

    let mut out = String::from("pub static SOURCES: &[(&str, &str)] = &[\n");
    for (module, path) in &files {
        println!("cargo:rerun-if-changed={}", path.display());
        out.push_str(&format!("    ({module:?}, include_str!({:?})),\n", path.display()));
    }
    out.push_str("];\n");
    fs::write(PathBuf::from(env::var("OUT_DIR").unwrap()).join("sources.rs"), out).unwrap();
    println!("cargo:rerun-if-changed={}", root.display());
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(root, &path, out);
        } else if path.extension().is_some_and(|e| e == "nash") {
            let rel = path.strip_prefix(root).unwrap().with_extension("");
            let module = rel.components().map(|c| c.as_os_str().to_str().unwrap()).collect::<Vec<_>>().join(".");
            out.push((module, path));
        }
    }
}
```

`crates/nash-core/src/lib.rs`:

```rust
//! The `nash/core` package, embedded at build time.
include!(concat!(env!("OUT_DIR"), "/sources.rs"));

pub const PACKAGE: &str = "nash/core";

/// `core://Prelude.nash`-style URL for a core module.
pub fn url(module: &str) -> url::Url {
    url::Url::parse(&format!("core:///{}.nash", module.replace('.', "/"))).expect("static module name")
}

pub fn source(module: &str) -> Option<&'static str> {
    SOURCES.iter().find(|(m, _)| *m == module).map(|(_, s)| *s)
}
```

Driver (`crates/nash-driver/src/project.rs`):

```rust
impl Project {
    /// Core modules first, then the project's own modules.
    pub async fn discover_modules(&self, db: &Database) -> Result<Vec<Url>, DriverError> {
        let mut modules: Vec<Url> = nash_core::SOURCES.iter().map(|(m, _)| nash_core::url(m)).collect();
        modules.extend(self.discover_project_modules(db).await?);
        Ok(modules)
    }
}
```

`crates/nash-driver/src/source.rs`: a `CoreSource` implementing
`FileSource` (`source.rs:22`) that serves `core:///` URLs from
`nash_core::SOURCES` and errors on `write`. `Database::new` wraps the
caller's source in `OverlaySource::new(CoreSource, user_source)`
(`source.rs:224`). `compile_module` sets `Context.package` to
`nash/core` for `core:///` URLs (`crates/nash-driver/src/compile.rs:176`).

`build_graph`'s `resolve_import` (`compile.rs:263`) already matches on
path suffix; `core:///Data/Decode.nash` matches `Data.Decode`. A user module
named like a core module is an error (`ModuleShadowsCore`), added to
`DriverError`.

**Elm/Aiken reference**

Elm: `elm/compiler/src/Elm/Details.hs` `verifyPkg` loads packages from
`ELM_HOME`; we replace that with the embedded table. Aiken:
`crates/aiken-project/src/lib.rs` `read_source_files` / `parse_sources`.

**Tests**

- `nash-core`: `sources_contain_prelude` and `url_roundtrip`.
- `nash-driver` `test_core_modules_are_always_present`: an in-memory project with one module `import Prelude` builds; `test_user_module_cannot_shadow_core`.

**Done when** `nash check` on `scratch/` builds `Prelude` plus the user
modules and reports `total == core count + user count`.

---

## Chunk 2: default imports

**Files**

- `crates/nash-can/src/environment/defaults.rs` (new)
- `crates/nash-can/src/environment/foreign.rs`
- `crates/nash-can/src/module.rs`
- `SPEC.md`

**Change**

Port `Elm.Compiler.Imports.defaults`. `Context.package` decides:
`nash/core` modules get no defaults, everyone else gets the list in
docs/stdlib.md "Default imports". In this chunk only modules that exist so
far are defaulted (`Prelude`); the list grows with later chunks, each one
adding its module and a test. The exposing forms needed are Elm's
`Exposed::Lower` (values), `Exposed::Upper { privacy: Private }` (a trait,
or a Big type without its constructors), and plans/01's `type` prefix for
little types (`Exposed::LittleType { open: true }` for `type option(..)`).

**Code**

```rust
// crates/nash-can/src/environment/defaults.rs
use nash_region::{Located, Region};
use nash_source::{Exposed, Exposing, Import, Privacy};

/// Mirrors Elm's `Imports.defaults`; the list is docs/stdlib.md "Default imports".
pub fn defaults<'a>(bump: &'a Bump) -> &'a [&'a Import<'a>] {
    let at = |n: &'a str| bump.alloc(Located::at(Region::zero(), n));
    let open = |name: &'a str| import(bump, name, Exposing::Open);
    let closed = |name: &'a str| import(bump, name, Exposing::Explicit(&[]));
    let explicit = |name: &'a str, items: &[Exposed<'a>]| import(bump, name, Exposing::Explicit(bump.alloc_slice_fill_iter(items.iter().map(|e| bump.alloc(*e) as &_))));
    let lower = |n| Exposed::Lower(at(n));
    // trait, or Big type without `(..)`: constructors stay qualified (`Option.Some`)
    let upper = |n| Exposed::Upper(at(n), Privacy::Private);
    // `type option(..)`: little constructors unqualified (`Some`)
    let little = |n| Exposed::LittleType(at(n), Privacy::Public);
    bump.alloc_slice_copy(&[
        open("Prelude"),
        explicit("Eq", &[upper("Eq")]),
        explicit("Ord", &[upper("Ord")]),
        explicit("Show", &[upper("Show")]),
        explicit("Num", &[upper("Num")]),
        explicit("Integral", &[upper("Integral")]),
        explicit("Semigroup", &[upper("Semigroup")]),
        explicit("Monoid", &[upper("Monoid")]),
        explicit("Functor", &[upper("Functor")]),
        explicit("Applicative", &[upper("Applicative")]),
        explicit("Monad", &[upper("Monad")]),
        explicit("Lift", &[upper("Lift")]),
        explicit("Data", &[upper("ToData"), upper("FromData")]),
        explicit("Literal", &[upper("FromInt"), upper("FromString"), upper("FromBytes")]),
        explicit("Bool", &[upper("Bool"), lower("not"), lower("and"), lower("or"), lower("xor")]),
        explicit("Unit", &[upper("Unit")]),
        explicit("Option", &[upper("Option"), little("option")]),
        explicit("Result", &[upper("Result"), little("result")]),
        explicit("Ordering", &[upper("Ordering"), little("ordering")]),
        explicit("Cons", &[little("cons")]),
        explicit("Derive", &[lower("derive")]),
        closed("Debug"),
        closed("Builtin"),
        closed("Int"),
        closed("Bytes"),
        closed("String"),
        closed("List"),
        closed("Pair"),
        closed("Array"),
        closed("Map"),
        closed("Fuzz"),
        closed("Test"),
    ])
}

fn import<'a>(bump: &'a Bump, name: &'a str, exposing: Exposing<'a>) -> &'a Import<'a> {
    bump.alloc(Import {
        import: bump.alloc(Located::at(Region::zero(), name)),
        alias: None,
        exposing: bump.alloc(exposing),
    })
}
```

`crates/nash-can/src/module.rs` `canonicalize` (line 48):

```rust
let imports: &[&SourceImport] = if context.package.is_some_and(|p| p.author == "nash" && p.project == "core") {
    module.imports
} else {
    bump.alloc_slice_fill_iter(defaults(bump).iter().chain(module.imports.iter()).copied())
};
let mut env = environment::foreign::create_initial_env(bump, home, context.interfaces, imports)?;
```

User imports come after defaults so an explicit `import List exposing (map)`
adds to what the defaults gave (Elm's `Map.union` semantics in
`merge_exposed`, `environment.rs:340`). An import with `Region::zero()`
never appears in an error message: `ImportNotFound` for a default import
is an internal error (core is embedded), so `find_interface`
(`foreign.rs:387`) panics with a message naming the module if the region
is zero.

`SPEC.md`: tick "Prelude / default imports".

Because the Big twins are exposed without `(..)`, `Some` resolves only to
the little constructor and `Option.Some` only to the Big one, through the
qualified namespace Elm's env already keeps for imported-but-unexposed
constructors. No expected-type resolution and no dual constructor entries
are needed.

**Elm/Aiken reference**

`Elm/Compiler/Imports.hs` `defaults`, `import_`, `typeOpen`, `typeClosed`,
`operator`; `Compile.hs` `compile` (`Imports.addDefaults` applied when the
package is not `elm/core`).

**Tests** (`crates/nash-can/src/snapshots`)

- `defaults_bring_identity_into_scope`: `main = identity 1` with a `Prelude` interface in `interfaces` canonicalizes to `VarForeign { home: Prelude }`.
- `core_modules_get_no_defaults`: the same source with `package = nash/core` fails `NotFoundVar identity`.
- `user_import_extends_defaults`: `import Prelude exposing (always)` plus use of `identity` still works.
- (after chunk 4) `twin_ctors_resolve_by_qualification`: `Some 1` is the little ctor, `Option.Some (lift 1)` the Big one, bare `Option.Some 1` without `lift` is a type error, and `case x of Bool.True -> ..` matches the Big ctor.

**Done when** `test_core_modules_are_always_present` (chunk 1) passes
without an explicit `import Prelude`.

---

## Chunk 3: the synthetic `Builtin` module

**Files**

- `crates/nash-ast/src/primitives.rs` (plans/02 chunk 3's module; add the `BUILTINS` table next to `PRIMITIVES`)
- `crates/nash-ast/Cargo.toml` (depend on `nash-plutus` for `DefaultFunction`)
- `crates/nash-driver/src/compile.rs`
- `crates/nash-codegen/src/builtin.rs` (plans/07, one match)

**Change**

`Builtin` has no `.nash` file. `nash_ast::primitives::PRIMITIVES` (plans/02
chunk 3) already holds its types; this chunk adds `BUILTINS`, a table that
maps every `DefaultFunction` to a Nash name and a type string, in the same
module. At driver start `primitives::interface(bump)` (plans/02) is
extended to parse and canonicalize each type string into the `Builtin`
`Interface`'s values, inserted into the build-wide interface map before
any module compiles. Codegen lowers `VarForeign { home: Builtin, name }`
to the builtin node.

**Code**

```rust
// crates/nash-ast/src/primitives.rs (continued from plans/02 chunk 3)
use nash_plutus::builtin::default_function::DefaultFunction as F;

pub struct Builtin {
    pub name: &'static str,
    pub function: Option<F>,
    /// Nash type, parsed by `nash-parse` at driver start.
    pub typ: &'static str,
}

pub static BUILTINS: &[Builtin] = &[
    Builtin { name: "addInteger", function: Some(F::AddInteger), typ: "int -> int -> int" },
    Builtin { name: "subtractInteger", function: Some(F::SubtractInteger), typ: "int -> int -> int" },
    Builtin { name: "multiplyInteger", function: Some(F::MultiplyInteger), typ: "int -> int -> int" },
    Builtin { name: "divideInteger", function: Some(F::DivideInteger), typ: "int -> int -> int" },
    Builtin { name: "quotientInteger", function: Some(F::QuotientInteger), typ: "int -> int -> int" },
    Builtin { name: "remainderInteger", function: Some(F::RemainderInteger), typ: "int -> int -> int" },
    Builtin { name: "modInteger", function: Some(F::ModInteger), typ: "int -> int -> int" },
    Builtin { name: "equalsInteger", function: Some(F::EqualsInteger), typ: "int -> int -> bool" },
    Builtin { name: "lessThanInteger", function: Some(F::LessThanInteger), typ: "int -> int -> bool" },
    Builtin { name: "lessThanEqualsInteger", function: Some(F::LessThanEqualsInteger), typ: "int -> int -> bool" },
    Builtin { name: "appendByteString", function: Some(F::AppendByteString), typ: "bytes -> bytes -> bytes" },
    Builtin { name: "consByteString", function: Some(F::ConsByteString), typ: "int -> bytes -> bytes" },
    Builtin { name: "sliceByteString", function: Some(F::SliceByteString), typ: "int -> int -> bytes -> bytes" },
    Builtin { name: "lengthOfByteString", function: Some(F::LengthOfByteString), typ: "bytes -> int" },
    Builtin { name: "indexByteString", function: Some(F::IndexByteString), typ: "bytes -> int -> int" },
    Builtin { name: "equalsByteString", function: Some(F::EqualsByteString), typ: "bytes -> bytes -> bool" },
    Builtin { name: "lessThanByteString", function: Some(F::LessThanByteString), typ: "bytes -> bytes -> bool" },
    Builtin { name: "lessThanEqualsByteString", function: Some(F::LessThanEqualsByteString), typ: "bytes -> bytes -> bool" },
    Builtin { name: "sha2_256", function: Some(F::Sha2_256), typ: "bytes -> bytes" },
    Builtin { name: "sha3_256", function: Some(F::Sha3_256), typ: "bytes -> bytes" },
    Builtin { name: "blake2b_256", function: Some(F::Blake2b_256), typ: "bytes -> bytes" },
    Builtin { name: "blake2b_224", function: Some(F::Blake2b_224), typ: "bytes -> bytes" },
    Builtin { name: "keccak_256", function: Some(F::Keccak_256), typ: "bytes -> bytes" },
    Builtin { name: "ripemd_160", function: Some(F::Ripemd_160), typ: "bytes -> bytes" },
    Builtin { name: "verifyEd25519Signature", function: Some(F::VerifyEd25519Signature), typ: "bytes -> bytes -> bytes -> bool" },
    Builtin { name: "verifyEcdsaSecp256k1Signature", function: Some(F::VerifyEcdsaSecp256k1Signature), typ: "bytes -> bytes -> bytes -> bool" },
    Builtin { name: "verifySchnorrSecp256k1Signature", function: Some(F::VerifySchnorrSecp256k1Signature), typ: "bytes -> bytes -> bytes -> bool" },
    Builtin { name: "appendString", function: Some(F::AppendString), typ: "string -> string -> string" },
    Builtin { name: "equalsString", function: Some(F::EqualsString), typ: "string -> string -> bool" },
    Builtin { name: "encodeUtf8", function: Some(F::EncodeUtf8), typ: "string -> bytes" },
    Builtin { name: "decodeUtf8", function: Some(F::DecodeUtf8), typ: "bytes -> string" },
    Builtin { name: "ifThenElse", function: Some(F::IfThenElse), typ: "bool -> 'a -> 'a -> 'a" },
    Builtin { name: "chooseUnit", function: Some(F::ChooseUnit), typ: "unit -> 'a -> 'a" },
    Builtin { name: "trace", function: Some(F::Trace), typ: "string -> 'a -> 'a" },
    Builtin { name: "fstPair", function: Some(F::FstPair), typ: "pair 'a 'b -> 'a" },
    Builtin { name: "sndPair", function: Some(F::SndPair), typ: "pair 'a 'b -> 'b" },
    Builtin { name: "chooseList", function: Some(F::ChooseList), typ: "list 'a -> 'b -> 'b -> 'b" },
    Builtin { name: "mkCons", function: Some(F::MkCons), typ: "'a -> list 'a -> list 'a" },
    Builtin { name: "headList", function: Some(F::HeadList), typ: "list 'a -> 'a" },
    Builtin { name: "tailList", function: Some(F::TailList), typ: "list 'a -> list 'a" },
    Builtin { name: "nullList", function: Some(F::NullList), typ: "list 'a -> bool" },
    Builtin { name: "dropList", function: Some(F::DropList), typ: "int -> list 'a -> list 'a" },
    Builtin { name: "chooseData", function: Some(F::ChooseData), typ: "Data -> 'a -> 'a -> 'a -> 'a -> 'a -> 'a" },
    Builtin { name: "constrData", function: Some(F::ConstrData), typ: "int -> list Data -> Data" },
    Builtin { name: "mapData", function: Some(F::MapData), typ: "list (pair Data Data) -> Data" },
    Builtin { name: "listData", function: Some(F::ListData), typ: "list Data -> Data" },
    Builtin { name: "iData", function: Some(F::IData), typ: "int -> Data" },
    Builtin { name: "bData", function: Some(F::BData), typ: "bytes -> Data" },
    Builtin { name: "unConstrData", function: Some(F::UnConstrData), typ: "Data -> pair int (list Data)" },
    Builtin { name: "unMapData", function: Some(F::UnMapData), typ: "Data -> list (pair Data Data)" },
    Builtin { name: "unListData", function: Some(F::UnListData), typ: "Data -> list Data" },
    Builtin { name: "unIData", function: Some(F::UnIData), typ: "Data -> int" },
    Builtin { name: "unBData", function: Some(F::UnBData), typ: "Data -> bytes" },
    Builtin { name: "equalsData", function: Some(F::EqualsData), typ: "Data -> Data -> bool" },
    Builtin { name: "serialiseData", function: Some(F::SerialiseData), typ: "Data -> bytes" },
    Builtin { name: "mkPairData", function: Some(F::MkPairData), typ: "Data -> Data -> pair Data Data" },
    Builtin { name: "mkNilData", function: Some(F::MkNilData), typ: "unit -> list Data" },
    Builtin { name: "mkNilPairData", function: Some(F::MkNilPairData), typ: "unit -> list (pair Data Data)" },
    Builtin { name: "bls12_381_g1_add", function: Some(F::Bls12_381_G1_Add), typ: "bls_g1 -> bls_g1 -> bls_g1" },
    Builtin { name: "bls12_381_g1_neg", function: Some(F::Bls12_381_G1_Neg), typ: "bls_g1 -> bls_g1" },
    Builtin { name: "bls12_381_g1_scalarMul", function: Some(F::Bls12_381_G1_ScalarMul), typ: "int -> bls_g1 -> bls_g1" },
    Builtin { name: "bls12_381_g1_equal", function: Some(F::Bls12_381_G1_Equal), typ: "bls_g1 -> bls_g1 -> bool" },
    Builtin { name: "bls12_381_g1_compress", function: Some(F::Bls12_381_G1_Compress), typ: "bls_g1 -> bytes" },
    Builtin { name: "bls12_381_g1_uncompress", function: Some(F::Bls12_381_G1_Uncompress), typ: "bytes -> bls_g1" },
    Builtin { name: "bls12_381_g1_hashToGroup", function: Some(F::Bls12_381_G1_HashToGroup), typ: "bytes -> bytes -> bls_g1" },
    Builtin { name: "bls12_381_g1_multiScalarMul", function: Some(F::Bls12_381_G1_MultiScalarMul), typ: "list int -> list bls_g1 -> bls_g1" },
    Builtin { name: "bls12_381_g2_add", function: Some(F::Bls12_381_G2_Add), typ: "bls_g2 -> bls_g2 -> bls_g2" },
    Builtin { name: "bls12_381_g2_neg", function: Some(F::Bls12_381_G2_Neg), typ: "bls_g2 -> bls_g2" },
    Builtin { name: "bls12_381_g2_scalarMul", function: Some(F::Bls12_381_G2_ScalarMul), typ: "int -> bls_g2 -> bls_g2" },
    Builtin { name: "bls12_381_g2_equal", function: Some(F::Bls12_381_G2_Equal), typ: "bls_g2 -> bls_g2 -> bool" },
    Builtin { name: "bls12_381_g2_compress", function: Some(F::Bls12_381_G2_Compress), typ: "bls_g2 -> bytes" },
    Builtin { name: "bls12_381_g2_uncompress", function: Some(F::Bls12_381_G2_Uncompress), typ: "bytes -> bls_g2" },
    Builtin { name: "bls12_381_g2_hashToGroup", function: Some(F::Bls12_381_G2_HashToGroup), typ: "bytes -> bytes -> bls_g2" },
    Builtin { name: "bls12_381_g2_multiScalarMul", function: Some(F::Bls12_381_G2_MultiScalarMul), typ: "list int -> list bls_g2 -> bls_g2" },
    Builtin { name: "bls12_381_millerLoop", function: Some(F::Bls12_381_MillerLoop), typ: "bls_g1 -> bls_g2 -> bls_mlr" },
    Builtin { name: "bls12_381_mulMlResult", function: Some(F::Bls12_381_MulMlResult), typ: "bls_mlr -> bls_mlr -> bls_mlr" },
    Builtin { name: "bls12_381_finalVerify", function: Some(F::Bls12_381_FinalVerify), typ: "bls_mlr -> bls_mlr -> bool" },
    Builtin { name: "integerToByteString", function: Some(F::IntegerToByteString), typ: "bool -> int -> int -> bytes" },
    Builtin { name: "byteStringToInteger", function: Some(F::ByteStringToInteger), typ: "bool -> bytes -> int" },
    Builtin { name: "andByteString", function: Some(F::AndByteString), typ: "bool -> bytes -> bytes -> bytes" },
    Builtin { name: "orByteString", function: Some(F::OrByteString), typ: "bool -> bytes -> bytes -> bytes" },
    Builtin { name: "xorByteString", function: Some(F::XorByteString), typ: "bool -> bytes -> bytes -> bytes" },
    Builtin { name: "complementByteString", function: Some(F::ComplementByteString), typ: "bytes -> bytes" },
    Builtin { name: "readBit", function: Some(F::ReadBit), typ: "bytes -> int -> bool" },
    Builtin { name: "writeBits", function: Some(F::WriteBits), typ: "bytes -> list int -> bool -> bytes" },
    Builtin { name: "replicateByte", function: Some(F::ReplicateByte), typ: "int -> int -> bytes" },
    Builtin { name: "shiftByteString", function: Some(F::ShiftByteString), typ: "bytes -> int -> bytes" },
    Builtin { name: "rotateByteString", function: Some(F::RotateByteString), typ: "bytes -> int -> bytes" },
    Builtin { name: "countSetBits", function: Some(F::CountSetBits), typ: "bytes -> int" },
    Builtin { name: "findFirstSetBit", function: Some(F::FindFirstSetBit), typ: "bytes -> int" },
    Builtin { name: "expModInteger", function: Some(F::ExpModInteger), typ: "int -> int -> int -> int" },
    Builtin { name: "lengthOfArray", function: Some(F::LengthOfArray), typ: "array 'a -> int" },
    Builtin { name: "listToArray", function: Some(F::ListToArray), typ: "list 'a -> array 'a" },
    Builtin { name: "indexArray", function: Some(F::IndexArray), typ: "array 'a -> int -> 'a" },
    Builtin { name: "insertCoin", function: Some(F::InsertCoin), typ: "bytes -> bytes -> int -> value -> value" },
    Builtin { name: "lookupCoin", function: Some(F::LookupCoin), typ: "bytes -> bytes -> value -> int" },
    Builtin { name: "unionValue", function: Some(F::UnionValue), typ: "value -> value -> value" },
    Builtin { name: "valueContains", function: Some(F::ValueContains), typ: "value -> value -> bool" },
    Builtin { name: "valueData", function: Some(F::ValueData), typ: "value -> Data" },
    Builtin { name: "unValueData", function: Some(F::UnValueData), typ: "Data -> value" },
    Builtin { name: "scaleValue", function: Some(F::ScaleValue), typ: "int -> value -> value" },
    Builtin { name: "identity", function: None, typ: "'a -> 'a" },
];
```

A unit test asserts that every `DefaultFunction` variant appears exactly
once (iterate `0..=100u8`, transmute-free: keep a `DefaultFunction::ALL`
array in `nash-plutus`, added in this chunk).

```rust
// crates/nash-can/src/environment/builtin.rs
/// The `Builtin` interface: `PRIMITIVES` as unions (plans/02 `primitives::interface`)
/// plus one value per `BUILTINS` row, its type string parsed and canonicalized
/// in an environment that has only the compiler-known types.
pub fn builtin_interface<'a>(bump: &'a Bump) -> Interface<'a> {
    let base = nash_ast::primitives::interface(bump);
    let env = known_types_env(bump, base.home);
    let values = bump.alloc_slice_fill_iter(nash_ast::primitives::BUILTINS.iter().map(|b| {
        let src = bump.alloc_str(b.typ);
        let mut parser = nash_parse::Parser::new(bump, src.as_bytes());
        let typ = parser.type_expr().expect("builtin table types are valid");
        let annotation = nash_can::types::to_annotation(bump, &env, typ).expect("builtin table types are closed");
        InterfaceValue { name: b.name, annotation }
    }));
    Interface { values, ..base }
}
```

`nash_can::types::to_annotation` is `canonicalize_type` followed by
`free_vars` collection (`crates/nash-can/src/types.rs`; Elm's
`Type.toAnnotation`). `known_types_env` is `create_initial_env` seeded
from `PRIMITIVES` with no imports.

Driver: `build_sync` (`compile.rs:86`) inserts `builtin_interface(&store)`
into `interfaces` before the loop. `Builtin` is not a URL and never
appears in `BuildResult.modules`.

Codegen (`crates/nash-codegen/src/builtin.rs`):

```rust
pub fn lower_builtin(name: &str) -> Option<Core<'static>> {
    let entry = nash_ast::primitives::BUILTINS.iter().find(|b| b.name == name)?;
    Some(match entry.function {
        Some(f) => Core::Builtin(f),
        None => Core::Identity,
    })
}
```

Until plans/02 lands, `bool`, `unit`, `pair`, `array`, `bls_*`, `value`,
`Data` are pre-seeded as opaque `Type::Union { arity }` entries with
`home = Builtin` so the table canonicalizes; plans/02 chunk 3 replaces
that seed with `PRIMITIVES`.

**Elm/Aiken reference**

Aiken `crates/aiken-lang/src/builtins.rs` `from_default_function`
(type per builtin, including the `bool` endianness argument of
`integerToByteString`) and `DefaultFunction::aiken_name`.

**Tests**

- `nash-ast`: `builtins_cover_every_default_function`, `builtin_types_parse`.
- `nash-driver`: `test_builtin_call_type_checks`: `main = Builtin.addInteger 1 2` builds; `main = Builtin.addInteger "a" 2` fails with a type error; `main = Builtin.nope` fails `NotFoundVarQual`.
- `nash docs` (plans/13) renders the table; not tested here.

**Done when** the driver tests pass and `Builtin.*` is usable from user
modules.

---

## Chunk 4: compiler-known types and the twin type modules

**Files**

- `core/src/Bool.nash`, `Unit.nash`, `Option.nash`, `Result.nash`, `Ordering.nash` (new; type declarations only, functions in chunk 6, impls in chunk 5)
- `crates/nash-can/src/environment/foreign.rs` (`make_union_ctor` special case for `Builtin.bool`; the `List` pre-seed at `foreign.rs:34` is gone once plans/02 chunk 3 seeds from `PRIMITIVES`)
- `crates/nash-can/src/environment/defaults.rs` (add the five modules)

**Change**

With kinds (plans/02) available: every compiler-known type comes from
`nash_ast::primitives::PRIMITIVES` with home `Builtin` (docs/stdlib.md
"Compiler-known types"); this chunk adds no second table. Declare the
twins in their own modules, `type Bool = False | True`
in `Bool.nash`, `type Unit = Unit` in `Unit.nash`,
`type option 'a = Some 'a | None` and `type Option 'a = Some 'a | None` in
`Option.nash`, and likewise `Result`, `Ordering` (representation.md
"Prelude twins", constructor order is load-bearing); move the
`Basics.Bool` special case (`foreign.rs:349`) to `Builtin.bool`.

**Code**

`core/src/Option.nash` (chunk 4 version):

```elm
module Option exposing (Option(..), type option(..))

type option 'a = Some 'a | None
type Option 'a = Some 'a | None
```

Inside `Option.nash` both constructor sets are in scope unqualified, so the
module's own code (chunk 6) writes `Option.Some` for the Big one, exactly
as users do; a bare `Some` inside the module is the little constructor.
Canonicalization treats a module's own Big twin constructors as
qualified-only when a little constructor of the same name is declared in
the same module (`Env.ctors` keeps the little one, `Env.q_ctors` the Big
one). User modules may not declare two constructors with one name; only
`nash/core` twin modules may, and only for a little/Big pair
(`Context.package` check).

The type seed is plans/02 chunk 3's (`primitives::PRIMITIVES`, homed by
`primitives::builtin_home()`, `Data`/`Int`/`Bytes`/`List`/`Map` Big and
`int`/`bytes`/`string`/`bool`/`unit`/`list`/`pair`/`array`/`bls_*`/`value`
Const). What this chunk adds to that seed: `bool` gets constructors
`False`, `True`, `unit` gets `()`, and `Data` gets `Constr`, `Map`, `List`,
`I`, `B` with the little field types from data.md. `foreign.rs`
`make_union_ctor`:

```rust
if home.name == "Builtin" && union_name == "bool" {
    return Ctor::Bool { home, union: can_union, index: ctor.index };
}
```

Big `Bool` is an ordinary union (`Constr 0`/`Constr 1`).

**Elm/Aiken reference**

`Canonicalize/Environment/Foreign.hs` `toCtor` (Bool special case);
`Elm/Compiler/Type/Extract.hs` has nothing to port here. Aiken
`builtins.rs` `prelude` (how `Option`, `Ordering`, `Bool` are declared as
compiler-known ADTs).

**Tests**

- `core` compiles (`nash check core/` via the driver test `test_core_compiles`, kept green from here on).
- nash-can: `if` on `bool` uses `Ctor::Bool`; `type t = A | B` little and `type T = A | B` Big in one user module is a dup-ctor error; `Some 1` and `Option.Some (lift 1)` from the defaults resolve to the little and the Big constructor.
- kinds: `list (option int)` is a kind error; `list Int` and `list int` are fine.

**Done when** `test_core_compiles` passes and the little/Big pairs are
usable side by side in one user module.

---

## Chunk 5: trait modules, operators, `Lift`, `ToData`, `FromData`

**Files**

- `core/src/Eq.nash`, `Ord.nash`, `Show.nash`, `Num.nash`, `Integral.nash`, `Semigroup.nash`, `Monoid.nash`, `Functor.nash`, `Applicative.nash`, `Monad.nash`, `Lift.nash`, `Data.nash`, `Literal.nash` (new; the file list of plans/03 chunk 12)
- `core/src/Prelude.nash` (the `infix` table and the tuple impls)
- `core/src/Bool.nash` (`not`, `and`, `or`, `xor`)
- `core/src/Debug.nash` (new)
- `core/src/Option.nash`, `Result.nash`, `Ordering.nash` (their `Eq`/`Functor`/`Applicative`/`Monad`/`Lift` impls)
- `crates/nash-can/src/environment/foreign.rs` (lazy `and`/`or` special case marker)
- `crates/nash-codegen/src/special.rs` (plans/07: `Bool.and`/`or` delay the second argument)

**Change**

Write each trait of traits.md "Core trait hierarchy" in its own module
with the impls for compiler-known types listed in docs/stdlib.md "Trait
modules"; `Lift.nash` holds representation.md's impl table (the reflexive
`Big 'a => Lift 'a 'a` is compiler-provided and not written); `Prelude`
gets the `infix` table, the operator helper functions, and the tuple
impls; `Bool` gets the `bool` functions; `Debug` gets `trace`, `todo`,
`fail` over `Builtin.trace` and `error`. Impls for the twin types go in
the twin's module.

**Code**

`core/src/Debug.nash`:

```elm
module Debug exposing (trace, todo, fail)

import Builtin

trace : string -> 'a -> 'a
trace = Builtin.trace

fail : string -> 'a
fail msg = Builtin.trace msg (Builtin.error ())

todo : string -> 'a
todo msg = fail (Builtin.appendString "TODO: " msg)
```

`Builtin.error : unit -> 'a` is added to the table (`function: None`,
lowered to `Core::Error`); it is the one entry that is not a
`DefaultFunction` besides `identity`.

Import order that type-checks (no module imports `Prelude`; each trait
module imports only `Builtin` and its superclass module):

```
Builtin
  └─ Eq ─ Ord          Show      Num ─ Integral      Semigroup ─ Monoid
     Functor ─ Applicative ─ Monad
     Lift             Data (ToData/FromData)          Literal
Bool, Unit                              (types only; `Bool` functions use `if`)
Prelude                                 (imports every trait module and Bool)
Option, Result, Ordering                (import Prelude and the trait modules they impl)
List, Int, Bytes, String, Map, ...      (import Prelude for operators; chunks 6–7)
```

`core/src/Functor.nash` carries `impl Functor list` with a local
recursive `mapList`; chunk 6's `List.map` is `Functor.map` specialized at
`list`, so `List` imports `Functor`, never the other way round.

`core/src/Prelude.nash` is docs/stdlib.md "Prelude" verbatim: the `infix`
block (`infix non 4 (==) = eq`, `infix left 6 (+) = add`,
`infix left 7 (/) = div`, `infix right 5 (::) = prepend`, ...), `identity`,
`always`, `applyForward`, `applyBackward`, `composeLeft`, `composeRight`,
`prepend = Builtin.mkCons` (kept here because `List` imports `Prelude`;
named `prepend` because `Cons` is the `Cons` module's constructor), and
`impl (Eq 'a, Eq 'b) => Eq ('a, 'b)` and friends up to 4-tuples for
`Eq`, `Ord`, `Show` (tuples count as defined in `nash/core` for the orphan
rule).

Lazy `and`/`or`: codegen recognizes `VarForeign { home: Bool, name: "and" | "or" }`
in call position with two arguments and emits `if a then b else False`
directly (the `if` is already lazy). Partial applications of `and` fall
back to the strict function.

**Elm/Aiken reference**

Elm `core/src/Basics.elm` for the operator table, precedences, and
`&&`/`||` (Elm's compiler special-cases them in `Optimize/Expression.hs`).
Aiken `builtins.rs` `prelude` for `Ordering`, `Option`, and the
`ToData`-like `Data` conversions (`builtins::data`).

**Tests**

- `core/src/Prelude.nash` `tests` block (runs after plans/10): `1 + 2 == 3`, `compare 1 2 == LT`, `lift 1 == (1 : Int)`, `lower (lift "a" : Bytes) == "a"`, `Some 1 == Some 1` and `Option.Some (lift 1) == Option.Some (lift 1)`, `[1,2] ++ [3] == [1,2,3]`, `fail` raises (`test "fail fails" fail = do fail "x"`).
- nash-can/nash-solve: `1 + 2` resolves to `Num int`; `(1 : Int) + 2` resolves `Num Int` with the literal at `Int` via `FromInt Int`; `lift [1, 2] : List Int` resolves `Lift (list int) (List Int)` through `Lift int Int`; `lift ([] : list Int) : List Int` resolves the element through the reflexive impl.
- codegen: `False && fail "x"` evaluates to `False` (laziness).

**Done when** `nash check core/` and the user-facing operator tests pass.

---

## Chunk 6: type modules

**Files**

- `core/src/Int.nash`, `Bytes.nash`, `String.nash`, `List.nash`, `Cons.nash`, `Pair.nash`, `Array.nash`, `Option.nash`, `Result.nash`, `Ordering.nash`, `Bool.nash`

**Change**

Write the APIs listed in docs/stdlib.md "Little-type modules" and "Twin
modules". Every function takes the little twin (`list 'a`, `option 'a`,
`int`, ...); the Big twins (`List 'a`, `Int`, `Option 'a`, ...) get no
functions, only the impls from chunk 5. `String.nash` is the little
`string` module; there is no Big `String`. `Cons.nash` is the Term-kind
linked list `type cons 'a = Nil | Cons 'a (cons 'a)` (docs/stdlib.md
"`Cons`") that chunk 10's `Ast` and plans/11 depend on; its `fromList`
and `toList` carry the `Storable` bound of `list`. All functions are
total unless documented (`Array.at`, `Option.unwrap`).

**Code** (`core/src/List.nash` excerpt, the shape everything else follows)

```elm
module List exposing (..)

import Prelude exposing (..)
import Builtin exposing (chooseList, headList, tailList, nullList)
import Functor

map : ('a -> 'b) -> list 'a -> list 'b
map = Functor.map

foldr : ('a -> 'b -> 'b) -> 'b -> list 'a -> 'b
foldr f acc xs =
    chooseList xs acc (f (headList xs) (foldr f acc (tailList xs)))

foldl : ('a -> 'b -> 'b) -> 'b -> list 'a -> 'b
foldl f acc xs =
    chooseList xs acc (foldl f (f (headList xs) acc) (tailList xs))

length : list 'a -> int
length = foldl (\_ n -> n + 1) 0

filter : ('a -> bool) -> list 'a -> list 'a
filter p =
    foldr (\x acc -> if p x then x :: acc else acc) []

member : Eq 'a => 'a -> list 'a -> bool
member x = any (\y -> x == y)

sortBy : ('a -> 'a -> ordering) -> list 'a -> list 'a
sortBy cmp xs =
    case xs of
        [] -> []
        pivot :: rest ->
            let
                (smaller, larger) = partition (\y -> cmp y pivot == LT) rest
            in
            sortBy cmp smaller ++ (pivot :: sortBy cmp larger)
```

`chooseList xs acc (...)` is strict in its branches; the recursive calls
are guarded because `chooseList`'s builtin type in the table is strict but
codegen wraps `chooseList` branches in delays when both branches are
present (plans/07 special case, same mechanism as `and`/`or`). Until that
lands, write `foldr` with `if nullList xs then acc else ...`; the `if` is
lazy today. This chunk uses the `if` form; plans/07 may rewrite.

**Elm/Aiken reference**

Elm `core/src/List.elm` for names and argument order. Aiken
`stdlib/lib/aiken/collection/list.ak` for what is worth having on-chain
(no `zip` on `list`, `at` returns `option`).

**Tests**

Each module has a `tests` block: `List.reverse [1,2,3] == [3,2,1]`,
`List.sort [3,1,2] == [1,2,3]`, `Bytes.slice 1 2 "abcd" == "bc"`,
`Int.pow 2 10 == 1024`, `Option.withDefault 0 None == 0`, and one `prop`
per module (`List.reverse (List.reverse xs) == xs` with `xs via listOf int`
once chunk 8 lands; before that the `prop`s are written but `nash test`
only runs `test`s).

**Done when** `nash check core/` passes and the `test`s pass under
`nash test core/`.

---

## Chunk 7: `Data` and `Map` modules

**Files**

- `core/src/Data.nash` (`serialise`, `tag`, `fields` added to chunk 5's traits), `Data/Decode.nash`, `Data/Encode.nash`, `Map.nash`

**Change**

Functions over the `Data` type, the decoder/encoder combinators, and
`Map` (the one module whose functions take a Big type, because its little
form `list (pair 'k 'v)` is not nominal). `Data`, `Data.Decode` and
`Data.Encode` are about `Data` only; there are no `Data.List`-style Big
counterparts of the type modules. Needs `Data` patterns (data.md).

**Code** (`core/src/Data/Decode.nash` excerpt)

```elm
module Data.Decode exposing (..)

import Builtin
import List

type alias decoder 'a = Data -> option 'a

int : decoder int
int d =
    case d of
        I n -> Some (lower n)
        _ -> None

bytes : decoder bytes
bytes d =
    case d of
        B b -> Some (lower b)
        _ -> None

list : decoder 'a -> decoder (list 'a)
list item d =
    case d of
        List xs -> traverse item (lower xs)
        _ -> None

constr : int -> decoder 'a -> decoder 'a
constr tag inner d =
    case d of
        Constr t fields -> if lower t == tag then inner d else None
        _ -> None

field : int -> decoder 'a -> decoder 'a
field i inner d =
    case d of
        Constr _ fields ->
            case List.at i (lower fields) of
                Some f -> inner f
                None -> None
        _ -> None

andThen : ('a -> decoder 'b) -> decoder 'a -> decoder 'b
andThen f dec d =
    case dec d of
        Some a -> f a d
        None -> None

traverse : decoder 'a -> list Data -> option (list 'a)
traverse dec xs =
    List.foldr
        (\x acc ->
            case (dec x, acc) of
                (Some a, Some rest) -> Some (a :: rest)
                _ -> None)
        (Some [])
        xs
```

`core/src/Map.nash` works on `Map 'k 'v` through `lower`/`lift`
(`Lift (list (pair 'k 'v)) (Map 'k 'v)`, one `unMapData`/`mapData` each):

```elm
module Map exposing (..)

import Prelude exposing (..)
import Builtin
import Eq exposing (Eq)
import Lift exposing (Lift)
import List
import Option
import Pair

get : Eq 'k => 'k -> Map 'k 'v -> option 'v
get k m =
    Option.map Pair.snd (List.find (\p -> Pair.fst p == k) (lower m))
```

`lower xs : list Int` on a `List Int` goes through
`Lift 'a 'b => Lift (list 'a) (List 'b)` with the reflexive element impl
and costs one `unListData` after the optimizer drops the identity map.

**Elm/Aiken reference**

Elm `core/src/Dict.elm` for `Map` API names (insert/get/remove/keys/values).
Aiken `stdlib/lib/aiken/collection/dict.ak` (association list semantics),
`stdlib/lib/aiken/cbor.ak` for diagnostics. Elm `Json.Decode` for the
combinator shapes (`field`, `andThen`, `oneOf`, `succeed`, `fail`).

**Tests**

`tests` blocks: `Decode.run (Decode.constr 0 (Decode.field 0 Decode.int)) (Encode.constr 0 [Encode.int 5]) == Some 5`;
`Decode.run Decode.int (Encode.bytes "x") == None`;
`Map.get (lift 1) (Map.fromList [Pair.make (lift 1) (lift "a")]) == Some (lift "a")`;
a `prop` that `Encode` then `Decode` is identity for `int`, `bytes`, `list int`.

**Done when** `nash check core/` passes and the decode tests pass.

---

## Chunk 8: `Fuzz`

**Files**

- `core/src/Fuzz.nash`
- `crates/nash-test/src/prng.rs` (plans/10 chunk 5: `Prng::from_seed`, `from_choices`, `to_data`, `from_data`)

**Change**

docs/testing.md "Fuzzers", verbatim: the **Big** `Prng` ADT that the
runner builds as `PlutusData`, the **little** `fuzzer 'a` wrapper with
`Functor`/`Applicative`/`Monad` impls, `choice` as the single primitive
over `u64` integer choices (not Aiken's bytes), and the generators listed
in docs/stdlib.md "`Fuzz`" built on `choice`.

**Code** (`core/src/Fuzz.nash` excerpt; the type and impl definitions are
docs/testing.md's)

```elm
module Fuzz exposing (..)

import Prelude exposing (..)
import Builtin
import Functor exposing (Functor)
import Applicative exposing (Applicative)
import Monad exposing (Monad)
import Lift exposing (Lift)
import List

type Prng = Seeded Bytes (List Int) | Replayed Int (List Int)

type fuzzer 'a = Fuzzer (Prng -> option (Prng, 'a))

run : fuzzer 'a -> Prng -> option (Prng, 'a)
run (Fuzzer f) = f

-- Draw an integer in [0, bound]. The only primitive.
choice : int -> fuzzer int
choice bound =
    Fuzzer
        (\prng ->
            case prng of
                Seeded seed choices ->
                    let
                        seed2 = Builtin.blake2b_256 (lower seed)
                        n = Builtin.byteStringToInteger True seed2 % (bound + 1)
                    in
                    Some (Seeded (lift seed2) (lift (lift n :: lower choices)), n)

                Replayed 0 _ -> None
                Replayed k rest ->
                    case lower rest of
                        c :: cs ->
                            if lower c <= bound then Some (Replayed (lift (k - 1)) (lift cs), lower c) else None
                        [] -> None)

impl Functor fuzzer where
    map f (Fuzzer g) =
        Fuzzer (\prng ->
            case g prng of
                None -> None
                Some (p, a) -> Some (p, f a))

impl Applicative fuzzer where
    pure a = Fuzzer (\prng -> Some (prng, a))
    apply ff fa = bind ff (\f -> map f fa)

impl Monad fuzzer where
    bind (Fuzzer g) k =
        Fuzzer (\prng ->
            case g prng of
                None -> None
                Some (p, a) -> run (k a) p)

constant : 'a -> fuzzer 'a
constant = pure

intBetween : int -> int -> fuzzer int
intBetween lo hi =
    if hi <= lo then constant lo else map (\n -> lo + n) (choice (hi - lo))

-- width first so small choices give small magnitudes (testing.md "Shrinking")
int : fuzzer int
int =
    do
        width <- choice 2
        case width of
            0 -> choice 255
            1 -> intBetween -32768 32767
            _ -> intBetween -9223372036854775808 9223372036854775807

listOf : fuzzer 'a -> fuzzer (list 'a)
listOf = listBetween 0 20

-- one `choice 1` continue bit per element; `0` stops
listBetween : int -> int -> fuzzer 'a -> fuzzer (list 'a)
listBetween lo hi item =
    let
        go n =
            if n >= hi then constant []
            else if n < lo then more n
            else
                do
                    continue <- choice 1
                    if continue == 0 then constant [] else more n
        more n =
            do
                x <- item
                xs <- go (n + 1)
                pure (x :: xs)
    in
    go 0
```

`Seeded`/`Replayed` field kinds are Big (`Bytes`, `List Int`, `Int`), so
`choice` lowers them to work and lifts them back; the runner reads the
returned `Prng` with `unwrap_constr`. The runner protocol (`draw`/`run`
programs, `Prng::from_seed`, `Prng::from_choices`, replay returning `None`
when the sequence runs out or a choice exceeds its bound) is
docs/testing.md "How the runner drives a property" and plans/10 chunks
4–7.

**Elm/Aiken reference**

Aiken `stdlib/lib/aiken/fuzz.ak` (`rand`, `int`, `list`, `bool`,
`bytearray`) and `crates/aiken-lang/src/test_framework.rs` `Prng`
(constructor tags: `Seeded = 0`, `Replayed = 1`; `Some = 0`, `None = 1`
must match the little `option` layout in plans/04). Nash differs in the
choice element type: `Int`, not bytes (testing.md "Open questions").

**Tests**

- `tests` block: `run (choice 10) (Seeded (lift "seed") (lift []))` is `Some`; `run (choice 10) (Replayed (lift 0) (lift []))` is `None`; `run (choice 10) (Replayed (lift 1) (lift [lift 11]))` is `None` (over bound); `intBetween 3 3` is `3`.
- `prop "intBetween in range"`: `let lo via int; n via intBetween 0 1000` then `intBetween lo (lo + n)` sampled through `Fuzz.run` stays in range.
- Rust (plans/10 chunk 6): a shrink test that a failing `listOf int` counterexample shrinks to `[0]` or `[]`.

**Done when** `nash test core/` runs the props with the plans/10 runner.

---

## Chunk 9: `Test`

**Files**

- `core/src/Test.nash`
- `crates/nash-codegen/src/test.rs` (plans/10 chunks 3–4: power-assert rewrite, `draw`/`run` programs)

**Change**

`label` and `assertFailed` from docs/stdlib.md "`Test`". There is no test
monad: a test body is a sequencing `do` block that desugars to plain `let`
(docs/testing.md "Test body"), `label : string -> unit` compiles to a
`\0label\0` trace, and `assert` is a keyword whose power-assert rewrite
(plans/10 chunk 3) ends in `Test.assertFailed`, which traces one
`\0assert\0` payload line per captured operand and then errors. This
module provides the runtime side.

**Code**

```elm
module Test exposing (label, assertFailed)

import Builtin

label : string -> unit
label s = Builtin.trace (Builtin.appendString "\u{0}label\u{0}" s) ()

-- Target of the power-assert rewrite; payload lines in order, then the error.
assertFailed : list string -> 'a
assertFailed msgs =
    case msgs of
        [] -> Builtin.error ()
        m :: rest -> Builtin.trace m (\() -> assertFailed rest) ()
```

Codegen for `Builtin.trace` delays its second argument (plans/07), which
is what makes the traces fire before the error.

**Elm/Aiken reference**

Aiken `crates/aiken-lang/src/test_framework.rs` `Assertion` (operand
capture for `==`, `!=`, `<`, etc.); Elm `elm-explorations/test` `Expect`
for naming only.

**Tests**

`tests` block: `test "labels are traces" = do label "a"` passes and the
runner's label table shows `a`; `test "assert False fails" fail = do assert False`;
`test "assertFailed traces then errors" fail = do assertFailed ["x", "y"]`
with the trace log `["x", "y"]`.

**Done when** `nash test` on a user project reports labels and
power-assert output through this module.

---

## Chunk 10: `Ast` and `Derive`

**Files**

- `core/src/Ast.nash`
- `core/src/Derive.nash`
- `core/tests/DeriveTests.nash`

**Change**

The `Ast` types and builders from docs/macros.md, matching
`crates/nash-macro/src/tags.rs` (plans/11 chunk 4; constructor tags are
declaration indices, so the two files change together); `Derive` from
plans/11 chunk 10. Both are plain Nash. Every `Ast` type is a little ADT
with native `string`/`int`/`bytes` fields, `option` slots, and `cons`
child lists (chunk 6); `Ast` imports `Prelude`, `Cons`, `String`, and the
trait modules. No `Data`, `Lift`, or Big type appears.

**Code** (`core/src/Ast.nash` builders excerpt)

```elm
expr : exprNode -> expr
expr node = Expr { span = None, typ = None } node

name : string -> name
name s = Local s

var : name -> expr
var n = expr (Var n)

int : int -> expr
int n = expr (IntLit n)

call : expr -> cons expr -> expr
call f args = expr (Call f args)

tuple : cons expr -> expr
tuple es = expr (Tuple es)

and : cons expr -> expr
and es =
    case es of
        Nil -> expr (Var (Global builtinModule "True"))
        Cons e rest -> Cons.foldl (\b acc -> expr (BinOp (Global boolModule "and") acc b)) e rest

builtinModule : modname
builtinModule = { package = Some "nash/core", name = "Builtin" }

boolModule : modname
boolModule = { package = Some "nash/core", name = "Bool" }

exprName : expr -> option string
exprName (Expr _ node) =
    case node of
        Var (Raw s) -> Some s
        Var (Global _ s) -> Some s
        _ -> None
```

**Elm/Aiken reference**

None; see plans/11.

**Tests**

`core/tests/DeriveTests.nash` per plans/11 chunk 10; `Ast.nash` `tests`:
`exprName (var (raw "Eq")) == Some "Eq"`, `and Nil` is the `True` node,
`Cons.length (Cons (int 1) Nil) == 1`.

**Done when** plans/11 chunk 12's `decl_macro_derive_eq` snapshot passes.

---

## Chunk 11: `Cardano.*`

**Files**

- `core/src/Cardano/Tx.nash`, `Cardano/Address.nash`, `Cardano/Value.nash`, `Cardano/Time.nash`
- `core/tests/golden/*.cbor` (real V3 script contexts)
- `core/tests/CardanoTests.nash`

**Change**

Big ADTs for the V3 `ScriptContext` per docs/stdlib.md, `Lift value Value`,
interval helpers. Golden tests decode real contexts with
`FromData.validateData` and check a few fields.

**Code** (`core/src/Cardano/Value.nash` excerpt)

```elm
module Cardano.Value exposing (..)

import Builtin

type alias Value = Map Bytes (Map Bytes Int)

impl Lift value Value where
    lift = Builtin.unValueData << toData
    lower = fromData << Builtin.valueData

lovelace : Value -> int
lovelace v = Builtin.lookupCoin "" "" (lift v)

quantityOf : bytes -> bytes -> Value -> int
quantityOf policy name v = Builtin.lookupCoin policy name (lift v)
```

Note the naming: `lift : value -> Value` here goes from Const to Big,
matching `Lift 'small 'big`'s direction (little is small).

**Elm/Aiken reference**

Aiken `stdlib/lib/cardano/transaction.ak`, `cardano/address.ak`,
`cardano/assets.ak` for field order and constructor tags (they match the
ledger). Plutus `plutus-ledger-api` `V3/Contexts.hs` is the source of
truth for the encoding.

**Tests**

`core/tests/CardanoTests.nash`: `validateData` on each golden context is
`Some`; `Tx.inputs` length matches; `lovelace` of the first output
matches the fixture.

**Done when** all fixtures decode and `nash check core/` stays green.

---

## Test harness

- `crates/nash-driver/src/compile.rs` `test_core_compiles`: builds an
  empty in-memory project (core only). Every chunk keeps it green.
- `nash test core/` (from plans/10) runs the `tests` blocks; CI runs it
  after `cargo test`. Until plans/10 lands, `tests` blocks are parsed and
  type-checked by `nash check` (plans/01 syntax, plans/10 chunks 1–2) but
  not executed.
- `crates/nash-ast` unit tests cover the `PRIMITIVES`/`BUILTINS` tables;
  `crates/nash-core` covers the embedded sources.

## Open questions

Same as docs/stdlib.md (`Fuzz`/`Test` as default imports; `value`
builtins gated by target version). Neither blocks a chunk.
