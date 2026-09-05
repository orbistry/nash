use std::collections::BTreeMap;

use bumpalo::Bump;
use nash_can::{Context, Error, canonicalize};

macro_rules! assert_kinds_snapshot {
    ($source:expr) => {{
        let bump = Bump::new();
        let source = bump.alloc_str(&format!("module Main exposing (..)\n\nimport Builtin exposing (..)\n\n{}\n", $source));
        let module = nash_parse::Parser::new(&bump, source.as_bytes()).module().expect("source parses");
        let interfaces = BTreeMap::from([("Builtin", nash_can::kinds::builtin_interface(&bump))]);
        let result = canonicalize(&bump, Context { package: None, interfaces: Some(&interfaces) }, &module).expect("kind checking succeeds");
        let unions: Vec<_> = result.module.unions.iter().map(|u| (u.value.name.value, u.value.kind)).collect();
        let aliases: Vec<_> = result.module.aliases.iter().map(|a| (a.value.name.value, a.value.kind)).collect();
        insta::with_settings!({description => $source, omit_expression => true}, {
            insta::assert_debug_snapshot!((unions, aliases));
        });
    }};
}

macro_rules! assert_kind_error_snapshot {
    ($source:expr) => {{
        let bump = Bump::new();
        let source = bump.alloc_str(&format!("module Main exposing (..)\n\nimport Builtin exposing (..)\n\n{}\n", $source));
        let module = nash_parse::Parser::new(&bump, source.as_bytes()).module().expect("source parses");
        let interfaces = BTreeMap::from([("Builtin", nash_can::kinds::builtin_interface(&bump))]);
        let errors = canonicalize(&bump, Context { package: None, interfaces: Some(&interfaces) }, &module).expect_err("kind checking fails");
        assert!(errors.iter().all(|error| matches!(error, Error::KindMismatch { .. } | Error::KindInfinite { .. } | Error::KindTooManyArgs { .. })), "wrong compiler phase: {errors:?}");
        insta::with_settings!({description => $source, omit_expression => true}, {
            insta::assert_debug_snapshot!(errors);
        });
    }};
}

#[test]
fn big_union() {
    assert_kinds_snapshot!("type Box = Box Int");
}
#[test]
fn big_parameter() {
    assert_kinds_snapshot!("type Box 'a = Box 'a");
}
#[test]
fn phantom_parameter() {
    assert_kinds_snapshot!("type Tag 'a = Tag Int");
}
#[test]
fn little_union() {
    assert_kinds_snapshot!("type option 'a = None | Some 'a");
}
#[test]
fn little_function_field() {
    assert_kinds_snapshot!("type thunk 'a = Thunk (unit -> 'a)");
}
#[test]
fn higher_kinded_parameter() {
    assert_kinds_snapshot!("type wrap 'f 'a = Wrap ('f 'a)");
}
#[test]
fn mutual_union_alias() {
    assert_kinds_snapshot!("type Tree = Node (List Branch)\ntype alias Branch = Tree");
}
#[test]
fn independent_instantiations() {
    assert_kinds_snapshot!(
        "type option 'a = None | Some 'a\ntype both = Both (option Int) (option int)"
    );
}
#[test]
fn big_alias() {
    assert_kinds_snapshot!("type alias Id = Int");
}
#[test]
fn little_alias() {
    assert_kinds_snapshot!("type alias count = int");
}
#[test]
fn big_record_alias() {
    assert_kinds_snapshot!("type alias Vault = { owner : Bytes, amount : Int }");
}
#[test]
fn little_record_alias() {
    assert_kinds_snapshot!("type alias acc = { total : int, seen : list Int }");
}
#[test]
fn labeled_fields() {
    assert_kinds_snapshot!("type Datum = Datum { owner : Bytes, amount : Int }");
}
#[test]
fn big_field_const() {
    assert_kind_error_snapshot!("type Datum = Datum bytes");
}
#[test]
fn big_field_tuple() {
    assert_kind_error_snapshot!("type Datum = Datum (Int, Int)");
}
#[test]
fn list_of_little() {
    assert_kind_error_snapshot!(
        "type option 'a = None | Some 'a\ntype alias xs = list (option int)"
    );
}
#[test]
fn lowercase_alias_big_body() {
    assert_kind_error_snapshot!("type alias id = Int");
}
#[test]
fn uppercase_alias_little_body() {
    assert_kind_error_snapshot!("type alias Count = int");
}
#[test]
fn infinite_kind() {
    assert_kind_error_snapshot!("type bad 'f = Bad ('f 'f)");
}
#[test]
fn pair_requires_big() {
    assert_kind_error_snapshot!("type alias p = pair int Int");
}
#[test]
fn independent_errors() {
    assert_kind_error_snapshot!("type Bad = Bad int\ntype Wrong = Wrong bytes");
}
#[test]
fn dependent_on_invalid_declaration_does_not_panic() {
    assert_kind_error_snapshot!("type Bad = Bad int\ntype Dependent = Dependent Bad");
}

