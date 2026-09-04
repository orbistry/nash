# Plan 11: macros and comptime

Goal: implement [docs/macros.md](../docs/macros.md): `macro` declarations,
`@attr` and `name!()` invocations, `quote`/`~`, the `Ast` reification in a
new `nash-macro` crate, the expansion loop in `nash-driver`, hygiene,
`comptime`, `@derive` in `nash/core`, diagnostics, and expansion snapshot
tests.

Prerequisites:

- plans/01 (syntax): `@attr(..)` on declarations, `name!(..)`, `comptime`,
  and partial operator sections such as `(> 5)` parsed into `nash-source`.
  Sections canonicalize to ordinary lambdas before macro arguments are
  reified. This plan adds `macro`, `quote`, `~`. If plans/01 named the
  surface types differently, use its names; the shapes below are what this
  plan needs.
- plans/02 (kinds): `Kind` and `kind_of(&CanType)` in `nash-constrain`.
- plans/03 (traits): `trait`/`impl` in `nash-source`/`nash-ast`, predicate
  resolution with a hook to defer unresolved predicates.
- plans/07 (codegen): `nash_codegen::lower_value(&ModuleSet, QualifiedName) -> &Term<DeBruijn>`
  and `Core::Const`.
- plans/12 (stdlib) chunk "Ast": `core/src/Ast.nash` is the Nash side of the
  reifier/unreifier in this plan (chunks 4 and 5), and chunk 6 there
  supplies `Cons.cons`. The tag table and `Ast.nash` must be changed
  together.

Crates touched: `nash-source`, `nash-parse`, `nash-ast`, `nash-can`,
`nash-constrain`, `nash-solve`, `nash-codegen`, `nash-driver`, `nash-report`,
new `nash-macro`, `core/`.

References:

- Elm: `elm/compiler/src/Parse/Declaration.hs` (declaration dispatch),
  `Parse/Expression.hs` (`term`), `Canonicalize/Expression.hs`
  (`canonicalize`, `findVar`), `Canonicalize/Environment/Foreign.hs`,
  `Type/Constrain/Expression.hs` (`constrain`), `Type/Solve.hs` (`run`),
  `Elm/Interface.hs` (`fromModule`).
- Aiken: `crates/aiken-lang/src/gen_uplc.rs` (`generate_raw`,
  `generate_test`) for compiling a single definition to a program;
  `crates/aiken-project/src/lib.rs` (`run_tests`) for running programs
  with a budget and collecting traces; `crates/uplc/src/ast.rs`
  (`Term::Constr` construction).
- Current code: `crates/nash-driver/src/compile.rs:145` (`compile_module`),
  `crates/nash-can/src/module.rs:48` (`canonicalize`),
  `crates/nash-constrain/src/expression.rs:25` (`constrain`),
  `crates/nash-plutus/src/program.rs:51` (`eval_version_budget`),
  `crates/nash-plutus/src/term.rs:31` (`Term::Constr`),
  `crates/nash-plutus/src/machine/discharge.rs` (`value_as_term`),
  `crates/nash-plutus/src/constant.rs` (`Constant::{Integer, String, ByteString}`).

Conventions: `'a` is the module arena lifetime. `Arena` is
`nash_plutus::arena::Arena`; `'p` is its lifetime. The `Ast` family is
little (kind `Term`), so its runtime layout is representation.md's
"Term types": constructor = `constr i [fields]` with `i` the declaration
index and fields positional; a little record alias or labeled constructor
is `constr 0 [..]`/`constr i [..]` in field order; `string`/`int`/`bytes`
fields are the UPLC constants `Constant::String`/`Integer`/`ByteString`;
`option` = `Some` tag 0 / `None` tag 1; `cons` = `Nil` tag 0 /
`Cons` tag 1 (`core/Cons.nash`). Nothing in the macro path is `Data`.
`comptime` results (chunk 9) are unchanged: they must be UPLC constants
(`Const` or `Big` kind) and are spliced as `Core::Const`.

---

## Chunk 1: surface syntax for `macro`, `quote`, `~`

**Files**

- `crates/nash-source/src/lib.rs`
- `crates/nash-parse/src/keyword.rs`
- `crates/nash-parse/src/declaration/mod.rs`
- `crates/nash-parse/src/declaration/macro_.rs` (new)
- `crates/nash-parse/src/expression/mod.rs`
- `crates/nash-parse/src/expression/quote.rs` (new)
- `crates/nash-parse/src/error.rs`
- `SPEC.md`

**Change**

Add the `macro` declaration, `quote (e)`, and `~x` / `~(e)` to the surface
AST and parser. Attributes, `MacroCall`, and `Comptime` come from plans/01;
this chunk assumes these exist in `nash-source`:

```rust
#[derive(Debug)]
pub struct Attribute<'a> {
    pub region: Region,
    pub module: Option<&'a str>,
    pub name: &'a Located<&'a str>,
    pub arguments: &'a [&'a Located<Expr<'a>>],
}

// on Value, Union, Alias, Trait, Impl:
pub attributes: &'a [&'a Attribute<'a>],

// in Expr:
MacroCall {
    module: Option<&'a str>,
    name: &'a Located<&'a str>,
    arguments: &'a [&'a Located<Expr<'a>>],
},
Comptime(&'a Located<Expr<'a>>),
```

**Code**

`crates/nash-source/src/lib.rs`:

```rust
#[derive(Debug)]
pub struct Module<'a> {
    // ...existing fields...
    pub macros: &'a [&'a Located<Macro<'a>>],
}

/// `macro name : T` followed by `name args = body`.
#[derive(Debug)]
pub struct Macro<'a> {
    pub name: &'a Located<&'a str>,
    pub annotation: &'a Located<Type<'a>>,
    pub arguments: &'a [&'a Located<Pattern<'a>>],
    pub body: &'a Located<Expr<'a>>,
}

pub enum Expr<'a> {
    // ...existing...
    Quote(&'a Located<Expr<'a>>),
    Splice(&'a Located<Expr<'a>>),
    /// Produced only by the macro decoder, never by the parser: a name
    /// that resolves through the interface table regardless of imports.
    VarGlobal {
        package: Option<&'a str>,
        module: &'a str,
        name: &'a str,
    },
}

pub enum Pattern<'a> {
    // ...existing...
    CtorGlobal {
        region: Region,
        package: Option<&'a str>,
        module: &'a str,
        name: &'a str,
        args: &'a [&'a Located<Pattern<'a>>],
    },
}

pub enum Type<'a> {
    // ...existing...
    TypeGlobal {
        region: Region,
        package: Option<&'a str>,
        module: &'a str,
        name: &'a str,
        args: &'a [&'a Located<Type<'a>>],
    },
}
```

`crates/nash-parse/src/declaration/mod.rs` — extend `Decl` and dispatch:

```rust
pub enum Decl<'a> {
    Value(Option<&'a Comment<'a>>, &'a Located<Value<'a>>),
    Union(Option<&'a Comment<'a>>, &'a Located<Union<'a>>),
    Alias(Option<&'a Comment<'a>>, &'a Located<Alias<'a>>),
    Macro(Option<&'a Comment<'a>>, &'a Located<Macro<'a>>),
}

// in `declaration`: try `keyword_macro` before `value_decl`
Box::new(|p: &mut Parser<'a>| p.macro_decl(maybe_docs, start)),
```

`crates/nash-parse/src/declaration/macro_.rs`:

```rust
impl<'a> Parser<'a> {
    /// `macro name : type` then `name patterns = expr` on the next fresh line.
    pub(crate) fn macro_decl(
        &mut self,
        _docs: Option<&'a Comment<'a>>,
        start: Position,
    ) -> Result<(Decl<'a>, Position), error::Decl<'a>> {
        self.keyword_macro(error::Decl::Start)?;
        self.chomp_and_check_indent(error::Decl::Space, error::Decl::IndentMacroName)?;
        let name = self.lower_var(error::Decl::MacroName)?;
        self.chomp_and_check_indent(error::Decl::Space, error::Decl::IndentMacroColon)?;
        self.symbol_colon(error::Decl::MacroColon)?;
        self.chomp_and_check_indent(error::Decl::Space, error::Decl::IndentMacroType)?;
        let annotation = self.type_expr().map_err(|e| error::Decl::MacroType(self.alloc(e)))?;
        self.chomp(error::Decl::Space)?;
        self.check_fresh_line(error::Decl::MacroDefinition)?;
        let def_name = self.lower_var(error::Decl::MacroDefinition)?;
        if def_name.value != name.value {
            return Err(error::Decl::MacroNameMismatch(name.value, def_name.value, def_name.region));
        }
        let (arguments, body, end) = self.definition_tail(error::Decl::Def)?;
        let region = Region::new(start, end);
        let m = self.alloc(Located::at(region, Macro { name, annotation, arguments, body }));
        Ok((Decl::Macro(_docs, m), end))
    }
}
```

`definition_tail` is the existing argument-patterns-then-`=`-then-body
parser factored out of `value_decl` (`crates/nash-parse/src/declaration/value.rs`).

`crates/nash-parse/src/expression/quote.rs`:

```rust
impl<'a> Parser<'a> {
    /// `quote ( expr )`. Nested quotes are an error; splices inside are allowed.
    pub(crate) fn quote(&mut self, start: Position) -> Result<&'a Located<Expr<'a>>, error::Expr<'a>> {
        self.keyword_quote(error::Expr::Start)?;
        if self.in_quote {
            return Err(error::Expr::NestedQuote(start.line, start.column));
        }
        self.chomp(error::Expr::Space)?;
        self.symbol_open_paren(error::Expr::QuoteOpen)?;
        self.in_quote = true;
        let inner = self.expression();
        self.in_quote = false;
        let inner = inner?;
        self.chomp(error::Expr::Space)?;
        self.symbol_close_paren(error::Expr::QuoteClose)?;
        let end = self.get_position();
        Ok(self.alloc(Located::at(Region::new(start, end), Expr::Quote(inner))))
    }

    /// `~x` or `~( expr )`; only valid while `in_quote`.
    pub(crate) fn splice(&mut self, start: Position) -> Result<&'a Located<Expr<'a>>, error::Expr<'a>> {
        self.symbol_tilde(error::Expr::Start)?;
        if !self.in_quote {
            return Err(error::Expr::SpliceOutsideQuote(start.line, start.column));
        }
        let inner = if self.peek() == Some(b'(') {
            self.advance();
            let e = self.expression()?;
            self.symbol_close_paren(error::Expr::SpliceClose)?;
            e
        } else {
            self.lower_var_expr()?
        };
        let end = self.get_position();
        Ok(self.alloc(Located::at(Region::new(start, end), Expr::Splice(inner))))
    }
}
```

