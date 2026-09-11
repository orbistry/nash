//! Walk the canonical AST and check every pattern match, from Elm's
//! `Nitpick/PatternMatches.hs` (`check` .. `checkPatterns`).

use bumpalo::Bump;
use nash_ast::{CaseBranch, Decls, Def, Expr, Module, Pattern as CanPattern};
use nash_region::{Located, Region};

use crate::matrix::{Row, is_exhaustive, is_useful};
use crate::pattern::{Context, Error, simplify};

/// Elm's `check`. `Err` is never empty.
pub fn check<'a>(bump: &'a Bump, module: &Module<'a>) -> Result<(), Vec<Error<'a>>> {
    let mut checker = Checker {
        bump,
        errors: Vec::new(),
    };
    let mut definitions = Vec::new();
    collect_decls(module.decls, &mut definitions);
    for trait_ in module.traits {
        definitions.extend(
            trait_
                .value
                .methods
                .iter()
                .filter_map(|method| method.default),
        );
    }
    for impl_ in module.impls {
        definitions.extend_from_slice(impl_.value.methods);
    }
    checker.defs(definitions);
    if checker.errors.is_empty() {
        Ok(())
    } else {
        Err(checker.errors)
    }
}

struct Checker<'a> {
    bump: &'a Bump,
    errors: Vec<Error<'a>>,
}

enum LetBinding<'a> {
    Definition(&'a Def<'a>),
    Destructure(&'a Located<CanPattern<'a>>, &'a Located<Expr<'a>>),
}

impl<'a> Checker<'a> {
    fn defs(&mut self, mut definitions: Vec<&Def<'a>>) {
        definitions.sort_by_key(|def| match def {
            Def::Def { name, .. } | Def::TypedDef { name, .. } => name.region.start,
        });
        for def in definitions {
            self.def(def);
        }
    }

    /// Elm's `checkDef`.
    fn def(&mut self, def: &Def<'a>) {
        match def {
            Def::Def { args, body, .. } => {
                for arg in *args {
                    self.arg(arg);
                }
                self.expr(body);
            }
            Def::TypedDef { args, body, .. } => {
                for arg in *args {
                    self.arg(arg.pattern);
                }
                self.expr(body);
            }
        }
    }

    /// Elm's `checkArg` / `checkTypedArg`.
    fn arg(&mut self, pattern: &'a Located<CanPattern<'a>>) {
        self.patterns(pattern.region, Context::BadArg, &[pattern]);
    }

    /// Elm's `checkExpr`.
    fn expr(&mut self, expr: &Located<Expr<'a>>) {
        match &expr.value {
            Expr::VarMethod { .. }
            | Expr::Bytes(_)
            | Expr::VarLocal(_)
            | Expr::VarTopLevel(_)
            | Expr::VarForeign { .. }
            | Expr::VarConstructor { .. }
            | Expr::VarOperator { .. }
            | Expr::Str(_)
            | Expr::Int(_)
            | Expr::Accessor(_)
            | Expr::Unit => {}
            Expr::Assert(inner) | Expr::Comptime(inner) => self.expr(inner),
            Expr::Fail(message) | Expr::Todo(message) => {
                if let Some(message) = message {
                    self.expr(message);
                }
            }
            Expr::Trace { message, body } => {
                self.expr(message);
                self.expr(body);
            }
            Expr::List(entries) => {
                for entry in *entries {
                    self.expr(entry);
                }
            }
            Expr::Binop { left, right, .. } => {
                self.expr(left);
                self.expr(right);
            }
            Expr::Lambda { parameters, body } => {
                for parameter in *parameters {
                    self.arg(parameter);
                }
                self.expr(body);
            }
            Expr::Call {
                function,
                arguments,
            } => {
                self.expr(function);
                // Labeled constructors reorder arguments into declaration order.
                // Other calls keep their canonical order: generated do lambdas
                // share an earlier statement region but execute after the RHS.
                if matches!(function.value, Expr::VarConstructor { .. }) {
                    let mut ordered = arguments.to_vec();
                    ordered.sort_by_key(|argument| argument.region.start);
                    for argument in ordered {
                        self.expr(argument);
                    }
                } else {
                    for argument in *arguments {
                        self.expr(argument);
                    }
                }
            }
            Expr::If {
                branches,
                final_else,
            } => {
                for branch in *branches {
                    self.expr(branch.condition);
                    self.expr(branch.then_branch);
                }
                self.expr(final_else);
            }
            Expr::Let { .. } | Expr::LetRec { .. } | Expr::LetDestruct { .. } => {
                self.let_expr(expr)
            }
            Expr::Case {
                scrutinee,
                branches,
            } => {
                self.expr(scrutinee);
                self.cases(expr.region, branches);
            }
            Expr::Access { record, .. } => self.expr(record),
            Expr::Update { base, fields, .. } => {
                self.expr(base);
                let mut ordered: Vec<_> = fields.iter().collect();
                ordered.sort_by_key(|field| field.field.region.start);
                for field in ordered {
                    self.expr(field.value);
                }
            }
            Expr::Record { fields, .. } => {
                let mut ordered: Vec<_> = fields.iter().collect();
                ordered.sort_by_key(|field| field.field.region.start);
                for field in ordered {
                    self.expr(field.value);
                }
            }
            Expr::Tuple {
                first,
                second,
                rest,
            } => {
                self.expr(first);
                self.expr(second);
                for entry in *rest {
                    self.expr(entry);
                }
            }
        }
    }

