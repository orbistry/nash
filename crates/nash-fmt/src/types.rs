use crate::{
    doc::{Doc, cat, join, text},
    printer::Printer,
};
use nash_region::Located;
use nash_source::*;

impl Printer<'_> {
    pub fn typ(&mut self, located: &Located<Type<'_>>, context: u8) -> Doc {
        let leading = self.before(located.region.start);
        let (doc, precedence) = match &located.value {
            Type::Var(name) => (text(format!("'{name}")), 3),
            Type::Unit => (text("()"), 3),
            Type::Repr { typ, repr } => {
                let inner = self.typ(typ, 0);
                (
                    cat([text("("), inner, text(format!(" : {:?})", repr.value))]),
                    3,
                )
            }
            Type::Lambda { from, to } => {
                let from = self.typ(from, 1);
                let to = self.typ(to, 0);
                (
                    cat([from, text(" ->"), cat([Doc::Line(" "), to]).nest()]).group(),
                    0,
                )
            }
            Type::Type { name, args, .. } | Type::VarApp { name, args, .. } => {
                let name = if matches!(located.value, Type::VarApp { .. }) {
                    format!("'{name}")
                } else {
                    name.to_string()
                };
                let docs = args.iter().map(|a| self.typ(a, 2)).collect();
                (
                    self.args(text(name), docs, false),
                    if args.is_empty() { 3 } else { 1 },
                )
            }
            Type::TypeQual {
                module, name, args, ..
            } => {
                let docs = args.iter().map(|a| self.typ(a, 2)).collect();
                (
                    self.args(text(format!("{module}.{name}")), docs, false),
                    if args.is_empty() { 3 } else { 1 },
                )
            }
            Type::Record(fields) => {
                let docs = fields
                    .iter()
                    .map(|f| {
                        let before = self.before(f.field.region.start);
                        let typ = self.typ(f.typ, 0);
                        cat([before, text(f.field.value), text(" : "), typ])
                    })
                    .collect();
                (self.collection_at("{", "}", docs, located.region), 3)
            }
            Type::Tuple {
                first,
                second,
                rest,
            } => {
                let docs = [*first, *second]
                    .into_iter()
                    .chain(rest.iter().copied())
                    .map(|t| self.typ(t, 0))
                    .collect();
                (self.collection_at("(", ")", docs, located.region), 3)
            }
        };
        cat([
            leading,
            if precedence < context {
                self.parens(doc)
            } else {
                doc
            },
        ])
    }
    pub fn constraint(&mut self, c: &Located<Constraint<'_>>) -> Doc {
        let before = self.before(c.region.start);
        let c = &c.value;
        let name = c.module.map_or_else(
            || c.class.value.to_string(),
            |m| format!("{m}.{}", c.class.value),
        );
        let args = c.args.iter().map(|a| self.typ(a, 2)).collect();
        cat([before, self.args(text(name), args, false)])
    }
    fn constraints(&mut self, constraints: &[&Located<Constraint<'_>>]) -> Doc {
        let docs: Vec<_> = constraints.iter().map(|c| self.constraint(c)).collect();
        if docs.len() == 1 {
            docs.into_iter().next().unwrap()
        } else {
            self.collection("(", ")", docs, false)
        }
    }
    pub fn context(&mut self, constraints: &[&Located<Constraint<'_>>]) -> Doc {
        if constraints.is_empty() {
            text("")
        } else {
            cat([self.constraints(constraints), text(" => ")])
        }
    }
    pub fn annotation(&mut self, annotation: &Annotation<'_>) -> Doc {
        let context = if annotation.constraints.is_empty() {
            text("")
        } else {
            cat([
                self.constraints(annotation.constraints),
                Doc::Line(" "),
                text("=> "),
            ])
        };
        cat([context, self.typ(annotation.typ, 0)]).nest().group()
    }
    pub fn params(&mut self, params: &[&TypeParam<'_>]) -> Doc {
        join(
            params.iter().map(|p| {
                let before = self.before(p.name.region.start);
                let name = format!("'{}", p.name.value);
                cat([
                    before,
                    text(
                        p.repr
                            .map_or(name.clone(), |r| format!("({name} : {:?})", r.value)),
                    ),
                ])
            }),
            text(" "),
        )
    }
}
