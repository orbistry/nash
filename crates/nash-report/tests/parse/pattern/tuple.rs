use crate::support::*;

#[test]
fn error_unclosed() {
    assert_pattern_error_snapshot!("(a, b");
}

#[test]
fn error_trailing_comma() {
    assert_pattern_error_snapshot!("(a, b,)");
}
