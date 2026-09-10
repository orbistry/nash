use super::*;

/// Operator syntax can make a repair more specific than a generic type mismatch.
pub(super) fn hint(
    op: &str,
    right: bool,
    actual: &ErrorType<'_>,
    expected: &ErrorType<'_>,
) -> Option<Doc> {
    fn list_element<'a>(typ: &'a ErrorType<'a>) -> Option<&'a ErrorType<'a>> {
        match iterated_dealias(typ) {
            ErrorType::Type {
                home,
                name: "list",
                args: [element],
            } if *home == nash_ast::primitives::builtin_home() => Some(element),
            _ => None,
        }
    }
    match op {
        "::" if right
            && list_element(expected).and_then(list_element).is_some()
            && list_element(actual).is_some() =>
        {
            Some(Doc::text(
                "Use (++) to join two lists; (::) adds one element.",
            ))
        }
        "|>" if right && !matches!(iterated_dealias(actual), ErrorType::Lambda(..)) => {
            Some(Doc::text("The right operand of (|>) must be a function."))
        }
        "<|" if !right
            && !matches!(iterated_dealias(actual), ErrorType::Lambda(..))
            && matches!(iterated_dealias(expected), ErrorType::Lambda(..)) =>
        {
            Some(Doc::text("The left operand of (<|) must be a function."))
        }
        _ => None,
    }
}
