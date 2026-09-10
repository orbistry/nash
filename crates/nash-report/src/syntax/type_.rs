use super::{Doc, Report, Source, closing, problem, to_space_report, wide};
use crate::code::{Next, to_keyword_region};
use nash_parse::error::{Repr, TRecord, TTuple, Type, TypeParam};
use nash_parse::{Col, Row};

#[derive(Clone, Copy)]
pub(crate) enum TContext<'a> {
    Annotation(&'a str),
    CustomType,
    TypeAlias,
    Superclass,
    TraitMethod,
    ImplHead,
}

fn reserved(row: Row, col: Col, keyword: &str, role: &str) -> Report {
    Report::snippet(
        "RESERVED WORD",
        to_keyword_region(row, col, keyword),
        None,
        Doc::text(format!("Reserved word `{keyword}` cannot be a {role}.")),
        Doc::text("Choose another name."),
    )
}

pub(crate) fn to_type_report(
    source: &Source<'_>,
    context: TContext<'_>,
    error: &Type<'_>,
    sr: Row,
    sc: Col,
) -> Report {
    let place = match context {
        TContext::Annotation(name) => format!("annotation for `{name}`"),
        TContext::CustomType => "datatype declaration".into(),
        TContext::TypeAlias => "type alias".into(),
        TContext::Superclass => "superclass constraint".into(),
        TContext::TraitMethod => "trait method".into(),
        TContext::ImplHead => "impl head".into(),
    };
    let report = match *error {
        Type::Record(e, r, c) => return to_t_record_report(source, context, e, r, c),
        Type::Tuple(e, r, c) => return to_t_tuple_report(source, context, e, r, c),
        Type::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        Type::Start(r, c) => match source.what_is_next(r, c) {
            Next::Keyword(keyword) => reserved(r, c, keyword, "type"),
            _ => problem(
                "EXPECTED TYPE",
                r,
                c,
                &format!("Expected a type in the {place}."),
                if matches!(context, TContext::Superclass | TContext::ImplHead) {
                    "Write a trait name followed by its type arguments."
                } else {
                    "Write a type name or a quoted type variable, such as `'a`."
                },
            ),
        },
        Type::IndentStart(r, c) => problem(
            "EXPECTED TYPE",
            r,
            c,
            &format!("Expected an indented type in the {place}."),
            "Indent the type to continue the declaration.",
        ),
        Type::VarStart(r, c) => problem(
            "MISSING TYPE VARIABLE",
            r,
            c,
            "Expected a name after the type-variable quote.",
            "Use a lowercase name, such as `'a`.",
        ),
        Type::Context(r, c) => problem(
            "BAD TYPE CONSTRAINT",
            r,
            c,
            "Invalid type constraint.",
            "Write `Trait args => type`; enclose multiple constraints in parentheses, separated by commas.",
        ),
        Type::IndentAfterContext(r, c) => problem(
            "UNFINISHED CONSTRAINED TYPE",
            r,
            c,
            "Expected a type after `=>`.",
            "Indent the type to continue the annotation.",
        ),
    };
    wide(report, sr, sc)
}

pub(super) fn to_t_record_report(
    source: &Source<'_>,
    context: TContext<'_>,
    error: &TRecord<'_>,
    sr: Row,
    sc: Col,
) -> Report {
    let report = match *error {
        TRecord::Open(r, c) if matches!(source.what_is_next(r, c), Next::Close(_, found) if found != '}') =>
        {
            return closing(source, r, c, sr, sc, '}');
        }
        TRecord::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        TRecord::Type(e, r, c) => return to_type_report(source, context, e, r, c),
        TRecord::End(r, c) | TRecord::IndentEnd(r, c) => return closing(source, r, c, sr, sc, '}'),
        TRecord::Open(r, c) | TRecord::Field(r, c) => match source.what_is_next(r, c) {
            Next::Keyword(keyword) => reserved(r, c, keyword, "field name"),
            Next::Other(Some(',')) => problem(
                "EXTRA COMMA",
                r,
                c,
                "Extra comma in record type.",
                "Remove the repeated comma.",
            ),
            Next::Close(_, '}') => problem(
                "EXTRA COMMA",
                r,
                c,
                "Trailing comma in record type.",
                "Remove the comma before `}`.",
            ),
            _ => problem(
                "EXPECTED FIELD",
                r,
                c,
                "Expected a record field.",
                "Write `name : Type`.",
            ),
        },
        TRecord::Colon(r, c) | TRecord::IndentColon(r, c) => problem(
            "MISSING COLON",
            r,
            c,
            "Expected `:` after the field name.",
            "Separate the field name and its type with `:`.",
        ),
        TRecord::IndentOpen(r, c) | TRecord::IndentField(r, c) => problem(
            "EXPECTED FIELD",
            r,
            c,
            "Expected an indented record field.",
            "Indent the field inside the braces.",
        ),
        TRecord::IndentType(r, c) => problem(
            "EXPECTED TYPE",
            r,
            c,
            "Expected an indented field type after `:`.",
            "Indent the type to continue the field.",
        ),
    };
    wide(report, sr, sc)
}