`Parser` gains `in_quote: bool` (`crates/nash-parse/src/lib.rs:39`), reset
in `new`. `term` (`expression/mod.rs`) dispatches on `quote` keyword and
`~`. `~` must not clash with an operator: add `~` to the set of reserved
symbol characters in `symbol.rs` (like `@`).

Errors (`crates/nash-parse/src/error.rs`): `Decl::MacroName`,
`Decl::MacroColon`, `Decl::MacroType`, `Decl::MacroDefinition`,
`Decl::MacroNameMismatch(&'a str, &'a str, Region)`, `Decl::Indent*`,
`Expr::NestedQuote(Row, Col)`, `Expr::SpliceOutsideQuote(Row, Col)`,
`Expr::QuoteOpen/Close`, `Expr::SpliceClose`.

`SPEC.md`: add the EBNF from docs/macros.md (`macro_decl`, `quote_expr`,
`splice`).

**Elm/Aiken reference**

`Parse/Declaration.hs` `declaration` and `valueDecl` (for
`definition_tail`); `Parse/Expression.hs` `term` for adding new term
forms; `Parse/Keyword.hs` for the keyword helpers.

**Tests** (`crates/nash-parse/src/declaration/macro_.rs`, `expression/quote.rs`)

- `assert_decl_snapshot!("macro derive : Decl -> List Expr -> List Decl\nderive decl traits = decl :: traits")`
- `assert_decl_error_snapshot!("macro derive : Decl\nother decl = decl")` → `MacroNameMismatch`
- `assert_decl_error_snapshot!("macro derive decl = decl")` → `MacroColon`
- `assert_expression_snapshot!("quote (f x 1)")`
- `assert_expression_snapshot!("quote (~x + ~(g y))")`
- `assert_expression_error_snapshot!("~x")` → `SpliceOutsideQuote`
- `assert_expression_error_snapshot!("quote (quote (x))")` → `NestedQuote`
- `assert_module_snapshot!` with a macro and a value in one module.

**Done when** the snapshots above pass and `cargo clippy` is clean.

---

## Chunk 2: canonical macro declarations, invocations, lenient mode

**Files**

- `crates/nash-ast/src/lib.rs`
- `crates/nash-can/src/module.rs`, `expression.rs`, `pattern.rs`, `types.rs`
- `crates/nash-can/src/environment.rs`, `environment/foreign.rs`
- `crates/nash-can/src/interface.rs`
- `crates/nash-can/src/error.rs`
- `crates/nash-can/src/quote.rs` (new)

**Change**

1. Canonicalize `macro` declarations into `Module.macros`; export them in
   the interface with their shape.
2. Canonicalize invocations: `Attribute` and `Expr::MacroCall` resolve the
   macro name through the env and check shape and same-module use.
   Collect every use into `CanResult.macro_uses`.
3. `Context.mode`: `Strict` (today's behaviour) or `Lenient` (unknown
   names become holes).
4. Resolve `VarGlobal` / `CtorGlobal` / `TypeGlobal` through
   `Context.interfaces` directly.
5. Desugar `Expr::Quote` into calls of `Ast` constructors.

**Code**

`crates/nash-ast/src/lib.rs`:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacroShape {
    /// `Ast.decl -> cons Ast.expr -> cons Ast.decl`
    Decl,
    /// `cons Ast.expr -> Ast.expr`
    Expr,
}

#[derive(Debug)]
pub struct MacroDef<'a> {
    pub name: &'a Located<&'a str>,
    pub shape: MacroShape,
    pub def: &'a Def<'a>,
}

pub struct Module<'a> {
    // ...
    pub macros: &'a [&'a MacroDef<'a>],
}

pub enum Expr<'a> {
    // ...
    MacroCall {
        reference: QualifiedName<'a>,
        arguments: &'a [&'a Located<Expr<'a>>],
    },
    Comptime(&'a Located<Expr<'a>>),
    /// Lenient mode only: an unresolved name. Never reaches codegen.
    Hole,
}

pub enum Pattern<'a> { /* ... */ Hole }
pub enum Type<'a> { /* ... */ Hole }

/// A declaration decorated with attributes, in source order.
#[derive(Debug)]
pub struct Attribute<'a> {
    pub region: Region,
    pub reference: QualifiedName<'a>,
    /// Untyped: attribute arguments are never canonicalized.
    pub arguments: &'a [&'a Located<nash_source::Expr<'a>>],
}

#[derive(Debug)]
pub enum MacroUse<'a> {
    Decl {
        target: DeclTarget<'a>,
        attribute: &'a Attribute<'a>,
    },
    Expr {
        region: Region,
        reference: QualifiedName<'a>,
        arguments: &'a [&'a Located<Expr<'a>>],
    },
}

#[derive(Clone, Copy, Debug)]
pub enum DeclTarget<'a> {
    Value(&'a str),
    Union(&'a str),
    Alias(&'a str),
    Trait(&'a str),
    Impl(u32),
}
```

`crates/nash-can/src/module.rs`:

```rust
/// Read by canonicalization (`Context.mode`) and passed unchanged to
/// `nash_solve::run(.., mode)` (plans/03 chunk 5 "Solver API"). The one mode type.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Strict,
    Lenient,
}

pub struct Context<'a> {
    pub package: Option<PackageName<'a>>,
    pub interfaces: Option<&'a BTreeMap<&'a str, Interface<'a>>>,
    pub mode: Mode,
}

pub struct CanResult<'a> {
    pub module: CanModule<'a>,
    pub warnings: Vec<Warning<'a>>,
    pub macro_uses: Vec<MacroUse<'a>>,
}
```

Macro shape from the annotation (in `module.rs`, after `canonicalize_aliases`):

```rust
fn macro_shape<'a>(annotation: &Located<CanType<'a>>) -> Result<MacroShape, Error<'a>> {
    match &annotation.value {
        CanType::Lambda { from, to } if is_ast(from, "decl") => match &to.value {
            CanType::Lambda { from: args, to: out }
                if is_cons_of(args, "expr") && is_cons_of(out, "decl") =>
            {
                Ok(MacroShape::Decl)
            }
            _ => Err(Error::MacroBadShape { region: annotation.region }),
        },
        CanType::Lambda { from, to } if is_cons_of(from, "expr") && is_ast(to, "expr") => {
            Ok(MacroShape::Expr)
        }
        _ => Err(Error::MacroBadShape { region: annotation.region }),
    }
}

/// `Cons.cons` applied to the named `Ast` type.
fn is_cons_of(t: &Located<CanType<'_>>, name: &str) -> bool {
    matches!(&t.value, CanType::Named { reference, args: [elem] }
        if reference.home.name == "Cons" && reference.name == "cons" && is_ast(elem, name))
}

fn is_ast(t: &Located<CanType<'_>>, name: &str) -> bool {
    matches!(&t.value, CanType::Named { reference, args: [] }
        if reference.home.name == "Ast" && reference.name == name)
}
```

Env entries: `Var::Macro { home: ModuleName, shape: MacroShape }` for
locally declared macros (never callable locally, but they must shadow
correctly) and `Var::ForeignMacro(ModuleName, MacroShape)` for imported
ones, populated in `foreign.rs` from `Interface.macros`:

```rust
// crates/nash-can/src/interface.rs
#[derive(Clone, Copy, Debug)]
pub struct InterfaceMacro<'a> {
    pub name: &'a str,
    pub shape: MacroShape,
}

pub struct Interface<'a> {
    // ...
    pub macros: &'a [InterfaceMacro<'a>],
}
```

Attribute resolution (`module.rs`, called while canonicalizing each decl):

```rust
fn canonicalize_attribute<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    attr: &'a nash_source::Attribute<'a>,
) -> Result<&'a Attribute<'a>, Error<'a>> {
    let (home, shape) = match attr.module {
        None => env.find_macro(attr.name)?,
        Some(prefix) => env.find_macro_qual(prefix, attr.name)?,
    };
    if home == env.home {
        return Err(Error::MacroSameModule { region: attr.region, name: attr.name.value });
    }
    if shape != MacroShape::Decl {
        return Err(Error::MacroWrongKind { region: attr.region, name: attr.name.value, expected: MacroShape::Decl });
    }
    Ok(bump.alloc(Attribute {
        region: attr.region,
        reference: QualifiedName { home, name: attr.name.value },
        arguments: attr.arguments,
    }))
}
```

`Expr::MacroCall` in `expression.rs` does the same with `MacroShape::Expr`,
canonicalizes the arguments normally, and pushes `MacroUse::Expr` onto a
`&mut Vec<MacroUse>` threaded like `warnings` is today.

Lenient mode in `expression.rs` `find_var`:

```rust
Err(err @ Error::NotFoundVar { .. }) if mode == Mode::Lenient => Ok(CanExpr::Hole),
```

Same for constructors in `pattern.rs` and types in `types.rs`. Every
other error is unchanged.

`VarGlobal` resolution:

```rust
SourceExpr::VarGlobal { package, module, name } => {
    let interface = env.global_interface(package, module)
        .ok_or(Error::MacroGlobalNotFound { region, module, name })?;
    let value = interface.values.iter().find(|v| v.name == *name)
        .ok_or(Error::MacroGlobalNotFound { region, module, name })?;
    Ok(CanExpr::VarForeign {
        reference: QualifiedName { home: interface.home, name: value.name },
        annotation: value.annotation,
    })
}
```

`Env::global_interface` looks in `Context.interfaces` by module name (the
map is build-wide, see `crates/nash-driver/src/compile.rs:86`).

Quote desugaring (`crates/nash-can/src/quote.rs`): canonicalize the quoted
expression **in the current env** (so free names resolve here), then
convert the canonical tree into a canonical expression that *builds* the
`Ast` value. The `Ast` constructors are looked up through the interface of
`Ast`; if `Ast` is not imported the error is `QuoteWithoutAst`.

```rust
pub(crate) fn quote<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    inner: &'a Located<CanExpr<'a>>,
) -> Result<CanExpr<'a>, Error<'a>> {
    let ast = AstCtors::lookup(env, inner.region)?;
    Ok(Quoter { bump, ast, env }.expr(inner))
}

struct Quoter<'a, 'e> { bump: &'a Bump, ast: AstCtors<'a>, env: &'e Env<'a> }