    // Canonicalization nests let groups in dependency order. Collect their
    // bindings before visiting, retaining each binding's own traversal order.
    fn let_expr(&mut self, mut expression: &Located<Expr<'a>>) {
        let mut bindings = Vec::new();
        loop {
            match &expression.value {
                Expr::Let { definition, body } => {
                    bindings.push(LetBinding::Definition(definition));
                    expression = body;
                }
                Expr::LetRec { definitions, body } => {
                    bindings.extend(definitions.iter().map(|def| LetBinding::Definition(def)));
                    expression = body;
                }
                Expr::LetDestruct {
                    pattern,
                    value,
                    body,
                } => {
                    bindings.push(LetBinding::Destructure(pattern, value));
                    expression = body;
                }
                _ => break,
            }
        }
        bindings.sort_by_key(|binding| match binding {
            LetBinding::Definition(Def::Def { name, .. } | Def::TypedDef { name, .. }) => {
                name.region.start
            }
            LetBinding::Destructure(pattern, _) => pattern.region.start,
        });
        for binding in bindings {
            match binding {
                LetBinding::Definition(def) => self.def(def),
                LetBinding::Destructure(pattern, value) => {
                    self.patterns(pattern.region, Context::BadDestruct, &[pattern]);
                    self.expr(value);
                }
            }
        }
        self.expr(expression);
    }

    /// Elm's `checkCases` + `checkCaseBranch`.
    fn cases(&mut self, region: Region, branches: &[CaseBranch<'a>]) {
        let patterns: Vec<_> = branches.iter().map(|branch| branch.pattern).collect();
        self.patterns(region, Context::BadCase, &patterns);
        for branch in branches {
            self.expr(branch.body);
        }
    }

    /// Elm's `checkPatterns`.
    fn patterns(
        &mut self,
        region: Region,
        context: Context,
        patterns: &[&'a Located<CanPattern<'a>>],
    ) {
        match self.to_non_redundant_rows(region, patterns) {
            Err(error) => self.errors.push(error),
            Ok(matrix) => {
                let missing = is_exhaustive(self.bump, &matrix, 1);
                if !missing.is_empty() {
                    // Every missing row has length 1 (Elm's `map head`).
                    let unhandled = self
                        .bump
                        .alloc_slice_fill_iter(missing.iter().map(|row| row[0]));
                    self.errors.push(Error::Incomplete {
                        region,
                        context,
                        unhandled,
                    });
                }
            }
        }
    }

    /// Elm's `toNonRedundantRows` / `toSimplifiedUsefulRows`. Every row has
    /// length 1.
    fn to_non_redundant_rows(
        &self,
        case_region: Region,
        patterns: &[&'a Located<CanPattern<'a>>],
    ) -> Result<Vec<Row<'a>>, Error<'a>> {
        let mut checked: Vec<Row<'a>> = Vec::with_capacity(patterns.len());
        for pattern in patterns {
            let next_row = vec![simplify(self.bump, pattern)];
            if !is_useful(&checked, &next_row) {
                return Err(Error::Redundant {
                    case_region,
                    pattern_region: pattern.region,
                    index: checked.len() + 1,
                });
            }
            checked.push(next_row);
        }
        Ok(checked)
    }
}

/// Canonical declarations are in dependency order, not necessarily source order.
fn collect_decls<'a>(mut decls: &Decls<'a>, definitions: &mut Vec<&'a Def<'a>>) {
    loop {
        match decls {
            Decls::Declare { definition, next } => {
                definitions.push(definition);
                decls = next;
            }
            Decls::DeclareRec {
                definition,
                following,
                next,
            } => {
                definitions.push(definition);
                definitions.extend_from_slice(following);
                decls = next;
            }
            Decls::Empty => break,
        }
    }
}
