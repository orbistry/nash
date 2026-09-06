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
fn ground_big_proof_preserves_constructor_bounds_and_aliases() {
    use nash_ast::{AliasType, Kind, KindScheme, KindSet, QualifiedName, Type};
    use nash_region::Located;
    let bump = Bump::new();
    let mut env = nash_can::kinds::KindEnv::default();
    let home = nash_ast::primitives::builtin_home();
    let named = |name, args| {
        &*bump.alloc(Located::at_zero(Type::Named {
            reference: QualifiedName { home, name },
            args,
        }))
    };
    let big = named("Int", &[]);
    let small = named("int", &[]);
    let list = named("list", &[]);
    let big_list = named("List", bump.alloc_slice_copy(&[big]));
    let invalid_big_list = named("List", bump.alloc_slice_copy(&[small]));
    let unknown = QualifiedName {
        home,
        name: "UnknownKind",
    };
    env.insert(
        unknown,
        KindScheme {
            applications: &[],
            bounds: &[KindSet::ANY],
            kind: &Kind::Var(0),
        },
    );
    let unknown = named("UnknownKind", &[]);
    let alias_name = QualifiedName {
        home,
        name: "Alias",
    };
    env.insert(
        alias_name,
        KindScheme {
            applications: &[],
            bounds: &[],
            kind: &Kind::Base(nash_ast::BaseKind::Const),
        },
    );
    // Representation bodies do not determine a nominal alias's kind.
    let alias = bump.alloc(Located::at_zero(Type::Alias {
        reference: alias_name,
        arguments: &[],
        remaining: &[],
        target: AliasType::Open(big),
    }));
    let variable = bump.alloc(Located::at_zero(Type::Var("a")));
    for (typ, expected) in [
        (big, true),
        (small, false),
        (list, false),
        (big_list, true),
        (invalid_big_list, false),
        (unknown, false),
        (alias, false),
        (variable, false),
    ] {
        assert_eq!(
            nash_can::kinds::proves_ground_big(&bump, &env, typ),
            expected,
            "{typ:?}"
        );
    }
    assert_eq!(
        env.scheme(QualifiedName {
            home,
            name: "UnknownKind"
        })
        .bounds,
        &[KindSet::ANY]
    );
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
    assert_kind_error_snapshot!("type bad 'f = Bad (bad bad)");
}
#[test]
fn pair_of_const_and_const() {
    assert_kinds_snapshot!("type alias p = pair int (list Data)");
}
#[test]
fn pair_of_const_and_big() {
    assert_kinds_snapshot!(
        "type alias p = pair int Data\ntype alias q = pair Data int\ntype alias r = pair Data Data"
    );
}
#[test]
fn pair_requires_storable() {
    assert_kind_error_snapshot!(
        "type option 'a = None | Some 'a\ntype alias p = pair (option int) Int"
    );
}
#[test]
fn pair_requires_storable_second_argument() {
    assert_kind_error_snapshot!(
        "type option 'a = None | Some 'a\ntype alias p = pair Int (option int)"
    );
}
#[test]
fn pair_builtin_signatures() {
    assert_kinds_snapshot!(
        "type alias unConstrData = Data -> pair int (list Data)\ntype alias fstPair 'a 'b = pair 'a 'b -> 'a\ntype alias sndPair 'a 'b = pair 'a 'b -> 'b\ntype alias mkPairData = Data -> Data -> pair Data Data"
    );
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
fn named_partial_constructors_in_higher_kinded_arguments() {
    assert_kinds_snapshot!(
        "type box 'f = Box ('f int)\ntype alias pairAlias 'a 'b = pair 'a 'b\ntype alias withList = box list\ntype alias withPair = box (pair int)\ntype alias withAlias = box (pairAlias int)"
    );
}

#[test]
fn partial_constructor_is_not_a_value_type() {
    assert_kind_error_snapshot!("value : list\nvalue = ()\nother : pair int\nother = ()");
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
    let Type::Named { reference, args } = &substituted.value else {
        panic!("known application was not normalized");
    };
    assert_eq!(reference.home, nash_ast::primitives::builtin_home());
    assert_eq!(reference.name, "list");
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
    let bump = Bump::new();
    let source = "module Main exposing (..)\nimport Builtin exposing (..)\nf : 'a -> list 'a -> list 'a\nf x xs = xs\n";
    let module = nash_parse::Parser::new(&bump, source.as_bytes())
        .module()
        .unwrap();
    let interfaces = BTreeMap::from([("Builtin", nash_can::kinds::builtin_interface(&bump))]);
    let result = canonicalize(
        &bump,
        Context {
            package: None,
            interfaces: Some(&interfaces),
        },
        &module,
    )
    .unwrap();
    let nash_ast::Decls::Declare { definition, .. } = result.module.decls else {
        panic!("expected f declaration")
    };
    let nash_ast::Def::TypedDef {
        free_vars, kinds, ..
    } = definition
    else {
        panic!("expected typed f")
    };
    assert_eq!(*free_vars, &["a"]);
    assert_eq!(kinds.bounds, &[nash_ast::KindSet::STORABLE]);
    assert_eq!(kinds.kinds, &[&nash_ast::Kind::Var(0)]);
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
fn imported_interfaces_retain_higher_kinded_types() {
    use nash_ast::{Kind, Type};
    let destination = Bump::new();
    let interface = {
        let source_arena = &destination;
        let source = source_arena.alloc_str("module Shapes exposing (type wrap(..), type applied)\n\ntype wrap 'f 'a = Wrap ('f 'a)\ntype alias applied 'f 'a = 'f 'a\n");
        let module = nash_parse::Parser::new(source_arena, source.as_bytes())
            .module()
            .unwrap();
        let canonical = canonicalize(source_arena, Context::default(), &module).unwrap();
        nash_can::from_module(source_arena, &canonical.module, &BTreeMap::new())
    };
    let scheme = interface.unions[0].kind;
    assert!(matches!(scheme.kind, Kind::Arrow(Kind::Var(_), _)));
    assert_eq!(scheme.applications.len(), 1);
    assert!(matches!(interface.aliases[0].typ.value, Type::App { .. }));
    let interfaces = BTreeMap::from([("Shapes", interface)]);
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
    insta::assert_debug_snapshot!((interface, canonical.module.unions[0].value.kind));
}

#[test]
fn annotation_kinds_keep_application_parameters_correlated() {
    use nash_ast::{Annotation, Type};
    use nash_region::Located;
    let bump = Bump::new();
    let var = |name| &*bump.alloc(Located::at_zero(Type::Var(name)));
    let application = bump.alloc(Located::at_zero(Type::App {
        head: var("f"),
        args: bump.alloc_slice_fill_iter([var("a"), var("b")]),
    }));
    let typ = bump.alloc(Located::at_zero(Type::Lambda {
        from: application,
        to: application,
    }));
    let annotation = Annotation {
        kinds: nash_ast::ValueKinds::unconstrained(&bump, 3),
        free_vars: &["f", "b", "a"],
        context: &[],
        typ,
    };
    let kinds = nash_can::kinds::check_annotation(
        &bump,
        &nash_can::kinds::KindEnv::from_interfaces(None),
        nash_ast::ModuleName {
            package: None,
            name: "Main",
        },
        "identityK",
        &annotation,
    )
    .unwrap();
    let kind = |name| {
        kinds.kinds[annotation
            .free_vars
            .iter()
            .position(|var| *var == name)
            .unwrap()]
    };
    let [first_application, second_application] = kinds.applications else {
        panic!("two constructor applications")
    };
    assert_eq!(first_application.head, kind("f"));
    assert_eq!(first_application.argument, kind("a"));
    assert_eq!(first_application.result, second_application.head);
    assert_eq!(second_application.argument, kind("b"));
    assert_ne!(kind("a"), kind("b"));
    let mut infer = nash_can::kinds::Infer::new(&bump);
    let first = infer.instantiate_values(&kinds);
    let second = infer.instantiate_values(&kinds);
    let big = bump.alloc(nash_can::kinds::K::Base(nash_ast::BaseKind::Big));
    let constant = bump.alloc(nash_can::kinds::K::Base(nash_ast::BaseKind::Const));
    let term = bump.alloc(nash_can::kinds::K::Base(nash_ast::BaseKind::Term));
    let concrete = bump.alloc(nash_can::kinds::K::Arrow(
        big,
        bump.alloc(nash_can::kinds::K::Arrow(constant, term)),
    ));
    infer.unify(first[0], concrete).unwrap();
    assert!(
        infer.unify(first[2], constant).is_err(),
        "a must satisfy the selected constructor's first domain"
    );
    infer.unify(first[1], constant).unwrap();
    infer.unify(second[2], constant).unwrap();
    insta::assert_debug_snapshot!(kinds);
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
        kinds: nash_ast::ValueKinds::unconstrained(&bump, 1),
        context: &[],
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
    assert_eq!(kinds.kinds.len(), 1);
    assert_eq!(kinds.bounds, &[nash_ast::KindSet::STORABLE]);
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
        impls: &[],
        traits: &[],
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

#[test]
fn kind_annotation_fix() {
    assert_kinds_snapshot!("type Fix ('f : Big -> Big) = Fix ('f (Fix 'f))");
}

#[test]
fn kind_annotation_storable() {
    assert_kinds_snapshot!("type alias xs ('a : Storable) = list 'a");
}

#[test]
fn kind_annotation_base_mismatch() {
    assert_kind_error_snapshot!("type Box ('a : Const) = Box 'a");
}

#[test]
fn kind_annotation_arrow_mismatch() {
    assert_kind_error_snapshot!("type wrap ('f : Big) 'a = Wrap ('f 'a)");
}

#[test]
fn storable_annotation_occurrences_are_independent() {
    assert_kinds_snapshot!("type wrap ('f : Storable -> Storable) 'a = Wrap ('f 'a)");
}

#[test]
fn narrowed_function_alias_accepts_big_parameter() {
    assert_kinds_snapshot!("type alias fn ('a : Big) = 'a -> 'a\nf : fn Int\nf x = x");
}

#[test]
fn narrowed_function_alias_rejects_const_parameter() {
    assert_kind_error_snapshot!("type alias fn ('a : Big) = 'a -> 'a\nf : fn int\nf x = x");
}

#[test]
fn narrowed_function_alias_rejects_const_parameter_in_let() {
    assert_kind_error_snapshot!(
        "type alias fn ('a : Big) = 'a -> 'a\nf =\n    let\n        g : fn int\n        g x = x\n    in\n    g"
    );
}

#[test]
fn parameter_annotation_is_checked_after_all_recursive_uses() {
    assert_kind_error_snapshot!(
        "type Box 'a = Box 'a\ntype first ('a : Const) = First (second 'a)\ntype second 'a = Second (Box 'a) (first 'a)"
    );
}

#[test]
fn big_record_field_rejects_const() {
    assert_kind_error_snapshot!("type alias Vault = { amount : int }");
}

#[test]
fn little_record_field_rejects_an_arrow_kind() {
    assert_kind_error_snapshot!("type alias record 'f 'a = { applied : 'f 'a, head : 'f }");
}

#[test]
fn kind_annotation_term() {
    assert_kinds_snapshot!("type wrapper ('a : Term) = Wrap 'a");
}
