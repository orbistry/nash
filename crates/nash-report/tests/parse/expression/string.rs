use crate::support::*;

#[test]
fn error_overflowing_unicode_escape() {
    assert_expr_error_snapshot!(r#""\u{FFFFFFFFF}""#);
}

#[test]
fn error_endless() {
    assert_expr_error_snapshot!(r#""hello"#);
}
