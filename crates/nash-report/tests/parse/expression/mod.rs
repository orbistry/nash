mod bytes;
mod do_;
mod keyword;
mod list;
mod macro_;
mod number;
mod record;
mod string;
mod tuple;
mod variable;

use crate::support::*;

#[test]
fn error_fat_arrow() {
    assert_expression_error_snapshot!("a => b");
}

#[test]
fn error_left_arrow() {
    assert_expression_error_snapshot!("a <- b");
}
