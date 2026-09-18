use crate::support::*;

#[test]
fn scheme_error_variable_context() {
    assert_scheme_error_snapshot!("'a => 'a");
}

#[test]
fn scheme_error_constraint_without_argument() {
    assert_scheme_error_snapshot!("Eq => 'a");
}

#[test]
fn scheme_error_tuple_member() {
    assert_scheme_error_snapshot!("(Eq 'a, 'b) => 'a");
}

#[test]
fn scheme_error_little_class() {
    assert_scheme_error_snapshot!("int 'a => 'a");
}

#[test]
fn representation_annotation_rejects_arrow() {
    assert_type_error_snapshot!("('f : Big -> Big)");
}

#[test]
fn inline_representation_annotation_reports_unknown_name() {
    assert_type_error_snapshot!("list ('a : Wrong)");
}

#[test]
fn error_empty_type_variable() {
    assert_type_error_snapshot!("'");
}

#[test]
fn error_upper_type_variable() {
    assert_type_error_snapshot!("'A");
}

#[test]
fn error_record_extension() {
    assert_type_error_snapshot!("{ r | x : int }");
}
