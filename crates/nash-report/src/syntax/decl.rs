use super::{Doc, Report, Source, expr, pattern, problem, to_space_report, type_, wide};
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
            "EXPECTING DECLARATION",
            r,
            c,
            "I just saw a doc comment, but then I got stuck here:",
            "I was expecting to see the corresponding declaration next, starting on a fresh line with no indentation.",
        ),
        Decl::Attribute(e, r, c) => to_attribute_report(source, e, r, c),
        Decl::Trait(e, r, c) => to_trait_report(source, e, r, c),
        Decl::Impl(e, r, c) => to_impl_report(source, e, r, c),
    }
}

pub(crate) fn to_decl_start_report(source: &Source<'_>, r: Row, c: Col) -> Report {
    match source.what_is_next(r, c) {
        Next::Close(term, ch) => problem(
            &format!("STRAY {}", term.to_uppercase()),
            r,
            c,
            &format!("I was not expecting to see a {term} here:"),
            &format!("This {ch} does not match up with an earlier open {term}. Try deleting it?"),
        ),
        Next::Keyword(k) => {
            let after = match k {
                "import" => {
                    "It is reserved for declaring imports at the top of your module. If you want another import, try moving it up top with the other imports. If you want to define a value or function, try changing the name to something else!"
                }
                "case" => {
                    "It is reserved for writing `case` expressions. Try using a different name? If you are trying to write a `case` expression, it needs to be part of a definition."
                }
                "if" => {
                    "It is reserved for writing `if` expressions. Try using a different name? If you are trying to write an `if` expression, it needs to be part of a definition."
                }
                _ => "It is a reserved word. Try changing the name to something else?",
            };
            Report::snippet(
                "RESERVED WORD",
                to_keyword_region(r, c, k),
                None,
                Doc::reflow(&format!(
                    "I was not expecting to run into the `{k}` keyword here:"
                )),
                Doc::reflow(after),
            )
        }
        Next::Upper(name) => {
            let lower = name
                .chars()
                .next()
                .map(|ch| ch.to_lowercase().to_string() + &name[ch.len_utf8()..])
                .unwrap_or_default();
            let mut report = problem(
                "UNEXPECTED CAPITAL LETTER",
                r,
                c,
                "Declarations always start with a lower-case letter, so I am getting stuck here:",
                &format!("Try a name like {lower} instead?"),
            );
            report.after = Doc::stack([
                report.after,
                decl_def_note(),
                Doc::reflow(
                    "Notice that they always start with a lower-case letter. Capitalization matters!",
                ),
            ]);
            report
        }
        Next::Operator(op) => with_note(
            problem(
                "UNEXPECTED SYMBOL",
                r,
                c,
                &format!("I am getting stuck because this line starts with the {op} symbol:"),
                "When a line has no spaces at the beginning, I expect it to be a declaration. If this is not supposed to be a declaration, try adding some spaces before it?",
            ),
            decl_def_note(),
        ),
        Next::Other(Some(ch))
            if [
                '(', '{', '[', '+', '-', '*', '/', '^', '&', '|', '"', '\'', '!', '@', '#', '$',
                '%',
            ]
            .contains(&ch) =>
        {
            with_note(
                problem(
                    "UNEXPECTED SYMBOL",
                    r,
                    c,
                    &format!("I am getting stuck because this line starts with the {ch} symbol:"),
                    "When a line has no spaces at the beginning, I expect it to be a declaration. If this is not supposed to be a declaration, try adding some spaces before it?",
                ),
                decl_def_note(),
            )
        }
        Next::Lower(_) | Next::Other(_) => with_note(
            problem(
                "WEIRD DECLARATION",
                r,
                c,
                "I am trying to parse a declaration, but I am getting stuck here:",
                "When a line has no spaces at the beginning, I expect it to be a declaration. Try to make your declaration look like the example? Or if this is not supposed to be a declaration, try adding some spaces before it?",
            ),
            decl_def_note(),
        ),
    }
}
fn with_note(mut report: Report, note: Doc) -> Report {
    report.after = Doc::stack([report.after, note]);
    report
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
            with_note(
                problem(
                    "EXPECTING TYPE NAME",
                    r,
                    c,
                    "I think I am parsing a type declaration, but I got stuck here:",
                    "I was expecting a name like status or option next. Nash uses lower-case names for little types and capitalized names for Big types.",
                ),
                custom_type_note(),
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
        TypeAlias::Body(e, r, c) => {
            return type_::to_type_report(source, TContext::TypeAlias, e, r, c);
        }
        TypeAlias::Param(e, r, c) => return type_::to_type_param_report(source, e, r, c),
        TypeAlias::Name(r, c) => problem(
            "EXPECTING TYPE ALIAS NAME",
            r,
            c,
            "I am partway through parsing a type alias, but I got stuck here:",
            "I was expecting a name like account or point next. Nash uses lower-case names for little aliases and capitalized names for Big aliases.",
        ),
        TypeAlias::Equals(r, c) => match source.what_is_next(r, c) {
            Next::Keyword(k) => Report::snippet(
                "RESERVED WORD",
                to_keyword_region(r, c, k),
                None,
                Doc::reflow(
                    "I ran into a reserved word unexpectedly while parsing this type alias:",
                ),
                Doc::reflow(&format!(
                    "It looks like you are trying to use `{k}` as a type variable, but it is a reserved word. Try using a different name?"
                )),
            ),
            _ => problem(
                "PROBLEM IN TYPE ALIAS",
                r,
                c,
                "I am partway through parsing a type alias, but I got stuck here:",
                "I was expecting to see a type variable or an equals sign next.",
            ),
        },
        TypeAlias::IndentEquals(r, c) => problem(
            "UNFINISHED TYPE ALIAS",
            r,
            c,
            "I am partway through parsing a type alias, but I got stuck here:",
            "I was expecting to see a type variable or an equals sign next.",
        ),
        TypeAlias::IndentBody(r, c) => problem(
            "UNFINISHED TYPE ALIAS",
            r,
            c,
            "I am partway through parsing a type alias, but I got stuck here:",
            "I was expecting to see a type next. Something as simple as int or string would work!",
        ),
    };
    wide(with_note(report, type_alias_note()), sr, sc)
}
fn type_alias_note() -> Doc {
    Doc::stack([
        Doc::to_simple_note("Here is an example of a valid `type alias` for reference:"),
        Doc::indent(
            4,
            Doc::text("type alias Account = { owner : Bytes, balance : Int }"),
        ),
        Doc::reflow(
            "This would let us use `Account` as a shorthand for that record type. Using this shorthand makes type annotations much easier to read, and makes changing code easier if you decide later that there is more to an account than owner and balance!",
        ),
    ])
}
fn custom_type_note() -> Doc {
    Doc::stack([
        Doc::to_simple_note("Here is an example of a valid `type` declaration for reference:"),
        Doc::indent(4, Doc::text("type option 'a = None | Some 'a")),
        Doc::reflow(
            "This defines a new `option` type with two variants. The Some variant has some associated data, allowing us to store a value when one is available. None represents the absence of a value.",
        ),
    ])
}
pub(super) fn to_custom_type_report(
    source: &Source<'_>,
    error: &CustomType<'_>,
    sr: Row,
    sc: Col,
) -> Report {
    let before = "I am partway through parsing a custom type, but I got stuck here:";
    let report = match *error {
        CustomType::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        CustomType::Param(e, r, c) => return type_::to_type_param_report(source, e, r, c),
        CustomType::VariantArg(e, r, c) | CustomType::FieldType(e, r, c) => {
            return type_::to_type_report(source, TContext::CustomType, e, r, c);
        }
        CustomType::Name(r, c) => problem(
            "EXPECTING TYPE NAME",
            r,
            c,
            "I think I am parsing a type declaration, but I got stuck here:",
            "I was expecting a name like status or option next. Nash uses lower-case names for little types and capitalized names for Big types.",
        ),
        CustomType::Equals(r, c) => match source.what_is_next(r, c) {
            Next::Keyword(k) => Report::snippet(
                "RESERVED WORD",
                to_keyword_region(r, c, k),
                None,
                Doc::reflow(
                    "I ran into a reserved word unexpectedly while parsing this custom type:",
                ),
                Doc::reflow(&format!(
                    "It looks like you are trying to use `{k}` as a type variable, but it is a reserved word. Try using a different name?"
                )),
            ),
            _ => problem(
                "PROBLEM IN CUSTOM TYPE",
                r,
                c,
                before,
                "I was expecting to see a type variable or an equals sign next.",
            ),
        },
        CustomType::Bar(r, c) => problem(
            "PROBLEM IN CUSTOM TYPE",
            r,
            c,
            before,
            "I was expecting to see a vertical bar like | next.",
        ),
        CustomType::Variant(r, c) => problem(
            "PROBLEM IN CUSTOM TYPE",
            r,
            c,
            before,
            "I was expecting to see a variant name next. Something like Success or Sandwich. Any name that starts with a capital letter really!",
        ),
        CustomType::IndentEquals(r, c) => problem(
            "UNFINISHED CUSTOM TYPE",
            r,
            c,
            before,
            "I was expecting to see a type variable or an equals sign next.",
        ),
        CustomType::IndentBar(r, c) => problem(
            "UNFINISHED CUSTOM TYPE",
            r,
            c,
            before,
            "I was expecting to see a vertical bar like | next.",
        ),
        CustomType::IndentAfterBar(r, c) => problem(
            "UNFINISHED CUSTOM TYPE",
            r,
            c,
            before,
            "I just saw a vertical bar, so I was expecting to see another variant defined next.",
        ),
        CustomType::IndentAfterEquals(r, c) => problem(
            "UNFINISHED CUSTOM TYPE",
            r,
            c,
            before,
            "I just saw an equals sign, so I was expecting to see the first variant defined next.",
        ),
        CustomType::Field(r, c) | CustomType::IndentField(r, c) => problem(
            "UNFINISHED CONSTRUCTOR FIELD",
            r,
            c,
            before,
            "I was expecting a field name next. A named constructor field looks like `owner : Bytes`.",
        ),
        CustomType::FieldColon(r, c) => problem(
            "MISSING FIELD COLON",
            r,
            c,
            before,
            "I have the field name, so I was expecting a colon followed by its type.",
        ),
        CustomType::FieldEnd(r, c) => problem(
            "UNFINISHED CONSTRUCTOR FIELDS",
            r,
            c,
            before,
            "Separate constructor fields with commas, and close the field list with }.",
        ),
        CustomType::IndentFieldType(r, c) => problem(
            "UNFINISHED FIELD TYPE",
            r,
            c,
            before,
            "I just saw a colon, so I was expecting the field type next. Indent it farther than the constructor declaration.",
        ),
    };
    wide(with_note(report, custom_type_note()), sr, sc)
}
pub(super) fn to_decl_def_report(
    source: &Source<'_>,
    name: &str,
    error: &DeclDef<'_>,
    sr: Row,
    sc: Col,
) -> Report {
    let before = format!("I got stuck while parsing the `{name}` definition:");
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
            Next::Keyword(k) => Report::snippet(
                "RESERVED WORD",
                to_keyword_region(r, c, k),
                None,
                Doc::reflow(&format!(
                    "The name `{k}` is reserved in Nash, so it cannot be used as an argument here:"
                )),
                Doc::stack([
                    Doc::reflow("Try renaming it to something else."),
                    Doc::to_simple_note(&format!(
                        "The `{k}` keyword has a special meaning in Nash, so it can only be used in certain situations."
                    )),
                ]),
            ),
            Next::Operator("->") => problem(
                "MISSING COLON?",
                r,
                c,
                "I was not expecting to see an arrow here:",
                "This usually means a : is missing a bit earlier in a type annotation. It could be something else though, so here is a valid definition for reference:",
            ),
            Next::Operator(_) => problem(
                "UNEXPECTED SYMBOL",
                r,
                c,
                "I was not expecting to see this symbol here:",
                "I am not sure what is going wrong exactly, so here is a valid definition (with an optional type annotation) for reference:",
            ),
            _ => problem(
                "PROBLEM IN DEFINITION",
                r,
                c,
                &before,
                "I am not sure what is going wrong exactly, so here is a valid definition (with an optional type annotation) for reference:",
            ),
        },
        DeclDef::NameRepeat(r, c) => problem(
            "EXPECTING DEFINITION",
            r,
            c,
            &format!(
                "I just saw the type annotation for `{name}` so I was expecting to see its definition here:"
            ),
            "Type annotations always appear directly above the relevant definition, without anything else in between. (Not even doc comments!)",
        ),
        DeclDef::NameMatch(actual, r, c) => {
            let mut report = problem(
                "NAME MISMATCH",
                r,
                c,
                &format!(
                    "I just saw a type annotation for `{name}`, but it is followed by a definition for `{actual}`:"
                ),
                "These names do not match! Is there a typo?",
            );
            report.after = Doc::stack([
                report.after,
                Doc::indent(
                    4,
                    Doc::cat([
                        Doc::text(actual).dullyellow(),
                        Doc::text(" -> "),
                        Doc::text(name).green(),
                    ]),
                ),
            ]);
            return wide(report.with_suggestions(vec![name.to_string()]), sr, sc);
        }
        DeclDef::IndentType(r, c) => problem(
            "UNFINISHED DEFINITION",
            r,
            c,
            &format!("I got stuck while parsing the `{name}` type annotation:"),
            "I just saw a colon, so I am expecting to see a type next.",
        ),
        DeclDef::IndentEquals(r, c) => problem(
            "UNFINISHED DEFINITION",
            r,
            c,
            &before,
            "I was expecting to see an argument or an equals sign next.",
        ),
        DeclDef::IndentBody(r, c) => problem(
            "UNFINISHED DEFINITION",
            r,
            c,
            &before,
            "I was expecting to see an expression next. What is it equal to?",
        ),
    };
    let report = match *error {
        DeclDef::Equals(r, c) => match source.what_is_next(r, c) {
            Next::Keyword(_) => report,
            _ => with_note(
                report,
                Doc::stack([
                    decl_def_example(),
                    Doc::reflow(&format!(
                        "Try to use that format with your `{name}` definition!"
                    )),
                ]),
            ),
        },
        _ => with_note(report, decl_def_note()),
    };
    wide(report, sr, sc)
}
fn decl_def_example() -> Doc {
    Doc::indent(
        4,
        Doc::vcat([
            Doc::text("greet : string -> string"),
            Doc::text("greet name ="),
            Doc::text("  \"Hello \" ++ name ++ \"!\""),
        ]),
    )
}
fn decl_def_note() -> Doc {
    Doc::stack([
        Doc::reflow("Here is a valid definition (with a type annotation) for reference:"),
        Doc::indent(
            4,
            Doc::vcat([
                Doc::text("greet : string -> string"),
                Doc::text("greet name ="),
                Doc::text("  \"Hello \" ++ name ++ \"!\""),
            ]),
        ),
        Doc::reflow(
            "The top line (called a \"type annotation\") is optional. You can leave it off if you want. As you get more comfortable with Nash and as your project grows, it becomes more and more valuable to add them though! They work great as compiler-verified documentation, and they often improve error messages!",
        ),
    ])
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
            "I just saw @, but I got stuck here:",
            "Write the attribute name immediately after @, such as `@derive(Eq)`.",
        ),
        Attribute::End(r, c) | Attribute::IndentEnd(r, c) => problem(
            "UNFINISHED ATTRIBUTE",
            r,
            c,
            "I was parsing an attribute, but I got stuck here:",
            "I was expecting a comma between arguments or a closing parenthesis after the final argument.",
        ),
        Attribute::FreshLine(r, c) => problem(
            "ATTRIBUTE NEEDS FRESH LINE",
            r,
            c,
            "I finished this attribute, but I got stuck here:",
            "Put the declaration or next attribute on a fresh line with the same indentation.",
        ),
        Attribute::IndentArg(r, c) => problem(
            "MISSING ATTRIBUTE ARGUMENT",
            r,
            c,
            "I was parsing an attribute argument, but I got stuck here:",
            "Add the argument expression and indent it farther than the start of the attribute.",
        ),
    };
    wide(report, sr, sc)
}
pub(super) fn to_trait_report(source: &Source<'_>, error: &Trait<'_>, sr: Row, sc: Col) -> Report {
    let before = "I was parsing a trait declaration, but I got stuck here:";
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
            before,
            "I was expecting a capitalized trait name, such as Eq or Show.",
        ),
        Trait::SuperArg(r, c) => problem(
            "BAD SUPERCLASS ARGUMENT",
            r,
            c,
            before,
            "A superclass argument must be one of the trait's quoted type parameters, such as 'a.",
        ),
        Trait::Where(r, c) | Trait::IndentWhere(r, c) => problem(
            "MISSING TRAIT WHERE",
            r,
            c,
            before,
            "Add `where` after the trait parameters and superclass constraints, before the method declarations.",
        ),
        Trait::MethodName(r, c) | Trait::IndentMethod(r, c) => problem(
            "MISSING TRAIT METHOD",
            r,
            c,
            before,
            "I was expecting an indented method signature, such as `show : 'a -> string`.",
        ),
        Trait::Colon(r, c) | Trait::IndentColon(r, c) => problem(
            "MISSING METHOD COLON",
            r,
            c,
            before,
            "Add a colon between the method name and its type annotation.",
        ),
        Trait::IndentParam(r, c) => problem(
            "MISSING TRAIT PARAMETER",
            r,
            c,
            before,
            "Add a quoted type parameter, such as 'a, and keep it indented farther than the trait declaration.",
        ),
        Trait::IndentType(r, c) => problem(
            "MISSING METHOD TYPE",
            r,
            c,
            before,
            "I just saw a colon, so I was expecting a method type next. Indent the type farther than the method name.",
        ),
        Trait::Alignment(indent, r, c) => problem(
            "TRAIT METHOD ALIGNMENT",
            r,
            c,
            before,
            &format!("All methods in this trait must start in column {indent}."),
        ),
    };
    wide(report, sr, sc)
}
pub(super) fn to_impl_report(source: &Source<'_>, error: &Impl<'_>, sr: Row, sc: Col) -> Report {
    let before = "I was parsing an impl declaration, but I got stuck here:";
    let report = match *error {
        Impl::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        Impl::Head(e, r, c) => {
            return type_::to_type_report(source, TContext::ImplHead, e, r, c);
        }
        Impl::Method(name, e, r, c) => return expr::to_let_def_report(source, name, e, r, c),
        Impl::BadHead(r, c) | Impl::IndentHead(r, c) => problem(
            "BAD IMPL HEAD",
            r,
            c,
            before,
            "I was expecting a trait name followed by its type arguments. For example: `impl Show int where`.",
        ),
        Impl::Where(r, c) | Impl::IndentWhere(r, c) => problem(
            "MISSING IMPL WHERE",
            r,
            c,
            before,
            "Add `where` after the impl head, then indent the method definitions below it.",
        ),
        Impl::MethodName(r, c) | Impl::IndentMethod(r, c) => problem(
            "MISSING IMPL METHOD",
            r,
            c,
            before,
            "I was expecting a method definition. Write its name and arguments followed by = and the body.",
        ),
        Impl::Alignment(indent, r, c) => problem(
            "IMPL METHOD ALIGNMENT",
            r,
            c,
            before,
            &format!("All methods in this impl must start in column {indent}."),
        ),
    };
    wide(report, sr, sc)
}
