use crate::support::*;

#[test]
fn error_assert_without_body() {
    assert_expression_error_snapshot!("assert");
}

#[test]
fn error_trace_without_body() {
    assert_expression_error_snapshot!("trace \"m\"");
}

#[test]
fn error_comptime_without_body() {
    assert_expression_error_snapshot!("comptime");
}
