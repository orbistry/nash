//! Rendering helpers and snapshot macros for parser error reports.
//!
//! Wrapping a sub-parser error in the enclosing declaration shape lets each
//! parser entry point render through the same module report path.

/// Indent every non-empty line of a fragment by four spaces, so multiline
/// layout tests exercise the fragment as it would appear inside a
/// definition. A token at column 1 always starts a new top-level
/// declaration, so bare multiline fragments are not valid input.
pub fn indent_fragment(fragment: &str) -> String {
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

pub fn render_module_error(input: &str, error: &nash_parse::error::Module<'_>) -> String {
    let source = nash_report::Source::new(input);
    let report =
        nash_report::syntax::to_report(&source, &nash_parse::error::Error::ParseError(error));
    nash_report::render_plain(&report, &source, "src/Main.nash")
}

use nash_parse::error::{Decl, DeclDef, Exposing, Expr, Module, Pattern, Type};

pub fn render_decl_error(input: &str, error: &Decl<'_>) -> String {
    render_module_error(input, &Module::Declarations(error, 1, 1))
}

pub fn render_expr_error(input: &str, error: &Expr<'_>) -> String {
    render_decl_error(
        input,
        &Decl::Def("value", &DeclDef::Body(error, 1, 1), 1, 1),
    )
}

pub fn render_pattern_error(input: &str, error: &Pattern<'_>) -> String {
    render_decl_error(input, &Decl::Def("value", &DeclDef::Arg(error, 1, 1), 1, 1))
}

pub fn render_type_error(input: &str, error: &Type<'_>) -> String {
    render_decl_error(
        input,
        &Decl::Def("value", &DeclDef::Type(error, 1, 1), 1, 1),
    )
}

pub fn render_exposing_error(input: &str, error: &Exposing) -> String {
    render_module_error(input, &Module::Exposing(error, 1, 1))
}

macro_rules! assert_decl_error_snapshot {
    ($src:expr) => {{
        let bump = bumpalo::Bump::new();
        let src = indoc::indoc!($src);
        let src_in_arena = bump.alloc_str(src);
        let mut parser = nash_parse::Parser::new(&bump, src_in_arena);
        let error = parser.declaration().expect_err("expected declaration parse error");
        insta::with_settings!({
            description => src,
            omit_expression => true,
                info => &"diagnostic",
        }, {
            insta::assert_snapshot!($crate::support::render_decl_error(src, &error));
        });
    }};
}
pub(crate) use assert_decl_error_snapshot;

macro_rules! assert_exposing_error_snapshot {
    ($input:expr) => {{
        let input = indoc::indoc!($input);
        let bump = bumpalo::Bump::new();
        let src = bump.alloc_str(input);
        let mut parser = nash_parse::Parser::new(&bump, src);
        let error = parser.exposing().expect_err("expected exposing parse error");
        insta::with_settings!({
            description => format!("Code:\n\n{}", input),
            omit_expression => true,
            info => &"diagnostic",
        }, {
            insta::assert_snapshot!($crate::support::render_exposing_error(src, &error));
        });
    }};
}
pub(crate) use assert_exposing_error_snapshot;

macro_rules! assert_indented_do_error_snapshot {
    ($code:expr) => {{
        let bump = bumpalo::Bump::new();
        let indented = $crate::support::indent_fragment(indoc::indoc!($code));
        let source = bump.alloc_str(&indented);
        let mut parser = nash_parse::Parser::new(&bump, source);
        parser
            .chomp(|_, _, _| "space error")
            .expect("expected leading indent");
        let error = parser.expression().expect_err("expected do parse error");
        insta::with_settings!({
            description => format!("Code (indented inside a def):\n\n{}", indented),
            omit_expression => true,
            info => &"diagnostic",
        }, {
            insta::assert_snapshot!($crate::support::render_expr_error(source, &error));
        });
    }};
}
pub(crate) use assert_indented_do_error_snapshot;

macro_rules! assert_expr_error_snapshot {
    ($code:expr) => {{
        let bump = bumpalo::Bump::new();
        let src = bump.alloc_str(indoc::indoc!($code));
        let mut parser = nash_parse::Parser::new(&bump, src);
        let result = parser.term().expect_err("expected parse error");

        insta::with_settings!({
            description => format!("Code:\n\n{}", indoc::indoc!($code)),
            omit_expression => true,
                info => &"diagnostic",
        }, {
            insta::assert_snapshot!($crate::support::render_expr_error(src, &result));
        });
    }};
}
pub(crate) use assert_expr_error_snapshot;

macro_rules! assert_expression_error_snapshot {
    ($code:expr) => {{
        let bump = bumpalo::Bump::new();
        let src = bump.alloc_str(indoc::indoc!($code));
        let mut parser = nash_parse::Parser::new(&bump, src);
        let result = parser.expression().expect_err("expected parse error");

        insta::with_settings!({
            description => format!("Code:\n\n{}", indoc::indoc!($code)),
            omit_expression => true,
                info => &"diagnostic",
        }, {
            insta::assert_snapshot!($crate::support::render_expr_error(src, &result));
        });
    }};
}
pub(crate) use assert_expression_error_snapshot;

macro_rules! assert_module_error_snapshot {
    ($input:expr) => {{
        let input = indoc::indoc!($input);
        let bump = bumpalo::Bump::new();
        let src = bump.alloc_str(input);
        let mut parser = nash_parse::Parser::new(&bump, src);
        let error = parser.module().expect_err("expected module parse error");
        insta::with_settings!({
            description => format!("Code:\n\n{}", input),
            omit_expression => true,
            info => &"diagnostic",
        }, {
            insta::assert_snapshot!($crate::support::render_module_error(src, &error));
        });
    }};
}
pub(crate) use assert_module_error_snapshot;

macro_rules! assert_pattern_error_snapshot {
    ($code:expr) => {{
        let bump = bumpalo::Bump::new();
        let src = bump.alloc_str(indoc::indoc!($code));
        let mut parser = nash_parse::Parser::new(&bump, src);
        let result = parser.pattern_expr().expect_err("expected parse error");

        insta::with_settings!({
            description => format!("Code:\n\n{}", indoc::indoc!($code)),
            omit_expression => true,
                info => &"diagnostic",
        }, {
            insta::assert_snapshot!($crate::support::render_pattern_error(src, &result));
        });
    }};
}
pub(crate) use assert_pattern_error_snapshot;

macro_rules! assert_tests_module_error_snapshot {
    ($input:expr) => {{
        let input = indoc::indoc!($input);
        let bump = bumpalo::Bump::new();
        let source = bump.alloc_str(input);
        let mut parser = nash_parse::Parser::new(&bump, source);
        let error = parser.module().expect_err("expected module parse error");
        insta::with_settings!({
            description => format!("Code:\n\n{}", input),
            omit_expression => true,
            info => &"diagnostic",
        }, {
            insta::assert_snapshot!($crate::support::render_module_error(source, &error));
        });
    }};
}
pub(crate) use assert_tests_module_error_snapshot;

macro_rules! assert_type_error_snapshot {
    ($code:expr) => {{
        let bump = bumpalo::Bump::new();
        let src = bump.alloc_str(indoc::indoc!($code));
        let mut parser = nash_parse::Parser::new(&bump, src);
        let result = parser.type_expr().expect_err("expected parse error");

        insta::with_settings!({
            description => format!("Code:\n\n{}", indoc::indoc!($code)),
            omit_expression => true,
                info => &"diagnostic",
        }, {
            insta::assert_snapshot!($crate::support::render_type_error(src, &result));
        });
    }};
}
pub(crate) use assert_type_error_snapshot;

macro_rules! assert_scheme_error_snapshot {
    ($code:expr) => {{
        let bump = bumpalo::Bump::new();
        let src = bump.alloc_str(indoc::indoc!($code));
        let mut parser = nash_parse::Parser::new(&bump, src);
        let result = parser.type_scheme().expect_err("expected parse error");

        insta::with_settings!({
            description => format!("Code:\n\n{}", indoc::indoc!($code)),
            omit_expression => true,
                info => &"diagnostic",
        }, {
            insta::assert_snapshot!($crate::support::render_type_error(src, &result));
        });
    }};
}
pub(crate) use assert_scheme_error_snapshot;
