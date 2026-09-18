use crate::support::*;

#[test]
fn test_body_requires_do() {
    assert_tests_module_error_snapshot!(
        r#"
        module Main exposing (..)

        tests
            test "t" =
                assert True
    "#
    );
}

#[test]
fn tests_block_must_be_last() {
    assert_tests_module_error_snapshot!(
        r#"
        module Main exposing (..)

        tests
        x = 1
    "#
    );
}

#[test]
fn test_name_must_be_string() {
    assert_tests_module_error_snapshot!(
        r#"
        module Main exposing (..)

        tests
            test name = do
                assert True
    "#
    );
}

#[test]
fn property_requires_let() {
    assert_tests_module_error_snapshot!(
        r#"
        module Main exposing (..)

        tests
            prop "p" = assert True
    "#
    );
}

#[test]
fn property_binder_requires_via() {
    assert_tests_module_error_snapshot!(
        r#"
        module Main exposing (..)

        tests
            prop "p" = let x = int in x
    "#
    );
}

#[test]
fn unit_test_rejects_via_let() {
    assert_tests_module_error_snapshot!(
        r#"
        module Main exposing (..)

        tests
            test "t" = let x via int in do
                assert True
    "#
    );
}

#[test]
fn property_requires_at_least_one_binder() {
    assert_tests_module_error_snapshot!(
        r#"
        module Main exposing (..)

        tests
            prop "p" = let in do
                assert True
    "#
    );
}

#[test]
fn once_is_invalid_on_unit_test() {
    assert_tests_module_error_snapshot!(
        r#"
        module Main exposing (..)

        tests
            test "t" fail once = do
                assert True
    "#
    );
}

#[test]
fn test_block_must_end_in_expression() {
    assert_tests_module_error_snapshot!(
        r#"
        module Main exposing (..)

        tests
            test "t" = do
                x <- e
    "#
    );
}

#[test]
fn budget_kinds_cannot_repeat() {
    assert_tests_module_error_snapshot!(
        r#"
        module Main exposing (..)

        tests
            test "t" within (cpu 1, cpu 2) = do
                assert True
    "#
    );
}

#[test]
fn test_items_must_align() {
    assert_tests_module_error_snapshot!(
        r#"
        module Main exposing (..)

        tests
            test "first" = do
                assert True
             test "second" = do
                 assert True
    "#
    );
}
