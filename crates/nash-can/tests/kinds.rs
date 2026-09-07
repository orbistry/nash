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
        let unions: Vec<_> = result.module.unions.iter().map(|u| (u.value.name.value, u.value.kind, u.value.context)).collect();
        let aliases: Vec<_> = result.module.aliases.iter().map(|a| (a.value.name.value, a.value.kind, a.value.context)).collect();
        insta::with_settings!({description => $source, omit_expression => true}, {
            insta::assert_debug_snapshot!((unions, aliases));
        });
    }};
}

macro_rules! assert_kind_error_snapshot {
    ($source:expr) => {
        assert_kind_error_snapshot!($source, Error::KindMismatch { .. } | Error::KindInfinite { .. } | Error::RepresentationMismatch { .. } | Error::ContradictoryRepresentation { .. });
    };
    ($source:expr, $expected:pat) => {{
        let bump = Bump::new();
        let source = bump.alloc_str(&format!("module Main exposing (..)\n\nimport Builtin exposing (..)\n\n{}\n", $source));
        let module = nash_parse::Parser::new(&bump, source.as_bytes()).module().expect("source parses");
        let interfaces = BTreeMap::from([("Builtin", nash_can::kinds::builtin_interface(&bump))]);
        let errors = canonicalize(&bump, Context { package: None, interfaces: Some(&interfaces) }, &module).expect_err("kind checking fails");
        assert!(errors.iter().all(|error| matches!(error, $expected)), "wrong compiler phase: {errors:?}");
        insta::with_settings!({description => $source, omit_expression => true}, {
            insta::assert_debug_snapshot!(errors);
        });
    }};
}

#[test]
fn inline_kind_bounds_reject_contradictory_repeated_variables() {
    assert_kind_error_snapshot!("type small 'a = Small (pair ('a : Big) ('a : Const))");
}

#[test]
fn ground_big_proof_preserves_constructor_bounds_and_aliases() {
    use nash_ast::{AliasType, Kind, QualifiedName, Type, primitives::Repr};
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
    let alias_name = QualifiedName {
        home,
        name: "Alias",
    };
    env.types.insert(
        alias_name,
        nash_can::kinds::TypeInfo::Defined {
            kind: &Kind::Type,
            parameters: &[],
            context: &[],
            repr: None,
            alias: Some(big),
        },
    );
    let alias = bump.alloc(Located::at_zero(Type::Alias {
        reference: alias_name,
        arguments: &[],
        remaining: &[],
        target: AliasType::Open(big),
    }));
    let variable = bump.alloc(Located::at_zero(Type::Var("a")));
    for (typ, expected) in [
        (big, Some(Repr::Big)),
        (small, Some(Repr::Const)),
        (list, None),
        (big_list, Some(Repr::Big)),
        (invalid_big_list, Some(Repr::Big)),
        (alias, Some(Repr::Big)),
        (variable, None),
    ] {
        assert_eq!(
            nash_can::kinds::repr_of(&bump, &env, typ),
            expected,
            "{typ:?}"
        );
    }
    let group = Default::default();
    let mut formation = nash_can::kinds::Formation::new(&bump, &env, &group);
    assert!(
        formation.typ(invalid_big_list).is_err(),
        "known representation does not bypass the constructor context"
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
        free_vars, context, ..
    } = definition
    else {
        panic!("expected typed f")
    };
    assert_eq!(*free_vars, &["a"]);
    assert!(
        matches!(context, [pred] if pred.trait_ref() == Some(nash_ast::primitives::ReprTrait::Storable.qualified()))
    );
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
    assert_eq!(
        scheme,
        &Kind::Arrow(
            &Kind::Arrow(&Kind::Type, &Kind::Type),
            &Kind::Arrow(&Kind::Type, &Kind::Type)
        )
    );
    assert!(matches!(
        interface.unions[0].context,
        [nash_ast::Pred::Apply { .. }]
    ));
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
    assert!(matches!(
        canonical.module.unions[0].value.context,
        [nash_ast::Pred::Apply { .. }]
    ));
    insta::assert_debug_snapshot!((
        interface,
        canonical.module.unions[0].value.kind,
        canonical.module.unions[0].value.context
    ));
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
    let [nash_ast::Pred::Apply { head, args: [a, b] }] = kinds.context else {
        panic!("one n-ary constructor application")
    };
    assert!(matches!(head.value, Type::Var("f")));
    assert!(matches!(a.value, Type::Var("a")));
    assert!(matches!(b.value, Type::Var("b")));
    let env = nash_can::kinds::KindEnv::default();
    let mut checker = nash_can::kinds::TypeChecker::new(&bump, &env);
    checker.value(typ).unwrap();
    let f = checker.variable("f");
    assert_eq!(
        checker.infer.default_and_zonk(f),
        &nash_ast::Kind::Arrow(
            &nash_ast::Kind::Type,
            &nash_ast::Kind::Arrow(&nash_ast::Kind::Type, &nash_ast::Kind::Type)
        )
    );
    let substitution = BTreeMap::from([("a", var("x")), ("b", var("y")), ("f", var("g"))]);
    let copied = nash_can::kinds::substitute_predicate(&bump, &substitution, kinds.context[0]);
    assert!(
        matches!(copied, nash_ast::Pred::Apply { head, args: [a,b] } if matches!(head.value, Type::Var("g")) && matches!(a.value, Type::Var("x")) && matches!(b.value, Type::Var("y")))
    );
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
    assert!(
        matches!(kinds.context, [pred] if pred.trait_ref() == Some(nash_ast::primitives::ReprTrait::Storable.qualified()) && matches!(pred.args()[0].value, nash_ast::Type::Var("a")))
    );
    insta::assert_debug_snapshot!(kinds);
}

