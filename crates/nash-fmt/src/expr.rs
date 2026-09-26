use crate::{
    doc::{Doc, cat, join, text},
    printer::Printer,
};
use nash_region::Located;
use nash_source::*;

impl Printer<'_> {
    pub fn pattern(&mut self, located: &Located<Pattern<'_>>, context: u8) -> Doc {
        let leading = self.before(located.region.start);
        let (doc, precedence) = match &located.value {
            Pattern::Anything => (text("_"), 3),
            Pattern::Var(name) => (text(name), 3),
            Pattern::Unit => (text("()"), 3),
            Pattern::Str(_) | Pattern::Bytes(_) | Pattern::Int(_) => {
                (self.literal(located.region), 3)
            }
            Pattern::Alias { pattern, name } => {
                let inner = self.pattern(pattern, 1);
                (cat([inner, text(" as "), text(name.value)]), 0)
            }
            Pattern::Cons { head, tail } => {
                let head = self.pattern(head, 2);
                let tail = self.pattern(tail, 1);
                (cat([head, text(" :: "), tail]), 1)
            }
            Pattern::Ctor { name, args, .. } | Pattern::CtorQual { name, args, .. } => {
                let name = if let Pattern::CtorQual { module, .. } = &located.value {
                    format!("{module}.{name}")
                } else {
                    name.to_string()
                };
                let docs = args.iter().map(|a| self.pattern(a, 3)).collect();
                (
                    self.args(text(name), docs, false),
                    if args.is_empty() { 3 } else { 2 },
                )
            }
            Pattern::Record(fields) => {
                let docs = fields
                    .iter()
                    .map(|f| cat([self.before(f.region.start), text(f.value)]))
                    .collect();
                (self.collection_at("{", "}", docs, located.region), 3)
            }
            Pattern::List(items) => {
                let docs = items.iter().map(|p| self.pattern(p, 0)).collect();
                (self.collection_at("[", "]", docs, located.region), 3)
            }
            Pattern::Pair { first, second } => {
                let first = self.pattern(first, 0);
                let second = self.pattern(second, 0);
                (
                    cat([text("pair("), first, text(", "), second, text(")")]),
                    3,
                )
            }
            Pattern::Tuple {
                first,
                second,
                rest,
            } => {
                let docs = [*first, *second]
                    .into_iter()
                    .chain(rest.iter().copied())
                    .map(|p| self.pattern(p, 0))
                    .collect();
                (self.collection_at("(", ")", docs, located.region), 3)
            }
        };
        let trailing = self.trailing(located.region.end);
        cat([
            leading,
            if precedence < context {
                self.parens(doc)
            } else {
                doc
            },
            trailing,
        ])
    }
    pub fn expr(&mut self, located: &Located<Expr<'_>>, context: u8) -> Doc {
        let leading = self.before(located.region.start);
        let (doc, precedence) = match &located.value {
            Expr::Str(_) | Expr::Bytes(_) | Expr::Int(_) => (self.literal(located.region), 4),
            Expr::Unit => (text("()"), 4),
            Expr::Var { name, .. } => (text(name), 4),
            Expr::VarQual { module, name, .. } => (text(format!("{module}.{name}")), 4),
            Expr::Op(op) => (text(format!("({op})")), 4),
            Expr::Accessor(field) => (text(format!(".{field}")), 4),
            Expr::Access { record, field } => {
                let record = self.expr(record, 4);
                (cat([record, text("."), text(field.value)]), 4)
            }
            Expr::Negate(value) => {
                let value = self.expr(value, 3);
                (cat([text("-"), value]), 2)
            }
            Expr::List(items) => {
                let docs = items.iter().map(|e| self.expr(e, 0)).collect();
                (self.collection_at("[", "]", docs, located.region), 4)
            }
            Expr::Tuple {
                first,
                second,
                rest,
            } => {
                let docs = [*first, *second]
                    .into_iter()
                    .chain(rest.iter().copied())
                    .map(|e| self.expr(e, 0))
                    .collect();
                (self.collection_at("(", ")", docs, located.region), 4)
            }
            Expr::Record { fields, grouped } => {
                let docs = self.fields(fields);
                let doc = self.collection_at("{", "}", docs, located.region);
                (if *grouped { self.parens(doc) } else { doc }, 4)
            }
            Expr::Update { record, fields } => {
                let docs = self.fields(fields);
                (
                    self.collection_at(
                        &format!("{{ {} |", record.value),
                        "}",
                        docs,
                        located.region,
                    ),
                    4,
                )
            }
            Expr::Call {
                function,
                arguments,
            } => {
                let function = self.expr(function, 3);
                let args = arguments.iter().map(|e| self.expr(e, 3)).collect();
                (self.args(function, args, self.multiline(located.region)), 2)
            }
            Expr::MacroCall { name, module, args } => {
                let head = module
                    .map_or_else(|| name.value.to_string(), |m| format!("{m}.{}", name.value));
                let args = args.iter().map(|e| self.expr(e, 0)).collect();
                (
                    self.collection(
                        &format!("{head}!("),
                        ")",
                        args,
                        self.multiline(located.region),
                    ),
                    4,
                )
            }
            Expr::LeftSection { left, operator } => {
                let left = self.expr(left, 0);
                (cat([text("("), left, text(format!(" {operator})"))]), 4)
            }
            Expr::RightSection { operator, right } => {
                let right = self.expr(right, 0);
                (cat([text(format!("({operator} ")), right, text(")")]), 4)
            }
            Expr::BinOps { operands, last } => {
                let mut docs = Vec::new();
                let broken = self.multiline(located.region)
                    && operands
                        .iter()
                        .any(|operand| matches!(operand.op.value, "|>" | "<|"));
                for operand in *operands {
                    docs.push(self.expr(operand.expr, 2));
                    let line = if broken { Doc::Hard } else { Doc::Line(" ") };
                    if matches!(operand.op.value, "|>" | "<|") {
                        docs.push(line);
                        docs.push(text(operand.op.value));
                        docs.push(text(" "));
                    } else {
                        docs.push(text(" "));
                        docs.push(text(operand.op.value));
                        docs.push(line);
                    }
                }
                docs.push(self.expr(last, 2));
                (cat([docs.remove(0), cat(docs).nest()]).group(), 1)
            }
            Expr::Lambda { parameters, body } => {
                let args = join(parameters.iter().map(|p| self.pattern(p, 3)), text(" "));
                let body = self.expr(body, 0);
                (
                    cat([
                        text("\\"),
                        args,
                        text(" ->"),
                        cat([Doc::Line(" "), body]).nest(),
                    ])
                    .group(),
                    0,
                )
            }
            Expr::If {
                branches,
                final_else,
            } => {
                let mut docs = Vec::new();
                for (i, b) in branches.iter().enumerate() {
                    if i > 0 {
                        docs.extend([Doc::Hard, text("else ")]);
                    }
                    docs.extend([
                        text("if "),
                        self.expr(b.condition, 0),
                        text(" then"),
                        cat([Doc::Hard, self.expr(b.then_branch, 0)]).nest(),
                    ]);
                }
                docs.extend([
                    Doc::Hard,
                    text("else"),
                    cat([Doc::Hard, self.expr(final_else, 0)]).nest(),
                ]);
                (cat(docs), 0)
            }
            Expr::Let { defs, body } => {
                let defs = join(
                    defs.iter().map(|d| self.def(d)),
                    cat([Doc::Hard, Doc::Hard]),
                );
                let body = self.expr(body, 0);
                (
                    cat([
                        text("let"),
                        cat([Doc::Hard, defs]).nest(),
                        Doc::Hard,
                        text("in"),
                        cat([Doc::Hard, body]).nest(),
                    ]),
                    0,
                )
            }
            Expr::Case { scrutinee, arms } => {
                let scrutinee = self.expr(scrutinee, 0);
                let arms = join(
                    arms.iter().map(|a| {
                        let pattern = self.pattern(a.pattern, 0).nest();
                        let body = self.expr(a.body, 0);
                        cat([pattern, text(" ->"), cat([Doc::Hard, body]).nest()])
                    }),
                    cat([Doc::Hard, Doc::Hard]),
                );
                (
                    cat([
                        text("case "),
                        scrutinee,
                        text(" of"),
                        cat([Doc::Hard, arms]).nest(),
                    ]),
                    0,
                )
            }
            Expr::Do { stmts, last } => (self.block(stmts, last), 0),
            Expr::Assert(value) | Expr::Comptime(value) => {
                let keyword = if matches!(located.value, Expr::Assert(_)) {
                    "assert"
                } else {
                    "comptime"
                };
                let value = self.expr(value, 3);
                (self.args(text(keyword), vec![value], false), 0)
            }
            Expr::Fail(value) | Expr::Todo(value) => {
                let keyword = if matches!(located.value, Expr::Fail(_)) {
                    "fail"
                } else {
                    "todo"
                };
                let args = value.iter().map(|e| self.expr(e, 3)).collect();
                (self.args(text(keyword), args, false), 0)
            }
            Expr::Trace { message, body } => {
                let message = self.expr(message, 3);
                let body = self.expr(body, 0);
                (
                    cat([text("trace "), message, cat([Doc::Hard, body]).nest()]),
                    0,
                )
            }
        };
        let trailing = self.trailing(located.region.end);
        cat([
            leading,
            if precedence < context {
                self.parens(doc)
            } else {
                doc
            },
            trailing,
        ])
    }
    fn fields(&mut self, fields: &[&FieldAssign<'_>]) -> Vec<Doc> {
        fields
            .iter()
            .map(|f| {
                let before = self.before(f.field.region.start);
                let value = self.expr(f.value, 3);
                cat([
                    before,
                    text(f.field.value),
                    text(" ="),
                    cat([Doc::Line(" "), value]).nest(),
                ])
                .group()
            })
            .collect()
    }
    pub fn definition(
        &mut self,
        name: &Located<&str>,
        args: &[&Located<Pattern<'_>>],
        body: &Located<Expr<'_>>,
        annotation: Option<&Annotation<'_>>,
    ) -> Doc {
        let before = self.before(name.region.start);
        let annotation = annotation.map_or_else(
            || text(""),
            |a| cat([text(name.value), text(" : "), self.annotation(a), Doc::Hard]),
        );
        let args = args.iter().map(|p| self.pattern(p, 3)).collect();
        let head = self.args(text(name.value), args, false);
        let definition = self.rhs(cat([head, text(" =")]), body);
        cat([before, annotation, definition])
    }
    fn rhs(&mut self, head: Doc, body: &Located<Expr<'_>>) -> Doc {
        let hanging =
            matches!(body.value, Expr::Do { .. }) && !self.has_comment_before(body.region.start);
        let body = self.expr(body, 0);
        if hanging {
            cat([head, text(" "), body])
        } else {
            cat([head, cat([Doc::Line(" "), body]).nest()]).group()
        }
    }
    pub fn def(&mut self, located: &Located<Def<'_>>) -> Doc {
        match &located.value {
            Def::Define {
                name,
                args,
                body,
                annotation,
            } => self.definition(name, args, body, *annotation),
            Def::Destruct { pattern, body } => {
                let pattern = self.pattern(pattern, 0).nest();
                self.rhs(cat([pattern, text(" =")]), body)
            }
        }
    }
    pub fn block(&mut self, stmts: &[&Located<Stmt<'_>>], last: &Located<Expr<'_>>) -> Doc {
        let mut docs = Vec::new();
        for stmt in stmts {
            let before = self.before(stmt.region.start);
            let doc = match &stmt.value {
                Stmt::Let(defs) => {
                    let defs = join(
                        defs.iter().map(|d| self.def(d)),
                        cat([Doc::Hard, Doc::Hard]),
                    );
                    cat([text("let"), cat([Doc::Hard, defs]).nest()])
                }
                Stmt::Bind { pattern, expr } => {
                    let pattern = self.pattern(pattern, 0).nest();
                    self.rhs(cat([pattern, text(" <-")]), expr)
                }
                Stmt::Expr(expr) => self.expr(expr, 0),
            };
            docs.push(cat([before, doc]));
        }
        docs.push(self.expr(last, 0));
        cat([text("do"), cat([Doc::Hard, join(docs, Doc::Hard)]).nest()])
    }
}