impl<'a> Quoter<'a, '_> {
    fn expr(&self, e: &'a Located<CanExpr<'a>>) -> CanExpr<'a> {
        let node = match &e.value {
            CanExpr::Int(n) => self.ctor("IntLit", &[CanExpr::Int(*n)]),
            CanExpr::Str(s) => self.ctor("StrLit", &[CanExpr::Str(s)]),
            CanExpr::VarLocal(name) => self.ctor("Var", &[self.name_local(name)]),
            CanExpr::VarTopLevel(q) | CanExpr::VarForeign { reference: q, .. } => {
                self.ctor("Var", &[self.name_global(*q)])
            }
            CanExpr::Binop { reference, left, right, .. } => self.ctor(
                "BinOp",
                &[self.name_global(*reference), self.expr_ref(left), self.expr_ref(right)],
            ),
            CanExpr::Lambda { parameters, body } => self.ctor(
                "Lambda",
                &[self.cons_list(parameters.iter().map(|p| self.pattern(p))), self.expr_ref(body)],
            ),
            CanExpr::Call { function, arguments } => self.ctor(
                "Call",
                &[self.expr_ref(function), self.cons_list(arguments.iter().map(|a| self.expr_ref(a)))],
            ),
            // Splice: the user's expression already evaluates to an Ast.expr.
            CanExpr::Splice(inner) => return inner.value_copy(),
            // ...one arm per variant, mirroring Reifier::expr in chunk 4...
        };
        self.wrap_expr(node)   // Ast.Expr { span = None, typ = None } node
    }

    /// `Cons x0 (Cons x1 ... Nil)` built from the `Cons` module's constructors.
    fn cons_list(&self, items: impl Iterator<Item = CanExpr<'a>>) -> CanExpr<'a> { ... }

    fn name_local(&self, s: &str) -> CanExpr<'a> {
        // Binders inside the quote are hygienic.
        self.ctor("Local", &[CanExpr::Str(s)])
    }
}
```

`CanExpr::Splice` exists only between canonicalizing the quote body and
running `Quoter`; `Quoter` consumes it. A `Splice` outside a quote cannot
occur (parser rejects it).

Also: `canonicalize_decls` skips macro definitions for SCC purposes? No.
Macro bodies are ordinary top-level definitions (they may call helpers),
so they are part of `Decls`; `Module.macros` just points at them with
their shape. The interface exports them as values too, so `nash-codegen`
can lower them like any value.

**Elm/Aiken reference**

`Canonicalize/Expression.hs` `findVar`, `findVarQual`;
`Canonicalize/Environment/Foreign.hs` `createInitialEnv` (macro
entries follow `addExposedValue`); `Elm/Interface.hs` `fromModule`
(macro list next to `values`).

**Tests** (`crates/nash-can/src/snapshots`)

- `assert_can_snapshot!` module declaring `macro m : cons Ast.expr -> Ast.expr` with `import Ast` and `import Cons` interface stubs.
- `assert_can_error_snapshot!` same module invoking `m!(1)` → `MacroSameModule`.
- Importing module using `@Derive.derive(Eq)` on a union → `macro_uses` has one `Decl` use; snapshot it.
- `m!(x)` where `m` is a value → `MacroNotAMacro`; a decl macro used as `m!()` → `MacroWrongKind`.
- Lenient: `main = missing 1` with `Mode::Lenient` canonicalizes to `Call(Hole, [1])`; `Mode::Strict` errors as today.
- Quote: `quote (f ~x 1)` with `f` foreign snapshot shows `Ast.Call (Ast.Var (Global ..f)) [x, Ast.Int 1]`.

**Done when** all snapshot tests pass and `nash-driver` still compiles
(pass `Mode::Strict` in `compile_module`).

---

## Chunk 3: per-node types and lenient solving

**Files**

- `crates/nash-constrain/src/lib.rs`, `expression.rs`, `pattern.rs`
- `crates/nash-solve/src/lib.rs`, `solve.rs`, `annotation.rs`

**Change**

The reifier needs the solved type of every expression and pattern. Elm
only produces top-level annotations. Record, per node, the type variable
the constraint generator used, then resolve after solving.

**Code**

```rust
// crates/nash-constrain/src/lib.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(usize);

impl NodeId {
    pub fn of<T>(node: &T) -> Self {
        NodeId(node as *const T as usize)
    }
}

#[derive(Default)]
pub struct NodeTypes<'a> {
    pub exprs: Vec<(NodeId, &'a Type<'a>)>,
    pub patterns: Vec<(NodeId, &'a Type<'a>)>,
}

pub fn constrain<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    module: &'a CanModule<'a>,
    nodes: &mut NodeTypes<'a>,
) -> Constraint<'a>
```

In `expression.rs` `constrain`, the first line becomes:

```rust
let region = expr.region;
nodes.exprs.push((NodeId::of(expr), expected.type_ref()));
```

`Expected::type_ref` returns the `&'a Type<'a>` inside `NoExpectation` /
`FromContext` / `FromAnnotation`. Patterns likewise in `pattern.rs`
`add`. `CanExpr::Hole` and `CanExpr::MacroCall` constrain as
`Constraint::True` after pushing the node (a `MacroCall` still constrains
its arguments against fresh variables so they get types).

```rust
// crates/nash-solve/src/lib.rs
pub type NodeTypeMap<'a> = BTreeMap<NodeId, &'a nash_ast::Type<'a>>;

/// Resolve recorded node types after `run` succeeded.
pub fn node_types<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    nodes: &NodeTypes<'a>,
) -> NodeTypeMap<'a>
```

It reuses `annotation::to_annotation`'s variable-to-canonical-type walk
(`crates/nash-solve/src/annotation.rs:16`) per recorded type; free
variables become `Type::Var` with the solver's generated names.

Lenient predicates: the solver entry is plans/03 chunk 5's "Solver API",
`nash_solve::run(bump, uf, constraint, tables, fields, mode)` with
`mode: nash_can::Mode` (the `Strict | Lenient` enum chunk 2 puts on
`nash_can::Context`; no separate solver flag type exists), and `run` sets
`Solver.mode` from it (plans/03 chunk 6, `unresolved` helper in
`resolve.rs`). In `Lenient`, a predicate that would report
`nash_constrain::Error::MissingImpl` (or a missing constraint, an
ambiguous variable, or polymorphic recursion) is detached instead, and
its use site gets no `Instance.evidence` entry. Dependency: plans/03 chunk
6 owns the resolver and the helper; this chunk only threads the flag from
`compile_module`.

**Elm/Aiken reference**

`Type/Constrain/Expression.hs` `constrain` (every case receives
`expected`); `Type/Solve.hs` `run`; `Type/Solve.hs` `toAnnotation` is
already `annotation.rs`. Aiken keeps types on every `TypedExpr` node
(`crates/aiken-lang/src/expr.rs`) which is the information this map
reproduces.

**Tests** (`crates/nash-solve/src/tests.rs`)

- `main = \x -> x + 1` with an `int` `Num` impl: node map has `Lambda : int -> int`, `x : int`, `1 : int`.
- `m!(1, "a")` in lenient mode: arguments typed `int` and `string`, call node is a free `Var`.
- Strict mode with an unresolved predicate still errors; lenient drops it.

**Done when** `compile_module` threads `NodeTypes` and discards it, and
tests pass.

---

## Chunk 4: `nash-macro` crate and the reifier (`nash_ast` → `Term::Constr`)

**Files**

- `crates/nash-macro/Cargo.toml` (new; deps: `nash-ast`, `nash-source`, `nash-region`, `nash-plutus`, `nash-solve`, `bumpalo`)
- `crates/nash-macro/src/lib.rs`, `tags.rs`, `reify.rs`
- `core/src/Ast.nash`, `core/src/Cons.nash` (plans/12 chunks 10 and 6 — same PR)

**Change**

Build, for a `MacroUse`, the UPLC `Term::Constr` tree that *is* the
little `Ast` value the macro expects, matching `core/src/Ast.nash`. The
tree is arena-allocated (`nash_plutus::arena::Arena`), constructor tags
are declaration indices, leaves are `Constant::String`/`Integer`/
`ByteString` constants, child lists are `Cons`/`Nil` chains, and the
result is handed to the CEK machine with `Term::apply` (chunk 7). No
`PlutusData` is built anywhere.

`tags.rs` is the single place the two sides agree. Constructor tags are
declaration indices in `Ast.nash`; keep the two files side by side.

**Code**

```rust
// crates/nash-macro/src/tags.rs
pub mod cons { pub const NIL: usize = 0; pub const CONS: usize = 1; }          // core/Cons.nash
pub mod name { pub const LOCAL: u64 = 0; pub const RAW: u64 = 1; pub const GLOBAL: u64 = 2; }
pub mod kind { pub const BIG: u64 = 0; pub const CONST: u64 = 1; pub const TERM: u64 = 2; pub const STORABLE: u64 = 3; pub const ANY: u64 = 4; pub const ARROW: u64 = 5; pub const VAR: u64 = 6; }
pub mod expr {
    pub const INT: u64 = 0; pub const STR: u64 = 1; pub const BYTES: u64 = 2; pub const VAR: u64 = 3;
    pub const OP: u64 = 4; pub const LIST: u64 = 5; pub const NEGATE: u64 = 6; pub const BINOP: u64 = 7;
    pub const LAMBDA: u64 = 8; pub const CALL: u64 = 9; pub const IF: u64 = 10; pub const LET: u64 = 11;
    pub const CASE: u64 = 12; pub const ACCESSOR: u64 = 13; pub const ACCESS: u64 = 14; pub const UPDATE: u64 = 15;
    pub const RECORD: u64 = 16; pub const UNIT: u64 = 17; pub const TUPLE: u64 = 18; pub const MACRO_CALL: u64 = 19;
    pub const COMPTIME: u64 = 20;
}
pub mod def { pub const DEFINE: u64 = 0; pub const DESTRUCT: u64 = 1; }
pub mod pattern {
    pub const ANY: u64 = 0; pub const VAR: u64 = 1; pub const RECORD: u64 = 2; pub const ALIAS: u64 = 3;
    pub const UNIT: u64 = 4; pub const TUPLE: u64 = 5; pub const CTOR: u64 = 6; pub const LIST: u64 = 7;
    pub const CONS: u64 = 8; pub const INT: u64 = 9; pub const STR: u64 = 10; pub const BYTES: u64 = 11;
}
pub mod typ {
    pub const VAR: u64 = 0; pub const CON: u64 = 1; pub const FUN: u64 = 2; pub const RECORD: u64 = 3;
    pub const TUPLE: u64 = 4; pub const UNIT: u64 = 5;
}
pub mod decl {
    pub const VALUE: u64 = 0; pub const UNION: u64 = 1; pub const ALIAS: u64 = 2; pub const TRAIT: u64 = 3;
    pub const IMPL: u64 = 4; pub const INFIX: u64 = 5;
}
pub mod assoc { pub const LEFT: u64 = 0; pub const RIGHT: u64 = 1; pub const NON: u64 = 2; }
pub mod option { pub const SOME: u64 = 0; pub const NONE: u64 = 1; }
```

Matching Nash (`core/src/Ast.nash`, little types, order is load-bearing;
the full listing is docs/macros.md "What the macro sees"):

```elm
type name = Local string | Raw string | Global modname string
type alias modname = { package : option string, name : string }
type kind = Big | Const | Term | Storable | Any | Arrow kind kind | KindVar string
type alias meta = { span : option span, typ : option typ }
type expr = Expr meta exprNode
type exprNode
    = IntLit int | StrLit string | BytesLit bytes | Var name | Op name | ListLit (cons expr)
    | Negate expr | BinOp name expr expr | Lambda (cons pattern) expr | Call expr (cons expr)
    | If expr expr expr | Let (cons def) expr | Case expr (cons arm) | Accessor string
    | Access expr string | Update name (cons fieldAssign) | Record (cons fieldAssign)
    | UnitLit | Tuple (cons expr) | MacroCall name (cons expr) | Comptime expr
