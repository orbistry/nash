use crate::support::*;

#[test]
fn error_trailing_bind() {
    assert_indented_do_error_snapshot!(
        r#"
        do
            x <- e
    "#
    );
}

#[test]
fn error_trailing_let() {
    assert_indented_do_error_snapshot!(
        r#"
        do
            let y = 1
    "#
    );
}

#[test]
fn error_misaligned_statement() {
    assert_indented_do_error_snapshot!(
        r#"
        do
            x <- e
          y
    "#
    );
}

#[test]
fn error_empty_do() {
    assert_indented_do_error_snapshot!("do");
}
