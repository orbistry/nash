//! Type error prose from Elm's `Reporting/Error/Type.hs`, adapted to Nash.

mod operators;
mod records;
#[cfg(test)]
mod tests;
mod traits;

use nash_constrain::error::{
    Category, Context, Error, Expected, MaybeName, PCategory, PContext, PExpected, SubContext,
};
use nash_constrain::error_type::{ErrorType, iterated_dealias};
use nash_region::Region;

use crate::doc::{args, ordinal};
use crate::localizer::Localizer;
use crate::render_type::Ctx;
use crate::type_diff::{self, Direction, Problem};
use crate::{Doc, Report};

/// Elm's `toReport`, including Nash's trait, representation and record errors.
pub fn to_report(localizer: &Localizer, error: &Error<'_>) -> Report {
    match error {
        Error::BadExpr(region, category, actual, expected) => {
            to_expr_report(localizer, *region, *category, actual, expected)
        }
        Error::BadPattern(region, category, actual, expected) => {
            to_pattern_report(localizer, *region, *category, actual, expected)
        }
        Error::InfiniteType {
            region,
            name,
            overall_type,
        } => to_infinite_report(localizer, *region, name, overall_type),
        Error::FieldMismatch {
            region,
            context,
            field,
            actual,
            expected,
        } => records::field_mismatch(localizer, *region, *context, field, actual, expected),
        Error::MissingField {
            region,
            context,
            field,
            record,
            available,
        } => records::missing_field(localizer, *region, *context, field, record, available),
        Error::NotARecord {
            region,
            context,
            field,
            record,
        } => records::not_a_record(localizer, *region, *context, *field, record),
        Error::UpdateNotRecord { region, record } => {
            records::update_not_record(localizer, *region, record)
        }
        Error::AmbiguousRecordAccess {
            region,
            context,
            field,
            record,
        } => records::ambiguous_access(localizer, *region, *context, *field, record),
        Error::BadKind {
            region,
            name,
            args,
            reason,
        } => traits::bad_kind(localizer, *region, name, args, reason),
        Error::AmbiguousType {
            region,
            name,
            variable,
            predicates,
        } => traits::ambiguous_type(localizer, *region, name, variable, predicates),
        Error::ContradictoryRepresentation {
            region,
            name,
            typ,
            requirements,
        } => traits::contradictory_representation(localizer, *region, name, typ, requirements),
        Error::PolymorphicRecursion {
            region,
            name,
            trait_,
            args,
        } => traits::polymorphic_recursion(localizer, *region, name, *trait_, args),
        Error::UnresolvedConstraint {
            region,
            name,
            trait_,
            args,
        } => traits::unresolved_constraint(localizer, *region, name, *trait_, args),
        Error::UnresolvedApplication {
            region,
            name,
            head,
            args,
        } => traits::unresolved_application(localizer, *region, name, head, args),
        Error::MissingImpl {
            region,
            name,
            trait_,
            args,
            available,
            because,
        } => traits::missing_impl(localizer, *region, name, *trait_, args, available, because),
        Error::ImplResolutionLimit {
            region,
            name,
            trait_,
        } => traits::resolution_limit(localizer, *region, name, *trait_),
        Error::MissingConstraint {
            region,
            name,
            trait_,
            args,
            binder,
        } => traits::missing_constraint(localizer, *region, name, *trait_, args, binder),
        Error::AnnotationVariableEscapes {
            region,
            name,
            variable,
        } => traits::annotation_variable_escapes(localizer, *region, *name, variable),
    }
}

