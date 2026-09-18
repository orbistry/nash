use crate::support::*;

#[test]
fn validator_requires_module_keyword() {
    assert_module_error_snapshot!("validator Vesting exposing (main)");
}

#[test]
fn validator_module_must_remain_indented() {
    assert_module_error_snapshot!("validator\nmodule V exposing (..)");
}

#[test]
fn module_preserves_import_alias_error() {
    assert_module_error_snapshot!("import Cardano.Tx as tx");
}

#[test]
fn module_preserves_type_alias_error() {
    assert_module_error_snapshot!("type alias account");
}

#[test]
fn module_preserves_pattern_error() {
    assert_module_error_snapshot!("f (x as) = x");
}

#[test]
fn module_preserves_expression_error() {
    assert_module_error_snapshot!("value = if True then 42");
}

#[test]
fn module_preserves_annotation_name_error() {
    assert_module_error_snapshot!("f : int\ng = 1");
}
