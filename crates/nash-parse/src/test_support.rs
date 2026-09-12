//! Helpers for snapshot tests.

/// Indent every non-empty line of a fragment by four spaces, so multiline
/// layout tests exercise the fragment as it would appear inside a
/// definition. A token at column 1 always starts a new top-level
/// declaration, so bare multiline fragments are not valid input.
pub(crate) fn indent_fragment(fragment: &str) -> String {
    let mut out = String::with_capacity(fragment.len() + 4 * fragment.lines().count());
    for line in fragment.lines() {
        if !line.is_empty() {
            out.push_str("    ");
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

pub(crate) fn render_module_error(input: &str, error: &nash_parse::error::Module<'_>) -> String {
    let source = nash_report::Source::new(input);
    let report =
        nash_report::syntax::to_report(&source, &nash_parse::error::Error::ParseError(error));
    nash_report::render_plain(&report, &source, "src/Main.nash")
}

// Use the normal library's error types: nash-report depends on that library,
// while unit tests compile a separate copy of the parser crate.
use nash_parse::error::{Decl, DeclDef, Exposing, Expr, Module, Pattern, Type};

pub(crate) fn render_decl_error(input: &str, error: &Decl<'_>) -> String {
    render_module_error(input, &Module::Declarations(error, 1, 1))
}

pub(crate) fn render_expr_error(input: &str, error: &Expr<'_>) -> String {
    render_decl_error(
        input,
        &Decl::Def("value", &DeclDef::Body(error, 1, 1), 1, 1),
    )
}

pub(crate) fn render_pattern_error(input: &str, error: &Pattern<'_>) -> String {
    render_decl_error(input, &Decl::Def("value", &DeclDef::Arg(error, 1, 1), 1, 1))
}

pub(crate) fn render_type_error(input: &str, error: &Type<'_>) -> String {
    render_decl_error(
        input,
        &Decl::Def("value", &DeclDef::Type(error, 1, 1), 1, 1),
    )
}

pub(crate) fn render_exposing_error(input: &str, error: &Exposing) -> String {
    render_module_error(input, &Module::Exposing(error, 1, 1))
}
