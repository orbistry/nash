use super::*;

type Docs = (Doc, Doc);
pub(super) enum RightDocs {
    EmphBoth(Doc, Doc),
    EmphRight(Doc, Doc),
}
fn right((before, after): Docs) -> RightDocs {
    RightDocs::EmphRight(before, after)
}

pub(super) fn op_left_to_docs(
    l: &Localizer,
    category: Category<'_>,
    op: &str,
    actual: &ErrorType<'_>,
    expected: &ErrorType<'_>,
) -> Docs {
    match op {
        "+" => bad_math(l, category, "Addition", "left", op, actual, expected),
        "-" => bad_math(l, category, "Subtraction", "left", op, actual, expected),
        "*" => bad_math(l, category, "Multiplication", "left", op, actual, expected),
        "^" => bad_math(l, category, "Exponentiation", "left", op, actual, expected),
        "/" => bad_div(l, "left", actual, expected),
        "&&" | "||" => bad_bool(l, op, "left", actual, expected),
        "<" | ">" | "<=" | ">=" => bad_comp_left(l, category, op, actual, expected),
        "++" => bad_append_left(l, category, actual, expected),
        "<|" => (
            Doc::reflow(
                "The left side of (<|) needs to be a function so I can pipe arguments to it!",
            ),
            lone_type(
                l,
                actual,
                expected,
                Doc::reflow(&add_category("I am seeing", category)),
                vec![Doc::reflow(
                    "This needs to be some kind of function though!",
                )],
            ),
        ),
        _ => (
            Doc::reflow(&format!("The left argument of ({op}) is causing problems:")),
            type_comparison(
                l,
                actual,
                expected,
                &add_category("The left argument is", category),
                &format!("But ({op}) needs the left argument to be:"),
                vec![],
            ),
        ),
    }
}

