use super::{Doc, Report, Source, problem, to_space_report, wide};
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
pub(crate) fn to_type_report(
    source: &Source<'_>,
    context: TContext<'_>,
    error: &Type<'_>,
    sr: Row,
    sc: Col,
) -> Report {
    let thing = match context {
        TContext::Annotation(_) => "type annotation",
        TContext::CustomType => "custom type",
        TContext::TypeAlias => "type alias",
        TContext::Superclass => "superclass constraint",
        TContext::TraitMethod => "trait method type",
        TContext::ImplHead => "impl head",
    };
    let expected = match context {
        TContext::Superclass => {
            "I was expecting a trait name and its type arguments next. For example, Eq 'a."
        }
        TContext::ImplHead => {
            "I was expecting a trait name and its type arguments next. For example, Show int."
        }
        TContext::Annotation(_)
        | TContext::CustomType
        | TContext::TypeAlias
        | TContext::TraitMethod => {
            "I was expecting to see a type next. Try putting int or string for now?"
        }
    };
    let report = match *error {
        Type::Record(e, r, c) => return to_t_record_report(source, context, e, r, c),
        Type::Tuple(e, r, c) => return to_t_tuple_report(source, context, e, r, c),
        Type::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        Type::Start(r, c) => match source.what_is_next(r, c) {
            Next::Keyword(k) => Report::snippet(
                "RESERVED WORD",
                to_keyword_region(r, c, k),
                None,
                Doc::reflow(
                    "I was expecting to see a type next, but I got stuck on this reserved word:",
                ),
                Doc::reflow(&format!(
                    "It looks like you are trying to use `{k}` as a type variable, but it is a reserved word. Try using a different name!"
                )),
            ),
            _ => {
                let something = match context {
                    TContext::Annotation(name) => format!("the `{name}` type annotation"),
                    TContext::CustomType => "a custom type".into(),
                    TContext::TypeAlias => "a type alias".into(),
                    TContext::Superclass => "a superclass constraint".into(),
                    TContext::TraitMethod => "a trait method type".into(),
                    TContext::ImplHead => "an impl head".into(),
                };
                problem(
                    &format!("PROBLEM IN {}", thing.to_uppercase()),
                    r,
                    c,
                    &format!("I was partway through parsing {something}, but I got stuck here:"),
                    expected,
                )
            }
        },
        Type::IndentStart(r, c) => with_note(
            problem(
                &format!("UNFINISHED {}", thing.to_uppercase()),
                r,
                c,
                &format!(
                    "I was partway through parsing {} {thing}, but I got stuck here:",
                    if matches!(context, TContext::ImplHead) {
                        "an"
                    } else {
                        "a"
                    }
                ),
                expected,
            ),
            Doc::to_simple_note(
                "I can get confused by indentation. If you think there is already a type next, maybe it is not indented enough?",
            ),
        ),
        Type::VarStart(r, c) => problem(
            "MISSING TYPE VARIABLE",
            r,
            c,
            "I just saw a quote marking a type variable, but I got stuck here:",
            "Type variables have a quote followed by a lower-case name, such as 'a or 'result.",
        ),
        Type::Context(r, c) => problem(
            "BAD TYPE CONSTRAINT",
            r,
            c,
            "I was parsing a type constraint, but I got stuck here:",
            "Write a trait followed by its type arguments before =>. For example: `Eq 'a => 'a -> 'a -> bool`. Separate multiple constraints with commas inside parentheses.",
        ),
        Type::IndentAfterContext(r, c) => problem(
            "UNFINISHED CONSTRAINED TYPE",
            r,
            c,
            "I just saw => after the type constraints, but I got stuck here:",
            "Add the type after => and indent it farther than the start of the annotation.",
        ),
    };
    wide(report, sr, sc)
}
fn with_note(mut report: Report, note: Doc) -> Report {
    report.after = Doc::stack([report.after, note]);
    report
}
fn field_keyword(r: Row, c: Col, k: &str, before: &str) -> Report {
    Report::snippet(
        "RESERVED WORD",
        to_keyword_region(r, c, k),
        None,
        Doc::reflow(before),
        Doc::reflow(&format!(
            "It looks like you are trying to use `{k}` as a field name, but that is a reserved word. Try using a different name!"
        )),
    )
}
pub(super) fn to_t_record_report(
    source: &Source<'_>,
    context: TContext<'_>,
    error: &TRecord<'_>,
    sr: Row,
    sc: Col,
) -> Report {
    let before = "I am partway through parsing a record type, but I got stuck here:";
    let report = match *error {
        TRecord::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        TRecord::Type(e, r, c) => return to_type_report(source, context, e, r, c),
        TRecord::Open(r, c) => match source.what_is_next(r, c) {
            Next::Keyword(k) => field_keyword(
                r,
                c,
                k,
                "I just started parsing a record type, but I got stuck on this field name:",
            ),
            _ => problem(
                "UNFINISHED RECORD TYPE",
                r,
                c,
                "I just started parsing a record type, but I got stuck here:",
                "Record types look like { name : string, age : int }, so I was expecting to see a field name next.",
            ),
        },
        TRecord::End(r, c) => with_note(
            problem(
                "UNFINISHED RECORD TYPE",
                r,
                c,
                before,
                "I was expecting to see a closing curly brace before this, so try adding a } and see if that helps?",
            ),
            Doc::to_simple_note(
                "When I get stuck like this, it usually means that there is a missing parenthesis or bracket somewhere earlier. It could also be a stray keyword or operator.",
            ),
        ),
        TRecord::Field(r, c) => match source.what_is_next(r, c) {
            Next::Keyword(k) => field_keyword(
                r,
                c,
                k,
                "I am partway through parsing a record type, but I got stuck on this field name:",
            ),
            Next::Other(Some(',')) => with_note(
                problem(
                    "EXTRA COMMA",
                    r,
                    c,
                    before,
                    "I am seeing two commas in a row. This is the second one! Just delete one of the commas and you should be all set!",
                ),
                note_for_record_type_error(false),
            ),
            Next::Close(_, '}') => with_note(
                problem(
                    "EXTRA COMMA",
                    r,
                    c,
                    before,
                    "Trailing commas are not allowed in record types. Try deleting the comma that appears before this closing curly brace.",
                ),
                note_for_record_type_error(false),
            ),
            _ => with_note(
                problem(
                    "PROBLEM IN RECORD TYPE",
                    r,
                    c,
                    before,
                    "I was expecting to see another record field defined next, so I am looking for a name like userName or plantHeight.",
                ),
                note_for_record_type_error(false),
            ),
        },
        TRecord::Colon(r, c) => with_note(
            problem(
                "UNFINISHED RECORD TYPE",
                r,
                c,
                before,
                "I just saw a field name, so I was expecting to see a colon next. So try putting a : sign here?",
            ),
            note_for_record_type_error(false),
        ),
        TRecord::IndentOpen(r, c) => with_note(
            problem(
                "UNFINISHED RECORD TYPE",
                r,
                c,
                "I just saw the opening curly brace of a record type, but then I got stuck here:",
                "I am expecting a record like { name : string, age : int } here. Try defining some fields of your own?",
            ),
            note_for_record_type_error(true),
        ),
        TRecord::IndentEnd(r, c) => match source.next_line_starts_with_close_curly(r) {
            Some((r, c)) => with_note(
                problem(
                    "NEED MORE INDENTATION",
                    r,
                    c,
                    "I was partway through parsing a record type, but I got stuck here:",
                    "I need this curly brace to be indented more. Try adding some spaces before it!",
                ),
                note_for_record_type_error(false),
            ),
            None => with_note(
                problem(
                    "UNFINISHED RECORD TYPE",
                    r,
                    c,
                    "I was partway through parsing a record type, but I got stuck here:",
                    "I was expecting to see a closing curly brace next. Try putting a } next and see if that helps?",
                ),
                note_for_record_type_error(true),
            ),
        },
        TRecord::IndentField(r, c) => with_note(
            problem(
                "UNFINISHED RECORD TYPE",
                r,
                c,
                "I am partway through parsing a record type, but I got stuck after that last comma:",
                "Trailing commas are not allowed in record types, so the fix may be to delete that last comma? Or maybe you were in the middle of defining an additional field?",
            ),
            note_for_record_type_error(true),
        ),
        TRecord::IndentColon(r, c) => with_note(
            problem(
                "UNFINISHED RECORD TYPE",
                r,
                c,
                "I am partway through parsing a record type. I just saw a record field, so I was expecting to see a colon next:",
                "Try putting a : followed by a type?",
            ),
            note_for_record_type_error(true),
        ),
        TRecord::IndentType(r, c) => with_note(
            problem(
                "UNFINISHED RECORD TYPE",
                r,
                c,
                "I am partway through parsing a record type, and I was expecting to run into a type next:",
                "Try putting something like int or string for now?",
            ),
            note_for_record_type_error(true),
        ),
    };
    wide(report, sr, sc)
}
fn note_for_record_type_error(indent: bool) -> Doc {
    Doc::stack([
        Doc::to_simple_note(if indent {
            "I may be confused by indentation. For example, if you are trying to define a record type across multiple lines, I recommend using this format:"
        } else {
            "If you are trying to define a record type across multiple lines, I recommend using this format:"
        }),
        Doc::indent(
            4,
            Doc::vcat([
                Doc::text("{ name : string"),
                Doc::text(", age : int"),
                Doc::text(", value : 'a"),
                Doc::text("}"),
            ]),
        ),
        Doc::reflow(
            "Notice that each line starts with some indentation. Usually two or four spaces. This is the stylistic convention in the Nash ecosystem.",
        ),
    ])
}
pub(super) fn to_t_tuple_report(
    source: &Source<'_>,
    context: TContext<'_>,
    error: &TTuple<'_>,
    sr: Row,
    sc: Col,
) -> Report {
    let report = match *error {
        TTuple::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        TTuple::Type(e, r, c) => return to_type_report(source, context, e, r, c),
        TTuple::Repr(e, r, c) => return to_repr_report(source, e, r, c),
        TTuple::IndentRepr(r, c) => problem(
            "MISSING REPRESENTATION",
            r,
            c,
            "I was parsing a representation annotation, but I got stuck here:",
            "Add a representation bound such as Storable after the colon, and keep it indented inside the parentheses.",
        ),
        TTuple::Open(r, c) => match source.what_is_next(r, c) {
            Next::Keyword(k) => Report::snippet(
                "RESERVED WORD",
                to_keyword_region(r, c, k),
                None,
                Doc::reflow("I ran into a reserved word unexpectedly:"),
                Doc::reflow(&format!(
                    "It looks like you are trying to use `{k}` as a variable name, but it is a reserved word. Try using a different name!"
                )),
            ),
            _ => problem(
                "UNFINISHED PARENTHESES",
                r,
                c,
                "I just saw an open parenthesis, so I was expecting to see a type next.",
                "Something like (option int) or (list 'a). Anything where you are putting parentheses around normal types.",
            ),
        },
        TTuple::End(r, c) => with_note(
            problem(
                "UNFINISHED PARENTHESES",
                r,
                c,
                "I was expecting to see a closing parenthesis next, but I got stuck here:",
                "Try adding a ) to see if that helps?",
            ),
            Doc::to_simple_note(
                "I can get stuck when I run into keywords, operators, parentheses, or brackets unexpectedly. So there may be some earlier syntax trouble (like extra parentheses or missing brackets) that is confusing me.",
            ),
        ),
        TTuple::IndentType1(r, c) => with_note(
            problem(
                "UNFINISHED PARENTHESES",
                r,
                c,
                "I just saw an open parenthesis, so I was expecting to see a type next.",
                "Something like (option int) or (list 'a). Anything where you are putting parentheses around normal types.",
            ),
            Doc::to_simple_note(
                "I can get confused by indentation in cases like this, so maybe you have a type but it is not indented enough?",
            ),
        ),
        TTuple::IndentTypeN(r, c) => with_note(
            problem(
                "UNFINISHED TUPLE TYPE",
                r,
                c,
                "I think I am in the middle of parsing a tuple type. I just saw a comma, so I was expecting to see a type next.",
                "A tuple type looks like (int,int) or (string,'a), so I think there is a type missing here?",
            ),
            Doc::to_simple_note(
                "I can get confused by indentation in cases like this, so maybe you have a type but it is not indented enough?",
            ),
        ),
        TTuple::IndentEnd(r, c) => with_note(
            problem(
                "UNFINISHED PARENTHESES",
                r,
                c,
                "I was expecting to see a closing parenthesis next:",
                "Try adding a ) to see if that helps!",
            ),
            Doc::to_simple_note(
                "I can get confused by indentation in cases like this, so maybe you have a closing parenthesis but it is not indented enough?",
            ),
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
        TypeParam::Start(r, c) => problem(
            "MISSING TYPE PARAMETER",
            r,
            c,
            "I was parsing a type parameter, but I got stuck here:",
            "Type parameters start with a quote, such as 'a. A representation annotation looks like ('a : Storable).",
        ),
        TypeParam::Colon(r, c) | TypeParam::IndentColon(r, c) => problem(
            "MISSING REPRESENTATION COLON",
            r,
            c,
            "I have the type parameter name, but I got stuck here:",
            "Put a colon between the type parameter and its representation bound, as in ('a : Storable).",
        ),
        TypeParam::End(r, c) | TypeParam::IndentEnd(r, c) => problem(
            "UNFINISHED TYPE PARAMETER",
            r,
            c,
            "I was parsing an annotated type parameter, but I got stuck here:",
            "Add a closing parenthesis after the representation bound, as in ('a : Storable).",
        ),
        TypeParam::IndentRepr(r, c) => problem(
            "MISSING REPRESENTATION",
            r,
            c,
            "I just saw a colon after the type parameter, but I got stuck here:",
            "Add a representation bound such as Storable, indented inside the parentheses.",
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
            "I found an arrow inside a representation bound:",
            "Representation bounds do not have arrows. Use a bound such as Storable; the compiler infers constructor kinds from type use.",
        ),
        Repr::Start(r, c) => problem(
            "MISSING REPRESENTATION",
            r,
            c,
            "I was expecting a representation bound here:",
            "Write a representation name such as Storable after the colon.",
        ),
        Repr::Name(name, r, c) => problem(
            "UNKNOWN REPRESENTATION",
            r,
            c,
            &format!("I do not recognize `{name}` as a representation bound:"),
            "Use a supported representation bound: Big, Const, Term, or Storable.",
        ),
    };
    wide(report, sr, sc)
}
