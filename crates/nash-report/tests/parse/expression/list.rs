use crate::support::*;

#[test]
fn error_unclosed() {
    assert_expr_error_snapshot!("[1, 2");
}

#[test]
fn error_trailing_comma() {
    assert_expr_error_snapshot!("[1, 2,]");
}

#[test]
fn error_tab() {
    assert_expr_error_snapshot!("[1,\t2]");
}
