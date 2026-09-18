use crate::support::*;

#[test]
fn error_unclosed() {
    assert_expr_error_snapshot!("(1, 2");
}

#[test]
fn error_trailing_comma() {
    assert_expr_error_snapshot!("(1, 2,)");
}

#[test]
fn error_empty_comma() {
    assert_expr_error_snapshot!("(,)");
}

#[test]
fn error_section_unclosed() {
    assert_expr_error_snapshot!("(> 5");
}

#[test]
fn error_reserved_section_operator() {
    assert_expr_error_snapshot!("(=> 5)");
}

#[test]
fn error_section_missing_operand() {
    assert_expr_error_snapshot!("(> ,)");
}
