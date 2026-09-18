//! Moved from `nash-can/src/module.rs` so the canonicalizer needs no dev-dependency on its reporter.
mod tests {
    fn test_constructor_kind<'a>(bump: &'a bumpalo::Bump, arity: usize) -> &'a nash_ast::Kind<'a> {
        (0..arity).fold(&nash_ast::Kind::Type, |result, _| {
            bump.alloc(nash_ast::Kind::Arrow(&nash_ast::Kind::Type, result))
        })
    }

    use std::collections::BTreeMap;

    use bumpalo::Bump;
    use indoc::indoc;
    use nash_ast::{
        Associativity, Ctor as CanCtor, CtorOpts, Module as CanModule, ModuleName, PackageName,
        Precedence, Type as CanType,
    };
    use nash_region::{Located, Region};

    use crate::snapshot_support::{errors as render_errors, warnings as render_warnings};
    use nash_can::{
        AliasVisibility, InterfaceAlias, InterfaceBinop, InterfaceUnion, InterfaceValue,
        UnionVisibility,
    };
    use nash_can::{Context, canonicalize};
    use nash_can::{Error, Interface};

    fn parse_and_canonicalize<'a>(
        bump: &'a Bump,
        input: &str,
        context: Context<'a, '_>,
    ) -> Result<CanModule<'a>, Vec<Error<'a>>> {
        let src = bump.alloc_str(input);
        let mut parser = nash_parse::Parser::new(bump, src);
        let module = parser.module().expect("expected successful parse");
        canonicalize(bump, context, &module).map(|r| r.module)
    }

    macro_rules! assert_module_snapshot {
        ($input:expr) => {{
            let input = indoc!($input);
            let bump = Bump::new();
            let result = parse_and_canonicalize(&bump, input, Context::default())
                .expect("expected successful canonicalization");

            insta::with_settings!({
                description => format!("Code:\n\n{}", input),
                omit_expression => true,
            }, {
                insta::assert_debug_snapshot!(result);
            });
        }};
    }

    macro_rules! assert_module_error_snapshot {
        ($input:expr) => {{
            let input = indoc!($input);
            let bump = Bump::new();
            let result = parse_and_canonicalize(&bump, input, Context::default())
                .expect_err("expected canonicalization error");

            insta::with_settings!({info => &"diagnostic",
                description => format!("Code:\n\n{}", input),
                omit_expression => true,
            }, {
                insta::assert_snapshot!(render_errors(input, &result));
            });
        }};
    }

    macro_rules! assert_interface_snapshot {
        ($input:expr) => {{
            let input = indoc!($input);
            let bump = Bump::new();
            let can_module = parse_and_canonicalize(&bump, input, Context::default())
                .expect("expected successful canonicalization");
            let annotations = mock_annotations(&bump, &can_module);
            let result = nash_can::from_module(&bump, &can_module, &annotations);
            insta::with_settings!({
                description => format!("Code:\n\n{}", input),
                omit_expression => true,
            }, {
                insta::assert_debug_snapshot!(result);
            });
        }};
    }

    #[test]
    fn record_type_outside_alias_errors() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)
            f : { x : int } -> int
            f r = r.x
        "#
        );
    }

    #[test]
    fn nested_record_type_in_alias_errors() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)
            type alias outer = { inner : { x : int } }
        "#
        );
    }

    #[test]
    fn record_type_in_transparent_alias_errors() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)
            type alias wrapped = list { x : int }
        "#
        );
    }

    #[test]
    fn alias_record_fields_sorted_by_name() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)
            type alias point = { z : int, a : int }
        "#
        );
    }

    #[test]
    fn record_literal_no_alias_error() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)
            value = { x = () }
        "#
        );
    }

    #[test]
    fn record_literal_ambiguous_error() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)
            type alias first = { x : unit }
            type alias second = { x : unit }
            value = { x = () }
        "#
        );
    }

    #[test]
    fn record_literal_fields_in_wire_order() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)
            type alias point = { z : unit, a : unit }
            value = { a = (), z = () }
        "#
        );
    }

    fn var_type<'a>(bump: &'a Bump, name: &'a str) -> &'a Located<CanType<'a>> {
        bump.alloc(Located::at(Region::zero(), CanType::Var(name)))
    }

    /// Stand-in for a solver-produced annotation in tests: `Forall [a] a`.
    fn test_annotation<'a>(bump: &'a Bump) -> &'a nash_ast::Annotation<'a> {
        bump.alloc(nash_ast::Annotation {
            context: &[],
            free_vars: bump.alloc_slice_fill_iter(["a"]),
            typ: var_type(bump, "a"),
        })
    }

    /// Solver stand-in for interface extraction tests: give every
    /// top-level value a `Forall [a] a` annotation, mimicking the map
    /// Elm's `Interface.fromModule` receives from the solver.
    fn mock_annotations<'a>(bump: &'a Bump, module: &CanModule<'a>) -> nash_can::Annotations<'a> {
        fn walk<'a>(
            decls: &nash_ast::Decls<'a>,
            bump: &'a Bump,
            out: &mut nash_can::Annotations<'a>,
        ) {
            match decls {
                nash_ast::Decls::Declare { definition, next } => {
                    add(definition, bump, out);
                    walk(next, bump, out);
                }
                nash_ast::Decls::DeclareRec {
                    definition,
                    following,
                    next,
                } => {
                    add(definition, bump, out);
                    for def in *following {
                        add(def, bump, out);
                    }
                    walk(next, bump, out);
                }
                nash_ast::Decls::Empty => {}
            }
        }
        fn add<'a>(def: &nash_ast::Def<'a>, bump: &'a Bump, out: &mut nash_can::Annotations<'a>) {
            let name = match def {
                nash_ast::Def::Def { name, .. } | nash_ast::Def::TypedDef { name, .. } => {
                    name.value
                }
            };
            out.insert(name, test_annotation(bump));
        }
        let mut annotations = nash_can::Annotations::new();
        walk(module.decls, bump, &mut annotations);
        annotations
    }

    fn union_interface<'a>(
        bump: &'a Bump,
        module_name: &'a str,
        union_name: &'a str,
        parameters: &'a [&'a str],
    ) -> Interface<'a> {
        Interface {
            impls: &[],
            traits: &[],
            home: ModuleName {
                package: None,
                name: module_name,
            },
            values: &[],
            aliases: &[],
            unions: bump.alloc_slice_fill_iter([InterfaceUnion {
                context: &[],
                kind: test_constructor_kind(bump, parameters.len()),
                name: union_name,
                parameters,
                ctors: &[],
                alternatives: 0,
                options: CtorOpts::Normal,
                visibility: UnionVisibility::Open,
            }]),
            binops: &[],
        }
    }

    fn alias_interface<'a>(
        bump: &'a Bump,
        module_name: &'a str,
        alias_name: &'a str,
        parameters: &'a [&'a str],
        typ: &'a Located<CanType<'a>>,
    ) -> Interface<'a> {
        Interface {
            impls: &[],
            traits: &[],
            home: ModuleName {
                package: None,
                name: module_name,
            },
            values: &[],
            aliases: bump.alloc_slice_fill_iter([InterfaceAlias {
                context: &[],
                kind: test_constructor_kind(bump, parameters.len()),
                name: alias_name,
                parameters,
                typ,
                visibility: AliasVisibility::Public,
            }]),
            unions: &[],
            binops: &[],
        }
    }

    // === Module tests ===
    #[test]
    fn module_shell_header_only() {
        assert_module_snapshot!("module Main exposing (..)\n");
    }

    #[test]
    fn validator_missing_main() {
        assert_module_error_snapshot!("validator module Foo exposing (main)\n\nx = 1\n");
    }

    #[test]
    fn validator_main_not_exposed() {
        assert_module_error_snapshot!(
            "validator module Foo exposing (x)\n\nx = 1\n\nmain c = ()\n"
        );
    }

    #[test]
    fn validator_main_explicitly_exposed() {
        assert_module_snapshot!("validator module Foo exposing (main)\n\nmain ctx = ()\n");
    }

    #[test]
    fn validator_module_kind_is_preserved() {
        assert_module_snapshot!("validator module Main exposing (..)\n\nmain = 1\n");
    }

    #[test]
    fn module_shell_with_infix_metadata() {
        assert_module_snapshot!(
            r#"
            module Main exposing ((|>))

            infix left 6 (|>) = apR

            apR x f = f x
        "#
        );
    }

    #[test]
    fn module_shell_with_enum_union() {
        assert_module_snapshot!(
            r#"
            module Main exposing (Bool(..))

            type Bool
                = True
                | False
        "#
        );
    }

    #[test]
    fn module_shell_with_aliases_and_unions() {
        assert_module_snapshot!(
            r#"
            module Main exposing (type pair, Maybe(..))

            type alias pair 'a 'b = ('a, 'b)

            type Maybe 'a
                = Just 'a
                | Nothing
        "#
        );
    }

    #[test]
    fn module_shell_with_local_named_types() {
        assert_module_snapshot!(
            r#"
            module Main exposing (type pair, type wrappedPair, WrappedMaybe, Maybe(..))

            type alias pair 'a 'b = ('a, 'b)

            type alias wrappedPair 'a 'b = pair 'a 'b

            type alias WrappedMaybe 'a = Maybe 'a

            type Maybe 'a
                = Just 'a
                | Nothing
        "#
        );
    }

    #[test]
    fn module_shell_with_self_qualified_named_types() {
        assert_module_snapshot!(
            r#"
            module Main exposing (Maybe(..), Wrapped)

            type alias Wrapped 'a = Main.Maybe 'a

            type Maybe 'a
                = Just 'a
                | Nothing
        "#
        );
    }

    #[test]
    fn module_shell_reports_unresolved_named_types() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (Wrapped)

            type alias Wrapped = Missing
        "#
        );
    }

    #[test]
    fn module_shell_reports_unresolved_qualified_named_types() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (Wrapped)

            type alias Wrapped = Missing.Maybe
        "#
        );
    }

    #[test]
    fn module_shell_reports_missing_imported_interfaces() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (Wrapped)

            import Result

            type alias Wrapped 'e 'a = Result.Result 'e 'a
        "#
        );
    }

    #[test]
    fn module_shell_reports_ambiguous_open_imported_types() {
        let input = indoc!(
            r#"
            module Main exposing (Wrapped)

            import Maybe exposing (..)

            import Option exposing (..)

            type alias Wrapped 'a = Maybe 'a
        "#
        );
        let bump = Bump::new();
        let parameters = bump.alloc_slice_fill_iter(["a"]);
        let interfaces = BTreeMap::from([
            (
                "Maybe",
                union_interface(&bump, "Maybe", "Maybe", parameters),
            ),
            (
                "Option",
                union_interface(&bump, "Option", "Maybe", parameters),
            ),
        ]);
        let context = Context {
            package: None,
            interfaces: Some(&interfaces),
        };
        let result = parse_and_canonicalize(&bump, input, context)
            .expect_err("expected canonicalization error");

        insta::with_settings!({info => &"diagnostic",
            description => format!("Code:\n\n{}", input),
            omit_expression => true,
        }, {
            insta::assert_snapshot!(render_errors(input, &result));
        });
    }

    #[test]
    fn module_shell_reports_ambiguous_explicit_and_open_imported_types() {
        let input = indoc!(
            r#"
            module Main exposing (Wrapped)

            import Maybe exposing (Maybe)

            import Option exposing (..)

            type alias Wrapped 'a = Maybe 'a
        "#
        );
        let bump = Bump::new();
        let parameters = bump.alloc_slice_fill_iter(["a"]);
        let interfaces = BTreeMap::from([
            (
                "Maybe",
                union_interface(&bump, "Maybe", "Maybe", parameters),
            ),
            (
                "Option",
                union_interface(&bump, "Option", "Maybe", parameters),
            ),
        ]);
        let context = Context {
            package: None,
            interfaces: Some(&interfaces),
        };
        let result = parse_and_canonicalize(&bump, input, context)
            .expect_err("expected canonicalization error");

        insta::with_settings!({info => &"diagnostic",
            description => format!("Code:\n\n{}", input),
            omit_expression => true,
        }, {
            insta::assert_snapshot!(render_errors(input, &result));
        });
    }

    #[test]
    fn module_shell_reports_ambiguous_qualified_imported_types() {
        let input = indoc!(
            r#"
            module Main exposing (Wrapped)

            import Json.Decode as Decode

            import Html.Decode as Decode

            type alias Wrapped 'msg = Decode.Decoder 'msg
        "#
        );
        let bump = Bump::new();
        let parameters = bump.alloc_slice_fill_iter(["msg"]);
        let interfaces = BTreeMap::from([
            (
                "Json.Decode",
                alias_interface(
                    &bump,
                    "Json.Decode",
                    "Decoder",
                    parameters,
                    var_type(&bump, "msg"),
                ),
            ),
            (
                "Html.Decode",
                alias_interface(
                    &bump,
                    "Html.Decode",
                    "Decoder",
                    parameters,
                    var_type(&bump, "msg"),
                ),
            ),
        ]);
        let context = Context {
            package: None,
            interfaces: Some(&interfaces),
        };
        let result = parse_and_canonicalize(&bump, input, context)
            .expect_err("expected canonicalization error");

        insta::with_settings!({info => &"diagnostic",
            description => format!("Code:\n\n{}", input),
            omit_expression => true,
        }, {
            insta::assert_snapshot!(render_errors(input, &result));
        });
    }

    #[test]
    fn module_shell_rejects_constructor_as_alias_body() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (Wrapped, Maybe(..))

            type alias Wrapped = Maybe

            type Maybe 'a
                = Just 'a
                | Nothing
        "#
        );
    }

    #[test]
    fn module_shell_reports_missing_exported_values() {
        assert_module_error_snapshot!("module Main exposing (main)\n");
    }

    #[test]
    fn module_shell_reports_missing_exported_operators() {
        assert_module_error_snapshot!("module Main exposing ((|>))\n");
    }

    #[test]
    fn module_shell_reports_ambiguous_imported_types_with_multiple_modules() {
        let input = indoc!(
            r#"
            module Main exposing (Wrapped)

            import Maybe exposing (..)

            import Option exposing (..)

            import Choice exposing (..)

            type alias Wrapped 'a = Maybe 'a
        "#
        );
        let bump = Bump::new();
        let parameters = bump.alloc_slice_fill_iter(["a"]);
        let interfaces = BTreeMap::from([
            (
                "Maybe",
                union_interface(&bump, "Maybe", "Maybe", parameters),
            ),
            (
                "Option",
                union_interface(&bump, "Option", "Maybe", parameters),
            ),
            (
                "Choice",
                union_interface(&bump, "Choice", "Maybe", parameters),
            ),
        ]);
        let context = Context {
            package: None,
            interfaces: Some(&interfaces),
        };
        let result = parse_and_canonicalize(&bump, input, context)
            .expect_err("expected canonicalization error");

        insta::with_settings!({info => &"diagnostic",
            description => format!("Code:\n\n{}", input),
            omit_expression => true,
        }, {
            insta::assert_snapshot!(render_errors(input, &result));
        });
    }

    #[test]
    fn module_shell_with_imported_exposed_union_types() {
        let input = indoc!(
            r#"
            module Main exposing (Wrapped)

            import Maybe exposing (Maybe)

            type alias Wrapped 'a = Maybe 'a
        "#
        );
        let bump = Bump::new();
        let parameters = bump.alloc_slice_fill_iter(["a"]);
        let interfaces = BTreeMap::from([(
            "Maybe",
            union_interface(&bump, "Maybe", "Maybe", parameters),
        )]);
        let context = Context {
            package: None,
            interfaces: Some(&interfaces),
        };
        let result = parse_and_canonicalize(&bump, input, context)
            .expect("expected successful canonicalization");

        insta::with_settings!({
            description => format!("Code:\n\n{}", input),
            omit_expression => true,
        }, {
            insta::assert_debug_snapshot!(result);
        });
    }

    #[test]
    fn module_shell_with_imported_qualified_union_types() {
        let input = indoc!(
            r#"
            module Main exposing (Wrapped)

            import Result

            type alias Wrapped 'e 'a = Result.Result 'e 'a
        "#
        );
        let bump = Bump::new();
        let parameters = bump.alloc_slice_fill_iter(["e", "a"]);
        let interfaces = BTreeMap::from([(
            "Result",
            union_interface(&bump, "Result", "Result", parameters),
        )]);
        let context = Context {
            package: None,
            interfaces: Some(&interfaces),
        };
        let result = parse_and_canonicalize(&bump, input, context)
            .expect("expected successful canonicalization");

        insta::with_settings!({
            description => format!("Code:\n\n{}", input),
            omit_expression => true,
        }, {
            insta::assert_debug_snapshot!(result);
        });
    }

    #[test]
    fn module_shell_with_imported_aliased_alias_types() {
        let input = indoc!(
            r#"
            module Main exposing (Decoder)

            import Json.Decode as Decode

            type alias Decoder 'msg = Decode.Decoder 'msg
        "#
        );
        let bump = Bump::new();
        let parameters = bump.alloc_slice_fill_iter(["msg"]);
        let interfaces = BTreeMap::from([(
            "Json.Decode",
            alias_interface(
                &bump,
                "Json.Decode",
                "Decoder",
                parameters,
                var_type(&bump, "msg"),
            ),
        )]);
        let context = Context {
            package: None,
            interfaces: Some(&interfaces),
        };
        let result = parse_and_canonicalize(&bump, input, context)
            .expect("expected successful canonicalization");

        insta::with_settings!({
            description => format!("Code:\n\n{}", input),
            omit_expression => true,
        }, {
            insta::assert_debug_snapshot!(result);
        });
    }

    // === Converted tests ===

    #[test]
    fn module_shell_requires_explicit_header() {
        assert_module_error_snapshot!("main = 42\n");
    }

    #[test]
    fn module_shell_keeps_package_context() {
        let input = indoc!("module Json.Decode exposing (..)\n");
        let bump = Bump::new();
        let context = Context {
            package: Some(PackageName {
                author: "nash",
                project: "compiler",
            }),
            interfaces: None,
        };
        let result = parse_and_canonicalize(&bump, input, context)
            .expect("expected successful canonicalization");

        insta::with_settings!({
            description => format!("Code:\n\n{}", input),
            omit_expression => true,
        }, {
            insta::assert_debug_snapshot!(result);
        });
    }

    #[test]
    fn module_shell_reports_unresolved_exported_upper_names() {
        assert_module_error_snapshot!("module Main exposing (Missing)\n");
    }

    #[test]
    fn module_shell_reports_export_open_alias() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (Pair(..))

            type alias Pair 'a = 'a
        "#
        );
    }

    // === Interface tests ===

    #[test]
    fn interface_from_module_empty() {
        assert_interface_snapshot!("module Main exposing (..)\n");
    }

    #[test]
    fn interface_from_module_open_exports() {
        assert_interface_snapshot!(
            r#"
            module Main exposing (..)

            type alias pair 'a 'b = ('a, 'b)

            type Maybe 'a
                = Just 'a
                | Nothing
        "#
        );
    }

    #[test]
    fn interface_from_module_open_union() {
        assert_interface_snapshot!(
            r#"
            module Main exposing (Bool(..))

            type Bool
                = True
                | False
        "#
        );
    }

    #[test]
    fn interface_from_module_closed_union() {
        assert_interface_snapshot!(
            r#"
            module Main exposing (Bool)

            type Bool
                = True
                | False
        "#
        );
    }

    #[test]
    fn interface_from_module_mixed_visibility() {
        assert_interface_snapshot!(
            r#"
            module Main exposing (PublicAlias)

            type alias PublicAlias 'a = 'a

            type alias PrivateAlias 'a = 'a

            type PrivateUnion
                = Foo
                | Bar
        "#
        );
    }

    #[test]
    fn interface_from_module_with_binops() {
        assert_interface_snapshot!(
            r#"
            module Main exposing ((|>))

            infix left 6 (|>) = apR

            apR x f = f x
        "#
        );
    }

    // === to_public tests ===

    #[test]
    fn builtin_bool_patterns_count_as_import_uses() {
        let bump = Bump::new();
        let module = nash_parse::Parser::new(
            &bump,
            "module Main exposing (..)\nimport Builtin exposing (type bool(..))\nignore flag =\n    case flag of\n        False -> ()\n        True -> ()\n",
        ).module().unwrap();
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
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    }

    #[test]
    fn to_public_union_open_passes_through() {
        let union = InterfaceUnion {
            context: &[],
            kind: &nash_ast::Kind::Type,
            name: "Bool",
            parameters: &[],
            ctors: &[],
            alternatives: 2,
            options: CtorOpts::Enum,
            visibility: UnionVisibility::Open,
        };
        insta::with_settings!({
            omit_expression => true,
        }, {
            insta::assert_debug_snapshot!(union.to_public());
        });
    }

    #[test]
    fn to_public_union_closed_strips_ctors() {
        let bump = Bump::new();
        let ctor: &CanCtor = bump.alloc(CanCtor {
            labels: None,
            name: "True",
            index: 0,
            arity: 0,
            arguments: &[],
        });
        let union = InterfaceUnion {
            context: &[],
            kind: &nash_ast::Kind::Type,
            name: "Bool",
            parameters: &[],
            ctors: bump.alloc_slice_fill_iter([ctor]),
            alternatives: 2,
            options: CtorOpts::Enum,
            visibility: UnionVisibility::Closed,
        };
        insta::with_settings!({
            omit_expression => true,
        }, {
            insta::assert_debug_snapshot!(union.to_public());
        });
    }

    #[test]
    fn to_public_union_private_returns_none() {
        let union = InterfaceUnion {
            context: &[],
            kind: &nash_ast::Kind::Type,
            name: "Internal",
            parameters: &[],
            ctors: &[],
            alternatives: 0,
            options: CtorOpts::Normal,
            visibility: UnionVisibility::Private,
        };
        insta::with_settings!({
            omit_expression => true,
        }, {
            insta::assert_debug_snapshot!(union.to_public());
        });
    }

    #[test]
    fn to_public_alias_public_passes_through() {
        let bump = Bump::new();
        let typ = bump.alloc(Located::at(Region::zero(), CanType::unit()));
        let alias = InterfaceAlias {
            context: &[],
            kind: test_constructor_kind(&bump, 2),
            name: "Pair",
            parameters: &["a", "b"],
            typ,
            visibility: AliasVisibility::Public,
        };
        insta::with_settings!({
            omit_expression => true,
        }, {
            insta::assert_debug_snapshot!(alias.to_public());
        });
    }

    #[test]
    fn to_public_alias_private_returns_none() {
        let bump = Bump::new();
        let typ = bump.alloc(Located::at(Region::zero(), CanType::unit()));
        let alias = InterfaceAlias {
            context: &[],
            kind: &nash_ast::Kind::Type,
            name: "Internal",
            parameters: &[],
            typ,
            visibility: AliasVisibility::Private,
        };
        insta::with_settings!({
            omit_expression => true,
        }, {
            insta::assert_debug_snapshot!(alias.to_public());
        });
    }

    // === Validation tests ===

    #[test]
    fn duplicate_type_alias_and_union() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)

            type alias Foo = Int

            type Foo
                = Bar
        "#
        );
    }

    #[test]
    fn duplicate_type_two_aliases() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)

            type alias Foo = Int

            type alias Foo = String
        "#
        );
    }

    #[test]
    fn duplicate_value_decl() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)

            foo = 1

            foo = 2
        "#
        );
    }

    #[test]
    fn duplicate_binop() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)

            infix left 6 (|>) = apR

            infix left 6 (|>) = apR2

            apR x f = f x

            apR2 x f = f x
        "#
        );
    }

    #[test]
    fn record_type_duplicate_field() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)

            type alias R = { x : Int, x : String }
        "#
        );
    }

    #[test]
    fn recursive_alias_self() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)

            type alias Loop = Loop
        "#
        );
    }

    #[test]
    fn recursive_alias_cycle() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)

            type alias A = B

            type alias B = A
        "#
        );
    }

    #[test]
    fn union_unbound_type_var() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)

            type MyType 'a
                = MyTag 'b
        "#
        );
    }

    #[test]
    fn alias_unused_type_param() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)

            type alias Phantom 'a = Int
        "#
        );
    }

    #[test]
    fn alias_unbound_type_var() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)

            type alias Bad = List 'a
        "#
        );
    }

    #[test]
    fn union_duplicate_type_param() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)

            type Bad 'a 'a
                = Foo
        "#
        );
    }

    #[test]
    fn alias_duplicate_type_param() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)

            type alias Bad 'a 'a = 'a
        "#
        );
    }

    #[test]
    fn multiple_duplicate_types_all_reported() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)

            type alias Foo = Int

            type alias Foo = String

            type alias Bar = Int

            type alias Bar = String
        "#
        );
    }

    #[test]
    fn record_multiple_duplicate_fields_all_reported() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)

            type alias R = { x : Int, x : String, y : Int, y : String }
        "#
        );
    }

    #[test]
    fn duplicate_export() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (Foo, Foo)

            type alias Foo = Int
        "#
        );
    }

    // === Expression canonicalization tests ===

    fn parse_and_canonicalize_with_warnings<'a>(
        bump: &'a Bump,
        input: &str,
        context: Context<'a, '_>,
    ) -> Result<(CanModule<'a>, Vec<nash_can::Warning<'a>>), Vec<Error<'a>>> {
        let src = bump.alloc_str(input);
        let mut parser = nash_parse::Parser::new(bump, src);
        let module = parser.module().expect("expected successful parse");
        canonicalize(bump, context, &module).map(|r| (r.module, r.warnings))
    }

    macro_rules! assert_module_warning_snapshot {
        ($input:expr) => {{
            let input = indoc!($input);
            let bump = Bump::new();
            let (_, warnings) =
                parse_and_canonicalize_with_warnings(&bump, input, Context::default())
                    .expect("expected successful canonicalization");
            assert!(!warnings.is_empty(), "expected warnings but got none");
            insta::with_settings!({info => &"diagnostic",
                description => format!("Code:\n\n{}", input),
                omit_expression => true,
            }, {
                insta::assert_snapshot!(render_warnings(input, &warnings));
            });
        }};
    }

    // -- Positive expression tests --

    #[test]
    fn simple_value() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            x = 42
        "#
        );
    }

    #[test]
    fn function_def() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            f a b = a
        "#
        );
    }

    #[test]
    fn lambda_expr() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            f = \x -> x
        "#
        );
    }

    #[test]
    fn let_expr() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            f =
                let
                    x = 1
                in
                x
        "#
        );
    }

    #[test]
    fn if_then_else() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            f x = if x then 1 else 2
        "#
        );
    }

    #[test]
    fn record_literal() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            type alias point = { x : int, y : int }
            f = { x = 1, y = 2 }
        "#
        );
    }

    #[test]
    fn record_update() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            f r = { r | x = 1 }
        "#
        );
    }

    #[test]
    fn list_literal() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            f = [ 1, 2, 3 ]
        "#
        );
    }

    #[test]
    fn accessor_expr() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            f = .name
        "#
        );
    }

    #[test]
    fn field_access() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            f r = r.name
        "#
        );
    }

    #[test]
    fn case_expr() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            f x =
                case x of
                    1 -> "one"
                    _ -> "other"
        "#
        );
    }

    #[test]
    fn string_literal() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            f = "hello"
        "#
        );
    }

    #[test]
    fn negation_requires_core_num() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)

            trait Num 'a where
                negate : 'a -> 'a
            f x = -x
        "#
        );
    }

    #[test]
    fn function_call() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            f x = g x

            g y = y
        "#
        );
    }

    #[test]
    fn tuple_expr() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            f = ( 1, 2, 3 )
        "#
        );
    }

    #[test]
    fn unit_expr() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            f = ()
        "#
        );
    }

    #[test]
    fn let_recursive_function() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            f =
                let
                    go x = go x
                in
                go 1
        "#
        );
    }

    #[test]
    fn nested_let() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            f =
                let
                    x = 1
                    y = 2
                in
                x
        "#
        );
    }

    // -- Error expression tests --

    #[test]
    fn not_found_var() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)

            f = nonexistent
        "#
        );
    }

    #[test]
    fn recursive_let_value() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)

            f =
                let
                    x = x
                in
                x
        "#
        );
    }

    #[test]
    fn scope_recovery_between_case_branches() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)

            f outer =
                case outer of
                    first -> missing
                    second -> first
        "#
        );
    }

    #[test]
    fn scope_recovery_after_rejected_shadowing() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)

            f outer =
                case outer of
                    outer -> outer
                    next -> unknown
        "#
        );
    }

    #[test]
    fn scope_siblings_reuse_binding_names() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            f outer =
                case outer of
                    (first, value) -> value
                    (second, value) -> value
        "#
        );
    }

    #[test]
    fn shadowing_local() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)

            f x =
                let
                    x = 1
                in
                x
        "#
        );
    }

    #[test]
    fn duplicate_let_bindings() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)

            f =
                let
                    x = 1
                    x = 2
                in
                x
        "#
        );
    }

    #[test]
    fn tuple_four_in_expr() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            f = ( 1, 2, 3, 4 )
        "#
        );
    }

    #[test]
    fn duplicate_record_fields_in_expr() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)

            f = { x = 1, x = 2 }
        "#
        );
    }

    // -- Warning tests --

    #[test]
    fn unused_lambda_arg() {
        assert_module_warning_snapshot!(
            r#"
            module Main exposing (..)

            f = \x -> 1
        "#
        );
    }

    #[test]
    fn unused_let_binding() {
        assert_module_warning_snapshot!(
            r#"
            module Main exposing (..)

            f =
                let
                    x = 1
                in
                2
        "#
        );
    }

    // === SCC tests ===

    #[test]
    fn mutual_recursion_functions() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            f x = g x

            g x = f x
        "#
        );
    }

    #[test]
    fn self_recursive_function() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            f x = f x
        "#
        );
    }

    #[test]
    fn mixed_recursive_and_non_recursive() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            x = 1

            f a = g a

            g a = f a

            y = 2
        "#
        );
    }

    #[test]
    fn dependency_ordering() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            b = a

            a = 1
        "#
        );
    }

    #[test]
    fn value_and_function_in_cycle() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            x = f 1

            f a = x
        "#
        );
    }

    // -- SCC error tests --

    #[test]
    fn recursive_decl_self_reference() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)

            x = x
        "#
        );
    }

    #[test]
    fn recursive_decl_mutual_values() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)

            x = y

            y = x
        "#
        );
    }

    // === Unused import warning tests ===

    fn value_interface<'a>(
        bump: &'a Bump,
        module_name: &'a str,
        val_name: &'a str,
    ) -> Interface<'a> {
        Interface {
            impls: &[],
            traits: &[],
            home: ModuleName {
                package: None,
                name: module_name,
            },
            values: bump.alloc_slice_fill_iter([nash_can::InterfaceValue {
                name: val_name,
                annotation: test_annotation(bump),
            }]),
            aliases: &[],
            unions: &[],
            binops: &[],
        }
    }

    #[test]
    fn unused_import_warning() {
        let input = indoc!(
            r#"
            module Main exposing (..)

            import Foo

            x = 1
        "#
        );
        let bump = Bump::new();
        let interfaces = BTreeMap::from([("Foo", value_interface(&bump, "Foo", "bar"))]);
        let context = Context {
            package: None,
            interfaces: Some(&interfaces),
        };
        let (_, warnings) = parse_and_canonicalize_with_warnings(&bump, input, context)
            .expect("expected successful canonicalization");
        assert!(!warnings.is_empty(), "expected warnings but got none");
        insta::with_settings!({info => &"diagnostic",
            description => format!("Code:\n\n{}", input),
            omit_expression => true,
        }, {
            insta::assert_snapshot!(render_warnings(input, &warnings));
        });
    }

    #[test]
    fn used_import_no_warning() {
        let input = indoc!(
            r#"
            module Main exposing (..)

            import Foo

            x = Foo.bar
        "#
        );
        let bump = Bump::new();
        let interfaces = BTreeMap::from([("Foo", value_interface(&bump, "Foo", "bar"))]);
        let context = Context {
            package: None,
            interfaces: Some(&interfaces),
        };
        let (_, warnings) = parse_and_canonicalize_with_warnings(&bump, input, context)
            .expect("expected successful canonicalization");
        assert!(
            warnings.is_empty(),
            "expected no warnings but got: {warnings:?}"
        );
    }

    // === Helpers for interface-dependent tests ===

    fn maybe_with_ctors_interface<'a>(bump: &'a Bump) -> Interface<'a> {
        let just_arg = bump.alloc(Located::at(Region::zero(), CanType::Var("a")));
        let just_ctor: &CanCtor = bump.alloc(CanCtor {
            labels: None,
            name: "Just",
            index: 0,
            arity: 1,
            arguments: bump.alloc_slice_fill_iter([&*just_arg]),
        });
        let nothing_ctor: &CanCtor = bump.alloc(CanCtor {
            labels: None,
            name: "Nothing",
            index: 1,
            arity: 0,
            arguments: &[],
        });
        Interface {
            impls: &[],
            traits: &[],
            home: ModuleName {
                package: None,
                name: "Maybe",
            },
            values: &[],
            aliases: &[],
            unions: bump.alloc_slice_fill_iter([InterfaceUnion {
                context: &[],
                kind: test_constructor_kind(bump, 1),
                name: "Maybe",
                parameters: bump.alloc_slice_fill_iter(["a"]),
                ctors: bump.alloc_slice_fill_iter([just_ctor, nothing_ctor]),
                alternatives: 2,
                options: CtorOpts::Normal,
                visibility: UnionVisibility::Open,
            }]),
            binops: &[],
        }
    }

    fn basics_with_binops_interface<'a>(bump: &'a Bump) -> Interface<'a> {
        Interface {
            impls: &[],
            traits: &[],
            home: ModuleName {
                package: None,
                name: "Basics",
            },
            values: bump.alloc_slice_fill_iter([
                InterfaceValue {
                    name: "add",
                    annotation: test_annotation(bump),
                },
                InterfaceValue {
                    name: "sub",
                    annotation: test_annotation(bump),
                },
                InterfaceValue {
                    name: "mul",
                    annotation: test_annotation(bump),
                },
                InterfaceValue {
                    name: "apR",
                    annotation: test_annotation(bump),
                },
                InterfaceValue {
                    name: "apL",
                    annotation: test_annotation(bump),
                },
            ]),
            aliases: &[],
            unions: &[],
            binops: bump.alloc_slice_fill_iter([
                InterfaceBinop {
                    symbol: "+",
                    annotation: test_annotation(bump),
                    associativity: Associativity::Left,
                    precedence: Precedence(6),
                    function: nash_ast::QualifiedName {
                        home: ModuleName {
                            package: None,
                            name: "Basics",
                        },
                        name: "add",
                    },
                },
                InterfaceBinop {
                    symbol: "-",
                    annotation: test_annotation(bump),
                    associativity: Associativity::Left,
                    precedence: Precedence(6),
                    function: nash_ast::QualifiedName {
                        home: ModuleName {
                            package: None,
                            name: "Basics",
                        },
                        name: "sub",
                    },
                },
                InterfaceBinop {
                    symbol: "*",
                    annotation: test_annotation(bump),
                    associativity: Associativity::Left,
                    precedence: Precedence(7),
                    function: nash_ast::QualifiedName {
                        home: ModuleName {
                            package: None,
                            name: "Basics",
                        },
                        name: "mul",
                    },
                },
                InterfaceBinop {
                    symbol: "|>",
                    annotation: test_annotation(bump),
                    associativity: Associativity::Left,
                    precedence: Precedence(0),
                    function: nash_ast::QualifiedName {
                        home: ModuleName {
                            package: None,
                            name: "Basics",
                        },
                        name: "apR",
                    },
                },
                InterfaceBinop {
                    symbol: "<|",
                    annotation: test_annotation(bump),
                    associativity: Associativity::Right,
                    precedence: Precedence(0),
                    function: nash_ast::QualifiedName {
                        home: ModuleName {
                            package: None,
                            name: "Basics",
                        },
                        name: "apL",
                    },
                },
            ]),
        }
    }

    // === Constructor / operator expression tests ===

    #[test]
    fn constructor_in_expression() {
        let input = indoc!(
            r#"
            module Main exposing (..)

            import Maybe exposing (Maybe(..))

            x = Just
        "#
        );
        let bump = Bump::new();
        let interfaces = BTreeMap::from([("Maybe", maybe_with_ctors_interface(&bump))]);
        let context = Context {
            package: None,
            interfaces: Some(&interfaces),
        };
        let result = parse_and_canonicalize(&bump, input, context)
            .expect("expected successful canonicalization");
        insta::with_settings!({
            description => format!("Code:\n\n{}", input),
            omit_expression => true,
        }, {
            insta::assert_debug_snapshot!(result);
        });
    }

    #[test]
    fn record_ctor_in_expression() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            type alias Pair 'a 'b = { first : 'a, second : 'b }

            p = Pair
        "#
        );
    }

    // qualified_ctor_in_expression skipped: parser doesn't yet produce
    // VarQual { kind: CapVar } for `Module.Ctor` syntax.

    #[test]
    fn op_as_value() {
        let input = indoc!(
            r#"
            module Main exposing (..)

            import Basics exposing (..)

            x = (+)
        "#
        );
        let bump = Bump::new();
        let interfaces = BTreeMap::from([("Basics", basics_with_binops_interface(&bump))]);
        let context = Context {
            package: None,
            interfaces: Some(&interfaces),
        };
        let result = parse_and_canonicalize(&bump, input, context)
            .expect("expected successful canonicalization");
        insta::with_settings!({
            description => format!("Code:\n\n{}", input),
            omit_expression => true,
        }, {
            insta::assert_debug_snapshot!(result);
        });
    }

    #[test]
    fn binop_expression() {
        let input = indoc!(
            r#"
            module Main exposing (..)

            import Basics exposing (..)

            x a b = a + b
        "#
        );
        let bump = Bump::new();
        let interfaces = BTreeMap::from([("Basics", basics_with_binops_interface(&bump))]);
        let context = Context {
            package: None,
            interfaces: Some(&interfaces),
        };
        let result = parse_and_canonicalize(&bump, input, context)
            .expect("expected successful canonicalization");
        insta::with_settings!({
            description => format!("Code:\n\n{}", input),
            omit_expression => true,
        }, {
            insta::assert_debug_snapshot!(result);
        });
    }

    #[test]
    fn binop_multi_precedence() {
        let input = indoc!(
            r#"
            module Main exposing (..)

            import Basics exposing (..)

            x a b c = a + b * c
        "#
        );
        let bump = Bump::new();
        let interfaces = BTreeMap::from([("Basics", basics_with_binops_interface(&bump))]);
        let context = Context {
            package: None,
            interfaces: Some(&interfaces),
        };
        let result = parse_and_canonicalize(&bump, input, context)
            .expect("expected successful canonicalization");
        insta::with_settings!({
            description => format!("Code:\n\n{}", input),
            omit_expression => true,
        }, {
            insta::assert_debug_snapshot!(result);
        });
    }

    #[test]
    fn binop_right_assoc() {
        let input = indoc!(
            r#"
            module Main exposing (..)

            import Basics exposing (..)

            x a b c = a <| b <| c
        "#
        );
        let bump = Bump::new();
        let interfaces = BTreeMap::from([("Basics", basics_with_binops_interface(&bump))]);
        let context = Context {
            package: None,
            interfaces: Some(&interfaces),
        };
        let result = parse_and_canonicalize(&bump, input, context)
            .expect("expected successful canonicalization");
        insta::with_settings!({
            description => format!("Code:\n\n{}", input),
            omit_expression => true,
        }, {
            insta::assert_debug_snapshot!(result);
        });
    }

    // === Typed def tests ===

    #[test]
    fn typed_def_top_level() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            f : 'a -> 'a
            f x = x
        "#
        );
    }

    #[test]
    fn typed_def_in_let() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            g =
                let
                    f : 'a -> 'a
                    f x = x
                in
                f 1
        "#
        );
    }

    // === Let destruct ===

    #[test]
    fn let_destruct() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            f =
                let
                    (a, b) = (1, 2)
                in
                a
        "#
        );
    }

    #[test]
    fn let_mutual_recursion() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            f =
                let
                    go x = stop x
                    stop x = go x
                in
                go 1
        "#
        );
    }

    // === find_var gaps ===

    #[test]
    fn foreign_var_unqualified() {
        let input = indoc!(
            r#"
            module Main exposing (..)

            import Foo exposing (bar)

            x = bar
        "#
        );
        let bump = Bump::new();
        let interfaces = BTreeMap::from([("Foo", value_interface(&bump, "Foo", "bar"))]);
        let context = Context {
            package: None,
            interfaces: Some(&interfaces),
        };
        let result = parse_and_canonicalize(&bump, input, context)
            .expect("expected successful canonicalization");
        insta::with_settings!({
            description => format!("Code:\n\n{}", input),
            omit_expression => true,
        }, {
            insta::assert_debug_snapshot!(result);
        });
    }

    #[test]
    fn not_found_var_qualified() {
        let input = indoc!(
            r#"
            module Main exposing (..)

            import Foo

            x = Foo.nonexistent
        "#
        );
        let bump = Bump::new();
        let interfaces = BTreeMap::from([("Foo", value_interface(&bump, "Foo", "bar"))]);
        let context = Context {
            package: None,
            interfaces: Some(&interfaces),
        };
        let result = parse_and_canonicalize(&bump, input, context)
            .expect_err("expected canonicalization error");
        insta::with_settings!({info => &"diagnostic",
            description => format!("Code:\n\n{}", input),
            omit_expression => true,
        }, {
            insta::assert_snapshot!(render_errors(input, &result));
        });
    }

    // === verify_bindings ===

    #[test]
    fn underscore_prefix_no_warning() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            f = \_ -> 1
        "#
        );
    }

    // === canonicalize_if ===

    #[test]
    fn chained_if_else_if() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            f x =
                if x then
                    1
                else if x then
                    2
                else
                    3
        "#
        );
    }

    // === canonicalize_update ===

    #[test]
    fn update_missing_record_var() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)

            f = { nonexistent | x = 1 }
        "#
        );
    }

    // === case branches ===

    #[test]
    fn case_with_ctor_patterns() {
        let input = indoc!(
            r#"
            module Main exposing (..)

            import Maybe exposing (Maybe(..))

            f x =
                case x of
                    Just y -> y
                    Nothing -> 0
        "#
        );
        let bump = Bump::new();
        let interfaces = BTreeMap::from([("Maybe", maybe_with_ctors_interface(&bump))]);
        let context = Context {
            package: None,
            interfaces: Some(&interfaces),
        };
        let result = parse_and_canonicalize(&bump, input, context)
            .expect("expected successful canonicalization");
        insta::with_settings!({
            description => format!("Code:\n\n{}", input),
            omit_expression => true,
        }, {
            insta::assert_debug_snapshot!(result);
        });
    }

    // === Expression error tests ===

    #[test]
    fn not_found_binop() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)

            x = (+)
        "#
        );
    }

    #[test]
    fn ambiguous_var() {
        let input = indoc!(
            r#"
            module Main exposing (..)

            import Foo exposing (..)

            import Bar exposing (..)

            x = baz
        "#
        );
        let bump = Bump::new();
        let interfaces = BTreeMap::from([
            ("Foo", value_interface(&bump, "Foo", "baz")),
            ("Bar", value_interface(&bump, "Bar", "baz")),
        ]);
        let context = Context {
            package: None,
            interfaces: Some(&interfaces),
        };
        let result = parse_and_canonicalize(&bump, input, context)
            .expect_err("expected canonicalization error");
        insta::with_settings!({info => &"diagnostic",
            description => format!("Code:\n\n{}", input),
            omit_expression => true,
        }, {
            insta::assert_snapshot!(render_errors(input, &result));
        });
    }

    #[test]
    fn ambiguous_ctor_in_expr() {
        let input = indoc!(
            r#"
            module Main exposing (..)

            import Maybe exposing (Maybe(..))

            import Option exposing (Option(..))

            x = Just
        "#
        );
        let bump = Bump::new();
        // Build two interfaces that both expose "Just"
        let just_arg = bump.alloc(Located::at(Region::zero(), CanType::Var("a")));
        let just_ctor: &CanCtor = bump.alloc(CanCtor {
            labels: None,
            name: "Just",
            index: 0,
            arity: 1,
            arguments: bump.alloc_slice_fill_iter([&*just_arg]),
        });
        let nothing_ctor: &CanCtor = bump.alloc(CanCtor {
            labels: None,
            name: "Nothing",
            index: 1,
            arity: 0,
            arguments: &[],
        });
        let maybe_interface = Interface {
            impls: &[],
            traits: &[],
            home: ModuleName {
                package: None,
                name: "Maybe",
            },
            values: &[],
            aliases: &[],
            unions: bump.alloc_slice_fill_iter([InterfaceUnion {
                context: &[],
                kind: test_constructor_kind(&bump, 1),
                name: "Maybe",
                parameters: bump.alloc_slice_fill_iter(["a"]),
                ctors: bump.alloc_slice_fill_iter([just_ctor, nothing_ctor]),
                alternatives: 2,
                options: CtorOpts::Normal,
                visibility: UnionVisibility::Open,
            }]),
            binops: &[],
        };

        let just_arg2 = bump.alloc(Located::at(Region::zero(), CanType::Var("a")));
        let just_ctor2: &CanCtor = bump.alloc(CanCtor {
            labels: None,
            name: "Just",
            index: 0,
            arity: 1,
            arguments: bump.alloc_slice_fill_iter([&*just_arg2]),
        });
        let none_ctor: &CanCtor = bump.alloc(CanCtor {
            labels: None,
            name: "None",
            index: 1,
            arity: 0,
            arguments: &[],
        });
        let option_interface = Interface {
            impls: &[],
            traits: &[],
            home: ModuleName {
                package: None,
                name: "Option",
            },
            values: &[],
            aliases: &[],
            unions: bump.alloc_slice_fill_iter([InterfaceUnion {
                context: &[],
                kind: test_constructor_kind(&bump, 1),
                name: "Option",
                parameters: bump.alloc_slice_fill_iter(["a"]),
                ctors: bump.alloc_slice_fill_iter([just_ctor2, none_ctor]),
                alternatives: 2,
                options: CtorOpts::Normal,
                visibility: UnionVisibility::Open,
            }]),
            binops: &[],
        };

        let interfaces = BTreeMap::from([("Maybe", maybe_interface), ("Option", option_interface)]);
        let context = Context {
            package: None,
            interfaces: Some(&interfaces),
        };
        let result = parse_and_canonicalize(&bump, input, context)
            .expect_err("expected canonicalization error");
        insta::with_settings!({info => &"diagnostic",
            description => format!("Code:\n\n{}", input),
            omit_expression => true,
        }, {
            insta::assert_snapshot!(render_errors(input, &result));
        });
    }

    #[test]
    fn ambiguous_binop() {
        let input = indoc!(
            r#"
            module Main exposing (..)

            import Basics exposing (..)

            import MyMath exposing (..)

            x a b = a + b
        "#
        );
        let bump = Bump::new();
        let basics = basics_with_binops_interface(&bump);
        let mymath = Interface {
            impls: &[],
            traits: &[],
            home: ModuleName {
                package: None,
                name: "MyMath",
            },
            values: &[],
            aliases: &[],
            unions: &[],
            binops: bump.alloc_slice_fill_iter([InterfaceBinop {
                symbol: "+",
                annotation: test_annotation(&bump),
                associativity: Associativity::Left,
                precedence: Precedence(6),
                function: nash_ast::QualifiedName {
                    home: ModuleName {
                        package: None,
                        name: "MyMath",
                    },
                    name: "myAdd",
                },
            }]),
        };
        let interfaces = BTreeMap::from([("Basics", basics), ("MyMath", mymath)]);
        let context = Context {
            package: None,
            interfaces: Some(&interfaces),
        };
        let result = parse_and_canonicalize(&bump, input, context)
            .expect_err("expected canonicalization error");
        insta::with_settings!({info => &"diagnostic",
            description => format!("Code:\n\n{}", input),
            omit_expression => true,
        }, {
            insta::assert_snapshot!(render_errors(input, &result));
        });
    }

    #[test]
    fn binop_non_assoc_conflict() {
        let input = indoc!(
            r#"
            module Main exposing (..)

            import Basics exposing (..)

            x a b c = a == b == c
        "#
        );
        let bump = Bump::new();
        let basics = Interface {
            impls: &[],
            traits: &[],
            home: ModuleName {
                package: None,
                name: "Basics",
            },
            values: bump.alloc_slice_fill_iter([InterfaceValue {
                name: "eq",
                annotation: test_annotation(&bump),
            }]),
            aliases: &[],
            unions: &[],
            binops: bump.alloc_slice_fill_iter([InterfaceBinop {
                symbol: "==",
                annotation: test_annotation(&bump),
                associativity: Associativity::None,
                precedence: Precedence(4),
                function: nash_ast::QualifiedName {
                    home: ModuleName {
                        package: None,
                        name: "Basics",
                    },
                    name: "eq",
                },
            }]),
        };
        let interfaces = BTreeMap::from([("Basics", basics)]);
        let context = Context {
            package: None,
            interfaces: Some(&interfaces),
        };
        let result = parse_and_canonicalize(&bump, input, context)
            .expect_err("expected canonicalization error");
        insta::with_settings!({info => &"diagnostic",
            description => format!("Code:\n\n{}", input),
            omit_expression => true,
        }, {
            insta::assert_snapshot!(render_errors(input, &result));
        });
    }

    #[test]
    fn annotation_too_short() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)

            f : a
            f x = x
        "#
        );
    }

    #[test]
    fn duplicate_field_in_update() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)

            f r = { r | x = 1, x = 2 }
        "#
        );
    }

    #[test]
    fn duplicate_pattern_lambda_args() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)

            f = \x x -> x
        "#
        );
    }

    #[test]
    fn duplicate_pattern_func_args() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)

            f x x = x
        "#
        );
    }

    #[test]
    fn duplicate_pattern_destruct() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)

            f =
                let
                    (x, x) = (1, 2)
                in
                x
        "#
        );
    }

    #[test]
    fn let_mutual_recursion_values_error() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)

            f =
                let
                    a = b
                    b = a
                in
                a
        "#
        );
    }

    #[test]
    fn shadowing_toplevel() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)

            x = 1

            f =
                let
                    x = 2
                in
                x
        "#
        );
    }

    // === Expression warning tests ===

    #[test]
    fn unused_func_arg_warning() {
        assert_module_warning_snapshot!(
            r#"
            module Main exposing (..)

            f x = 42
        "#
        );
    }

    #[test]
    fn unused_case_branch_var() {
        let input = indoc!(
            r#"
            module Main exposing (..)

            import Maybe exposing (Maybe(..))

            f x =
                case x of
                    Just y -> 1
                    Nothing -> 0
        "#
        );
        let bump = Bump::new();
        let interfaces = BTreeMap::from([("Maybe", maybe_with_ctors_interface(&bump))]);
        let context = Context {
            package: None,
            interfaces: Some(&interfaces),
        };
        let (_, warnings) = parse_and_canonicalize_with_warnings(&bump, input, context)
            .expect("expected successful canonicalization");
        assert!(!warnings.is_empty(), "expected warnings but got none");
        insta::with_settings!({info => &"diagnostic",
            description => format!("Code:\n\n{}", input),
            omit_expression => true,
        }, {
            insta::assert_snapshot!(render_warnings(input, &warnings));
        });
    }

    // === Module-level tests ===

    #[test]
    fn duplicate_ctor_error() {
        // A record alias and union both produce a ctor named "Point"
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)

            type alias Point 'a = { x : 'a, y : 'a }

            type Shape 'a
                = Point 'a 'a
                | Circle 'a
        "#
        );
    }

    #[test]
    fn ctor_opts_unbox() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            type Wrap 'a
                = Wrap 'a
        "#
        );
    }

    #[test]
    fn multiple_unbound_union_vars() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)

            type Foo
                = Bar a b
        "#
        );
    }

    #[test]
    fn alias_both_unused_and_unbound() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)

            type alias Bad 'a = 'b
        "#
        );
    }

    #[test]
    fn export_explicit_value() {
        assert_module_snapshot!(
            r#"
            module Main exposing (foo)

            foo = 42
        "#
        );
    }

    #[test]
    fn infix_right_associativity() {
        assert_module_snapshot!(
            r#"
            module Main exposing ((<|))

            infix right 0 (<|) = apL

            apL f x = f x
        "#
        );
    }

    #[test]
    fn infix_non_associativity() {
        assert_module_snapshot!(
            r#"
            module Main exposing ((==))

            infix non 4 (==) = eq

            eq a b = a
        "#
        );
    }

    #[test]
    fn local_record_alias_ctor() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            type alias Point 'a = { x : 'a, y : 'a }

            p = Point
        "#
        );
    }

    // === Interface tests: non-exported binop ===

    #[test]
    fn interface_non_exported_binop() {
        assert_interface_snapshot!(
            r#"
            module Main exposing (foo)

            infix left 6 (|>) = apR

            apR x f = f x

            foo = 42
        "#
        );
    }

    // === Import validation tests ===

    #[test]
    fn import_exposing_not_found_value() {
        let input = indoc!(
            r#"
            module Main exposing (..)

            import Foo exposing (nonexistent)

            x = 1
        "#
        );
        let bump = Bump::new();
        let interfaces = BTreeMap::from([("Foo", value_interface(&bump, "Foo", "bar"))]);
        let context = Context {
            package: None,
            interfaces: Some(&interfaces),
        };
        let result = parse_and_canonicalize(&bump, input, context)
            .expect_err("expected canonicalization error");
        insta::with_settings!({info => &"diagnostic",
            description => format!("Code:\n\n{}", input),
            omit_expression => true,
        }, {
            insta::assert_snapshot!(render_errors(input, &result));
        });
    }

    #[test]
    fn import_exposing_not_found_type() {
        let input = indoc!(
            r#"
            module Main exposing (..)

            import Foo exposing (Nonexistent)

            x = 1
        "#
        );
        let bump = Bump::new();
        let interfaces = BTreeMap::from([("Foo", value_interface(&bump, "Foo", "bar"))]);
        let context = Context {
            package: None,
            interfaces: Some(&interfaces),
        };
        let result = parse_and_canonicalize(&bump, input, context)
            .expect_err("expected canonicalization error");
        insta::with_settings!({info => &"diagnostic",
            description => format!("Code:\n\n{}", input),
            omit_expression => true,
        }, {
            insta::assert_snapshot!(render_errors(input, &result));
        });
    }

    #[test]
    fn import_exposing_not_found_op() {
        let input = indoc!(
            r#"
            module Main exposing (..)

            import Foo exposing ((+))

            x = 1
        "#
        );
        let bump = Bump::new();
        let interfaces = BTreeMap::from([("Foo", value_interface(&bump, "Foo", "bar"))]);
        let context = Context {
            package: None,
            interfaces: Some(&interfaces),
        };
        let result = parse_and_canonicalize(&bump, input, context)
            .expect_err("expected canonicalization error");
        insta::with_settings!({info => &"diagnostic",
            description => format!("Code:\n\n{}", input),
            omit_expression => true,
        }, {
            insta::assert_snapshot!(render_errors(input, &result));
        });
    }

    #[test]
    fn import_ctor_by_name() {
        let input = indoc!(
            r#"
            module Main exposing (..)

            import Maybe exposing (Just)

            x = 1
        "#
        );
        let bump = Bump::new();
        let interfaces = BTreeMap::from([("Maybe", maybe_with_ctors_interface(&bump))]);
        let context = Context {
            package: None,
            interfaces: Some(&interfaces),
        };
        let result = parse_and_canonicalize(&bump, input, context)
            .expect_err("expected canonicalization error");
        insta::with_settings!({info => &"diagnostic",
            description => format!("Code:\n\n{}", input),
            omit_expression => true,
        }, {
            insta::assert_snapshot!(render_errors(input, &result));
        });
    }

    #[test]
    fn import_open_alias() {
        let input = indoc!(
            r#"
            module Main exposing (..)

            import Foo exposing (MyAlias(..))

            x = 1
        "#
        );
        let bump = Bump::new();
        let alias_type = bump.alloc(Located::at(Region::zero(), CanType::Var("a")));
        let foo = Interface {
            impls: &[],
            traits: &[],
            home: ModuleName {
                package: None,
                name: "Foo",
            },
            values: &[],
            aliases: bump.alloc_slice_fill_iter([InterfaceAlias {
                context: &[],
                kind: test_constructor_kind(&bump, 1),
                name: "MyAlias",
                parameters: bump.alloc_slice_fill_iter(["a"]),
                typ: alias_type,
                visibility: AliasVisibility::Public,
            }]),
            unions: &[],
            binops: &[],
        };
        let interfaces = BTreeMap::from([("Foo", foo)]);
        let context = Context {
            package: None,
            interfaces: Some(&interfaces),
        };
        let result = parse_and_canonicalize(&bump, input, context)
            .expect_err("expected canonicalization error");
        insta::with_settings!({info => &"diagnostic",
            description => format!("Code:\n\n{}", input),
            omit_expression => true,
        }, {
            insta::assert_snapshot!(render_errors(input, &result));
        });
    }

    // === collect_used_modules coverage ===

    #[test]
    fn collect_from_def_typed() {
        let input = indoc!(
            r#"
            module Main exposing (..)

            import Maybe exposing (Maybe)

            f : Maybe 'a -> Maybe 'a
            f x = x
        "#
        );
        let bump = Bump::new();
        let interfaces = BTreeMap::from([("Maybe", maybe_with_ctors_interface(&bump))]);
        let context = Context {
            package: None,
            interfaces: Some(&interfaces),
        };
        let (_, warnings) = parse_and_canonicalize_with_warnings(&bump, input, context)
            .expect("expected successful canonicalization");
        // Should not have an "unused import" warning for Maybe because
        // it is used in the type annotation.
        assert!(
            warnings.is_empty(),
            "expected no warnings but got: {warnings:?}"
        );
    }

    // === Import privacy (toPublicUnion / toPublicAlias) ===

    fn maybe_interface_with_visibility<'a>(
        bump: &'a Bump,
        visibility: UnionVisibility,
    ) -> Interface<'a> {
        let base = maybe_with_ctors_interface(bump);
        Interface {
            impls: &[],
            traits: &[],
            unions: bump.alloc_slice_fill_iter([InterfaceUnion {
                visibility,
                ..base.unions[0]
            }]),
            ..base
        }
    }

    #[test]
    fn closed_union_does_not_leak_ctors() {
        let input = indoc!(
            r#"
            module Main exposing (..)

            import Maybe exposing (Maybe(..))

            x = Just
        "#
        );
        let bump = Bump::new();
        let interfaces = BTreeMap::from([(
            "Maybe",
            maybe_interface_with_visibility(&bump, UnionVisibility::Closed),
        )]);
        let context = Context {
            package: None,
            interfaces: Some(&interfaces),
        };
        let result = parse_and_canonicalize(&bump, input, context)
            .expect_err("expected canonicalization error");
        insta::with_settings!({info => &"diagnostic",
            description => format!("Code:\n\n{}", input),
            omit_expression => true,
        }, {
            insta::assert_snapshot!(render_errors(input, &result));
        });
    }

    #[test]
    fn private_union_not_importable() {
        let input = indoc!(
            r#"
            module Main exposing (..)

            import Maybe exposing (Maybe)

            x = 1
        "#
        );
        let bump = Bump::new();
        let interfaces = BTreeMap::from([(
            "Maybe",
            maybe_interface_with_visibility(&bump, UnionVisibility::Private),
        )]);
        let context = Context {
            package: None,
            interfaces: Some(&interfaces),
        };
        let result = parse_and_canonicalize(&bump, input, context)
            .expect_err("expected canonicalization error");
        insta::with_settings!({info => &"diagnostic",
            description => format!("Code:\n\n{}", input),
            omit_expression => true,
        }, {
            insta::assert_snapshot!(render_errors(input, &result));
        });
    }

    #[test]
    fn aliased_import_exposes_ctors_unqualified() {
        let input = indoc!(
            r#"
            module Main exposing (..)

            import Maybe as M exposing (Maybe(..))

            x = Just
        "#
        );
        let bump = Bump::new();
        let interfaces = BTreeMap::from([("Maybe", maybe_with_ctors_interface(&bump))]);
        let context = Context {
            package: None,
            interfaces: Some(&interfaces),
        };
        let result = parse_and_canonicalize(&bump, input, context)
            .expect("expected successful canonicalization");
        insta::with_settings!({
            description => format!("Code:\n\n{}", input),
            omit_expression => true,
        }, {
            insta::assert_debug_snapshot!(result);
        });
    }

    // === Let destructuring ===

    #[test]
    fn let_destruct_self_reference_is_recursive() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing (..)

            main = let (a, b) = (a, 1) in b
        "#
        );
    }

    #[test]
    fn let_destruct_ctor_pattern_binds_names() {
        let input = indoc!(
            r#"
            module Main exposing (..)

            import Maybe exposing (Maybe(..))

            f w = let (Just x) = w in x
        "#
        );
        let bump = Bump::new();
        let interfaces = BTreeMap::from([("Maybe", maybe_with_ctors_interface(&bump))]);
        let context = Context {
            package: None,
            interfaces: Some(&interfaces),
        };
        let result = parse_and_canonicalize(&bump, input, context)
            .expect("expected successful canonicalization");
        insta::with_settings!({
            description => format!("Code:\n\n{}", input),
            omit_expression => true,
        }, {
            insta::assert_debug_snapshot!(result);
        });
    }

    #[test]
    fn let_destruct_list_pattern_binds_names() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            f w = let [a, b] = w in a
        "#
        );
    }

    #[test]
    fn let_destruct_local_ctor_pattern_binds_names() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            type Wrap 'a
                = Wrap 'a

            f w = let (Wrap x) = w in x
        "#
        );
    }

    #[test]
    fn binop_function_must_be_top_level() {
        assert_module_error_snapshot!(
            r#"
            module Main exposing ((|>))

            infix left 6 (|>) = missing
        "#
        );
    }

    // === Typed defs through parameterized aliases ===

    #[test]
    fn typed_def_through_parameterized_alias() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            type alias transform 'a = 'a -> 'a

            f : transform (List 'b)
            f x = x
        "#
        );
    }

    #[test]
    fn anonymous_record_constructor_argument_unsupported() {
        assert_module_error_snapshot!("module Main exposing (..)\n\ntype D = D int { x : int }\n");
    }

    #[test]
    fn little_type_open_export() {
        assert_interface_snapshot!(
            "module Main exposing (type option(..))\n\ntype option 'a = Some 'a | None\n"
        );
    }

    #[test]
    fn unknown_trait_in_context() {
        assert_module_error_snapshot!(
            "module Main exposing (..)\n\nid : Eq 'a => 'a -> 'a\nid x = x\n"
        );
    }

    #[test]
    fn keyword_children_retain_import_uses_and_local_dependencies() {
        let bump = Bump::new();
        let source = bump.alloc_str("module Main exposing (..)\nimport Builtin exposing (..)\nf message x = trace message (comptime (addInteger x x))\ncheck = assert True\nstop message = fail message\nlater message = todo message\nrecur x = comptime (recur x)\n");
        let parsed = nash_parse::Parser::new(&bump, source).module().unwrap();
        let interfaces = std::collections::BTreeMap::from([(
            "Builtin",
            nash_can::kinds::builtin_interface(&bump),
        )]);
        let result = canonicalize(
            &bump,
            Context {
                package: None,
                interfaces: Some(&interfaces),
            },
            &parsed,
        )
        .unwrap();
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        let mut decls = result.module.decls;
        let mut recursive = false;
        loop {
            match decls {
                nash_ast::Decls::Declare { next, .. } => decls = next,
                nash_ast::Decls::DeclareRec { next, .. } => {
                    recursive = true;
                    decls = next;
                }
                nash_ast::Decls::Empty => break,
            }
        }
        assert!(
            recursive,
            "a dependency through comptime still forms a recursive group"
        );
    }

    #[test]
    fn keyword_expressions_preserve_children_and_source_regions() {
        let bump = Bump::new();
        let module = parse_and_canonicalize(
            &bump,
            "module Main exposing (..)\nvalue message body = trace message (comptime body)\n",
            Context::default(),
        )
        .unwrap();
        let nash_ast::Decls::Declare { definition, .. } = module.decls else {
            panic!("definition")
        };
        let nash_ast::Def::Def { body, .. } = definition else {
            panic!("untyped")
        };
        let nash_ast::Expr::Trace {
            message,
            body: inner,
        } = body.value
        else {
            panic!("trace")
        };
        assert_eq!(body.region.start.column, 22);
        assert_eq!(message.region.start.column, 28);
        assert!(matches!(message.value, nash_ast::Expr::VarLocal("message")));
        let nash_ast::Expr::Comptime(value) = inner.value else {
            panic!("comptime")
        };
        assert!(matches!(value.value, nash_ast::Expr::VarLocal("body")));
        assert!(value.region.start.column > inner.region.start.column);
    }

    #[test]
    fn macro_call_unsupported() {
        assert_module_error_snapshot!("module Main exposing (..)\n\nvalue = json!(1)\n");
    }

    #[test]
    fn nested_right_operator_sections() {
        let input =
            "module Main exposing (..)\n\nimport Basics exposing (..)\n\nsection = (+ ((+ 1) 2))\n";
        let bump = Bump::new();
        let interfaces = BTreeMap::from([("Basics", basics_with_binops_interface(&bump))]);
        let result = parse_and_canonicalize(
            &bump,
            input,
            Context {
                package: None,
                interfaces: Some(&interfaces),
            },
        )
        .expect("expected successful canonicalization");
        insta::with_settings!({description => input, omit_expression => true}, {
            insta::assert_debug_snapshot!(result);
        });
    }

    #[test]
    fn nested_left_operator_sections() {
        let input =
            "module Main exposing (..)\n\nimport Basics exposing (..)\n\nsection = (((1 +) 2) +)\n";
        let bump = Bump::new();
        let interfaces = BTreeMap::from([("Basics", basics_with_binops_interface(&bump))]);
        let result = parse_and_canonicalize(
            &bump,
            input,
            Context {
                package: None,
                interfaces: Some(&interfaces),
            },
        )
        .expect("expected successful canonicalization");
        insta::with_settings!({description => input, omit_expression => true}, {
            insta::assert_debug_snapshot!(result);
        });
    }

    #[test]
    fn right_operator_section() {
        let input = "module Main exposing (..)\n\nimport Basics exposing (..)\n\nsection = (+ 5)\n";
        let bump = Bump::new();
        let interfaces = BTreeMap::from([("Basics", basics_with_binops_interface(&bump))]);
        let result = parse_and_canonicalize(
            &bump,
            input,
            Context {
                package: None,
                interfaces: Some(&interfaces),
            },
        )
        .expect("expected successful canonicalization");
        insta::with_settings!({description => input, omit_expression => true}, {
            insta::assert_debug_snapshot!(result);
        });
    }

    #[test]
    fn left_operator_section() {
        let input = "module Main exposing (..)\n\nimport Basics exposing (..)\n\nsection = (5 +)\n";
        let bump = Bump::new();
        let interfaces = BTreeMap::from([("Basics", basics_with_binops_interface(&bump))]);
        let result = parse_and_canonicalize(
            &bump,
            input,
            Context {
                package: None,
                interfaces: Some(&interfaces),
            },
        )
        .expect("expected successful canonicalization");
        insta::with_settings!({description => input, omit_expression => true}, {
            insta::assert_debug_snapshot!(result);
        });
    }

    #[test]
    fn attributes_unsupported() {
        assert_module_error_snapshot!("module Main exposing (..)\n\n@inline\nvalue = 1\n");
    }

    #[test]
    fn unknown_trait_in_impl() {
        assert_module_error_snapshot!(
            "module Main exposing (..)\n\nimpl Eq int where\n    eq a b = true\n"
        );
    }

    #[test]
    fn tests_block_unsupported() {
        assert_module_error_snapshot!(
            "module Main exposing (..)\n\ntests\n    test \"truth\" = do\n        assert True\n"
        );
    }
    #[test]
    fn builtin_types_resolve_qualified_without_import() {
        assert_module_snapshot!(
            "module Main exposing (..)\n\nidentity : Builtin.list Builtin.unit -> list unit\nidentity x = x\n"
        );
    }

    #[test]
    fn unit_syntax_is_named_builtin_type() {
        let bump = Bump::new();
        let result = parse_and_canonicalize(
            &bump,
            "module Main exposing (..)\n\nidentity : () -> unit\nidentity x = x\n",
            Context {
                package: None,
                interfaces: None,
            },
        )
        .expect("unit annotation canonicalizes");
        let (nash_ast::Decls::Declare { definition, .. }
        | nash_ast::Decls::DeclareRec { definition, .. }) = result.decls
        else {
            panic!("expected declaration group")
        };
        let nash_ast::Def::TypedDef { annotation, .. } = definition else {
            panic!("expected typed definition")
        };
        let nash_ast::Type::Lambda { from, to } = annotation.value else {
            panic!("expected function")
        };
        for typ in [from, to] {
            assert!(
                matches!(typ.value, nash_ast::Type::Named { reference, args } if reference.home == nash_ast::primitives::builtin_home() && reference.name == "unit" && args.is_empty())
            );
        }
    }
    #[test]
    fn labeled_ctor_construction_in_wire_order() {
        assert_module_snapshot!(
            "module Main exposing (..)\ntype packet = Packet { z : unit, a : unit }\nbuild first second = Packet { a = second, z = first }\n"
        );
    }

    #[test]
    fn labeled_ctor_missing_field_error() {
        assert_module_error_snapshot!(
            "module Main exposing (..)\ntype packet = Packet { z : unit, a : unit }\nmain = Packet { a = () }\n"
        );
    }

    #[test]
    fn labeled_ctor_extra_field_error() {
        assert_module_error_snapshot!(
            "module Main exposing (..)\ntype packet = Packet { z : unit }\nmain = Packet { z = (), a = () }\n"
        );
    }

    #[test]
    fn ctor_duplicate_label_error() {
        assert_module_error_snapshot!(
            "module Main exposing (..)\ntype packet = Packet { z : unit, z : unit }\n"
        );
    }

    #[test]
    fn labeled_ctor_pattern_in_wire_order() {
        assert_module_snapshot!(
            "module Main exposing (..)\ntype packet = Packet { z : unit, a : unit }\nget (Packet { a }) = a\n"
        );
    }

    #[test]
    fn labeled_ctor_pattern_unknown_field_error() {
        assert_module_error_snapshot!(
            "module Main exposing (..)\ntype packet = Packet { z : unit }\nget (Packet { a }) = a\n"
        );
    }

    #[test]
    fn labeled_ctor_pattern_duplicate_field_error() {
        assert_module_error_snapshot!(
            "module Main exposing (..)\ntype packet = Packet { z : unit }\nget (Packet { z, z }) = z\n"
        );
    }

    #[test]
    fn labeled_ctor_twins_require_equal_label_order() {
        assert_module_error_snapshot!(
            "module Main exposing (..)\ntype Box = Box { z : Int, a : Int }\ntype box = Box { a : int, z : int }\n"
        );
    }
}
