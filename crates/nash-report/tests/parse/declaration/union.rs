use crate::support::*;

#[test]
fn union_rejects_kind_arrow_annotation() {
    assert_decl_error_snapshot!("type Fix ('f : Big -> Big) = Fix ('f (Fix 'f))");
}

#[test]
fn error_bare_parameter() {
    assert_decl_error_snapshot!("type Maybe a = Just a");
}

#[test]
fn error_empty_representation_annotation() {
    assert_decl_error_snapshot!("type T ('f : ) = A");
}

#[test]
fn error_unknown_representation_annotation() {
    assert_decl_error_snapshot!("type T ('f : Foo) = A");
}

#[test]
fn error_labeled_field_without_type() {
    assert_decl_error_snapshot!("type D = D { owner }");
}