type def = Define name (cons pattern) expr (option typ) | Destruct pattern expr
type patternNode
    = PAny | PVar name | PRecord (cons name) | PAlias pattern name | PUnit | PTuple (cons pattern)
    | PCtor name (cons pattern) | PList (cons pattern) | PCons pattern pattern
    | PInt int | PStr string | PBytes bytes
type typ = TVar string | TCon name (cons typ) | TFun typ typ | TRecord (cons field) | TTuple (cons typ) | TUnit
type decl = Value {..} | Union {..} | Alias {..} | Trait traitDef | Impl implDef | Infix {..}
type assoc = LeftAssoc | RightAssoc | NonAssoc
```

Runtime layout (representation.md "Term types"): a constructor is
`constr i [fields]`, a record alias (`meta`, `modname`, `span`, `arm`,
...) is `constr 0 [fields]` in field order, a labeled constructor
(`Value {..}`) is `constr i [fields]` flat, and `cons` cells are
`constr 0 []` / `constr 1 [x, xs]`.

Reifier:

```rust
// crates/nash-macro/src/reify.rs
use nash_ast::{Expr as CanExpr, Pattern as CanPattern, Type as CanType, ModuleName, QualifiedName};
use nash_plutus::{arena::Arena, binder::DeBruijn, constant::{Constant, Integer}, term::Term};
use nash_solve::NodeTypeMap;
use nash_constrain::NodeId;

type T<'p> = &'p Term<'p, DeBruijn>;

pub struct Reifier<'p, 'a> {
    arena: &'p Arena,
    types: &'a NodeTypeMap<'a>,
    kinds: &'a dyn Fn(&CanType<'a>) -> nash_constrain::Kind,
}

impl<'p, 'a> Reifier<'p, 'a> {
    pub fn decl_use(&self, target: &'a DeclSnapshot<'a>, attr: &'a nash_ast::Attribute<'a>) -> (T<'p>, T<'p>) {
        let decl = self.decl(target);
        let args = self.cons_list(attr.arguments.iter().map(|a| self.surface_expr(a)));
        (decl, args)
    }

    pub fn expr_use(&self, arguments: &'a [&'a Located<CanExpr<'a>>]) -> T<'p> {
        self.cons_list(arguments.iter().map(|a| self.expr(a)))
    }

    pub fn expr(&self, e: &'a Located<CanExpr<'a>>) -> T<'p> {
        let typ = self.types.get(&NodeId::of(e)).map(|t| self.typ(t));
        let meta = self.meta(Some(e.region), typ);
        let node = match &e.value {
            CanExpr::Int(n) => self.constr(tags::expr::INT, &[self.int(*n)]),
            CanExpr::Str(s) => self.constr(tags::expr::STR, &[self.str(s)]),
            CanExpr::VarLocal(name) => self.constr(tags::expr::VAR, &[self.raw(name)]),
            CanExpr::VarTopLevel(q) => self.constr(tags::expr::VAR, &[self.global(*q)]),
            CanExpr::VarForeign { reference, .. } => self.constr(tags::expr::VAR, &[self.global(*reference)]),
            CanExpr::VarConstructor { reference, .. } => self.constr(
                tags::expr::VAR,
                &[self.global(QualifiedName { home: reference.home, name: reference.name })],
            ),
            CanExpr::VarOperator { reference, .. } => self.constr(tags::expr::OP, &[self.global(*reference)]),
            CanExpr::List(items) => self.constr(tags::expr::LIST, &[self.cons_list(items.iter().map(|i| self.expr(i)))]),
            CanExpr::Negate(inner) => self.constr(tags::expr::NEGATE, &[self.expr(inner)]),
            CanExpr::Binop { reference, left, right, .. } => self.constr(
                tags::expr::BINOP,
                &[self.global(*reference), self.expr(left), self.expr(right)],
            ),
            CanExpr::Lambda { parameters, body } => self.constr(
                tags::expr::LAMBDA,
                &[self.cons_list(parameters.iter().map(|p| self.pattern(p))), self.expr(body)],
            ),
            CanExpr::Call { function, arguments } => self.constr(
                tags::expr::CALL,
                &[self.expr(function), self.cons_list(arguments.iter().map(|a| self.expr(a)))],
            ),
            CanExpr::If { branches, final_else } => self.if_chain(branches, final_else),
            CanExpr::Let { definition, body } => self.constr(
                tags::expr::LET,
                &[self.cons_list([self.def(definition)]), self.expr(body)],
            ),
            CanExpr::LetRec { definitions, body } => self.constr(
                tags::expr::LET,
                &[self.cons_list(definitions.iter().map(|d| self.def(d))), self.expr(body)],
            ),
            CanExpr::LetDestruct { pattern, value, body } => self.constr(
                tags::expr::LET,
                &[self.cons_list([self.constr(tags::def::DESTRUCT, &[self.pattern(pattern), self.expr(value)])]), self.expr(body)],
            ),
            CanExpr::Case { scrutinee, branches } => self.constr(
                tags::expr::CASE,
                &[self.expr(scrutinee), self.cons_list(branches.iter().map(|b| self.arm(b)))],
            ),
            CanExpr::Accessor(field) => self.constr(tags::expr::ACCESSOR, &[self.str(field)]),
            CanExpr::Access { record, field } => self.constr(tags::expr::ACCESS, &[self.expr(record), self.str(field.value)]),
            CanExpr::Update { record, fields, .. } => self.constr(
                tags::expr::UPDATE,
                &[self.raw(record), self.cons_list(fields.iter().map(|f| self.field_assign(f.field.value, f.value)))],
            ),
            CanExpr::Record(fields) => self.constr(
                tags::expr::RECORD,
                &[self.cons_list(fields.iter().map(|f| self.field_assign(f.field.value, f.value)))],
            ),
            CanExpr::Unit => self.constr(tags::expr::UNIT, &[]),
            CanExpr::Tuple { first, second, rest } => self.constr(
                tags::expr::TUPLE,
                &[self.cons_list([first, second].into_iter().chain(rest.iter().copied()).map(|e| self.expr(e)))],
            ),
            CanExpr::MacroCall { reference, arguments } => self.constr(
                tags::expr::MACRO_CALL,
                &[self.global(*reference), self.cons_list(arguments.iter().map(|a| self.expr(a)))],
            ),
            CanExpr::Comptime(inner) => self.constr(tags::expr::COMPTIME, &[self.expr(inner)]),
            CanExpr::Hole => unreachable!("holes never survive to reification: strict pass ran"),
        };
        self.constr(0, &[meta, node])   // `Expr meta node`
    }

