use super::{Doc, Report, Source, closing, expr, pattern, problem, to_space_report, type_, wide};
use crate::code::{Next, to_keyword_region};
use nash_parse::error::{Attribute, CustomType, Decl, DeclDef, DeclType, Impl, Trait, TypeAlias};
use nash_parse::{Col, Row};
use pattern::PContext;
use type_::TContext;

pub(crate) fn to_declarations_report(source: &Source<'_>, error: &Decl<'_>) -> Report {
    match *error {
        Decl::Start(r, c) => to_decl_start_report(source, r, c),
        Decl::Space(ref e, r, c) => to_space_report(source, e, r, c),
        Decl::Type(e, r, c) => to_decl_type_report(source, e, r, c),
        Decl::Def(name, e, r, c) => to_decl_def_report(source, name, e, r, c),
        Decl::FreshLineAfterDocComment(r, c) => problem(
            "EXPECTED DECLARATION",
            r,
            c,
            "Expected a declaration after the doc comment.",
            "Start the declaration on a new line without indentation.",
        ),
        Decl::Attribute(e, r, c) => to_attribute_report(source, e, r, c),
        Decl::Trait(e, r, c) => to_trait_report(source, e, r, c),
        Decl::Impl(e, r, c) => to_impl_report(source, e, r, c),
    }
}

pub(crate) fn to_decl_start_report(source: &Source<'_>, row: Row, col: Col) -> Report {
    match source.what_is_next(row, col) {
        Next::Close(_, ch) => problem(
            "STRAY DELIMITER",
            row,
            col,
            &format!("Unexpected closing `{ch}`."),
            "Remove the unmatched delimiter.",
        ),
        Next::Keyword(keyword) => Report::snippet(
            "RESERVED WORD",
            to_keyword_region(row, col, keyword),
            None,
            Doc::text(format!(
                "Unexpected `{keyword}` at the start of a declaration."
            )),
            Doc::text(match keyword {
                "import" => "Move imports before the module's definitions.",
                "case" | "if" => "Put the expression inside a named definition.",
                _ => "Use a name that is not reserved.",
            }),
        ),
        Next::Upper(name) => problem(
            "UNEXPECTED CAPITAL LETTER",
            row,
            col,
            &format!("Definition name `{name}` must start lowercase."),
            "Use a lowercase value name; uppercase names identify constructors.",
        ),
        _ => problem(
            "EXPECTED DECLARATION",
            row,
            col,
            "Expected a declaration.",
            "Start a named definition here, or indent this line to continue the preceding definition.",
        ),
    }
}

pub(super) fn to_decl_type_report(
    source: &Source<'_>,
    error: &DeclType<'_>,
    sr: Row,
    sc: Col,
) -> Report {
    match *error {
        DeclType::Space(ref e, r, c) => to_space_report(source, e, r, c),
        DeclType::Alias(e, r, c) => to_type_alias_report(source, e, r, c),
        DeclType::Union(e, r, c) => to_custom_type_report(source, e, r, c),
        DeclType::Name(r, c) | DeclType::IndentName(r, c) => wide(
            problem(
                "EXPECTED TYPE NAME",
                r,
                c,
                "Expected a type name.",
                "Use lowercase for a little type or uppercase for a Big type.",
            ),
            sr,
            sc,
        ),
    }
}

pub(super) fn to_type_alias_report(
    source: &Source<'_>,
    error: &TypeAlias<'_>,
    sr: Row,
    sc: Col,
) -> Report {
    let report = match *error {
        TypeAlias::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        TypeAlias::Param(e, r, c) => return type_::to_type_param_report(source, e, r, c),
        TypeAlias::Body(e, r, c) => {
            return type_::to_type_report(source, TContext::TypeAlias, e, r, c);
        }
        TypeAlias::Name(r, c) => problem(
            "MISSING ALIAS NAME",
            r,
            c,
            "Expected a name after `type alias`.",
            "Name the alias before its parameters and body.",
        ),
        TypeAlias::Equals(r, c) | TypeAlias::IndentEquals(r, c) => problem(
            "MISSING EQUALS",
            r,
            c,
            "Expected `=` in the type alias.",
            "Write `type alias Name = Type`.",
        ),
        TypeAlias::IndentBody(r, c) => problem(
            "MISSING TYPE",
            r,
            c,
            "Expected a type after `=`.",
            "Add an indented alias body.",
        ),
    };
    wide(report, sr, sc)
}