pub(super) fn op_right_to_docs(
    l: &Localizer,
    category: Category<'_>,
    op: &str,
    actual: &ErrorType<'_>,
    expected: &ErrorType<'_>,
) -> RightDocs {
    match op {
        "+" => right(bad_math(
            l, category, "Addition", "right", op, actual, expected,
        )),
        "-" => right(bad_math(
            l,
            category,
            "Subtraction",
            "right",
            op,
            actual,
            expected,
        )),
        "*" => right(bad_math(
            l,
            category,
            "Multiplication",
            "right",
            op,
            actual,
            expected,
        )),
        "^" => right(bad_math(
            l,
            category,
            "Exponentiation",
            "right",
            op,
            actual,
            expected,
        )),
        "/" => right(bad_div(l, "right", actual, expected)),
        "&&" | "||" => right(bad_bool(l, op, "right", actual, expected)),
        "<" | ">" | "<=" | ">=" => bad_comp_right(l, op, actual, expected),
        "==" | "/=" => bad_equality(l, op, actual, expected),
        "::" => bad_cons_right(l, category, actual, expected),
        "++" => bad_append_right(l, category, actual, expected),
        "<|" => right((
            Doc::reflow("I cannot send this through the (<|) pipe:"),
            type_comparison(
                l,
                actual,
                expected,
                "The argument is:",
                "But (<|) is piping it to a function that expects:",
                vec![],
            ),
        )),
        "|>" => match (iterated_dealias(actual), iterated_dealias(expected)) {
            (ErrorType::Lambda(expected_arg, _, _), ErrorType::Lambda(arg, _, _)) => right((
                Doc::reflow("This function cannot handle the argument sent through the (|>) pipe:"),
                type_comparison(
                    l,
                    arg,
                    expected_arg,
                    "The argument is:",
                    "But (|>) is piping it to a function that expects:",
                    vec![],
                ),
            )),
            _ => right((
                Doc::reflow(
                    "The right side of (|>) needs to be a function so I can pipe arguments to it!",
                ),
                lone_type(
                    l,
                    actual,
                    expected,
                    Doc::reflow(&add_category(
                        "But instead of a function, I am seeing",
                        category,
                    )),
                    vec![],
                ),
            )),
        },
        _ => bad_op_right_fallback(l, category, op, actual, expected),
    }
}
fn bad_op_right_fallback(
    l: &Localizer,
    category: Category<'_>,
    op: &str,
    actual: &ErrorType<'_>,
    expected: &ErrorType<'_>,
) -> RightDocs {
    right((
        Doc::reflow(&format!(
            "The right argument of ({op}) is causing problems."
        )),
        type_comparison(
            l,
            actual,
            expected,
            &add_category("The right argument is", category),
            &format!("But ({op}) needs the right argument to be:"),
            vec![Doc::to_simple_hint(&format!(
                "With operators like ({op}) I always check the left side first. If it seems fine, I assume it is correct and check the right side. So the problem may be in how the left and right arguments interact!"
            ))],
        ),
    ))
}
fn is_int(t: &ErrorType<'_>) -> bool {
    matches!(iterated_dealias(t), ErrorType::Type { home, name: "int", args: [] } if *home == nash_ast::primitives::builtin_home())
}
fn is_string(t: &ErrorType<'_>) -> bool {
    matches!(iterated_dealias(t), ErrorType::Type { home, name: "string", args: [] } if *home == nash_ast::primitives::builtin_home())
}
fn is_list(t: &ErrorType<'_>) -> bool {
    list_element(t).is_some()
}
fn list_element<'a>(t: &'a ErrorType<'a>) -> Option<&'a ErrorType<'a>> {
    match iterated_dealias(t) {
        ErrorType::Type {
            home,
            name: "list",
            args: [element],
        } if *home == nash_ast::primitives::builtin_home() => Some(element),
        _ => None,
    }
}
fn bad_cons_right(
    l: &Localizer,
    category: Category<'_>,
    actual: &ErrorType<'_>,
    expected: &ErrorType<'_>,
) -> RightDocs {
    match (list_element(actual), list_element(expected)) {
        (Some(actual_element), Some(expected_element)) => RightDocs::EmphBoth(
            Doc::reflow("I am having trouble with this (::) operator:"),
            type_comparison(
                l,
                expected_element,
                actual_element,
                "The left side of (::) is:",
                "But you are trying to put that into a list filled with:",
                vec![if is_list(expected_element) {
                    Doc::to_simple_hint(
                        "Are you trying to append two lists? The (++) operator appends lists, whereas the (::) operator is only for adding ONE element to a list.",
                    )
                } else {
                    Doc::reflow("Lists need ALL elements to be the same type though.")
                }],
            ),
        ),
        (Some(_), None) => bad_op_right_fallback(l, category, "::", actual, expected),
        (None, _) => right((
            Doc::reflow("The (::) operator can only add elements onto lists."),
            lone_type(
                l,
                actual,
                expected,
                Doc::reflow(&add_category("The right side is", category)),
                vec![Doc::reflow("But (::) needs a `list` on the right.")],
            ),
        )),
    }
}
#[derive(Clone, Copy)]
enum AppendType {
    Number,
    String,
    List,
    Other,
}
fn to_append_type(t: &ErrorType<'_>) -> AppendType {
    if is_int(t) {
        AppendType::Number
    } else if is_string(t) {
        AppendType::String
    } else if is_list(t) {
        AppendType::List
    } else {
        AppendType::Other
    }
}
fn bad_append_left(
    l: &Localizer,
    category: Category<'_>,
    actual: &ErrorType<'_>,
    expected: &ErrorType<'_>,
) -> Docs {
    match to_append_type(actual) {
        AppendType::Number => (
            Doc::reflow(
                "The (++) operator can append list and string values, but not int values like this:",
            ),
            Doc::to_fancy_hint(
                [Doc::text("Try"), Doc::text("using"), Doc::text("Int.toString").green()]
                    .into_iter()
                    .chain("to turn it into a string? Or put it in [] to make it a list? Or switch to the (::) operator?".split_whitespace().map(Doc::text)),
            ),
        ),
        AppendType::String | AppendType::List | AppendType::Other => (
            Doc::reflow("The (++) operator cannot append this type of value:"),
            lone_type(
                l,
                actual,
                expected,
                Doc::reflow(&add_category("I am seeing", category)),
                vec![Doc::reflow(
                    "But the (++) operator is only for appending list and string values. Maybe put this value in [] to make it a list?",
                )],
            ),
        ),
    }
}
fn bad_append_right(
    l: &Localizer,
    category: Category<'_>,
    actual: &ErrorType<'_>,
    expected: &ErrorType<'_>,
) -> RightDocs {
    match (to_append_type(expected), to_append_type(actual)) {
        (AppendType::String, AppendType::Number) => right((
            Doc::reflow("I thought I was appending string values here, not int values like this:"),
            Doc::to_fancy_hint([
                Doc::text("Try using"),
                Doc::text("Int.toString").green(),
                Doc::text("to turn it into a string?"),
            ]),
        )),
        (AppendType::List, AppendType::Number) => right((
            Doc::reflow("I thought I was appending list values here, not int values like this:"),
            Doc::reflow("Try putting it in [] to make it a list?"),
        )),
        (AppendType::String, AppendType::List) => RightDocs::EmphBoth(
            Doc::reflow("The (++) operator needs the same type of value on both sides:"),
            Doc::reflow(
                "I see a string on the left and a list on the right. Which should it be? Does the string need [] around it to become a list?",
            ),
        ),
        (AppendType::List, AppendType::String) => RightDocs::EmphBoth(
            Doc::reflow("The (++) operator needs the same type of value on both sides:"),
            Doc::reflow(
                "I see a list on the left and a string on the right. Which should it be? Does the string need [] around it to become a list?",
            ),
        ),
        _ => RightDocs::EmphBoth(
            Doc::reflow("The (++) operator cannot append these two values:"),
            type_comparison(
                l,
                expected,
                actual,
                "I already figured out that the left side of (++) is:",
                &add_category("But this clashes with the right side, which is", category),
                vec![],
            ),
        ),
    }
}
fn bad_math(
    l: &Localizer,
    category: Category<'_>,
    operation: &str,
    direction: &str,
    op: &str,
    actual: &ErrorType<'_>,
    expected: &ErrorType<'_>,
) -> Docs {
    (
        Doc::reflow(&format!("{operation} does not work with this value:")),
        lone_type(
            l,
            actual,
            expected,
            Doc::reflow(&add_category(
                &format!("The {direction} side of ({op}) is"),
                category,
            )),
            vec![Doc::reflow(&format!(
                "But ({op}) only works with values whose type implements `Num`."
            ))],
        ),
    )
}
fn bad_div(
    l: &Localizer,
    direction: &str,
    actual: &ErrorType<'_>,
    expected: &ErrorType<'_>,
) -> Docs {
    (
        Doc::reflow("The (/) operator is integer division; both sides must be `int`."),
        lone_type(
            l,
            actual,
            expected,
            Doc::reflow(&format!("But the {direction} side is:")),
            vec![],
        ),
    )
}
fn bad_bool(
    l: &Localizer,
    op: &str,
    direction: &str,
    actual: &ErrorType<'_>,
    expected: &ErrorType<'_>,
) -> Docs {
    (
        Doc::reflow("I am struggling with this boolean operation:"),
        lone_type(
            l,
            actual,
            expected,
            Doc::reflow(&format!(
                "Both sides of ({op}) must be `bool` values, but the {direction} side is:"
            )),
            vec![],
        ),
    )
}
fn bad_comp_left(
    l: &Localizer,
    category: Category<'_>,
    op: &str,
    actual: &ErrorType<'_>,
    expected: &ErrorType<'_>,
) -> Docs {
    (
        Doc::reflow("I cannot do a comparison with this value:"),
        lone_type(
            l,
            actual,
            expected,
            Doc::reflow(&add_category(
                &format!("The left side of ({op}) is"),
                category,
            )),
            vec![Doc::reflow(&format!(
                "But ({op}) only works with values whose type implements `Ord`."
            ))],
        ),
    )
}
fn bad_comp_right(
    l: &Localizer,
    op: &str,
    actual: &ErrorType<'_>,
    expected: &ErrorType<'_>,
) -> RightDocs {
    RightDocs::EmphBoth(
        Doc::reflow(&format!("I need both sides of ({op}) to be the same type:")),
        type_comparison(
            l,
            expected,
            actual,
            &format!("The left side of ({op}) is:"),
            "But the right side is:",
            vec![Doc::reflow(&format!(
                "I cannot compare different types though! Which side of ({op}) is the problem? The type must implement `Ord`."
            ))],
        ),
    )
}
fn bad_equality(
    l: &Localizer,
    op: &str,
    actual: &ErrorType<'_>,
    expected: &ErrorType<'_>,
) -> RightDocs {
    RightDocs::EmphBoth(
        Doc::reflow(&format!("I need both sides of ({op}) to be the same type:")),
        type_comparison(
            l,
            expected,
            actual,
            &format!("The left side of ({op}) is:"),
            "But the right side is:",
            vec![Doc::reflow(
                "Different types can never be equal though! Which side is messed up? The type must implement `Eq`.",
            )],
        ),
    )
}
