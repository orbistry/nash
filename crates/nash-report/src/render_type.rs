//! Structural source and canonical type rendering, following Elm's Render.Type.
use crate::{doc::Doc, localizer::Localizer};
use nash_region::Located;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ctx {
    None,
    Func,
    App,
}
pub type Context = Ctx;
fn parens(doc: Doc) -> Doc {
    Doc::cat([Doc::text("("), doc, Doc::text(")")])
}
pub fn variable(name: &str) -> Doc {
    Doc::text(format!("'{}", name.trim_start_matches('\'')))
}
pub fn lambda(ctx: Ctx, a: Doc, b: Doc, rest: Vec<Doc>) -> Doc {
    let doc = Doc::align(Doc::sep(
        std::iter::once(a).chain(
            std::iter::once(b)
                .chain(rest)
                .map(|d| Doc::hsep([Doc::text("->"), d])),
        ),
    ));
    if ctx == Ctx::None { doc } else { parens(doc) }
}
pub fn apply(ctx: Ctx, name: Doc, args: Vec<Doc>) -> Doc {
    if args.is_empty() {
        return name;
    }
    let doc = Doc::hang(4, Doc::sep(std::iter::once(name).chain(args)));
    if ctx == Ctx::App { parens(doc) } else { doc }
}
pub fn tuple(a: Doc, b: Doc, rest: Vec<Doc>) -> Doc {
    let entries = std::iter::once(a)
        .chain(std::iter::once(b))
        .chain(rest)
        .enumerate()
        .map(|(i, d)| Doc::hsep([Doc::text(if i == 0 { "(" } else { "," }), d]));
    Doc::align(Doc::sep([Doc::cat(entries), Doc::text(")")]))
}
fn entry((name, typ): (Doc, Doc)) -> Doc {
    Doc::hang(4, Doc::sep([Doc::hsep([name, Doc::text(":")]), typ]))
}
fn record_docs(entries: Vec<(Doc, Doc)>, ext: Option<Doc>, vertical: bool) -> Doc {
    if entries.is_empty() && ext.is_none() {
        return Doc::text("{}");
    }
    let fields: Vec<_> = entries
        .into_iter()
        .map(entry)
        .enumerate()
        .map(|(i, d)| {
            Doc::hsep([
                Doc::text(if i == 0 {
                    if ext.is_some() { "|" } else { "{" }
                } else {
                    ","
                }),
                d,
            ])
        })
        .collect();
    let body = if let Some(ext) = ext {
        Doc::hang(
            4,
            Doc::sep([Doc::hsep([Doc::text("{"), ext]), Doc::cat(fields)]),
        )
    } else if vertical {
        Doc::vcat(fields)
    } else {
        Doc::cat(fields)
    };
    if vertical {
        Doc::vcat([body, Doc::text("}")])
    } else {
        Doc::align(Doc::sep([body, Doc::text("}")]))
    }
}
pub fn record(entries: Vec<(Doc, Doc)>, ext: Option<Doc>) -> Doc {
    record_docs(entries, ext, false)
}
pub fn vrecord(entries: Vec<(Doc, Doc)>, ext: Option<Doc>) -> Doc {
    record_docs(entries, ext, true)
}
pub fn vrecord_snippet(first: (Doc, Doc), rest: Vec<(Doc, Doc)>) -> Doc {
    Doc::vcat(
        std::iter::once(Doc::hsep([Doc::text("{"), entry(first)]))
            .chain(
                rest.into_iter()
                    .map(|e| Doc::hsep([Doc::text(","), entry(e)])),
            )
            .chain([Doc::text(", ..."), Doc::text("}")]),
    )
}
pub fn src_to_doc(ctx: Ctx, typ: &Located<nash_source::Type<'_>>) -> Doc {
    use nash_source::Type::*;
    match &typ.value {
        Repr { typ, repr } => {
            let annotation = match repr.value {
                nash_source::Repr::Big => "Big",
                nash_source::Repr::Const => "Const",
                nash_source::Repr::Term => "Term",
                nash_source::Repr::Storable => "Storable",
            };
            parens(Doc::hsep([
                src_to_doc(Ctx::None, typ),
                Doc::text(":"),
                Doc::text(annotation),
            ]))
        }
        Lambda { from, to } => {
            let mut parts = vec![src_to_doc(Ctx::Func, from)];
            let mut last = *to;
            while let Lambda { from, to } = &last.value {
                parts.push(src_to_doc(Ctx::Func, from));
                last = to;
            }
            parts.push(src_to_doc(Ctx::Func, last));
            let a = parts.remove(0);
            let b = parts.remove(0);
            lambda(ctx, a, b, parts)
        }
        Var(name) => variable(name),
        VarApp { name, args, .. } => apply(
            ctx,
            variable(name),
            args.iter().map(|t| src_to_doc(Ctx::App, t)).collect(),
        ),
        Type { name, args, .. } => apply(
            ctx,
            Doc::text(*name),
            args.iter().map(|t| src_to_doc(Ctx::App, t)).collect(),
        ),
        TypeQual {
            module, name, args, ..
        } => apply(
            ctx,
            Doc::text(format!("{module}.{name}")),
            args.iter().map(|t| src_to_doc(Ctx::App, t)).collect(),
        ),
        Record(fields) => record(
            fields
                .iter()
                .map(|f| (Doc::text(f.field.value), src_to_doc(Ctx::None, f.typ)))
                .collect(),
            None,
        ),
        Unit => Doc::text("()"),
        Tuple {
            first,
            second,
            rest,
        } => tuple(
            src_to_doc(Ctx::None, first),
            src_to_doc(Ctx::None, second),
            rest.iter().map(|t| src_to_doc(Ctx::None, t)).collect(),
        ),
    }
}
pub fn can_to_doc(localizer: &Localizer, ctx: Ctx, typ: &nash_ast::Type<'_>) -> Doc {
    use nash_ast::Type::*;
    match typ {
        Lambda { from, to } => {
            let mut parts = vec![can_to_doc(localizer, Ctx::Func, &from.value)];
            let mut last = &to.value;
            while let Lambda { from, to } = last {
                parts.push(can_to_doc(localizer, Ctx::Func, &from.value));
                last = &to.value;
            }
            parts.push(can_to_doc(localizer, Ctx::Func, last));
            let a = parts.remove(0);
            let b = parts.remove(0);
            lambda(ctx, a, b, parts)
        }
        Var(name) => variable(name),
        App { head, args } => apply(
            ctx,
            can_to_doc(localizer, Ctx::App, &head.value),
            args.iter()
                .map(|t| can_to_doc(localizer, Ctx::App, &t.value))
                .collect(),
        ),
        Named { reference, args } => apply(
            ctx,
            localizer.to_doc(reference.home, reference.name),
            args.iter()
                .map(|t| can_to_doc(localizer, Ctx::App, &t.value))
                .collect(),
        ),
        Record { fields } => {
            let mut fields: Vec<_> = fields.iter().collect();
            fields.sort_by_key(|f| f.index);
            record(
                fields
                    .into_iter()
                    .map(|f| {
                        (
                            Doc::text(f.field),
                            can_to_doc(localizer, Ctx::None, &f.typ.value),
                        )
                    })
                    .collect(),
                None,
            )
        }
        Tuple {
            first,
            second,
            rest,
        } => tuple(
            can_to_doc(localizer, Ctx::None, &first.value),
            can_to_doc(localizer, Ctx::None, &second.value),
            rest.iter()
                .map(|t| can_to_doc(localizer, Ctx::None, &t.value))
                .collect(),
        ),
        Alias {
            reference,
            arguments,
            ..
        } => apply(
            ctx,
            localizer.to_doc(reference.home, reference.name),
            arguments
                .iter()
                .map(|a| can_to_doc(localizer, Ctx::App, &a.typ.value))
                .collect(),
        ),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn function_parentheses() {
        insta::assert_snapshot!(lambda(Ctx::App,variable("a"),variable("b"),vec![]).render(80,false), @"('a -> 'b)");
    }
    #[test]
    fn narrow_application() {
        insta::assert_snapshot!(apply(Ctx::None,Doc::text("Container"),vec![Doc::text("LongArgument")]).render(12,false), @r"
Container
    LongArgument
");
    }
    #[test]
    fn vertical_snippet() {
        insta::assert_snapshot!(vrecord_snippet((Doc::text("x"),variable("a")),vec![]).render(80,false), @r"
{ x : 'a
, ...
}
");
    }
    #[test]
    fn source_type_shapes() {
        use nash_source::{Repr, Type};
        let a = Located::at_zero(Type::Var("a"));
        let b = Located::at_zero(Type::TypeQual {
            region: nash_region::Region::zero(),
            module: "A",
            name: "Thing",
            args: &[],
        });
        let func = Located::at_zero(Type::Lambda { from: &a, to: &b });
        let args = [&func];
        let app = Located::at_zero(Type::VarApp {
            region: nash_region::Region::zero(),
            name: "f",
            args: &args,
        });
        insta::assert_snapshot!(src_to_doc(Ctx::None,&app).render(80,false), @"'f ('a -> A.Thing)");
        let repr = Located::at_zero(Repr::Big);
        let annotated = Located::at_zero(Type::Repr {
            typ: &a,
            repr: &repr,
        });
        insta::assert_snapshot!(src_to_doc(Ctx::None,&annotated).render(80,false), @"('a : Big)");
        let unit = Located::at_zero(Type::Unit);
        insta::assert_snapshot!(src_to_doc(Ctx::None,&unit).render(80,false), @"()");
    }
    #[test]
    fn source_record_and_tuple() {
        use nash_source::{FieldType, Type};
        let a = Located::at_zero(Type::Var("a"));
        let name = Located::at_zero("field");
        let field = FieldType {
            field: &name,
            typ: &a,
        };
        let fields = [&field];
        let record = Located::at_zero(Type::Record(&fields));
        insta::assert_snapshot!(src_to_doc(Ctx::None,&record).render(80,false), @"{ field : 'a }");
        let rest = [&a, &a];
        let tuple = Located::at_zero(Type::Tuple {
            first: &a,
            second: &a,
            rest: &rest,
        });
        insta::assert_snapshot!(src_to_doc(Ctx::None,&tuple).render(80,false), @"( 'a, 'a, 'a, 'a )");
    }
    #[test]
    fn canonical_record_preserves_declaration_order() {
        use nash_ast::{FieldType, Type};
        let a = Located::at_zero(Type::Var("a"));
        let fields = [
            FieldType {
                index: 1,
                field: "first",
                typ: &a,
            },
            FieldType {
                index: 0,
                field: "zebra",
                typ: &a,
            },
        ];
        insta::assert_snapshot!(can_to_doc(&Localizer::default(),Ctx::None,&Type::Record{fields:&fields}).render(80,false), @"{ zebra : 'a, first : 'a }");
    }
    #[test]
    fn canonical_alias_keeps_public_name() {
        use nash_ast::{AliasArgument, AliasType, ModuleName, QualifiedName, Type};
        let a = Located::at_zero(Type::Var("a"));
        let args = [AliasArgument { name: "a", typ: &a }];
        let alias = Type::Alias {
            reference: QualifiedName {
                home: ModuleName {
                    package: None,
                    name: "A",
                },
                name: "Box",
            },
            arguments: &args,
            remaining: &[],
            target: AliasType::Open(&a),
        };
        insta::assert_snapshot!(can_to_doc(&Localizer::from_names(["A"]),Ctx::App,&alias).render(80,false), @"(Box 'a)");
    }
}