    fn if_chain(&self, branches: &'a [nash_ast::IfBranch<'a>], final_else: &'a Located<CanExpr<'a>>) -> T<'p> {
        let (first, rest) = branches.split_first().expect("If has at least one branch");
        let else_ = if rest.is_empty() {
            self.expr(final_else)
        } else {
            self.constr(0, &[self.meta(None, None), self.if_chain(rest, final_else)])
        };
        self.constr(tags::expr::IF, &[self.expr(first.condition), self.expr(first.then_branch), else_])
    }

    // --- names ---

    fn raw(&self, s: &str) -> T<'p> {
        self.constr(tags::name::RAW, &[self.str(s)])
    }

    fn global(&self, q: QualifiedName<'a>) -> T<'p> {
        self.constr(tags::name::GLOBAL, &[self.module(q.home), self.str(q.name)])
    }

    /// `modname` is a little record alias: `constr 0 [package, name]`.
    fn module(&self, m: ModuleName<'a>) -> T<'p> {
        let package = match m.package {
            Some(p) => self.some(self.str(&format!("{}/{}", p.author, p.project))),
            None => self.none(),
        };
        self.record(&[package, self.str(m.name)])
    }

    // --- primitives ---

    fn meta(&self, span: Option<Region>, typ: Option<T<'p>>) -> T<'p> {
        let span = match span {
            Some(r) => self.some(self.record(&[
                self.int(r.start.line as i128), self.int(r.start.column as i128),
                self.int(r.end.line as i128), self.int(r.end.column as i128),
            ])),
            None => self.none(),
        };
        let typ = typ.map_or_else(|| self.none(), |t| self.some(t));
        self.record(&[span, typ])
    }

    fn constr(&self, tag: u64, fields: &[T<'p>]) -> T<'p> {
        Term::constr(self.arena, tag as usize, self.arena.alloc_slice_copy(fields))
    }
    /// Little record alias: `constr 0 [fields]` in declaration order.
    fn record(&self, fields: &[T<'p>]) -> T<'p> { self.constr(0, fields) }
    /// `Cons x0 (Cons x1 (... Nil))`, built from the tail.
    fn cons_list(&self, items: impl IntoIterator<Item = T<'p>>) -> T<'p> {
        let items: Vec<T<'p>> = items.into_iter().collect();
        items.iter().rev().fold(self.constr(tags::cons::NIL as u64, &[]), |tail, x| {
            self.constr(tags::cons::CONS as u64, &[x, tail])
        })
    }
    fn some(&self, t: T<'p>) -> T<'p> { self.constr(tags::option::SOME, &[t]) }
    fn none(&self) -> T<'p> { self.constr(tags::option::NONE, &[]) }
    fn int(&self, n: i128) -> T<'p> {
        Term::constant(self.arena, self.arena.alloc(Constant::Integer(self.arena.alloc(Integer::from(n)))))
    }
    fn str(&self, s: &str) -> T<'p> {
        Term::constant(self.arena, self.arena.alloc(Constant::String(self.arena.alloc_str(s))))
    }
    fn bytes(&self, b: &[u8]) -> T<'p> {
        Term::constant(self.arena, self.arena.alloc(Constant::ByteString(self.arena.alloc_slice_copy(b))))
    }
}
```

`DeclSnapshot` is the canonical view of one decorated declaration
(`Def` + annotation for values, `Union`/`Alias` with the kind from
plans/02, `Trait`/`Impl` from plans/03). `surface_expr` reifies attribute
arguments from `nash_source::Expr` with `typ = None` and `Raw` names.
`typ` reifies `CanType` with `Alias` expanded through `AliasType::Filled`.

Check `Arena` has `alloc_slice_copy` and `alloc_str`
(`crates/nash-plutus/src/arena.rs`); add them if not. `Term::constr` and
`Term::constant` exist (`term.rs:70-76`).

**Elm/Aiken reference**

`crates/nash-plutus/src/term.rs` `Term::constr` and the `Value::Constr`
arm of `machine/discharge.rs` are the two directions of the same layout.
Aiken does not reify its AST; Elm has none.

**Tests** (`crates/nash-macro/src/reify.rs` tests)

Structural snapshots of the `Term` via `format!("{:?}")` (round trips are
chunk 5):

- `reify_int_literal`: `1` → `Constr 0 [Constr 0 [Some (Constr 0 [ints..]), Some (Constr 1 [Global Builtin "int", Nil])], Constr 0 [(con integer 1)]]`.
- `reify_lambda_local_binder`: `\x -> x` → binder and use both `Raw "x"`; the parameter list is `Cons (..) Nil`.
- `reify_foreign_var`: `List.map` → `Global {package: Some "nash/core", name: "List"} "map"`.
- `reify_if_chain`: `if a then 1 else if b then 2 else 3` → nested `If`.
- `reify_union_decl`: `type Foo 'a = A 'a | B` → `Union` with `kind = Some Big`.
- `reify_no_data`: no `Constant::Data` anywhere in the tree for any input (walk and assert).

**Done when** the crate builds in the workspace and the tests pass.

---

## Chunk 5: unreifier: CEK result `Term` → `nash_source`

**Files**

- `crates/nash-macro/src/unreify.rs`
- `crates/nash-macro/src/error.rs`

**Change**

Walk the macro's result back into surface AST allocated in the module
bump. The CEK machine returns the result `Value` already discharged to a
`Term` (`EvalResult.term`, via `machine/discharge.rs` `value_as_term`), so
the walker reads `Term::Constr { tag, fields }` nodes and
`Term::Constant` leaves and nothing else: a `Term::Lambda`, `Delay`,
`Builtin`/`Apply`/`Force` (a partially applied builtin), or a constant
where a constructor is expected is `UnreifyError::NotConstr`, reported as
`MacroBadOutput`. Every node gets the invocation region. Names decode per
docs/macros.md: `Raw` → plain; `Global` → `VarGlobal`/`CtorGlobal`/
`TypeGlobal`; `Local` → the text with a trailing `·` (U+00B7), renamed by
chunk 6.

**Code**

```rust
// crates/nash-macro/src/error.rs
#[derive(Debug)]
pub enum UnreifyError {
    /// `path` is the chain of constructor names from the root, for the diagnostic.
    BadShape { path: Vec<&'static str>, found: String },
    /// A node position held something that is not a `constr` tree.
    NotConstr { path: Vec<&'static str>, found: &'static str },   // "lambda" | "delay" | "builtin" | "constant"
    GlobalBinder { name: String },
    TupleTooShort { len: usize },
}

// crates/nash-macro/src/unreify.rs
type T<'p> = &'p Term<'p, DeBruijn>;

pub struct Unreifier<'a> {
    bump: &'a Bump,
    site: Region,
    path: Vec<&'static str>,
}

pub const LOCAL_MARK: char = '\u{00B7}';

impl<'a> Unreifier<'a> {
    pub fn new(bump: &'a Bump, site: Region) -> Self { Self { bump, site, path: Vec::new() } }

    pub fn decls(&mut self, t: T<'_>) -> Result<Vec<DecodedDecl<'a>>, UnreifyError> {
        self.cons_of(t, "cons decl", |s, x| s.decl(x))
    }

    /// `Term::Constr` or an error naming what was found instead.
    fn constr<'p>(&mut self, t: T<'p>, what: &'static str) -> Result<(usize, &'p [T<'p>]), UnreifyError> {
        self.path.push(what);
        match t {
            Term::Constr { tag, fields } => Ok((*tag, fields)),
            Term::Lambda { .. } => Err(self.not_constr("lambda")),
            Term::Delay(_) => Err(self.not_constr("delay")),
            Term::Builtin(_) | Term::Apply { .. } | Term::Force(_) => Err(self.not_constr("builtin")),
            Term::Constant(_) => Err(self.not_constr("constant")),
            // a discharged value is closed and fully evaluated
            Term::Var(_) | Term::Case { .. } | Term::Error => unreachable!("not a value"),
        }
    }

    /// Walk a `Cons x rest` chain to `Nil`.
    fn cons_of<'p, X>(&mut self, mut t: T<'p>, what: &'static str, f: impl Fn(&mut Self, T<'p>) -> Result<X, UnreifyError>) -> Result<Vec<X>, UnreifyError> {
        let mut out = Vec::new();
        loop {
            match self.constr(t, what)? {
                (tags::cons::NIL, []) => return Ok(out),
                (tags::cons::CONS, [x, rest]) => { out.push(f(self, x)?); t = rest; }
                (tag, fields) => return Err(self.bad_shape(format!("cons tag {tag} with {} fields", fields.len()))),
            }
        }
    }

    fn str(&mut self, t: T<'_>) -> Result<&'a str, UnreifyError> {
        match t {
            Term::Constant(Constant::String(s)) => Ok(self.bump.alloc_str(s)),
            other => Err(self.bad_shape(format!("expected a string constant, found {other:?}"))),
        }
    }
    // `int` (`Constant::Integer`), `bytes` (`Constant::ByteString`), and
    // `option` (`SOME [x]` / `NONE []`) follow the same shape.

    pub fn expr(&mut self, t: T<'_>) -> Result<&'a Located<SourceExpr<'a>>, UnreifyError> {
        let [_meta, node] = self.fields(t, 0, "expr")? else { unreachable!() };
        let (tag, fields) = self.constr(node, "exprNode")?;
        let value = match (tag, fields) {
            (tags::expr::INT, [n]) => SourceExpr::Int(self.int(n)?),
            (tags::expr::STR, [s]) => SourceExpr::Str(self.str(s)?),
            (tags::expr::VAR, [name]) => self.var(name)?,
            (tags::expr::OP, [name]) => SourceExpr::Op(self.raw_or_global_text(name)?),
            (tags::expr::LIST, [items]) => SourceExpr::List(self.exprs(items)?),
            (tags::expr::NEGATE, [e]) => SourceExpr::Negate(self.expr(e)?),
            (tags::expr::BINOP, [op, l, r]) => self.binop(op, l, r)?,
            (tags::expr::LAMBDA, [params, body]) => SourceExpr::Lambda {
                parameters: self.patterns(params)?,
                body: self.expr(body)?,
            },
            (tags::expr::CALL, [f, args]) => SourceExpr::Call {
                function: self.expr(f)?,
                arguments: self.exprs(args)?,
            },
            (tags::expr::IF, [c, t, e]) => SourceExpr::If {
                branches: self.bump.alloc_slice_copy(&[self.bump.alloc(IfBranch {
                    condition: self.expr(c)?,
                    then_branch: self.expr(t)?,
                }) as &_]),
                final_else: self.expr(e)?,
            },
            (tags::expr::LET, [defs, body]) => SourceExpr::Let {
                defs: self.cons_of_refs(defs, "cons def", |s, x| s.def(x))?,
                body: self.expr(body)?,
            },
            (tags::expr::CASE, [scrut, arms]) => SourceExpr::Case {
                scrutinee: self.expr(scrut)?,
                arms: self.cons_of_refs(arms, "cons arm", |s, x| s.arm(x))?,
            },
            (tags::expr::UNIT, []) => SourceExpr::Unit,
            (tags::expr::TUPLE, [items]) => {
                let items = self.exprs(items)?;
                let [first, second, rest @ ..] = items else {
                    return Err(UnreifyError::TupleTooShort { len: items.len() });
                };
                SourceExpr::Tuple { first, second, rest }
            }
            (tags::expr::MACRO_CALL, [name, args]) => self.macro_call(name, args)?,
            (tags::expr::COMPTIME, [e]) => SourceExpr::Comptime(self.expr(e)?),
            // ...ACCESSOR, ACCESS, UPDATE, RECORD, BYTES...
            (tag, fields) => return Err(self.bad_shape(format!("exprNode tag {tag} with {} fields", fields.len()))),
        };
        Ok(self.at(value))
    }

    /// `Var` with the three name modes.
    fn var(&mut self, name: T<'_>) -> Result<SourceExpr<'a>, UnreifyError> {
        Ok(match self.name(name)? {
            Name::Local(s) => SourceExpr::Var { kind: var_type(s), name: self.local(s) },
            Name::Raw(s) => SourceExpr::Var { kind: var_type(s), name: self.str_alloc(s) },
            Name::Global { package, module, name } => SourceExpr::VarGlobal { package, module, name },
        })
    }

    fn local(&self, s: &str) -> &'a str {
        let mut owned = String::with_capacity(s.len() + 2);
        owned.push_str(s);
        owned.push(LOCAL_MARK);
        self.bump.alloc_str(&owned)
    }

    /// `BinOp fn l r` becomes a two-operand `BinOps` chain using the operator
    /// symbol the invocation module will see: for a `Global` function name the
    /// decoder emits a `Call (VarGlobal fn) [l, r]` instead, because operator
    /// symbols are not stable across modules.
    fn binop(&mut self, op: T<'_>, l: T<'_>, r: T<'_>) -> Result<SourceExpr<'a>, UnreifyError> {
        let function = match self.name(op)? {
            Name::Global { package, module, name } => self.at(SourceExpr::VarGlobal { package, module, name }),
            Name::Raw(s) | Name::Local(s) => self.at(SourceExpr::Var { kind: VarType::LowVar, name: self.str_alloc(s) }),
        };
        Ok(SourceExpr::Call {
            function,
            arguments: self.bump.alloc_slice_copy(&[self.expr(l)?, self.expr(r)?]),
        })
    }

    fn at<T>(&self, value: T) -> &'a Located<T> {
        self.bump.alloc(Located::at(self.site, value))
    }

    fn name(&mut self, t: T<'_>) -> Result<Name<'a>, UnreifyError> {
        let (tag, fields) = self.constr(t, "name")?;
        Ok(match (tag, fields) {
            (tags::name::LOCAL, [s]) => Name::Local(self.str(s)?),
            (tags::name::RAW, [s]) => Name::Raw(self.str(s)?),
            (tags::name::GLOBAL, [m, s]) => {
                // `modname` record alias: constr 0 [package, name]
                let [package, module] = self.fields(m, 0, "modname")? else { unreachable!() };
                Name::Global {
                    package: self.option(package, |s, x| s.str(x))?,
                    module: self.str(module)?,
                    name: self.str(s)?,
                }
            }
            _ => return Err(self.bad_shape("name")),
        })
    }
}

enum Name<'a> {
    Local(&'a str),
    Raw(&'a str),
    Global { package: Option<&'a str>, module: &'a str, name: &'a str },
}

pub enum DecodedDecl<'a> {
    Value(&'a Located<Value<'a>>),
    Union(&'a Located<Union<'a>>),
    Alias(&'a Located<Alias<'a>>),
    Trait(&'a Located<Trait<'a>>),
    Impl(&'a Located<Impl<'a>>),
    Infix(&'a Located<Infix<'a>>),
}
```

Binder positions (`PVar`, `Define` name, lambda params via `PVar`,
`Union.name`, `ctor.name`, `Value.name`) reject `Global` with
`UnreifyError::GlobalBinder`.

Strings are native `Constant::String` (`&str`), so there is no UTF-8
check; ints are `Constant::Integer` (`BigInt`, converted with a range
check for spans and precedences); bytes are `Constant::ByteString`.

**Elm/Aiken reference**

`crates/nash-plutus/src/machine/discharge.rs` `value_as_term` is what
turns the CEK `Value` (with `Value::Constr(tag, fields)`) into the `Term`
this walker reads; `EvalResult.term` already carries it. Aiken's
`test_framework.rs` `Prng::from_result` is the analogous read-back of a
structured result, over `PlutusData` rather than `constr`.

**Tests** (`crates/nash-macro/src/unreify.rs`, round trips with chunk 4;
no CEK evaluation is needed because both sides are `Term`)

- `roundtrip_expr`: parse → canonicalize → reify → unreify → print equals
  the printed original for `\x -> if x then 1 else f x 2`, `case m of Some y -> y; None -> 0`,
  `let a = 1 in a + a`, `{ r | f = 1 }`, `(a, b, c)`.
- `unreify_local_marks`: `Lambda (Cons (PVar (Local "x")) Nil) (Var (Local "x"))` decodes to `\x· -> x·`.
- `unreify_global_binder_rejected`.
- `unreify_bad_shape_reports_path`: tag 99 at `expr/Call/arguments[1]`.
- `unreify_lambda_rejected`: a `Term::Lambda` in an argument position is `NotConstr { found: "lambda" }`.

**Done when** round trips are byte-identical under the debug printer of
chunk 12 (written first as a test helper, promoted in chunk 12).

---

## Chunk 6: hygiene (gensym pass)

**Files**

- `crates/nash-macro/src/hygiene.rs`

**Change**

Rename every `x·` name produced by the unreifier to `x·{round}_{use}` so
that each expansion's locals are distinct from user names and from other
expansions. No scope analysis is needed: all `Local "x"` in one
expansion denote the same binder family by construction (docs/macros.md).

**Code**

```rust
pub struct Gensym {
    pub round: u32,
    pub use_index: u32,
}

impl Gensym {
    pub fn rename<'a>(&self, bump: &'a Bump, decls: Vec<DecodedDecl<'a>>) -> Vec<DecodedDecl<'a>> { /* rebuild */ }
    pub fn rename_expr<'a>(&self, bump: &'a Bump, e: &'a Located<SourceExpr<'a>>) -> &'a Located<SourceExpr<'a>> { /* rebuild */ }

    fn name<'a>(&self, bump: &'a Bump, s: &'a str) -> &'a str {
        match s.strip_suffix(LOCAL_MARK) {
            Some(base) => bump.alloc_str(&format!("{base}{LOCAL_MARK}{}_{}", self.round, self.use_index)),
            None => s,
        }
    }
}
```

The walk is a full surface-AST rebuild (the arena AST is immutable);
it touches `Expr::Var`, `Pattern::Var`, `Pattern::Alias`, `Def::Define`
names, `Value.name`, `Union.name`, `Ctor.name`, `Alias.name`, and
record field names in `Pattern::Record` (a `Local` field name is an
error: fields are not binders — `UnreifyError::LocalField`).

Diagnostics: `nash-report` renders a `NotFoundVar` whose name contains
`LOCAL_MARK` as `MacroUnboundLocal` (chunk 11).

**Elm/Aiken reference**

None. Conceptually Racket's "marks", simplified to one mark per expansion.

**Tests** (`crates/nash-macro/src/hygiene.rs`)

- `renames_binder_and_use_consistently`: `\x· -> x·` → `\x·1_0 -> x·1_0`.
- `leaves_raw_alone`: `\x -> x·` → `\x -> x·1_0`.
- `two_uses_differ`: use 0 and use 1 of the same output give different names.
- Expansion-level test (chunk 12): a macro `let x = 1 in ~body` invoked with
  `body = x + 1` where the user's `x = 10` yields `11`, not `2`.

**Done when** tests pass and the expansion test is green after chunk 8.

---

## Chunk 7: compile macros to standalone programs

**Files**

- `crates/nash-codegen/src/macro_.rs` (new)
- `crates/nash-codegen/src/lib.rs`
- `crates/nash-macro/src/run.rs` (new)

**Change**

`nash-codegen` produces a UPLC program per macro. `nash-macro::run`
applies it to the reified input terms under a budget and returns the
result `Term` (the discharged value) or the failure message.

**Code**

```rust
// crates/nash-codegen/src/macro_.rs
/// Compile one macro definition to a closed program: the macro's Core
/// term with all dependencies inlined. Arguments are little `Ast` values,
/// i.e. `constr` terms applied directly, so no wrapper conversion is needed.
pub fn compile_macro<'p>(
    arena: &'p Arena,
    set: &ModuleSet<'_>,
    module: ModuleName<'_>,
    macro_def: &MacroDef<'_>,
) -> &'p Program<'p, DeBruijn> {
    let term = lower_value(arena, set, QualifiedName { home: module, name: macro_def.name.value });
    Program::new(arena, Version::plutus_v3(arena), term)
}

/// Serialized so the driver can keep it across module arenas.
pub fn encode_macro(program: &Program<'_, DeBruijn>) -> Vec<u8> {
    nash_plutus::flat::encode(program)
}
```

```rust
// crates/nash-macro/src/run.rs
pub struct MacroRun<'p> {
    /// The discharged result value (`EvalResult.term`); chunk 5 walks it.
    pub output: &'p Term<'p, DeBruijn>,
    pub budget: ExBudget,
    pub logs: Vec<String>,
}

pub enum MacroRunError {
    /// `message` is the last trace line, if any; `machine` the CEK error text.
    Failed { message: Option<String>, machine: String, logs: Vec<String> },
}

pub fn run_decl_macro<'p>(
    arena: &'p Arena,
    program: &[u8],
    decl: &'p Term<'p, DeBruijn>,
    args: &'p Term<'p, DeBruijn>,
    budget: ExBudget,
) -> Result<MacroRun<'p>, MacroRunError> {
    let program = nash_plutus::flat::decode::<DeBruijn>(arena, program).expect("macro program was encoded by this compiler");
    let applied = program.apply(arena, decl).apply(arena, args);   // Program::apply wraps Term::Apply
    finish(applied.eval_version_budget(arena, PlutusVersion::V3, budget))
}

pub fn run_expr_macro<'p>(arena: &'p Arena, program: &[u8], args: &'p Term<'p, DeBruijn>, budget: ExBudget) -> Result<MacroRun<'p>, MacroRunError> {
    let program = nash_plutus::flat::decode::<DeBruijn>(arena, program).expect("macro program was encoded by this compiler");
    finish(program.apply(arena, args).eval_version_budget(arena, PlutusVersion::V3, budget))
}

fn finish<'p>(result: EvalResult<'p, DeBruijn>) -> Result<MacroRun<'p>, MacroRunError> {
    let logs = result.info.logs;
    match result.term {
        Ok(term) => Ok(MacroRun { output: term, budget: result.info.consumed_budget, logs }),
        Err(e) => Err(MacroRunError::Failed { message: logs.last().cloned(), machine: format!("{e}"), logs }),
    }
}
```

The argument terms are closed `constr`/constant trees, so applying them
to a De Bruijn program needs no index shifting. Whether the result has
the right shape is chunk 5's job; `run` does not inspect it.

Match the real `Term`/`Constant` variant names in
`crates/nash-plutus/src/term.rs:9` and `constant.rs`.

**Elm/Aiken reference**

Aiken `crates/aiken-lang/src/gen_uplc.rs` `generate_raw` (compile one
definition with its dependencies to a `Program`), `crates/aiken-project/src/lib.rs`
`run_tests` (apply, eval with budget, keep logs).

**Tests** (`crates/nash-codegen/tests/macro_.rs`)

- Compile `macro id : cons Ast.expr -> Ast.expr; id args = case args of Cons e _ -> e; Nil -> fail "no args"` and run it on the reified `Cons (Ast.int 1) Nil`; the output `Term` equals the reified `Ast.int 1`.
- A macro that `fail "boom"`s returns `Failed { message: Some("boom") }`.
- Budget exhaustion returns `Failed` with the machine's budget error.

**Done when** the tests pass against plans/07's `lower_value`.

---

## Chunk 8: expansion loop in the driver

**Files**

- `crates/nash-driver/src/compile.rs`
- `crates/nash-driver/src/expand.rs` (new)
- `crates/nash-driver/src/splice.rs` (new)
- `crates/nash-config/src/config.rs` (`macroExpansionLimit`, `macroBudget`)

**Change**

`compile_module` (`crates/nash-driver/src/compile.rs:145`) becomes the
loop from docs/macros.md. Macro programs of already-compiled modules are
kept in a build-wide map next to `interfaces`.

**Code**

```rust
// crates/nash-driver/src/compile.rs
struct BuildState<'s> {
    store: &'s Bump,
    interfaces: BTreeMap<&'s str, Interface<'s>>,
    /// module name -> macro name -> flat-encoded program
    macros: BTreeMap<&'s str, BTreeMap<&'s str, Vec<u8>>>,
    limits: ExpansionLimits,
}

