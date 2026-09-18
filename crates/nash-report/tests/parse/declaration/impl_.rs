use crate::support::*;

#[test]
fn implementation_head_requires_trait() {
    assert_decl_error_snapshot!("impl int where");
}

#[test]
fn implementation_head_requires_argument() {
    assert_decl_error_snapshot!("impl Eq where");
}

#[test]
fn implementation_requires_where() {
    assert_decl_error_snapshot!("impl Eq int");
}

#[test]
fn implementation_method_cannot_have_annotation() {
    assert_decl_error_snapshot!(
        r#"
        impl Eq int where
            eq : int -> bool
    "#
    );
}

#[test]
fn implementation_methods_must_align() {
    assert_decl_error_snapshot!(
        r#"
        impl Lift int Int where
            lift = liftInt
             lower = lowerInt
    "#
    );
}