#[test]
fn annotation_checks_alias_contract_before_argument_splitting() {
    use nash_ast::{Kind, Type};
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
            kind: &Kind::Arrow(&Kind::Type, &Kind::Type),
            context: bump.alloc_slice_copy(&[nash_ast::Pred::Implied {
                trait_: nash_ast::primitives::ReprTrait::Big.qualified(),
                args: bump.alloc_slice_copy(&[&*a]),
            }]),
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
    assert!(matches!(
        errors.as_slice(),
        [Error::RepresentationMismatch { .. }]
    ));
    insta::assert_debug_snapshot!(errors);
}

#[test]
fn kind_annotation_fix() {
    assert_kinds_snapshot!("type Fix 'f = Fix ('f (Fix 'f))");
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
    assert_kinds_snapshot!("type wrap 'f ('a : Storable) = Wrap (('f 'a) : Storable)");
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

#[test]
fn retained_self_application_terminates() {
    assert_kind_error_snapshot!("type self 'f = Self ('f 'f)\ntype w = W (self self)");
}

#[test]
fn retained_self_application_rejects_a_term_argument() {
    assert_kind_error_snapshot!(
        "type self 'f = Self ('f 'f)\ntype tag 'a = Tag unit\ntype w = W (self (self tag))"
    );
}

#[test]
fn retained_self_application_in_value_annotations() {
    assert_kind_error_snapshot!(
        "type self 'f = Self ('f 'f)\nidentity : self self -> self self\nidentity x = x",
        Error::KindInfinite { .. }
    );
}

#[test]
fn finite_self_application_in_value_annotations() {
    assert_kind_error_snapshot!(
        "type self 'f = Self ('f 'f)\ntype tag 'a = Tag unit\nwitness : self tag\nwitness = Self (Tag ())"
    );
}

#[test]
fn saturated_binary_self_application_is_inductively_invalid() {
    assert_kind_error_snapshot!(
        "type s 'f 'a = S ('f 'f 'a)\ntype tag 'a = Tag\ntype w = W (s s tag)",
        Error::KindInfinite { .. }
    );
}

#[test]
fn unused_declaration_checks_conflicting_parameter_bounds_eagerly() {
    assert_kind_error_snapshot!(
        "type unused 'a = Unused (list 'a) ('a unit)",
        Error::KindMismatch { .. }
    );
}

#[test]
fn retained_application_preserves_three_arguments() {
    assert_kinds_snapshot!("type apply3 'f 'a 'b 'c = Apply3 ('f 'a 'b 'c)");
}

#[test]
fn annotated_head_preserves_remaining_application_arguments() {
    assert_kinds_snapshot!(
        "type apply2 'f ('a : Storable) ('b : Storable) = Apply2 (('f 'a 'b) : Term)"
    );
}

#[test]
fn retained_self_application_in_impl_heads() {
    assert_kind_error_snapshot!(
        "type self 'f = Self ('f 'f)\ntrait Marker 'a where\n    marker : 'a -> unit\nimpl Marker (self self) where\n    marker x = ()"
    );
}

#[test]
fn retained_nested_self_application_terminates() {
    assert_kind_error_snapshot!(
        "type g 'f = G ('f ('f 'f))\ntype w = W (g g)\ntype alias Count = int"
    );
}

#[test]
fn retained_finite_nested_constructor_application() {
    assert_kinds_snapshot!("type app 'f 'a = App ('f 'a)\ntype w = W (app (app list) int)");
}

#[test]
fn fresh_parameter_replay_in_declarations() {
    assert_kind_error_snapshot!(
        "type tag 'a = Tag\ntype s 'f 'g 'a = S ('g ('f 'f 'a))\ntype w = W (s s s)\ntype alias Count = int"
    );
}

#[test]
fn fresh_parameter_replay_in_annotations() {
    assert_kind_error_snapshot!(
        "type tag 'a = Tag\ntype s 'f 'g 'a = S ('g ('f 'f 'a))\nidentity : s s s tag -> s s s tag\nidentity value = value"
    );
}

#[test]
fn fresh_parameter_replay_in_impl_heads() {
    assert_kind_error_snapshot!(
        "type tag 'a = Tag\ntype s 'f 'g 'a = S ('g ('f 'f 'a))\ntrait Marker 'a where\n    marker : 'a -> unit\nimpl Marker (s s tag tag) where\n    marker _ = ()"
    );
}

macro_rules! supplied_obligation_cases {
    ($($name:ident: $number:literal),* $(,)?) => {$(
        #[test]
        fn $name() {
            let source = include_str!("fixtures/kind-obligations.md")
                .split("```").nth(2 * $number - 1).expect("supplied case").trim();
            let bump = Bump::new();
            let source = bump.alloc_str(source);
            let module = nash_parse::Parser::new(&bump, source.as_bytes()).module().expect("source parses");
            let result = canonicalize(&bump, Context { package: None, interfaces: None }, &module);
            let errors = result.expect_err("self application fails the H98 occurs check");
            assert!(errors.iter().all(|error| matches!(error, Error::KindInfinite { .. })), "declaration-time occurs check: {errors:?}");
            insta::assert_debug_snapshot!(errors);
        }
    )*};
}

supplied_obligation_cases! {
    supplied_obligation_01: 1,
    supplied_obligation_02: 2,
    supplied_obligation_03: 3,
    supplied_obligation_04: 4,
    supplied_obligation_05: 5,
    supplied_obligation_06: 6,
    supplied_obligation_07: 7,
    supplied_obligation_08: 8,
    supplied_obligation_09: 9,
    supplied_obligation_10: 10,
    supplied_obligation_11: 11,
    supplied_obligation_12: 12,
    supplied_obligation_13: 13,
    supplied_obligation_14: 14,
    supplied_obligation_15: 15,
    supplied_obligation_16: 16,
    supplied_obligation_17: 17,
    supplied_obligation_18: 18,
    supplied_obligation_19: 19,
    supplied_obligation_20: 20,
    supplied_obligation_21: 21,
    supplied_obligation_22: 22,
}

#[test]
fn partial_constructor_checks_closed_obligations() {
    assert_kind_error_snapshot!(
        "type self 'f = Self ('f 'f)\ntype p 'f 'a = P ('f 'f) 'a\ntype tag 'a = Tag\ntype w = W (tag (p self))"
    );
}

#[test]
fn phantom_constructor_checks_saturated_argument_validity() {
    assert_kind_error_snapshot!(
        "type self 'f = Self ('f 'f)\ntype tag 'a = Tag\ntype w = W (tag (self self))"
    );
}

#[test]
fn phantom_constructor_checks_supplied_partial_bounds() {
    assert_kind_error_snapshot!(
        "type option 'a = None | Some 'a\ntype use 'f = Use ('f int)\ntype w = W (use (pair (option int)))",
        Error::RepresentationMismatch { .. }
    );
}

#[test]
fn partial_constructor_projects_consumer_bounds_before_missing_arguments() {
    assert_kind_error_snapshot!(
        "type option 'a = None | Some 'a\ntype tag 'a = Tag\ntype s 'f 'g 'a = S ('g ('f 'a))\ntype w = W (tag (s s option))"
    );
}

#[test]
fn generated_capture_outside_finite_fragment_is_not_an_infinite_kind() {
    assert_kind_error_snapshot!(
        "type tag 'a = Tag\ntype option 'a = Some 'a\ntype app 'f 'a = App ('f 'a)\ntype wrap 'f 'g 'a = Wrap ('f ('g 'a))\ntype w = W (wrap (app tag) app option)",
        Error::KindMismatch { .. }
    );
}

#[test]
fn explicit_capture_has_a_finite_inductive_proof() {
    assert_kind_error_snapshot!(
        "type tag 'a = Tag\ntype option 'a = Some 'a\ntype app 'f 'a = App ('f 'a)\ntype w = W (app tag (app option))"
    );
}

#[test]
fn fragment_restriction_cannot_validate_an_annotation() {
    assert_kind_error_snapshot!(
        "type tag 'a = Tag\ntype option 'a = Some 'a\ntype app 'f 'a = App ('f 'a)\ntype wrap 'f 'g 'a = Wrap ('f ('g 'a))\nidentity : wrap (app tag) app option -> wrap (app tag) app option\nidentity x = x",
        Error::KindMismatch { .. }
    );
}

#[test]
fn fragment_restriction_cannot_validate_an_impl_head() {
    assert_kind_error_snapshot!(
        "type tag 'a = Tag\ntype option 'a = Some 'a\ntype app 'f 'a = App ('f 'a)\ntype wrap 'f 'g 'a = Wrap ('f ('g 'a))\ntrait Marker 'a where\n    marker : 'a -> unit\nimpl Marker (wrap (app tag) app option) where\n    marker x = ()",
        Error::KindMismatch { .. }
    );
}
