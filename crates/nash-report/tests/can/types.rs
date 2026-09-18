//! Moved from `nash-can/src/types.rs` so the canonicalizer needs no dev-dependency on its reporter.
use bumpalo::Bump;
use nash_ast::Type as CanType;
use nash_region::{Located, Region};
use nash_source::Type as SourceType;

mod tests {
    use super::*;
    use bumpalo::Bump;
    use nash_ast::ModuleName;
    use nash_can::types::{canonicalize_type, to_annotation};

    use nash_can::environment::{Env, Info, Type as EnvType};

    fn empty_env<'a>(bump: &'a Bump) -> Env<'a> {
        let _ = bump;
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

    fn env_with_int<'a>(bump: &'a Bump) -> Env<'a> {
        let home = ModuleName {
            package: None,
            name: "Basics",
        };
        let mut env = empty_env(bump);
        env.types.insert(
            "Int",
            Info::Specific(home, EnvType::Union { arity: 0, home }),
        );
        env
    }

    fn env_with_list_and_int<'a>(bump: &'a Bump) -> Env<'a> {
        let basics = ModuleName {
            package: None,
            name: "Basics",
        };
        let list_mod = nash_ast::primitives::builtin_home();
        let mut env = empty_env(bump);
        env.types.insert(
            "Int",
            Info::Specific(
                basics,
                EnvType::Union {
                    arity: 0,
                    home: basics,
                },
            ),
        );
        env.types.insert(
            "List",
            Info::Specific(
                list_mod,
                EnvType::Union {
                    arity: 1,
                    home: list_mod,
                },
            ),
        );
        env
    }

    fn env_with_maybe_alias<'a>(bump: &'a Bump) -> Env<'a> {
        let home = ModuleName {
            package: None,
            name: "Maybe",
        };
        let mut env = empty_env(bump);
        let alias_type = bump.alloc(Located::at(Region::zero(), CanType::Var("a")));
        env.types.insert(
            "Maybe",
            Info::Specific(
                home,
                EnvType::Alias {
                    arity: 1,
                    home,
                    parameters: bump.alloc_slice_fill_iter(["a"]),
                    typ: alias_type,
                },
            ),
        );
        env
    }

    fn parse_type<'a>(bump: &'a Bump, input: &str) -> &'a Located<SourceType<'a>> {
        let src = bump.alloc_str(input);
        let mut parser = nash_parse::Parser::new(bump, src);
        let (typ, _end) = parser.type_expr().expect("expected successful parse");
        typ
    }

    macro_rules! assert_type_snapshot {
        ($input:expr, $env_fn:ident) => {{
            let bump = Bump::new();
            let env = $env_fn(&bump);
            let typ = parse_type(&bump, $input);
            let result = canonicalize_type(&bump, &env, typ);
            insta::with_settings!({
                description => $input,
                omit_expression => true,
            }, {
                insta::assert_debug_snapshot!(result.unwrap());
            });
        }};
    }

    macro_rules! assert_type_error_snapshot {
        ($input:expr, $env_fn:ident) => {{
            let bump = Bump::new();
            let env = $env_fn(&bump);
            let typ = parse_type(&bump, $input);
            let result = canonicalize_type(&bump, &env, typ);
            insta::with_settings!({info => &"diagnostic",
                description => $input,
                omit_expression => true,
            }, {
                insta::assert_snapshot!(crate::snapshot_support::errors($input, &result.unwrap_err()));
            });
        }};
    }

    macro_rules! assert_annotation_snapshot {
        ($input:expr, $env_fn:ident) => {{
            let bump = Bump::new();
            let env = $env_fn(&bump);
            let typ = parse_type(&bump, $input);
            let annotation = bump.alloc(nash_source::Annotation { constraints: &[], typ });
            let result = to_annotation(&bump, &env, annotation);
            insta::with_settings!({
                description => $input,
                omit_expression => true,
            }, {
                insta::assert_debug_snapshot!(result.unwrap());
            });
        }};
    }

    #[test]
    fn annotation_simple_var() {
        assert_annotation_snapshot!("'a", empty_env);
    }

    #[test]
    fn annotation_function() {
        assert_annotation_snapshot!("'a -> 'b -> 'a", empty_env);
    }

    #[test]
    fn annotation_no_free_vars() {
        assert_annotation_snapshot!("Int", env_with_int);
    }

    #[test]
    fn annotation_mixed() {
        assert_annotation_snapshot!("'a -> List 'a", env_with_list_and_int);
    }

    #[test]
    fn type_tuple_three() {
        assert_type_snapshot!("( 'a, 'b, 'c )", empty_env);
    }

    #[test]
    fn type_tuple_four() {
        assert_type_snapshot!("( 'a, 'b, 'c, 'd )", empty_env);
    }

    #[test]
    fn type_alias_expansion() {
        assert_type_snapshot!("Maybe 'a", env_with_maybe_alias);
    }

    #[test]
    fn type_union_reference() {
        assert_type_snapshot!("List 'a", env_with_list_and_int);
    }

    #[test]
    fn type_var_application() {
        assert_type_snapshot!("'f 'a", empty_env);
    }

    #[test]
    fn anonymous_record_type_errors() {
        assert_type_error_snapshot!("{ z : Int, a : Int }", env_with_int);
    }

    #[test]
    fn bad_args_reported_before_arity() {
        assert_type_error_snapshot!("Maybe Bogus Other", env_with_maybe_alias);
    }
}