pub(super) fn to_t_tuple_report(
    source: &Source<'_>,
    context: TContext<'_>,
    error: &TTuple<'_>,
    sr: Row,
    sc: Col,
) -> Report {
    let report = match *error {
        TTuple::Open(r, c) if matches!(source.what_is_next(r, c), Next::Close(_, found) if found != ')') =>
        {
            return closing(source, r, c, sr, sc, ')');
        }
        TTuple::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        TTuple::Type(e, r, c) => return to_type_report(source, context, e, r, c),
        TTuple::Repr(e, r, c) => return to_repr_report(source, e, r, c),
        TTuple::End(r, c) | TTuple::IndentEnd(r, c) => return closing(source, r, c, sr, sc, ')'),
        TTuple::IndentRepr(r, c) => problem(
            "MISSING REPRESENTATION",
            r,
            c,
            "Expected a representation bound after `:`.",
            "Add a bound such as `Storable`, indented inside the parentheses.",
        ),
        TTuple::Open(r, c) => match source.what_is_next(r, c) {
            Next::Keyword(keyword) => reserved(r, c, keyword, "type"),
            _ => problem(
                "EXPECTED TYPE",
                r,
                c,
                "Expected a type after `(`.",
                "Add a type or close an empty tuple with `)`.",
            ),
        },
        TTuple::IndentType1(r, c) | TTuple::IndentTypeN(r, c) => problem(
            "EXPECTED TYPE",
            r,
            c,
            "Expected an indented tuple element type.",
            "Add a type inside the parentheses; separate elements with commas.",
        ),
    };
    wide(report, sr, sc)
}

pub(crate) fn to_type_param_report(
    source: &Source<'_>,
    error: &TypeParam<'_>,
    sr: Row,
    sc: Col,
) -> Report {
    let report = match *error {
        TypeParam::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        TypeParam::Repr(e, r, c) => return to_repr_report(source, e, r, c),
        TypeParam::End(r, c) | TypeParam::IndentEnd(r, c) => {
            return closing(source, r, c, sr, sc, ')');
        }
        TypeParam::Start(r, c) => problem(
            "MISSING TYPE PARAMETER",
            r,
            c,
            "Expected a type parameter.",
            "Use a quoted name such as `'a`, or a bounded parameter such as `('a : Storable)`.",
        ),
        TypeParam::Colon(r, c) | TypeParam::IndentColon(r, c) => problem(
            "MISSING REPRESENTATION COLON",
            r,
            c,
            "Expected `:` after the type parameter.",
            "Write `('a : Storable)` to specify a representation bound.",
        ),
        TypeParam::IndentRepr(r, c) => problem(
            "MISSING REPRESENTATION",
            r,
            c,
            "Expected an indented representation bound.",
            "Add a bound such as `Storable` after `:`.",
        ),
    };
    wide(report, sr, sc)
}

pub(super) fn to_repr_report(source: &Source<'_>, error: &Repr<'_>, sr: Row, sc: Col) -> Report {
    let report = match *error {
        Repr::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        Repr::Arrow(r, c) => problem(
            "REPRESENTATION ARROW",
            r,
            c,
            "Representation bounds cannot contain arrows.",
            "Use a bound such as `Storable`; constructor kinds are inferred.",
        ),
        Repr::Start(r, c) => problem(
            "MISSING REPRESENTATION",
            r,
            c,
            "Expected a representation bound.",
            "Use `Big`, `Const`, `Term`, or `Storable`.",
        ),
        Repr::Name(name, r, c) => problem(
            "UNKNOWN REPRESENTATION",
            r,
            c,
            &format!("Unknown representation bound `{name}`."),
            "Use `Big`, `Const`, `Term`, or `Storable`.",
        ),
    };
    wide(report, sr, sc)
}
