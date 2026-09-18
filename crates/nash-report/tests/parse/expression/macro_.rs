use crate::support::*;

#[test]
fn error_unclosed() {
    assert_expression_error_snapshot!("m!(");
}

#[test]
fn error_trailing_comma() {
    assert_expression_error_snapshot!("m!(1,)");
}

#[test]
fn error_double_comma() {
    assert_expression_error_snapshot!("m!(1,,2)");
}
