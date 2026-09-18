use crate::support::*;

#[test]
fn error_uppercase_after_type() {
    assert_exposing_error_snapshot!("(type Foo)");
}

#[test]
fn error_missing_little_type_name() {
    assert_exposing_error_snapshot!("(type)");
}

#[test]
fn error_unclosed_little_type() {
    assert_exposing_error_snapshot!("(type option(..)");
}
