use crate::support::*;

#[test]
fn error_reserved_if() {
    assert_expr_error_snapshot!("if");
}

#[test]
fn error_reserved_let() {
    assert_expr_error_snapshot!("let");
}

#[test]
fn error_reserved_do() {
    assert_expr_error_snapshot!("do");
}

#[test]
fn error_reserved_trait() {
    assert_expr_error_snapshot!("trait");
}
