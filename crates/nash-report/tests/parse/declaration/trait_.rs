use crate::support::*;

#[test]
fn trait_requires_parameter() {
    assert_decl_error_snapshot!("trait Eq where");
}

#[test]
fn trait_requires_uppercase_name() {
    assert_decl_error_snapshot!("trait eq 'a where");
}

#[test]
fn trait_requires_where() {
    assert_decl_error_snapshot!(
        r#"
        trait Eq 'a
            eq : 'a -> bool
    "#
    );
}

#[test]
fn trait_method_requires_signature() {
    assert_decl_error_snapshot!(
        r#"
        trait Eq 'a where
            eq a = true
    "#
    );
}

#[test]
fn trait_methods_must_align() {
    assert_decl_error_snapshot!(
        r#"
        trait Eq 'a where
            eq : 'a -> bool
             neq : 'a -> bool
    "#
    );
}

#[test]
fn superclass_argument_must_be_variable() {
    assert_decl_error_snapshot!("trait Eq int => Ord 'a where");
}
