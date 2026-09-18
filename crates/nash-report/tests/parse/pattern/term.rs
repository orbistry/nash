use crate::support::*;

#[test]
fn bytes_literal_odd_length() {
    assert_pattern_error_snapshot!("#\"0\"");
}

#[test]
fn error_wildcard_not_var() {
    assert_pattern_error_snapshot!("_foo");
}

#[test]
fn error_char_literal() {
    assert_pattern_error_snapshot!("'x'");
}