#[derive(Clone, Copy)]
pub struct ExpansionLimits {
    pub rounds: u32,
    pub budget: ExBudget,
}

fn compile_module<'s>(uri: &Url, source: &str, state: &BuildState<'s>) -> Result<Compiled<'s>, Vec<Diagnostic>> {
    let bump = Bump::new();
    let src: &str = bump.alloc_str(source);
    let mut parser = nash_parse::Parser::new(&bump, src.as_bytes());
    let mut module = parser.module().map_err(|e| vec![syntax(e)])?;

    let (can, node_types) = expand::expand(&bump, &mut module, state)?;   // strict result
    let interface = nash_can::from_module(&bump, &can.module, &annotations);
    let stored = nash_can::deep_copy_interface(state.store, &interface);
    let macros = codegen_macros(&bump, &can.module);                   // chunk 7
    Ok(Compiled { interface: stored, macros, decl_count: count_decls(can.module.decls) })
}
```

```rust
// crates/nash-driver/src/expand.rs
pub fn expand<'a, 's>(
    bump: &'a Bump,
    module: &mut SourceModule<'a>,
    state: &BuildState<'s>,
) -> Result<(CanResult<'a>, NodeTypeMap<'a>, Annotations<'a>), Vec<Diagnostic>> {
    // `solve` wraps plans/03's Solver API:
    // `nash_solve::run(bump, &mut uf, &constraint, &can.tables, &can.fields, mode)`
    // with `mode: nash_can::Mode`, the same value `canonicalize` put on `Context.mode`.
    for round in 0..=state.limits.rounds {
        let can = canonicalize(bump, Mode::Lenient, module, state)?;
        let (annotations, node_types) = solve(bump, &can.module, Mode::Lenient)?;

        if can.macro_uses.is_empty() {
            let can = canonicalize(bump, Mode::Strict, module, state)?;
            let (annotations, node_types) = solve(bump, &can.module, Mode::Strict)?;
            return Ok((can, node_types, annotations));
        }
        if round == state.limits.rounds {
            return Err(vec![Diagnostic::macro_expansion_limit(&can.macro_uses)]);
        }

        let arena = Arena::new();
        let reifier = Reifier::new(&arena, &node_types, kind_of);
        let mut outputs: Vec<Output<'a>> = Vec::with_capacity(can.macro_uses.len());
        for (use_index, use_) in can.macro_uses.iter().enumerate() {
            let gensym = Gensym { round, use_index: use_index as u32 };
            let program = state.macro_program(use_.reference())?;
            let output = match use_ {
                MacroUse::Decl { target, attribute } => {
                    let (decl, args) = reifier.decl_use(&can.module.snapshot(*target), attribute);
                    let run = run_decl_macro(&arena, program, decl, args, state.limits.budget)
                        .map_err(|e| Diagnostic::macro_failed(attribute.region, use_.reference(), e))?;
                    let decls = Unreifier::new(bump, attribute.region).decls(run.output)
                        .map_err(|e| Diagnostic::macro_bad_output(attribute.region, use_.reference(), e))?;
                    Output::Decls { target: *target, decls: gensym.rename(bump, decls) }
                }
                MacroUse::Expr { region, arguments, .. } => {
                    let args = reifier.expr_use(arguments);
                    let run = run_expr_macro(&arena, program, args, state.limits.budget)
                        .map_err(|e| Diagnostic::macro_failed(*region, use_.reference(), e))?;
                    let expr = Unreifier::new(bump, *region).expr(run.output)
                        .map_err(|e| Diagnostic::macro_bad_output(*region, use_.reference(), e))?;
                    Output::Expr { region: *region, expr: gensym.rename_expr(bump, expr) }
                }
            };
            outputs.push(output);
        }
        *module = splice::splice(bump, module, outputs);
    }
    unreachable!("loop returns or errors")
}
```

`splice::splice` rebuilds the `SourceModule`: for `Output::Decls` it
replaces the target declaration with the decoded list (attributes already
consumed are dropped; remaining attributes on the original are kept on
its replacement if the original is the first output element, which is
how `derive` chains); for `Output::Expr` it rebuilds the enclosing value
body, replacing the `MacroCall` node whose region matches. Regions are
unique per invocation because the parser assigned them from source; nodes
spliced in earlier rounds all carry the site region but are never
`MacroCall`s themselves unless the macro emitted one, and then the site
region is reused, which is fine because the walk replaces the first match
in pre-order and each round re-collects uses.

`state.macro_program(reference)` finds the flat bytes for
`reference.home.name` / `reference.name`; a miss is an internal error
(the interface said it was a macro, so the module compiled).

Config (`crates/nash-config/src/config.rs`, `Application` and `Package`):

```rust
/// Maximum macro expansion rounds per module (default 32).
#[serde(default = "default_macro_expansion_limit")]
pub macro_expansion_limit: u32,
/// CEK budget per macro run, `{ "cpu": N, "mem": M }` (default 10x mainnet tx budget).
#[serde(default)]
pub macro_budget: Option<Budget>,
```

**Elm/Aiken reference**

`elm/compiler/src/Compile.hs` `compile` (the per-module pipeline this
loop wraps). Aiken has no macros; its `aiken-project/src/lib.rs`
`compile` shows keeping per-module build artifacts across modules.

**Tests** (`crates/nash-driver/src/compile.rs` tests, in-memory sources)

- `test_expression_macro_expands`: module `M` with `macro twice : cons Ast.expr -> Ast.expr; twice args = case args of Cons a _ -> quote (~a + ~a); Nil -> fail "twice: one argument"`; module `Main` with `main = twice!(1)`; build succeeds and `Main`'s interface says `main : int`.
- `test_decl_macro_appends`: a decl macro that returns `[decl, decl2]`; `Main` exports both.
- `test_same_module_use_rejected`.
- `test_expansion_limit`: a macro that emits itself; error names the site.
- `test_macro_failure_message`: `fail "no"` → diagnostic text contains `no`.
- `test_lenient_then_strict`: `@derive(Eq)` on `Foo` and `a == b` on `Foo` in the same module builds.

**Done when** the driver tests pass and `nash check` on a project with no
macros behaves exactly as before.

---

## Chunk 9: comptime

**Files**

- `crates/nash-can/src/expression.rs` (closed-term check)
- `crates/nash-constrain/src/kind.rs` (plans/02; add `comptime` kind check)
- `crates/nash-codegen/src/comptime.rs` (new)
- `crates/nash-ir/src/lib.rs` (`Core::Comptime`)

**Change**

1. Canonicalize `Expr::Comptime`; reject free locals.
2. After solving, check the kind of the node's type is `Big` or `Const`.
3. In lowering, compile the body to a program, run it, replace with
   `Core::Const`.

**Code**

```rust
// crates/nash-can/src/expression.rs
SourceExpr::Comptime(inner) => {
    let inner = self.canonicalize(env, inner)?;
    if let Some(name) = first_free_local(&inner, env) {
        return Err(Error::ComptimeNotClosed { region, name });
    }
    Ok(CanExpr::Comptime(inner))
}