#[test]
fn base_kinded_parameter_cannot_be_applied() {
    assert_kind_error_snapshot!("type wrong 'f = Wrong 'f ('f Int)");
}

#[test]
fn named_constructor_arity_remains_a_canonicalization_error() {
    let bump = Bump::new();
    let source = bump.alloc_str(
        "module Main exposing (..)\n\nimport Builtin exposing (..)\n\ntype alias x = int Int\n",
    );
    let module = nash_parse::Parser::new(&bump, source.as_bytes())
        .module()
        .unwrap();
    let interfaces = BTreeMap::from([("Builtin", nash_can::kinds::builtin_interface(&bump))]);
    let errors = canonicalize(
        &bump,
        Context {
            package: None,
            interfaces: Some(&interfaces),
        },
        &module,
    )
    .unwrap_err();
    assert!(matches!(errors.as_slice(), [Error::BadArity { .. }]));
    insta::assert_debug_snapshot!(errors);
}

#[test]
fn alias_substitution_preserves_application_head_and_argument() {
    use nash_ast::{AliasArgument, AliasType, Type};
    use nash_region::Located;
    let bump = Bump::new();
    let head = bump.alloc(Located::at_zero(Type::Var("f")));
    let arg = bump.alloc(Located::at_zero(Type::Var("a")));
    let body = bump.alloc(Located::at_zero(Type::App {
        head,
        args: bump.alloc_slice_copy(&[&*arg]),
    }));
    let list = bump.alloc(Located::at_zero(Type::Named {
        reference: nash_ast::QualifiedName {
            home: nash_ast::primitives::builtin_home(),
            name: "list",
        },
        args: &[],
    }));
    let unit = bump.alloc(Located::at_zero(Type::Unit));
    let args = bump.alloc_slice_fill_iter([
        AliasArgument {
            name: "f",
            typ: list,
        },
        AliasArgument {
            name: "a",
            typ: unit,
        },
    ]);
    let substituted = nash_can::types::dealias(&bump, args, &AliasType::Open(body));
    let Type::App { head, args } = &substituted.value else {
        panic!("application was lost");
    };
    assert!(std::ptr::eq(*head, list));
    assert!(std::ptr::eq(args[0], unit));
    insta::assert_debug_snapshot!(substituted);
}

#[test]
fn applied_head_is_a_free_variable() {
    let bump = Bump::new();
    let source = bump.alloc_str("module Main exposing (..)\n\ntype wrap 'a = Wrap ('f 'a)\n");
    let module = nash_parse::Parser::new(&bump, source.as_bytes())
        .module()
        .unwrap();
    let errors = canonicalize(&bump, Context::default(), &module).unwrap_err();
    assert!(matches!(
        errors.as_slice(),
        [Error::TypeVarsUnboundInUnion { .. }]
    ));
    insta::assert_debug_snapshot!(errors);
}
