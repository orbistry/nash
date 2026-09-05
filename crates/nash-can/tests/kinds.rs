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

#[test]
fn annotation_storable_parameter() {
    assert_kinds_snapshot!("f : 'a -> list 'a -> list 'a\nf x xs = xs");
}

#[test]
fn annotation_rejects_little_container_element_in_argument() {
    assert_kind_error_snapshot!(
        "type option 'a = None | Some 'a\nf : list (option int) -> int\nf x = 1"
    );
}

#[test]
fn annotation_rejects_higher_kinded_value_position() {
    assert_kind_error_snapshot!("f : ('f 'a, 'f) -> int\nf x = 1");
}

#[test]
fn annotation_accepts_variable_application() {
    assert_kinds_snapshot!("f : 'f 'a -> 'f 'a\nf x = x");
}

#[test]
fn let_annotation_rejects_little_container_element() {
    assert_kind_error_snapshot!(
        "type option 'a = None | Some 'a\nf =\n    let\n        g : list (option int) -> int\n        g x = 1\n    in\n    g"
    );
}

#[test]
fn nested_annotation_in_lambda_is_checked() {
    assert_kind_error_snapshot!(
        "type option 'a = None | Some 'a\nf = \\x ->\n    let\n        g : list (option int) -> int\n        g y = 1\n    in\n    g x"
    );
}

#[test]
fn annotation_parameter_occurrences_share_kind_bounds() {
    assert_kind_error_snapshot!("f : list 'f -> 'f 'a -> int\nf xs g = 1");
}

#[test]
fn copied_kind_interfaces_survive_the_source_arena() {
    use nash_ast::{Kind, Type};
    let destination = Bump::new();
    let copied = {
        let source_arena = Bump::new();
        let source = source_arena.alloc_str("module Shapes exposing (type wrap(..), type applied)\n\ntype wrap 'f 'a = Wrap ('f 'a)\ntype alias applied 'f 'a = 'f 'a\n");
        let module = nash_parse::Parser::new(&source_arena, source.as_bytes())
            .module()
            .unwrap();
        let canonical = canonicalize(&source_arena, Context::default(), &module).unwrap();
        let interface = nash_can::from_module(&source_arena, &canonical.module, &BTreeMap::new());
        nash_can::deep_copy_interface(&destination, &interface)
    };
    assert!(matches!(
        copied.unions[0].kind.kind,
        Kind::Arrow(Kind::Arrow(_, _), _)
    ));
    assert!(matches!(copied.aliases[0].typ.value, Type::App { .. }));
    let interfaces = BTreeMap::from([("Shapes", copied)]);
    let source = destination.alloc_str("module Main exposing (..)\n\nimport Shapes exposing (type wrap)\n\ntype holder 'f 'a = Holder (wrap 'f 'a)\n");
    let module = nash_parse::Parser::new(&destination, source.as_bytes())
        .module()
        .unwrap();
    let canonical = canonicalize(
        &destination,
        Context {
            package: None,
            interfaces: Some(&interfaces),
        },
        &module,
    )
    .unwrap();
    insta::assert_debug_snapshot!((copied, canonical.module.unions[0].value.kind));
}

#[test]
fn annotation_returns_shared_storable_bound() {
    let bump = Bump::new();
    let interface = nash_can::kinds::builtin_interface(&bump);
    let interfaces = BTreeMap::from([("Builtin", interface)]);
    let env = nash_can::kinds::KindEnv::from_interfaces(Some(&interfaces));
    let a = bump.alloc(nash_region::Located::at_zero(nash_ast::Type::Var("a")));
    let list = bump.alloc(nash_region::Located::at_zero(nash_ast::Type::Named {
        reference: nash_ast::QualifiedName {
            home: nash_ast::primitives::builtin_home(),
            name: "list",
        },
        args: bump.alloc_slice_copy(&[&*a]),
    }));
    let typ = bump.alloc(nash_region::Located::at_zero(nash_ast::Type::Lambda {
        from: a,
        to: list,
    }));
    let annotation = nash_ast::Annotation {
        free_vars: &["a"],
        typ,
    };
    let kinds = nash_can::kinds::check_annotation(
        &bump,
        &env,
        nash_ast::ModuleName {
            package: None,
            name: "Main",
        },
        "f",
        &annotation,
    )
    .unwrap();
    assert_eq!(kinds.len(), 1);
    assert_eq!(kinds[0].1.bounds, &[nash_ast::KindSet::STORABLE]);
    insta::assert_debug_snapshot!(kinds);
}

#[test]
fn annotation_checks_alias_contract_before_argument_splitting() {
    use nash_ast::{BaseKind, Kind, KindScheme, Type};
    use nash_region::Located;
    let bump = Bump::new();
    let a = bump.alloc(Located::at_zero(Type::Var("a")));
    let body = bump.alloc(Located::at_zero(Type::Lambda { from: a, to: a }));
    let interface = nash_can::Interface {
        home: nash_ast::ModuleName {
            package: None,
            name: "Restricted",
        },
        values: &[],
        unions: &[],
        binops: &[],
        aliases: bump.alloc_slice_copy(&[nash_can::InterfaceAlias {
            name: "restricted",
            parameters: &["a"],
            typ: body,
            visibility: nash_can::AliasVisibility::Public,
            kind: KindScheme::mono(&Kind::Arrow(
                &Kind::Base(BaseKind::Big),
                &Kind::Base(BaseKind::Term),
            )),
        }]),
    };
    let interfaces = BTreeMap::from([("Restricted", interface)]);
    let source = bump.alloc_str("module Main exposing (..)\n\nimport Restricted exposing (type restricted)\n\nf : restricted ()\nf x = x\n");
    let module = nash_parse::Parser::new(&bump, source.as_bytes())
        .module()
        .unwrap();
    let errors = canonicalize(
        &bump,
        Context {
            package: None,
            interfaces: Some(&interfaces),
        },
        &module,
    )
    .expect_err("the alias parameter requires Big, even when the body accepts Const");
    assert!(matches!(errors.as_slice(), [Error::KindMismatch { .. }]));
    insta::assert_debug_snapshot!(errors);
}