/// First local variable (lambda parameter, let binding, pattern variable)
/// referenced by `expr`. Top-level and foreign references are allowed.
fn first_free_local<'a>(expr: &Located<CanExpr<'a>>, env: &Env<'a>) -> Option<&'a str>
```

`first_free_local` walks the tree with a scope stack: locals bound
*inside* the comptime body are fine; `VarLocal` names not on the stack are
free. Top-level names canonicalize to `VarTopLevel` so they are not
`VarLocal` and never trip this.

```rust
// crates/nash-constrain/src/kind.rs (plans/02)
pub fn check_comptime<'a>(node_types: &NodeTypeMap<'a>, comptimes: &[(Region, NodeId)]) -> Vec<KindError<'a>> {
    comptimes.iter().filter_map(|(region, id)| {
        let typ = node_types[id];
        match kind_of(typ) {
            Kind::Big | Kind::Const => None,
            kind => Some(KindError::ComptimeNotConstant { region: *region, typ, kind }),
        }
    }).collect()
}
```

```rust
// crates/nash-ir/src/lib.rs
pub enum Core<'a> {
    // ...
    /// Evaluated during lowering; never reaches UPLC.
    Comptime { region: Region, body: &'a Core<'a> },
    Const(&'a Constant<'a>),
}

// crates/nash-codegen/src/comptime.rs
pub struct ComptimeFailed { pub region: Region, pub message: Option<String>, pub machine: String }

