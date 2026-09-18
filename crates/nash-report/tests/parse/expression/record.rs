use crate::support::*;

#[test]
fn error_unclosed() {
    assert_expr_error_snapshot!("{ x = 1");
}

#[test]
fn error_trailing_comma() {
    assert_expr_error_snapshot!("{ x = 1, }");
}

#[test]
fn error_missing_equals() {
    assert_expr_error_snapshot!("{ x 1 }");
}

#[test]
fn error_missing_value() {
    assert_expr_error_snapshot!("{ x = }");
}

#[test]
fn error_uppercase_field() {
    assert_expr_error_snapshot!("{ X = 1 }");
}
