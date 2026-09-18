use crate::support::*;

#[test]
fn error_odd_length() {
    assert_expr_error_snapshot!("#\"f\"");
}

#[test]
fn error_bad_digit() {
    assert_expr_error_snapshot!("#\"zz\"");
}

#[test]
fn error_endless() {
    assert_expr_error_snapshot!("#\"ab");
}