pub(super) fn to_custom_type_report(
    source: &Source<'_>,
    error: &CustomType<'_>,
    sr: Row,
    sc: Col,
) -> Report {
    let report = match *error {
        CustomType::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        CustomType::Param(e, r, c) => return type_::to_type_param_report(source, e, r, c),
        CustomType::VariantArg(e, r, c) | CustomType::FieldType(e, r, c) => {
            return type_::to_type_report(source, TContext::CustomType, e, r, c);
        }
        CustomType::Name(r, c) => problem(
            "MISSING TYPE NAME",
            r,
            c,
            "Expected a datatype name.",
            "Use lowercase for a little type or uppercase for a Big type.",
        ),
        CustomType::Equals(r, c) | CustomType::IndentEquals(r, c) => problem(
            "MISSING EQUALS",
            r,
            c,
            "Expected `=` before the variants.",
            "Separate the datatype name and variants with `=`.",
        ),
        CustomType::Bar(r, c) | CustomType::IndentBar(r, c) => problem(
            "MISSING VARIANT SEPARATOR",
            r,
            c,
            "Expected `|` between variants.",
            "Separate each pair of variants with `|`.",
        ),
        CustomType::Variant(r, c)
        | CustomType::IndentAfterBar(r, c)
        | CustomType::IndentAfterEquals(r, c) => problem(
            "MISSING VARIANT",
            r,
            c,
            "Expected a constructor name.",
            "Start the constructor name with an uppercase letter.",
        ),
        CustomType::Field(r, c) | CustomType::IndentField(r, c) => problem(
            "MISSING FIELD",
            r,
            c,
            "Expected a constructor field name.",
            "Write a lowercase field name followed by `:` and its type.",
        ),
        CustomType::FieldColon(r, c) => problem(
            "MISSING COLON",
            r,
            c,
            "Expected `:` after the constructor field name.",
            "Separate the field name and type with `:`.",
        ),
        CustomType::FieldEnd(opening, r, c) => {
            closing(source, r, c, opening.line, opening.column, '}')
        }
        CustomType::IndentFieldType(r, c) => problem(
            "MISSING TYPE",
            r,
            c,
            "Expected a constructor field type.",
            "Add an indented type after `:`.",
        ),
    };
    wide(report, sr, sc)
}

pub(super) fn to_decl_def_report(
    source: &Source<'_>,
    name: &str,
    error: &DeclDef<'_>,
    sr: Row,
    sc: Col,
) -> Report {
    let report = match *error {
        DeclDef::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        DeclDef::Type(e, r, c) => {
            return type_::to_type_report(source, TContext::Annotation(name), e, r, c);
        }
        DeclDef::Arg(e, r, c) => return pattern::to_pattern_report(source, PContext::Arg, e, r, c),
        DeclDef::Body(e, r, c) => {
            return expr::to_expr_report(source, expr::Context::InDef(name, sr, sc), e, r, c);
        }
        DeclDef::Equals(r, c) => match source.what_is_next(r, c) {
            Next::Keyword(keyword) => Report::snippet(
                "RESERVED WORD",
                to_keyword_region(r, c, keyword),
                None,
                Doc::text(format!(
                    "Reserved word `{keyword}` cannot be an argument name."
                )),
                Doc::text("Choose another name."),
            ),
            Next::Operator("->") => problem(
                "MISSING COLON",
                r,
                c,
                "Unexpected `->` in a definition's arguments.",
                "If this is a type annotation, add `:` after the definition name.",
            ),
            _ => problem(
                "MISSING EQUALS",
                r,
                c,
                &format!("Expected `=` in the definition of `{name}`."),
                "Separate the arguments and body with `=`.",
            ),
        },
        DeclDef::NameRepeat(r, c) => problem(
            "MISSING DEFINITION",
            r,
            c,
            &format!("Expected the definition of `{name}` after its annotation."),
            "Repeat the annotated name and add its arguments and body.",
        ),
        DeclDef::NameMatch(found, r, c) => problem(
            "NAME MISMATCH",
            r,
            c,
            &format!("Expected definition `{name}`, found `{found}`."),
            "Use the same name for the annotation and definition.",
        ),
        DeclDef::IndentType(r, c) => problem(
            "MISSING TYPE",
            r,
            c,
            "Expected a type after `:`.",
            "Add an indented type.",
        ),
        DeclDef::IndentEquals(r, c) => problem(
            "MISSING EQUALS",
            r,
            c,
            "Expected `=` before the definition body.",
            "Add `=` after the name and argument patterns.",
        ),
        DeclDef::IndentBody(r, c) => problem(
            "MISSING EXPRESSION",
            r,
            c,
            "Expected an expression after `=`.",
            "Add an indented body, or `todo` for unfinished code.",
        ),
    };
    wide(report, sr, sc)
}