/// Replace every `Core::Comptime` with a `Core::Const`, bottom-up.
pub fn evaluate_comptimes<'a>(
    bump: &'a Bump,
    core: &'a Core<'a>,
    budget: ExBudget,
) -> Result<&'a Core<'a>, ComptimeFailed> {
    map_bottom_up(bump, core, &|node| match node {
        Core::Comptime { region, body } => {
            let arena = Arena::new();
            let program = Program::new(&arena, Version::plutus_v3(&arena), lower_closed(&arena, body));
            let result = program.eval_version_budget(&arena, PlutusVersion::V3, budget);
            match result.term {
                Ok(Term::Constant(c)) => Ok(Core::Const(copy_constant(bump, c))),
                Ok(other) => Err(ComptimeFailed { region: *region, message: None, machine: format!("not a constant: {other:?}") }),
                Err(e) => Err(ComptimeFailed { region: *region, message: result.info.logs.last().cloned(), machine: format!("{e}") }),
            }
        }
        other => Ok(other),
    })
}
```

`copy_constant` deep-copies a `nash_plutus::Constant` from the temporary
`Arena` into the module bump (the IR's own constant type per plans/07).
`lower_closed` is plans/07's closed-term lowering (the body has no free
locals by construction, so it is a closed Core term with its top-level
dependencies inlined).

**Elm/Aiken reference**

Aiken has no comptime. Its constant folding in
`crates/aiken-lang/src/gen_uplc/builder.rs` is the nearest analogue for
"evaluate then splice a constant". Elm: none.

**Tests**

- nash-can: `f x = comptime (x + 1)` → `ComptimeNotClosed { name: "x" }`; `f x = comptime (let y = 1 in y + 1)` ok.
- kinds: `comptime (\y -> y)` → `ComptimeNotConstant`; `comptime (Some 1)` with little `option` → `ComptimeNotConstant`; `comptime (Some 1 : Option Int)` ok.
- codegen: `x = comptime (List.foldl (+) 0 (List.range 1 100))` lowers to `Const(5050)`; a failing body reports `ComptimeFailed` with the trace.

**Done when** the tests pass and `nash build` of a module using `comptime`
emits the constant in the UPLC output.

---

## Chunk 10: `@derive` in `nash/core`

**Files**

- `core/src/Derive.nash` (new)
- `core/src/Ast.nash` (builders section)
- `crates/nash-driver/src/compile.rs` tests

**Change**

Write `derive` and the five derivations in Nash per docs/macros.md. `Eq`
is in the doc; `Ord` compares constructor index then fields; `Show`
renders `Ctor field1 field2` with parentheses for nested; `ToData` /
`FromData` require `kind == Some Big` and generate identity `toData` /
`fromData` plus a `validateData` built from `Data.Decode`.

**Code** (`core/src/Derive.nash`, excerpt beyond the doc's `deriveEq`)

```elm
deriveOrd : decl -> decl
deriveOrd decl =
    case decl of
        Union { name, params, ctors } ->
            let
                self = selfType name params
                a = Ast.name "a"
                b = Ast.name "b"

                -- (Ctor_i xs, Ctor_i ys) -> lexicographic; (Ctor_i _, Ctor_j _) -> compare i j
                sameCtor i ctor =
                    let
                        xs = binders "x" ctor
                        ys = binders "y" ctor
                        pairs = Cons.map2 (\x y -> quote (compare ~(Ast.var x) ~(Ast.var y))) xs ys
                    in
                    Ast.arm
                        (Ast.ptuple (Cons (ctorPat ctor xs) (Cons.singleton (ctorPat ctor ys))))
                        (Cons.foldr (\c acc -> quote (Ordering.then_ ~c ~acc)) (quote EQ) pairs)

                differentCtor =
                    Ast.arm Ast.wildcard
                        (quote (compare (~(indexOf a)) (~(indexOf b))))

                indexOf v =
                    Ast.case_ (Ast.var v)
                        (Cons.indexedMap (\i ctor -> Ast.arm (ctorPat ctor (wild ctor)) (Ast.int i)) ctors)
            in
            Ast.impl (Raw "Ord") (Cons.singleton self) (context "Ord" params)
                (Cons.singleton
                    (Ast.def (Raw "compare") (Cons (Ast.pvar a) (Cons.singleton (Ast.pvar b)))
                        (Ast.case_ (Ast.tuple (Cons (Ast.var a) (Cons.singleton (Ast.var b))))
                            (Cons.append (Cons.indexedMap sameCtor ctors) (Cons.singleton differentCtor)))))

        _ ->
            fail "derive(Ord): only `type` declarations can derive Ord"

deriveToData : decl -> decl
deriveToData decl =
    case decl of
        Union { name, params, kind } ->
            if kind /= Some Big then
                fail ("derive(ToData): " ++ Ast.nameText name ++ " is not a Big type")
            else
                Ast.impl (Raw "ToData") (Cons.singleton (selfType name params)) Nil
                    (Cons.singleton (Ast.def (Raw "toData") Nil (quote Builtin.identity)))

        Alias { name, params, kind, typ } ->
            ...

        _ ->
            fail "derive(ToData): only `type` and `type alias` declarations can derive ToData"
```

**Elm/Aiken reference**

Aiken derives nothing; `Eq` via `==` is builtin on all types. Haskell's
`deriving` semantics (GHC `Data.Deriving`) are the reference for the
`Ord` ordering rule (constructor order, then fields).

**Tests**

- `core/src/Derive.nash` `tests` block: cannot invoke `derive` in its own module, so the tests live in `core/tests/DeriveTests.nash` (a test-only module) with `@derive(Eq, Ord, Show)` on a three-constructor little type and a two-field Big record, and `prop`s for reflexivity, antisymmetry, `show` round trip via a hand-written parser stub.
- Driver expansion snapshot (chunk 12): `@derive(Eq)` on `type Foo = A int | B` prints the generated `impl`.

**Done when** `nash test core/` passes and the snapshot matches
docs/macros.md's description.

---

## Chunk 11: diagnostics

**Files**

- `crates/nash-report/src/macro_.rs` (new)
- `crates/nash-report/src/lib.rs`
- `crates/nash-can/src/error.rs`, `crates/nash-parse/src/error.rs` (variants from chunks 1, 2, 9)

**Change**

Render every macro/comptime error listed in docs/macros.md as a miette
`Diagnostic` with Elm-style prose. `MacroFailed` shows the macro name,
the site, and the message. `MacroBadOutput` shows the unreifier path and
the offending term (a lambda, a wrong tag, a wrong field count).
Strict-pass errors inside generated code include the generated
declaration pretty-printed by the chunk 12 printer (replaced by
`nash-fmt` when plans/13 lands).

**Code**

```rust
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum MacroDiagnostic {
    #[error("MACRO FAILED")]
    #[diagnostic(code(nash::macro_::failed))]
    Failed {
        #[source_code]
        src: NamedSource<String>,
        #[label("while expanding this")]
        site: SourceSpan,
        macro_name: String,
        #[help]
        message: String,
    },
    #[error("MACRO OUTPUT MALFORMED")]
    BadOutput { /* site, macro_name, path, found */ },
    #[error("MACRO EXPANSION LIMIT")]
    ExpansionLimit { /* sites: Vec<SourceSpan>, limit */ },
    #[error("UNBOUND HYGIENIC NAME")]
    UnboundLocal { /* site, name (without mark), macro_name */ },
    #[error("COMPTIME NOT CLOSED")]
    ComptimeNotClosed { /* region, name */ },
    #[error("COMPTIME NOT A CONSTANT")]
    ComptimeNotConstant { /* region, typ, kind */ },
    #[error("COMPTIME FAILED")]
    ComptimeFailed { /* region, message, machine */ },
}
```

`nash-report` keeps a `Vec<GeneratedDecl { site: Region, macro_name, text: String }>`
per module so that a `NotFoundVar`/type error whose region equals a site
region is rendered with the generated text appended.

**Elm/Aiken reference**

`elm/compiler/src/Reporting/Error/Canonicalize.hs` `toReport` for prose
style; `Reporting/Doc.hs` for the layout helpers `nash-report` already
ports.

**Tests** (`crates/nash-report/src/snapshots`)

One rendered snapshot per variant, using the in-memory driver from chunk 8.

**Done when** every error in docs/macros.md's table has a snapshot.

---

## Chunk 12: expansion snapshot tests and the debug printer

**Files**

- `crates/nash-source/src/print.rs` (new)
- `crates/nash-macro/tests/expand.rs` (new)
- `crates/nash-macro/tests/snapshots/`

**Change**

A deterministic surface-AST printer (`nash_source::print::module`) that
emits valid Nash. It is not layout-preserving and has no comments; it is
the test oracle until `nash-fmt` (plans/13) replaces it. Then a test
macro that builds an in-memory project, expands one module, and snapshots
the printed expanded module.

**Code**

```rust
// crates/nash-source/src/print.rs
pub fn module(m: &Module<'_>) -> String
pub fn expr(e: &Located<Expr<'_>>) -> String
pub fn pattern(p: &Located<Pattern<'_>>) -> String
pub fn typ(t: &Located<Type<'_>>) -> String
```

Rules: one declaration per blank-line-separated block, four-space
indentation, `let`/`case`/`if` always multi-line, calls single-line,
`VarGlobal` printed as `{package}:{module}.{name}` in braces so snapshots
show resolution, `x·1_0` printed as-is.

```rust
// crates/nash-macro/tests/expand.rs
macro_rules! assert_expansion_snapshot {
    ($name:literal, { $($module:literal => $source:literal),+ $(,)? }) => {{
        let expanded = support::expand_project(&[$(($module, indoc::indoc!($source))),+], $name);
        insta::assert_snapshot!(expanded);
    }};
}
```

`support::expand_project` runs chunk 8's loop with `Mode` hooks and
returns `print::module` of the named module after the final round.

**Tests**

- `expression_macro_quote`: `twice!(x)` → `Num.add x x` printed with global braces.
- `expression_macro_tail_literal`: `tail!(3, xs)` → three nested calls to
  the globally resolved `Builtin.tailList`; zero returns `xs`, a negative
  literal fails, and a variable count fails because macro arguments are AST
  rather than evaluated values. The test uses the partial builtin because
  safe `List.tail` returns an `option` that cannot feed the next call.
- `expression_macro_predicate_all_literal`:
  `predicateAll!((> 5), [10, 12, 15])` → the conjunction of three
  applications of the reified section lambda. The empty list becomes
  `True`; a variable list fails because it cannot be unrolled at expansion
  time.
- `decl_macro_derive_eq`: `@derive(Eq) type Foo = A int | B` → original plus `impl Eq Foo`.
- `hygiene_capture_avoided`: the `let x = 1 in ~body` case from chunk 6.
- `nested_macro_two_rounds`: a macro whose output calls another macro.
- `attribute_chain`: two attributes on one declaration, second sees the first's output.
- `comptime_in_macro_output`: output contains `comptime (1 + 2)`; expanded module prints it unchanged (comptime is evaluated in codegen, not here).

**Done when** snapshots are accepted and `cargo insta test --unreferenced delete` is clean.

---

## Order and dependencies

```
1 syntax ─┐
2 can ────┼─ 3 node types ─ 4 reifier ─ 5 unreifier ─ 6 hygiene ─┐
           │                                                   ├─ 8 driver loop ─ 10 derive ─ 12 snapshots
7 codegen (needs plans/07) ────────────────────────────────────┘        │
9 comptime (needs plans/02, plans/07)                                   11 diagnostics
```

Chunks 1–6 can land before plans/07; chunks 7–10 need it. Chunk 11 can
land any time after 8; chunk 12 after 10.
