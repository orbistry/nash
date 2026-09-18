//! Moved from `nash-can/src/pattern.rs` so the canonicalizer needs no dev-dependency on its reporter.
use nash_region::{Located, Region};
use nash_source::Pattern as SourcePattern;

mod tests {
    use super::*;
    use bumpalo::Bump;
    use nash_ast::{CtorOpts, ModuleName, Type as CanType, Union};
    use nash_can::DuplicatePatternContext;
    use nash_can::pattern::verify;

    use nash_can::environment::{Ctor, Env, Info};

    fn empty_env<'a>(_bump: &'a Bump) -> Env<'a> {
        Env {
            kinds: nash_can::kinds::KindEnv::from_interfaces(None),
            traits: Default::default(),
            q_traits: Default::default(),
            home: ModuleName {
                package: None,
                name: "Main",
            },
            vars: Default::default(),
            types: Default::default(),
            ctors: Default::default(),
            binops: Default::default(),
            q_vars: Default::default(),
            q_types: Default::default(),
            q_ctors: Default::default(),
        }
    }

    fn env_with_maybe<'a>(bump: &'a Bump) -> Env<'a> {
        let home = ModuleName {
            package: None,
            name: "Maybe",
        };
        let mut env = empty_env(bump);

        let nothing_ctor = bump.alloc(nash_ast::Ctor {
            labels: None,
            name: "Nothing",
            index: 1,
            arity: 0,
            arguments: &[],
        });
        let just_arg_typ = bump.alloc(Located::at(Region::zero(), CanType::Var("a")));
        let just_ctor = bump.alloc(nash_ast::Ctor {
            labels: None,
            name: "Just",
            index: 0,
            arity: 1,
            arguments: bump.alloc_slice_fill_iter([&*just_arg_typ]),
        });
        let maybe_union: &Union = bump.alloc(Union {
            kind: bump.alloc(nash_ast::Kind::Arrow(
                &nash_ast::Kind::Type,
                &nash_ast::Kind::Type,
            )),
            context: &[],
            name: bump.alloc(Located::at(Region::zero(), "Maybe")),
            parameters: bump.alloc_slice_fill_iter(["a"]),
            ctors: bump.alloc_slice_fill_iter([&*nothing_ctor, &*just_ctor]),
            alternatives: 2,
            options: CtorOpts::Normal,
        });

        // Nothing: arity 0
        let nothing = Ctor::Union {
            home,
            type_name: "Maybe",
            type_vars: &["a"],
            union: maybe_union,
            index: 1,
            arity: 0,
            arguments: &[],
            options: CtorOpts::Normal,
            alternatives: 2,
        };
        env.ctors.insert("Nothing", Info::Specific(home, nothing));

        // Just: arity 1
        let just = Ctor::Union {
            home,
            type_name: "Maybe",
            type_vars: &["a"],
            union: maybe_union,
            index: 0,
            arity: 1,
            arguments: bump.alloc_slice_fill_iter([&*just_arg_typ]),
            options: CtorOpts::Normal,
            alternatives: 2,
        };
        env.ctors.insert("Just", Info::Specific(home, just));

        env
    }

    fn env_with_record_ctor<'a>(bump: &'a Bump) -> Env<'a> {
        let home = ModuleName {
            package: None,
            name: "Main",
        };
        let mut env = empty_env(bump);

        let field_typ: &Located<CanType> =
            bump.alloc(Located::at(Region::zero(), CanType::Var("a")));
        let record_type: &Located<CanType> = bump.alloc(Located::at(
            Region::zero(),
            CanType::Record {
                fields: bump.alloc_slice_fill_iter([
                    nash_ast::FieldType {
                        index: 0,
                        field: "x",
                        typ: field_typ,
                    },
                    nash_ast::FieldType {
                        index: 1,
                        field: "y",
                        typ: field_typ,
                    },
                ]),
            },
        ));
        let ctor = match &record_type.value {
            CanType::Record { fields, .. } => nash_can::environment::make_record_ctor(
                bump,
                home,
                "Point",
                &[],
                record_type,
                fields,
            ),
            _ => unreachable!(),
        };
        env.ctors.insert("Point", Info::Specific(home, ctor));

        env
    }

    fn env_with_bool<'a>(bump: &'a Bump) -> Env<'a> {
        let module = nash_parse::Parser::new(
            bump,
            "module Main exposing (..)\nimport Builtin exposing (type bool(..))\n",
        )
        .module()
        .unwrap();
        let interfaces = std::collections::BTreeMap::from([(
            "Builtin",
            nash_can::kinds::builtin_interface(bump),
        )]);
        nash_can::environment::foreign::create_initial_env(
            bump,
            empty_env(bump).home,
            Some(&interfaces),
            module.imports,
        )
        .unwrap()
    }

    fn parse_pattern<'a>(bump: &'a Bump, input: &str) -> &'a Located<SourcePattern<'a>> {
        let src = bump.alloc_str(input);
        let mut parser = nash_parse::Parser::new(bump, src);
        let (pat, _end) = parser.pattern_expr().expect("expected successful parse");
        pat
    }

    macro_rules! assert_pattern_snapshot {
        ($input:expr, $env_fn:ident) => {{
            let bump = Bump::new();
            let env = $env_fn(&bump);
            let pat = parse_pattern(&bump, $input);
            let result = verify(&bump, &env, DuplicatePatternContext::CaseBranch, pat);
            insta::with_settings!({
                description => $input,
                omit_expression => true,
            }, {
                insta::assert_debug_snapshot!(result.unwrap());
            });
        }};
    }

    macro_rules! assert_pattern_error_snapshot {
        ($input:expr, $env_fn:ident) => {{
            let bump = Bump::new();
            let env = $env_fn(&bump);
            let pat = parse_pattern(&bump, $input);
            let result = verify(&bump, &env, DuplicatePatternContext::CaseBranch, pat);
            insta::with_settings!({info => &"diagnostic",
                description => $input,
                omit_expression => true,
            }, {
                insta::assert_snapshot!(crate::snapshot_support::errors($input, &result.unwrap_err()));
            });
        }};
    }

    // === Success tests ===

    #[test]
    fn wildcard() {
        assert_pattern_snapshot!("_", empty_env);
    }

    #[test]
    fn variable() {
        assert_pattern_snapshot!("x", empty_env);
    }

    #[test]
    fn record_pattern() {
        assert_pattern_snapshot!("{ x, y }", empty_env);
    }

    #[test]
    fn unit() {
        assert_pattern_snapshot!("()", empty_env);
    }

    #[test]
    fn tuple_two() {
        assert_pattern_snapshot!("( a, b )", empty_env);
    }

    #[test]
    fn tuple_three() {
        assert_pattern_snapshot!("( a, b, c )", empty_env);
    }

    #[test]
    fn literal_int() {
        assert_pattern_snapshot!("42", empty_env);
    }

    #[test]
    fn literal_str() {
        assert_pattern_snapshot!(r#""hello""#, empty_env);
    }

    #[test]
    fn list_pattern() {
        assert_pattern_snapshot!("[ a, b ]", empty_env);
    }

    #[test]
    fn cons_pattern() {
        assert_pattern_snapshot!("x :: xs", empty_env);
    }

    #[test]
    fn ctor_no_args() {
        assert_pattern_snapshot!("Nothing", env_with_maybe);
    }

    #[test]
    fn ctor_with_args() {
        assert_pattern_snapshot!("Just x", env_with_maybe);
    }

    #[test]
    fn bool_true_pattern() {
        assert_pattern_snapshot!("True", env_with_bool);
    }

    #[test]
    fn bool_false_pattern() {
        assert_pattern_snapshot!("False", env_with_bool);
    }

    #[test]
    fn alias_pattern() {
        assert_pattern_snapshot!("(x, y) as pair", empty_env);
    }

    // === Error tests ===

    #[test]
    fn tuple_four() {
        assert_pattern_snapshot!("( a, b, c, d )", empty_env);
    }

    #[test]
    fn ctor_wrong_arity() {
        assert_pattern_error_snapshot!("Just x y", env_with_maybe);
    }

    #[test]
    fn ctor_not_found() {
        assert_pattern_error_snapshot!("Foo", empty_env);
    }

    #[test]
    fn record_ctor_in_pattern() {
        assert_pattern_error_snapshot!("Point", env_with_record_ctor);
    }

    #[test]
    fn duplicate_vars() {
        assert_pattern_error_snapshot!("( x, x )", empty_env);
    }

    #[test]
    fn bool_pattern_with_args_is_bad_arity() {
        assert_pattern_error_snapshot!("True x", env_with_bool);
    }

    #[test]
    fn duplicate_across_sibling_patterns() {
        let bump = Bump::new();
        let input = "module Main exposing (..)\nf = \\x x -> x\n";
        let module = nash_parse::Parser::new(&bump, input).module().unwrap();
        let errors =
            nash_can::canonicalize(&bump, nash_can::Context::default(), &module).unwrap_err();
        insta::with_settings!({info => &"diagnostic", description => input, omit_expression => true}, {
            insta::assert_snapshot!(crate::snapshot_support::errors(input, &errors));
        });
    }
}
