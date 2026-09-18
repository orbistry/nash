use crate::support::*;

#[test]
fn error_leading_zero() {
    assert_expr_error_snapshot!("007");
}

#[test]
fn error_hex_no_digits() {
    assert_expr_error_snapshot!("0x");
}

#[test]
fn error_float_nonzero() {
    assert_expr_error_snapshot!("1.5");
}

#[test]
fn error_float_zero() {
    assert_expr_error_snapshot!("0.5");
}