fn to_pattern_report(
    localizer: &Localizer,
    region: Region,
    category: PCategory<'_>,
    actual: &ErrorType<'_>,
    expected: &PExpected<'_, &ErrorType<'_>>,
) -> Report {
    let (surroundings, before, seeing, instead, details, expected) = match expected {
        PExpected::NoExpectation(expected) => (
            region,
            "This pattern is being used in an unexpected way:".into(),
            "It is".into(),
            "But it needs to match:".into(),
            vec![],
            *expected,
        ),
        PExpected::FromContext(surroundings, context, expected) => {
            let (before, seeing, instead, details) = match context {
                PContext::TypedArg(name, index) => (
                    format!("The {} argument to `{name}` is weird.", ordinal(*index)),
                    "The argument is a pattern that matches".into(),
                    format!(
                        "But the type annotation on `{name}` says the {} argument should be:",
                        ordinal(*index)
                    ),
                    vec![],
                ),
                PContext::CaseMatch(0) => (
                    "The 1st pattern in this `case` is causing a mismatch:".into(),
                    "The first pattern is trying to match".into(),
                    "But the expression between `case` and `of` is:".into(),
                    vec![Doc::reflow(
                        "These can never match! Is the pattern the problem? Or is it the expression?",
                    )],
                ),
                PContext::CaseMatch(index) => (
                    format!(
                        "The {} pattern in this `case` does not match the previous ones.",
                        ordinal(*index)
                    ),
                    format!("The {} pattern is trying to match", ordinal(*index)),
                    "But all the previous patterns match:".into(),
                    vec![Doc::link(
                        "Note",
                        "A `case` expression can only handle one type of value, so you may want to use",
                        "custom-types",
                        "to handle “mixing” types.",
                    )],
                ),
                PContext::CtorArg(name, index) => (
                    format!("The {} argument to `{name}` is weird.", ordinal(*index)),
                    "It is trying to match".into(),
                    format!("But `{name}` needs its {} argument to be:", ordinal(*index)),
                    vec![],
                ),
                PContext::ListEntry(index) => (
                    format!(
                        "The {} pattern in this list does not match all the previous ones:",
                        ordinal(*index)
                    ),
                    format!("The {} pattern is trying to match", ordinal(*index)),
                    "But all the previous patterns in the list are:".into(),
                    vec![list_hint()],
                ),
                PContext::Tail => (
                    "The pattern after (::) is causing issues.".into(),
                    "The pattern after (::) is trying to match".into(),
                    "But it needs to match lists like this:".into(),
                    vec![],
                ),
            };
            (*surroundings, before, seeing, instead, details, *expected)
        }
    };
    Report::snippet(
        "TYPE MISMATCH",
        region,
        None,
        Doc::reflow(&before),
        pattern_type_comparison(
            localizer,
            actual,
            expected,
            &add_pattern_category(&seeing, category),
            &instead,
            details,
        ),
    )
    .with_region(surroundings)
}

fn pattern_type_comparison(
    localizer: &Localizer,
    actual: &ErrorType<'_>,
    expected: &ErrorType<'_>,
    seeing: &str,
    instead: &str,
    details: Vec<Doc>,
) -> Doc {
    let (actual, expected, problems) = type_diff::to_comparison(localizer, actual, expected);
    Doc::stack(
        [
            Doc::reflow(seeing),
            Doc::indent(4, actual),
            Doc::reflow(instead),
            Doc::indent(4, expected),
        ]
        .into_iter()
        .chain(problems_to_hint(&problems))
        .chain(details),
    )
}

fn add_pattern_category(seeing: &str, category: PCategory<'_>) -> String {
    format!(
        "{seeing}{}",
        match category {
            PCategory::Record => " record values of type:".into(),
            PCategory::Unit => " unit values:".into(),
            PCategory::Tuple => " tuples of type:".into(),
            PCategory::List => " lists of type:".into(),
            PCategory::Ctor(name) => format!(" `{name}` values of type:"),
            PCategory::Int => " integers:".into(),
            PCategory::Bytes => " bytes:".into(),
            PCategory::Str => " strings:".into(),
            PCategory::Bool => " booleans:".into(),
        }
    )
}

fn type_comparison(
    localizer: &Localizer,
    actual: &ErrorType<'_>,
    expected: &ErrorType<'_>,
    seeing: &str,
    instead: &str,
    details: Vec<Doc>,
) -> Doc {
    let (actual, expected, problems) = type_diff::to_comparison(localizer, actual, expected);
    Doc::stack(
        [
            Doc::reflow(seeing),
            Doc::indent(4, actual),
            Doc::reflow(instead),
            Doc::indent(4, expected),
        ]
        .into_iter()
        .chain(details)
        .chain(problems_to_hint(&problems)),
    )
}

fn lone_type(
    localizer: &Localizer,
    actual: &ErrorType<'_>,
    expected: &ErrorType<'_>,
    seeing: Doc,
    details: Vec<Doc>,
) -> Doc {
    let (actual, _, problems) = type_diff::to_comparison(localizer, actual, expected);
    Doc::stack(
        [seeing, Doc::indent(4, actual)]
            .into_iter()
            .chain(details)
            .chain(problems_to_hint(&problems)),
    )
}

