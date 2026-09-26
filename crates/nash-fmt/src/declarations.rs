use crate::{
    doc::{Doc, cat, join, text},
    printer::Printer,
};
use nash_region::Position;
use nash_source::*;

impl Printer<'_> {
    pub fn exposing(&mut self, exposing: &Exposing<'_>, broken: bool) -> Doc {
        match exposing {
            Exposing::Open => text("(..)"),
            Exposing::Explicit(items) => {
                let docs = items
                    .iter()
                    .map(|item| match item {
                        Exposed::Lower(name) => {
                            cat([self.before(name.region.start), text(name.value)])
                        }
                        Exposed::Upper { name, privacy } => cat([
                            self.before(name.region.start),
                            text(name.value),
                            text(if matches!(privacy, Privacy::Public(_)) {
                                "(..)"
                            } else {
                                ""
                            }),
                        ]),
                        Exposed::LowerType { name, privacy } => cat([
                            self.before(name.region.start),
                            text("type "),
                            text(name.value),
                            text(if matches!(privacy, Privacy::Public(_)) {
                                "(..)"
                            } else {
                                ""
                            }),
                        ]),
                        Exposed::Operator { region, op } => {
                            cat([self.before(region.start), text(format!("({op})"))])
                        }
                    })
                    .collect();
                self.collection("(", ")", docs, broken).nest()
            }
        }
    }
    fn import(&mut self, import: &Import<'_>) -> Doc {
        let before = self.before(import.import.region.start);
        let mut docs = vec![before, text("import "), text(import.import.value)];
        if let Some(alias) = import.alias {
            docs.push(text(format!(" as {alias}")));
        }
        if !matches!(import.exposing,Exposing::Explicit(items) if items.is_empty()) {
            docs.extend([text(" exposing "), self.exposing(import.exposing, false)]);
        }
        cat(docs)
    }
    fn attributes(&mut self, attributes: &[&Attribute<'_>]) -> Doc {
        let mut docs = Vec::new();
        for a in attributes {
            docs.extend([
                self.before(a.name.region.start),
                text(format!("@{}", a.name.value)),
            ]);
            if !a.args.is_empty() {
                let args = a.args.iter().map(|e| self.expr(e, 0)).collect();
                docs.push(self.collection("(", ")", args, false));
            }
            docs.push(Doc::Hard);
        }
        cat(docs)
    }
    fn union(&mut self, u: &Union<'_>) -> Doc {
        let docs = self.documented(u.docs);
        let attributes = self.attributes(u.attributes);
        let before = self.before(u.name.region.start);
        let params = self.params(u.arguments);
        let ctors = join(
            u.ctors.iter().map(|c| {
                let before = self.before(c.name.region.start);
                let args = match &c.arguments {
                    CtorArgs::Positional(args) => args
                        .iter()
                        .enumerate()
                        .map(|(index, t)| {
                            let doc = self.typ(t, 2);
                            if index == 0 && matches!(t.value, Type::Record(_)) {
                                self.parens(doc)
                            } else {
                                doc
                            }
                        })
                        .collect(),
                    CtorArgs::Labeled(fields) => {
                        let fields = fields
                            .iter()
                            .map(|(name, typ)| {
                                let before = self.before(name.region.start);
                                cat([before, text(name.value), text(" : "), self.typ(typ, 0)])
                            })
                            .collect();
                        vec![self.collection("{", "}", fields, false)]
                    }
                };
                cat([before, self.args(text(c.name.value), args, false)])
            }),
            cat([Doc::Hard, text("| ")]),
        );
        cat([
            docs,
            attributes,
            before,
            text(format!("type {}", u.name.value)),
            if u.arguments.is_empty() {
                text("")
            } else {
                cat([text(" "), params])
            },
            cat([Doc::Hard, text("= "), ctors]).nest(),
        ])
    }
    fn alias(&mut self, a: &Alias<'_>) -> Doc {
        let docs = self.documented(a.docs);
        let attributes = self.attributes(a.attributes);
        let before = self.before(a.name.region.start);
        let params = self.params(a.arguments);
        let typ = self.typ(a.typ, 0);
        cat([
            docs,
            attributes,
            before,
            text(format!("type alias {}", a.name.value)),
            if a.arguments.is_empty() {
                text("")
            } else {
                cat([text(" "), params])
            },
            text(" ="),
            cat([Doc::Hard, typ]).nest(),
        ])
    }
    fn trait_(&mut self, t: &Trait<'_>) -> Doc {
        let docs = self.documented(t.docs);
        let attributes = self.attributes(t.attributes);
        let context = self.context(t.supers);
        let before = self.before(t.name.region.start);
        let params = self.params(t.params);
        let methods = join(
            t.methods.iter().map(|m| {
                let before = self.before(m.name.region.start);
                let annotation = self.annotation(m.annotation);
                let default = m
                    .default
                    .map_or_else(|| text(""), |d| cat([Doc::Hard, self.def(d)]));
                cat([before, text(m.name.value), text(" : "), annotation, default])
            }),
            cat([Doc::Hard, Doc::Hard]),
        );
        cat([
            docs,
            attributes,
            before,
            text("trait "),
            context,
            text(t.name.value),
            text(" "),
            params,
            text(" where"),
            cat([Doc::Hard, methods]).nest(),
        ])
    }
    fn impl_(&mut self, i: &Impl<'_>) -> Doc {
        let docs = self.documented(i.docs);
        let attributes = self.attributes(i.attributes);
        let context = self.context(i.context);
        let head = self.constraint(i.head);
        let methods = join(
            i.methods.iter().map(|d| self.def(d)),
            cat([Doc::Hard, Doc::Hard]),
        );
        cat([
            docs,
            attributes,
            text("impl "),
            context,
            head,
            text(" where"),
            cat([Doc::Hard, methods]).nest(),
        ])
    }
    fn tests(&mut self, tests: &Tests<'_>) -> Doc {
        let mut docs = Vec::new();
        for import in tests.imports {
            docs.push(self.import(import));
        }
        for located in tests.tests {
            let before = self.before(located.region.start);
            let test = &located.value;
            let kind = if matches!(test.body, TestBody::Prop { .. }) {
                "prop"
            } else {
                "test"
            };
            let expectation = match test.expect {
                Expect::Pass => "",
                Expect::Fail => " fail",
                Expect::FailOnce => " fail once",
            };
            let budget = match test.budget {
                None => String::new(),
                Some(Budget::Cpu(n)) => format!(" within (cpu {n})"),
                Some(Budget::Mem(n)) => format!(" within (mem {n})"),
                Some(Budget::Both { cpu, mem }) => format!(" within (cpu {cpu}, mem {mem})"),
            };
            let body = match &test.body {
                TestBody::Unit(b) => self.block(b.stmts, b.last),
                TestBody::Prop { binders, body } => {
                    let binders = join(
                        binders.iter().map(|b| {
                            let pattern = self.pattern(b.value.pattern, 0);
                            let generator = self.expr(b.value.generator, 0);
                            cat([pattern, text(" via "), generator])
                        }),
                        Doc::Hard,
                    );
                    cat([
                        text("let"),
                        cat([Doc::Hard, binders]).nest(),
                        Doc::Hard,
                        text("in"),
                        cat([Doc::Hard, self.block(body.stmts, body.last)]).nest(),
                    ])
                }
            };
            docs.push(cat([
                before,
                text(format!("{kind} ")),
                self.literal(test.name.region),
                text(format!("{expectation}{budget} =")),
                cat([Doc::Hard, body]).nest(),
            ]));
        }
        cat([
            text("tests"),
            cat([Doc::Hard, join(docs, cat([Doc::Hard, Doc::Hard]))]).nest(),
        ])
    }
    pub fn module(&mut self, module: &Module<'_>) -> Doc {
        enum Decl<'a> {
            Value(&'a Value<'a>),
            Union(&'a Union<'a>),
            Alias(&'a Alias<'a>),
            Trait(&'a Trait<'a>),
            Impl(&'a Impl<'a>),
            Infix(&'a Infix<'a>),
        }
        let mut chunks = Vec::new();
        if let Some(name) = module.name {
            let before = self.before(name.region.start);
            let exports =
                self.exposing(&module.exports.value, self.multiline(module.exports.region));
            chunks.push(cat([
                before,
                text(if matches!(module.kind, ModuleKind::Validator(_)) {
                    "validator module "
                } else {
                    "module "
                }),
                text(name.value),
                text(" exposing "),
                exports,
            ]));
        }
        if let Docs::YesDocs { overview, .. } = module.docs {
            chunks.push(cat([
                self.before(overview.region.start),
                text(self.raw(overview.region)),
            ]));
        }
        if !module.imports.is_empty() {
            chunks.push(join(
                module.imports.iter().map(|i| self.import(i)),
                Doc::Hard,
            ));
        }
        let mut declarations = Vec::new();
        declarations.extend(
            module
                .values
                .iter()
                .map(|d| (d.region.start, Decl::Value(&d.value))),
        );
        declarations.extend(
            module
                .unions
                .iter()
                .map(|d| (d.region.start, Decl::Union(&d.value))),
        );
        declarations.extend(
            module
                .aliases
                .iter()
                .map(|d| (d.region.start, Decl::Alias(&d.value))),
        );
        declarations.extend(
            module
                .traits
                .iter()
                .map(|d| (d.region.start, Decl::Trait(&d.value))),
        );
        declarations.extend(
            module
                .impls
                .iter()
                .map(|d| (d.region.start, Decl::Impl(&d.value))),
        );
        declarations.extend(
            module
                .binops
                .iter()
                .map(|d| (d.region.start, Decl::Infix(&d.value))),
        );
        declarations.sort_by_key(|(p, _)| (p.line, p.column));
        for (position, decl) in declarations {
            let before = self.before(position);
            let doc = match decl {
                Decl::Value(v) => {
                    let docs = self.documented(v.docs);
                    let attributes = self.attributes(v.attributes);
                    cat([
                        docs,
                        attributes,
                        self.definition(v.name, v.arguments, v.body, v.annotation),
                    ])
                }
                Decl::Union(u) => self.union(u),
                Decl::Alias(a) => self.alias(a),
                Decl::Trait(t) => self.trait_(t),
                Decl::Impl(i) => self.impl_(i),
                Decl::Infix(i) => text(format!(
                    "infix {} {} ({}) = {}",
                    match i.associativity {
                        Associativity::Left => "left",
                        Associativity::Right => "right",
                        Associativity::None => "non",
                    },
                    i.precedence.0,
                    i.op,
                    i.name
                )),
            };
            chunks.push(cat([before, doc]));
        }
        if let Some(tests) = module.tests {
            chunks.push(self.tests(tests));
        }
        let trailing = self.before(Position::new(usize::MAX, usize::MAX));
        if chunks.is_empty() {
            return trailing;
        }
        cat([
            join(chunks, cat([Doc::Hard, Doc::Hard, Doc::Hard])),
            Doc::Hard,
            trailing,
        ])
    }
}
