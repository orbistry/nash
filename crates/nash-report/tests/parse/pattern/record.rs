use crate::support::*;

#[test]
fn error_unclosed() {
    assert_pattern_error_snapshot!("{ x, y");
}

#[test]
fn error_trailing_comma() {
    assert_pattern_error_snapshot!("{ x, y, }");
}