fn add_category(seeing: &str, category: Category<'_>) -> String {
    match category {
        Category::Local(name) | Category::Foreign(name) => format!("This `{name}` value is a:"),
        Category::Access(field) => format!("The value at .{field} is a:"),
        Category::Accessor(field) => format!("This .{field} field access function has type:"),
        Category::If => "This `if` expression produces:".into(),
        Category::Case => "This `case` expression produces:".into(),
        Category::List => format!("{seeing} a list of type:"),
        Category::String => format!("{seeing} a string of type:"),
        Category::Lambda => format!("{seeing} an anonymous function of type:"),
        Category::Record => format!("{seeing} a record of type:"),
        Category::Tuple => format!("{seeing} a tuple of type:"),
        Category::Unit => format!("{seeing} a unit value:"),
        Category::CallResult(MaybeName::FuncName(name) | MaybeName::CtorName(name)) => {
            format!("This `{name}` call produces:")
        }
        Category::CallResult(MaybeName::NoName | MaybeName::OpName(_)) => format!("{seeing}:"),
    }
}

fn list_hint() -> Doc {
    Doc::link(
        "Hint",
        "Everything in a list must be the same type of value. This way, we never run into unexpected values partway through a List.map, List.foldl, etc. Read",
        "custom-types",
        "to learn how to “mix” types.",
    )
}

fn to_expr_report(
    localizer: &Localizer,
    region: Region,
    category: Category<'_>,
    actual: &ErrorType<'_>,
    expected: &Expected<'_, &ErrorType<'_>>,
) -> Report {
    match expected {
        Expected::NoExpectation(expected) => Report::snippet(
            "TYPE MISMATCH",
            region,
            None,
            Doc::reflow("This expression is being used in an unexpected way:"),
            type_comparison(
                localizer,
                actual,
                expected,
                &add_category("It is", category),
                "But you are trying to use it as:",
                vec![],
            ),
        ),
        Expected::FromAnnotation(name, _, context, expected) => {
            let (thing, seeing) = match context {
                SubContext::TypedIfBranch(index) => (
                    format!("{} branch of this `if` expression:", ordinal(*index)),
                    format!("The {} branch is", ordinal(*index)),
                ),
                SubContext::TypedCaseBranch(index) => (
                    format!("{} branch of this `case` expression:", ordinal(*index)),
                    format!("The {} branch is", ordinal(*index)),
                ),
                SubContext::TypedBody => (
                    format!("body of the `{name}` definition:"),
                    "The body is".into(),
                ),
            };
            Report::snippet(
                "TYPE MISMATCH",
                region,
                None,
                Doc::reflow(&format!("Something is off with the {thing}")),
                type_comparison(
                    localizer,
                    actual,
                    expected,
                    &add_category(&seeing, category),
                    &format!("But the type annotation on `{name}` says it should be:"),
                    vec![],
                ),
            )
        }
        Expected::FromContext(surroundings, context, expected) => {
            let mismatch = |problem: &str, seeing: &str, instead: &str, details: Vec<Doc>| {
                Report::snippet(
                    "TYPE MISMATCH",
                    region,
                    None,
                    Doc::reflow(problem),
                    type_comparison(
                        localizer,
                        actual,
                        expected,
                        &add_category(seeing, category),
                        instead,
                        details,
                    ),
                )
                .with_region(*surroundings)
            };
            match context {
                Context::ListEntry(index) => mismatch(&format!("The {} element of this list does not match all the previous elements:", ordinal(*index)), &format!("The {} element is", ordinal(*index)), "But all the previous elements in the list are:", vec![list_hint()]),
                Context::IfCondition => Report::snippet("TYPE MISMATCH", region, None, Doc::reflow("This `if` condition does not evaluate to a boolean value, `True` or `False`."), lone_type(localizer, actual, expected, Doc::reflow(&add_category("It is", category)), vec![Doc::reflow("But I need this `if` condition to be a `bool` value.")])).with_region(*surroundings),
                Context::IfBranch(index) | Context::CaseBranch(index) => {
                    let article = if matches!(context, Context::IfBranch(_)) { "an" } else { "a" };
                    let keyword = if matches!(context, Context::IfBranch(_)) { "if" } else { "case" };
                    mismatch(&format!("The {} branch of this `{keyword}` does not match all the previous branches:", ordinal(*index)), &format!("The {} branch is", ordinal(*index)), "But all the previous branches result in:", vec![Doc::link("Hint", &format!("All branches in {article} `{keyword}` must produce the same type of values. This way, no matter which branch we take, the result is always a consistent shape. Read"), "custom-types", "to learn how to “mix” types.")])
                }
                Context::CallArg(name, index) => {
                    let function = match name { MaybeName::NoName => "this function".into(), MaybeName::FuncName(name) | MaybeName::CtorName(name) => format!("`{name}`"), MaybeName::OpName(op) => format!("({op})") };
                    mismatch(&format!("The {} argument to {function} is not what I expect:", ordinal(*index)), "This argument is", &format!("But {function} needs the {} argument to be:", ordinal(*index)), if *index == 0 { vec![] } else { vec![Doc::to_simple_hint("I always figure out the argument types from left to right. If an argument is acceptable, I assume it is “correct” and move on. So the problem may actually be in one of the previous arguments!")] })
                }
                Context::CallArity(name, given) => {
                    let count = count_args(actual);
                    let thing = match (name, count) {
                        (MaybeName::NoName, 0) => "This value".into(),
                        (MaybeName::NoName, _) => "This function".into(),
                        (MaybeName::FuncName(name) | MaybeName::CtorName(name), 0) => format!("The `{name}` value"),
                        (MaybeName::FuncName(name), _) => format!("The `{name}` function"),
                        (MaybeName::CtorName(name), _) => format!("The `{name}` constructor"),
                        (MaybeName::OpName(op), _) => format!("The ({op}) operator"),
                    };
                    let problem = if count == 0 { format!("{thing} is not a function, but it was given {}.", args(*given)) } else { format!("{thing} expects {}, but it got {given} instead.", args(count)) };
                    Report::snippet("TOO MANY ARGS", region, None, Doc::reflow(&problem), Doc::reflow("Are there any missing commas? Or missing parentheses?")).with_region(*surroundings)
                }
                Context::OpLeft(op) => {
                    let (before, after) = operators::op_left_to_docs(localizer, category, op, actual, expected);
                    Report::snippet("TYPE MISMATCH", region, None, before, after).with_region(*surroundings)
                }
                Context::OpRight(op) => {
                    let docs = operators::op_right_to_docs(localizer, category, op, actual, expected);
                    let (before, after, highlight) = match docs {
                        operators::RightDocs::EmphBoth(before, after) => (before, after, None),
                        operators::RightDocs::EmphRight(before, after) => (before, after, Some(region)),
                    };
                    let mut report = Report::snippet("TYPE MISMATCH", *surroundings, highlight, before, after);
                    report.region = region;
                    report
                }
                Context::RecordAccess { record_region, maybe_name, field_region, field } => records::access(localizer, region, *surroundings, *record_region, *maybe_name, *field_region, field, actual, expected),
                Context::RecordUpdateKeys(name, fields) => records::update(localizer, region, *surroundings, name, fields, actual, expected),
                Context::RecordUpdateValue(field) => mismatch(&format!("I cannot update the `{field}` field like this:"), &format!("You are trying to update `{field}` to be"), "But it should be:", vec![Doc::to_simple_note("The record update syntax does not allow you to change the type of fields. You can achieve that with record constructors or the record literal syntax.")]),
                Context::RecordField(name, field) => mismatch(&format!("The `{field}` field of `{name}` is not what I expect:"), "This field is", "But the record declaration says it should be:", vec![]),
                Context::Destructure => {
                    let mut report = mismatch("This definition is causing issues:", "You are defining", "But then trying to destructure it as:", vec![]);
                    if let crate::Snippet::Region { highlight, .. } = &mut report.snippet { *highlight = None; }
                    report
                }
            }
        }
    }
}