mod context_tests {
    use super::*;
    use crate::snapshot_support::SnapshotInputs;
    use nash_ast::{Kind, ModuleName};
    use nash_can::environment::TraitInfo;
    use nash_can::environment::{self, Env, Info};
    use nash_can::types::to_annotation;

    fn source_annotation<'a>(
        bump: &'a Bump,
        annotation: &str,
    ) -> (&'a str, &'a nash_source::Annotation<'a>) {
        let source = bump.alloc_str(&format!(
            "module Main exposing (..)\n\nf : {annotation}\nf x = x\n"
        ));
        let mut parser = nash_parse::Parser::new(bump, source);
        (
            source,
            parser.module().unwrap().values[0].value.annotation.unwrap(),
        )
    }

    fn trait_env(bump: &Bump) -> Env<'_> {
        let mut env = environment::foreign::create_initial_env(
            bump,
            ModuleName {
                package: None,
                name: "Main",
            },
            None,
            &[],
        )
        .unwrap();
        let info = bump.alloc(TraitInfo {
            home: ModuleName {
                package: None,
                name: "Equality",
            },
            name: "Eq",
            parameters: &["a"],
            kinds: &[&Kind::Type],
            supers: &[],
            methods: &[],
        });
        env.traits.insert("Eq", Info::Specific(info.home, info));
        env.q_traits
            .entry("Equality")
            .or_default()
            .insert("Eq", Info::Specific(info.home, info));
        env
    }

    #[test]
    fn qualified_annotation_context() {
        let snapshot_inputs = SnapshotInputs::default();
        let bump = Bump::new();
        let env = trait_env(&bump);
        let (source, annotation) =
            source_annotation(&bump, snapshot_inputs.record("Equality.Eq 'a => 'a -> 'a"));
        let result = to_annotation(&bump, &env, annotation).unwrap();
        insta::with_settings!({description => source, omit_expression => true}, {
            insta::assert_debug_snapshot!(result);
        });
    }

    #[test]
    fn context_variable_absent_from_type() {
        let snapshot_inputs = SnapshotInputs::default();
        let bump = Bump::new();
        let env = trait_env(&bump);
        let (source, annotation) =
            source_annotation(&bump, snapshot_inputs.record("Eq 'b => 'a -> 'a"));
        insta::with_settings!({info => &"diagnostic", description => source, omit_expression => true}, {
            insta::assert_snapshot!(crate::snapshot_support::errors(source, &to_annotation(&bump, &env, annotation).unwrap_err()));
        });
    }

    #[test]
    fn context_trait_arity() {
        let snapshot_inputs = SnapshotInputs::default();
        let bump = Bump::new();
        let env = trait_env(&bump);
        let (source, annotation) =
            source_annotation(&bump, snapshot_inputs.record("Eq 'a 'b => 'a -> 'b"));
        insta::with_settings!({info => &"diagnostic", description => source, omit_expression => true}, {
            insta::assert_snapshot!(crate::snapshot_support::errors(source, &to_annotation(&bump, &env, annotation).unwrap_err()));
        });
    }
}