pub(super) fn to_attribute_report(
    source: &Source<'_>,
    error: &Attribute<'_>,
    sr: Row,
    sc: Col,
) -> Report {
    let report = match *error {
        Attribute::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        Attribute::Arg(e, r, c) => {
            return expr::to_expr_report(source, expr::Context::InDestruct(sr, sc), e, r, c);
        }
        Attribute::Name(r, c) => problem(
            "MISSING ATTRIBUTE NAME",
            r,
            c,
            "Expected a name after `@`.",
            "Write the attribute name directly after `@`.",
        ),
        Attribute::End(opening, r, c) | Attribute::IndentEnd(opening, r, c) => {
            closing(source, r, c, opening.line, opening.column, ')')
        }
        Attribute::FreshLine(r, c) => problem(
            "ATTRIBUTE PLACEMENT",
            r,
            c,
            "Attributes must occupy their own lines.",
            "Move the following attribute or declaration to a new line.",
        ),
        Attribute::IndentArg(r, c) => problem(
            "MISSING ATTRIBUTE ARGUMENT",
            r,
            c,
            "Expected an attribute argument.",
            "Add an indented expression inside the parentheses.",
        ),
    };
    wide(report, sr, sc)
}

pub(super) fn to_trait_report(source: &Source<'_>, error: &Trait<'_>, sr: Row, sc: Col) -> Report {
    let report = match *error {
        Trait::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        Trait::Param(e, r, c) => return type_::to_type_param_report(source, e, r, c),
        Trait::Super(e, r, c) => {
            return type_::to_type_report(source, TContext::Superclass, e, r, c);
        }
        Trait::Type(e, r, c) => {
            return type_::to_type_report(source, TContext::TraitMethod, e, r, c);
        }
        Trait::Default(name, e, r, c) => return expr::to_let_def_report(source, name, e, r, c),
        Trait::Name(r, c) | Trait::IndentName(r, c) => problem(
            "MISSING TRAIT NAME",
            r,
            c,
            "Expected a trait name.",
            "Use an uppercase name, such as `Eq`.",
        ),
        Trait::SuperArg(r, c) => problem(
            "BAD SUPERCLASS ARGUMENT",
            r,
            c,
            "Superclass argument must be a trait parameter.",
            "Use one of this trait's quoted type parameters.",
        ),
        Trait::Where(r, c) | Trait::IndentWhere(r, c) => problem(
            "MISSING TRAIT WHERE",
            r,
            c,
            "Expected `where` before the trait methods.",
            "Add `where` after the trait parameters and constraints.",
        ),
        Trait::MethodName(r, c) | Trait::IndentMethod(r, c) => problem(
            "MISSING TRAIT METHOD",
            r,
            c,
            "Expected an indented method signature.",
            "Write `method : Type`.",
        ),
        Trait::Colon(r, c) | Trait::IndentColon(r, c) => problem(
            "MISSING METHOD COLON",
            r,
            c,
            "Expected `:` after the method name.",
            "Separate the method name and its type with `:`.",
        ),
        Trait::IndentParam(r, c) => problem(
            "MISSING TRAIT PARAMETER",
            r,
            c,
            "Expected an indented type parameter.",
            "Add a quoted parameter such as `'a`.",
        ),
        Trait::IndentType(r, c) => problem(
            "MISSING METHOD TYPE",
            r,
            c,
            "Expected a method type after `:`.",
            "Indent the type farther than the method name.",
        ),
        Trait::Alignment(indent, r, c) => problem(
            "INDENTATION",
            r,
            c,
            "Trait methods must align.",
            &format!("Start each method in column {indent}."),
        ),
    };
    wide(report, sr, sc)
}

pub(super) fn to_impl_report(source: &Source<'_>, error: &Impl<'_>, sr: Row, sc: Col) -> Report {
    let report = match *error {
        Impl::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        Impl::Head(e, r, c) => return type_::to_type_report(source, TContext::ImplHead, e, r, c),
        Impl::Method(name, e, r, c) => return expr::to_let_def_report(source, name, e, r, c),
        Impl::BadHead(r, c) | Impl::IndentHead(r, c) => problem(
            "BAD IMPL HEAD",
            r,
            c,
            "Expected a trait name and type arguments.",
            "Write `impl Trait Type where`.",
        ),
        Impl::Where(r, c) | Impl::IndentWhere(r, c) => problem(
            "MISSING IMPL WHERE",
            r,
            c,
            "Expected `where` after the impl head.",
            "Add `where`, then indent the method definitions.",
        ),
        Impl::MethodName(r, c) | Impl::IndentMethod(r, c) => problem(
            "MISSING IMPL METHOD",
            r,
            c,
            "Expected a method definition.",
            "Write the method name and arguments, followed by `=` and its body.",
        ),
        Impl::Alignment(indent, r, c) => problem(
            "INDENTATION",
            r,
            c,
            "Impl methods must align.",
            &format!("Start each method in column {indent}."),
        ),
    };
    wide(report, sr, sc)
}
