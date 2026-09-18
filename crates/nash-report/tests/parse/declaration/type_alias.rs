use crate::support::*;

#[test]
fn error_missing_name() {
    assert_decl_error_snapshot!("type alias = int");
}