fn count_args(tipe: &ErrorType<'_>) -> usize {
    match iterated_dealias(tipe) {
        ErrorType::Lambda(_, _, rest) => 1 + rest.len(),
        _ => 0,
    }
}

fn to_infinite_report(
    localizer: &Localizer,
    region: Region,
    name: &str,
    overall_type: &ErrorType<'_>,
) -> Report {
    Report::snippet(
        "INFINITE TYPE",
        region,
        None,
        Doc::reflow(&format!(
            "I am inferring a weird self-referential type for {name}:"
        )),
        Doc::stack([
            Doc::reflow(
                "Here is my best effort at writing down the type. You will see ∞ for parts of the type that repeat something already printed out infinitely.",
            ),
            Doc::indent(
                4,
                type_diff::to_doc(localizer, Ctx::None, overall_type).dullyellow(),
            ),
            Doc::reflow_link(
                "Staring at this type is usually not so helpful, so I recommend reading the hints at",
                "infinite-type",
                "to get unstuck!",
            ),
        ]),
    )
}

fn problems_to_hint(problems: &[Problem<'_>]) -> Vec<Doc> {
    problems.first().map_or_else(Vec::new, problem_to_hint)
}

fn problem_to_hint(problem: &Problem<'_>) -> Vec<Doc> {
    match problem {
        Problem::AnythingToBool => vec![Doc::to_simple_hint(
            "Nash does not have “truthiness” such that ints and strings and lists are automatically converted to booleans. Do that conversion explicitly!",
        )],
        Problem::AnythingFromOption => vec![Doc::to_fancy_hint(
            [Doc::text("Use"), Doc::text("Option.withDefault").green()]
                .into_iter()
                .chain("to handle possible errors. Longer term, it is usually better to write out the full `case` though!".split_whitespace().map(Doc::text)),
        )],
        Problem::ArityMismatch(actual, expected) => {
            vec![Doc::to_simple_hint(&if actual < expected {
                format!(
                    "It looks like it takes too few arguments. I was expecting {} more.",
                    expected - actual
                )
            } else {
                format!(
                    "It looks like it takes too many arguments. I see {} extra.",
                    actual - expected
                )
            })]
        }
        Problem::BadRigidVar(name, tipe) => match tipe {
            ErrorType::Lambda(..) => bad_rigid_var(name, "a function"),
            ErrorType::Infinite | ErrorType::Error | ErrorType::FlexVar(_) => vec![],
            ErrorType::RigidVar(other) => bad_double_rigid(name, other),
            ErrorType::Type { name: other, .. } | ErrorType::Alias { name: other, .. } => {
                bad_rigid_var(name, &format!("a value of type `{other}`"))
            }
            ErrorType::Record { .. } => bad_rigid_var(name, "a record"),
            ErrorType::Tuple(..) => bad_rigid_var(name, "a tuple"),
            ErrorType::VarApp(..) => bad_rigid_var(name, "an applied type constructor"),
        },
        Problem::FieldsMissing(fields) => {
            if fields.is_empty() {
                return vec![];
            }
            let names = Doc::comma_sep(
                "and",
                |doc| doc,
                fields.iter().map(|name| Doc::text(*name).green()).collect(),
            );
            vec![Doc::to_fancy_hint(
                [Doc::text(if fields.len() == 1 {
                    "Looks like the"
                } else {
                    "Looks like fields"
                })]
                .into_iter()
                .chain(names)
                .chain([Doc::text(if fields.len() == 1 {
                    "field is missing."
                } else {
                    "are missing."
                })]),
            )]
        }
        Problem::FieldTypo(typo, possibilities) => {
            let ranked = crate::suggest::sort(typo, |s| (*s).to_string(), possibilities.clone());
            match ranked.first() {
                None => vec![],
                Some(nearest) => vec![
                    Doc::to_fancy_hint([
                        Doc::text("Seems like a record field typo. Maybe"),
                        Doc::text(*typo).dullyellow(),
                        Doc::text("should be"),
                        Doc::cat([Doc::text(*nearest).green(), Doc::text("?")]),
                    ]),
                    Doc::to_simple_hint(
                        "Can more type annotations be added? Type annotations always help me give more specific messages, and I think they could help a lot in this case!",
                    ),
                ],
            }
        }
        Problem::BigLittle {
            big,
            little,
            direction,
        } => vec![Doc::to_simple_hint(&format!(
            "`{big}` is the Big (Data) type and `{little}` is the little type. They never convert implicitly. Where an appropriate `Lift` impl is available, use `lower` to go from `{big}` to `{little}`, or `lift` to go the other way.{}",
            match direction {
                Direction::Have => "",
                Direction::Need =>
                    " If this value comes from a validator argument, decode it with `fromData` first.",
            }
        ))],
    }
}

fn bad_rigid_var(name: &str, thing: &str) -> Vec<Doc> {
    vec![
        Doc::to_simple_hint(&format!(
            "Your type annotation uses type variable `'{name}` which means ANY type of value can flow through, but your code is saying it specifically wants {thing}. Maybe change your type annotation to be more specific? Maybe change the code to be more general?"
        )),
        Doc::reflow_link("Read", "type-annotations", "for more advice!"),
    ]
}
fn bad_double_rigid(x: &str, y: &str) -> Vec<Doc> {
    vec![
        Doc::to_simple_hint(&format!(
            "Your type annotation uses `'{x}` and `'{y}` as separate type variables. Your code seems to be saying they are the same though. Maybe they should be the same in your type annotation? Maybe your code uses them in a weird way?"
        )),
        Doc::reflow_link("Read", "type-annotations", "for more advice!"),
    ]
}
