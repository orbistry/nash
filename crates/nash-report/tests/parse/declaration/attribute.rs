use crate::support::*;

#[test]
fn error_upper_name() {
    assert_decl_error_snapshot!("@Derive(Eq)\ntype T = A");
}

#[test]
fn error_unclosed() {
    assert_decl_error_snapshot!("@derive(Eq\ntype T = A");
}

#[test]
fn error_same_line() {
    assert_decl_error_snapshot!("@derive(Eq) type T = A");
}
