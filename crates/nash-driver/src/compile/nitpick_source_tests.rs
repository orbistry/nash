use bumpalo::Bump;
use nash_nitpick::{Context, Error, check};

fn solve_source<'a>(
    bump: &'a Bump,
    source: &str,
    interfaces: &std::collections::BTreeMap<&'a str, nash_can::Interface<'a>>,
    package: Option<nash_ast::PackageName<'a>>,
) -> (&'a nash_ast::Module<'a>, nash_can::Annotations<'a>) {
    let source = bump.alloc_str(source);
    let parsed = nash_parse::Parser::new(bump, source)
        .module()
        .expect("source must parse");
    let can = nash_can::canonicalize(
        bump,
        nash_can::Context {
            package,
            interfaces: Some(interfaces),
        },
        &parsed,
    )
    .expect("source must canonicalize");
    let mut uf = nash_constrain::UnionFind::new();
    let module = &can.module;
    let (annotations, _) = nash_solve::run(bump, &mut uf, module, &can.tables)
        .expect("source must type check before nitpick");
    (bump.alloc(can.module), annotations)
}

fn run(source: &str) -> Result<(), Diagnostics> {
    run_modules(&[], source)
}

fn run_modules(providers: &[(&str, &str)], source: &str) -> Result<(), Diagnostics> {
    let bump = Bump::new();
    let mut interfaces = core_interfaces(&bump);
    for (name, provider) in providers {
        let (module, annotations) = solve_source(&bump, provider, &interfaces, None);
        check(&bump, module).expect("provider patterns must pass");
        interfaces.insert(name, nash_can::from_module(&bump, module, &annotations));
    }
    let (module, _) = solve_source(&bump, source, &interfaces, None);
    check(&bump, module).map_err(|errors| {
        let source_view = nash_report::Source::new(source);
        let summaries: Vec<_> = errors.iter().map(describe).collect();
        let reports = nash_report::to_reports(
            &source_view,
            "Main",
            &nash_report::ModuleError::Patterns(errors),
        );
        assert_eq!(reports.len(), summaries.len());
        Diagnostics {
            summaries,
            rendered: reports
                .iter()
                .map(|report| nash_report::render_plain(report, &source_view, "Main.nash"))
                .collect::<Vec<_>>()
                .join("\n"),
            description: source_description(providers, source),
        }
    })
}

#[derive(Debug)]
struct Diagnostics {
    summaries: Vec<String>,
    rendered: String,
    description: String,
}

fn source_description(providers: &[(&str, &str)], source: &str) -> String {
    [
        include_str!("fixtures/Eq.nash"),
        include_str!("fixtures/Literal.nash"),
        include_str!("fixtures/Monad.nash"),
    ]
    .into_iter()
    .chain(providers.iter().map(|(_, source)| *source))
    .chain([source])
    .collect::<Vec<_>>()
    .join("\n")
}

macro_rules! assert_diagnostics_snapshot {
    ($errors:expr) => {{
        let diagnostics = &$errors;
        assert!(!diagnostics.summaries.is_empty());
        insta::with_settings!({ description => &diagnostics.description, omit_expression => true }, {
            insta::assert_snapshot!(diagnostics.rendered);
        });
    }};
}

fn describe(error: &Error<'_>) -> String {
    match error {
        Error::Incomplete {
            region,
            context,
            unhandled,
        } => {
            let context = match context {
                Context::BadArg => "BadArg",
                Context::BadDestruct => "BadDestruct",
                Context::BadCase => "BadCase",
            };
            format!(
                "Incomplete {context} {}:{}-{}:{} {unhandled:?}",
                region.start.line, region.start.column, region.end.line, region.end.column
            )
        }
        Error::Redundant {
            case_region,
            pattern_region,
            index,
        } => format!(
            "Redundant #{index} {}:{}-{}:{} (case {}:{}-{}:{})",
            pattern_region.start.line,
            pattern_region.start.column,
            pattern_region.end.line,
            pattern_region.end.column,
            case_region.start.line,
            case_region.start.column,
            case_region.end.line,
            case_region.end.column
        ),
    }
}

macro_rules! assert_nitpick_snapshot {
    ($source:literal) => {{
        let source = indoc::indoc!($source);
        let value = run(source).expect("expected complete, useful patterns");
        insta::with_settings!({ description => source_description(&[], source), omit_expression => true }, { insta::assert_debug_snapshot!(value); });
    }};
}
macro_rules! assert_nitpick_error_snapshot {
    ($source:literal) => {{
        let source = indoc::indoc!($source);
        let errors = run(source).expect_err("expected incomplete or redundant patterns");
        assert_diagnostics_snapshot!(errors);
    }};
}

#[test]
fn case_bool_complete() {
    assert_nitpick_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f flag =
            case flag of
                True -> ()
                False -> ()
    "###
    );
}

#[test]
fn case_bool_missing() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f flag =
            case flag of
                True -> ()
    "###
    );
}

#[test]
fn wildcard_case() {
    assert_nitpick_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f x =
            case x of
                _ -> ()
    "###
    );
}

#[test]
fn redundant_after_wildcard() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f flag =
            case flag of
                _ -> ()
                True -> ()
    "###
    );
}

#[test]
fn redundant_after_all_constructors() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f flag =
            case flag of
                True -> ()
                False -> ()
                _ -> ()
    "###
    );
}

#[test]
fn redundancy_precedes_incompleteness() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f flag =
            case flag of
                True -> ()
                True -> ()
    "###
    );
}

#[test]
fn first_redundant_branch_only() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f flag =
            case flag of
                _ -> ()
                True -> ()
                False -> ()
    "###
    );
}

#[test]
fn untyped_arg_unsafe() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f (x :: _) = x
    "###
    );
}

#[test]
fn typed_arg_unsafe() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f : bool -> unit
        f True = ()
    "###
    );
}

#[test]
fn arguments_irrefutable() {
    assert_nitpick_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        type box 'a = Box 'a
        type alias point = { x : unit }
        f : point -> unit -> (unit, unit) -> box unit -> unit
        f { x } () (a, b) (Box c) = x
    "###
    );
}

#[test]
fn lambda_arg_unsafe() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f = \True -> ()
    "###
    );
}

#[test]
fn let_destructure_unsafe() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f xs =
            let
                (x :: rest) = xs
            in
            x
    "###
    );
}

#[test]
fn let_destructure_safe() {
    assert_nitpick_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f pair =
            let
                (x, y) = pair
            in
            x
    "###
    );
}

#[test]
fn nested_scrutinee_before_outer_case() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f flag =
            case (case flag of
                      True -> True) of
                True -> ()
    "###
    );
}

#[test]
fn branch_body_checked_after_redundancy() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f flag =
            case flag of
                _ -> ()
                True ->
                    case flag of
                        False -> ()
    "###
    );
}

#[test]
fn let_def_body_checked() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f flag =
            let
                g x =
                    case x of
                        True -> ()
            in
            g flag
    "###
    );
}

#[test]
fn recursive_definitions_checked() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f flag = g flag
        g True = f True
    "###
    );
}

#[test]
fn local_recursive_definitions_checked() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f flag =
            let
                a x = b x
                b True = a True
            in
            a flag
    "###
    );
}

#[test]
fn trait_default_checked() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        trait Choose 'a where
            choose : 'a -> bool -> unit
            choose _ flag =
                case flag of
                    True -> ()
    "###
    );
}

#[test]
fn impl_argument_checked() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        trait Choose 'a where
            choose : 'a -> unit
        impl Choose bool where
            choose True = ()
    "###
    );
}

#[test]
fn impl_body_checked() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        trait Choose 'a where
            choose : 'a -> unit
        impl Choose bool where
            choose flag =
                case flag of
                    True -> ()
    "###
    );
}

#[test]
fn trait_and_impl_complete() {
    assert_nitpick_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        trait Choose 'a where
            choose : 'a -> bool -> unit
            choose _ flag =
                case flag of
                    True -> ()
                    False -> ()
        impl Choose unit where
            choose _ flag =
                case flag of
                    False -> ()
                    True -> ()
    "###
    );
}

#[test]
fn list_complete() {
    assert_nitpick_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f xs =
            case xs of
                [] -> ()
                _ :: _ -> ()
    "###
    );
}

#[test]
fn nested_list_missing() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f xs =
            case xs of
                [] -> ()
                [_] -> ()
    "###
    );
}

#[test]
fn alias_pattern_preserves_hole() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f xs =
            case xs of
                (x :: rest) as all -> all
    "###
    );
}

#[test]
fn tuple_correlation_missing() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f pair =
            case pair of
                (True, _) -> ()
                (_, False) -> ()
    "###
    );
}

#[test]
fn triple_missing() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f triple =
            case triple of
                (True, (), False) -> ()
    "###
    );
}

#[test]
fn record_pattern_irrefutable() {
    assert_nitpick_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        type alias point = { x : unit }
        f : point -> unit
        f point =
            case point of
                { x } -> x
    "###
    );
}

#[test]
fn labeled_subset_irrefutable() {
    assert_nitpick_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        type packet = Packet { left : bool, right : unit }
        f packet =
            case packet of
                Packet { right } -> right
    "###
    );
}

#[test]
fn labeled_multi_constructor_missing() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        type packet = Packet { left : bool, right : unit } | Empty
        f packet =
            case packet of
                Packet { right, left } -> right
    "###
    );
}

#[test]
fn labeled_omitted_fields_cover_positional_patterns() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        type packet = Packet { left : bool, right : unit }
        f packet =
            case packet of
                Packet { right } -> right
                Packet True () -> ()
    "###
    );
}

#[test]
fn labeled_positional_field_order() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        type packet = Packet { z : bool, a : bool }
        f packet =
            case packet of
                Packet True False -> ()
                Packet False _ -> ()
    "###
    );
}

#[test]
fn bytes_need_wildcard() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f bytes =
            case bytes of
                #"00" -> ()
                #"ff" -> ()
    "###
    );
}

#[test]
fn bytes_duplicate() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f bytes =
            case bytes of
                #"00ff" -> ()
                #"00ff" -> ()
    "###
    );
}

#[test]
fn bytes_with_wildcard() {
    assert_nitpick_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f bytes =
            case bytes of
                #"00" -> ()
                _ -> ()
    "###
    );
}

#[test]
fn int_need_wildcard() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f n =
            case n of
                0 -> ()
                1 -> ()
    "###
    );
}

#[test]
fn string_need_wildcard() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f s =
            case s of
                "yes" -> ()
                "no" -> ()
    "###
    );
}

#[test]
fn data_missing_constructors() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f : Data -> unit
        f d =
            case d of
                Constr _ _ -> ()
                List _ -> ()
    "###
    );
}

#[test]
fn data_tag_literals_need_wildcard() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f : Data -> unit
        f d =
            case d of
                Constr 0 _ -> ()
                Constr 1 _ -> ()
                Map _ -> ()
                List _ -> ()
                I _ -> ()
                B _ -> ()
    "###
    );
}

#[test]
fn data_fields_list_complete() {
    assert_nitpick_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f : Data -> unit
        f d =
            case d of
                Constr _ [] -> ()
                Constr _ (_ :: _) -> ()
                _ -> ()
    "###
    );
}

#[test]
fn data_tag_redundant() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f : Data -> unit
        f d =
            case d of
                Constr _ _ -> ()
                Constr 0 _ -> ()
                _ -> ()
    "###
    );
}

#[test]
fn big_and_little_adt_coverage() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        type Redeemer = Claim | Cancel
        type step = Go | Stop
        f : Redeemer -> step -> unit
        f r s =
            case (r, s) of
                (Claim, Go) -> ()
                (Cancel, _) -> ()
    "###
    );
}

#[test]
fn transparent_alias_coverage() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        type step = Go | Stop
        type alias wrapper = step
        f : wrapper -> unit
        f value =
            case value of
                Go -> ()
    "###
    );
}

fn core_interfaces(bump: &Bump) -> std::collections::BTreeMap<&str, nash_can::Interface<'_>> {
    let mut interfaces =
        std::collections::BTreeMap::from([("Builtin", nash_can::kinds::builtin_interface(bump))]);
    for (name, source) in [
        ("Eq", include_str!("fixtures/Eq.nash")),
        ("Literal", include_str!("fixtures/Literal.nash")),
        ("Monad", include_str!("fixtures/Monad.nash")),
    ] {
        let (module, annotations) = solve_source(
            bump,
            source,
            &interfaces,
            Some(nash_ast::PackageName {
                author: "nash",
                project: "core",
            }),
        );
        check(bump, module).expect("core provider must pass nitpick");
        interfaces.insert(name, nash_can::from_module(bump, module, &annotations));
    }
    interfaces
}

#[test]
fn mixed_literal_and_constructor_patterns() {
    assert_nitpick_snapshot!(
        r#"
        module Main exposing (..)
        import Literal exposing (FromInt)
        type Token = A | B
        impl FromInt Token where
            fromInt _ = A
        f x =
            case x of
                A -> ()
                0 -> ()
                B -> ()
    "#
    );
}

#[test]
fn overloaded_literal_after_complete_constructors() {
    assert_nitpick_error_snapshot!(
        r#"
        module Main exposing (..)
        import Literal exposing (FromInt)
        type Token = A | B
        impl FromInt Token where
            fromInt _ = A
        f x =
            case x of
                A -> ()
                B -> ()
                0 -> ()
    "#
    );
}

#[test]
fn call_function_checked() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f flag =
            (case flag of
                 True -> (\x -> x)) ()
    "###
    );
}

#[test]
fn call_argument_checked() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        identity x = x
        f flag = identity (case flag of
                              True -> ())
    "###
    );
}

#[test]
fn if_condition_checked() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f flag =
            if (case flag of
                    True -> True) then () else ()
    "###
    );
}

#[test]
fn if_then_checked() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f flag =
            if flag then
                case flag of
                    True -> ()
            else ()
    "###
    );
}

#[test]
fn if_else_checked() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f flag =
            if flag then () else
                case flag of
                    True -> ()
    "###
    );
}

#[test]
fn list_entry_checked() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f flag =
            [(case flag of
                  True -> ())]
    "###
    );
}

#[test]
fn tuple_first_checked() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f flag =
            ((case flag of
                  True -> ()), ())
    "###
    );
}

#[test]
fn tuple_second_checked() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f flag =
            ((), (case flag of
                      True -> ()))
    "###
    );
}

#[test]
fn tuple_third_checked() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f flag =
            ((), (), (case flag of
                          True -> ()))
    "###
    );
}

#[test]
fn access_receiver_checked() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        type alias point = { x : unit }
        f flag =
            (case flag of
                 True -> point ()).x
    "###
    );
}

#[test]
fn record_fields_source_order() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        type alias point = { z : unit, a : unit }
        f flag =
            { a = (case flag of
                       True -> ())
            , z = (case flag of
                       False -> ())
            }
    "###
    );
}

#[test]
fn update_fields_source_order() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        type alias point = { z : unit, a : unit }
        f : point -> bool -> point
        f p flag =
            { p | a = (case flag of
                           True -> ())
                , z = (case flag of
                           False -> ())
            }
    "###
    );
}

#[test]
fn labeled_call_arguments_source_order() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        type packet = Packet { z : unit, a : unit }
        f flag =
            Packet { a = (case flag of
                              True -> ())
                   , z = (case flag of
                              False -> ())
                   }
    "###
    );
}

#[test]
fn declarations_source_order() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f True = g True
        g flag =
            case flag of
                False -> ()
    "###
    );
}

#[test]
fn method_roots_source_order() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f True = ()
        trait Choose 'a where
            choose : 'a -> unit
            choose _ =
                case False of
                    True -> ()
        g False = ()
        impl Choose bool where
            choose True = ()
    "###
    );
}

#[test]
fn let_definitions_source_order() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f flag =
            let
                a True = b False
                b False = ()
            in
            a flag
    "###
    );
}

#[test]
fn let_destructure_value_and_body_checked() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f flag =
            let
                True = (case flag of
                            True -> True)
            in
            case flag of
                False -> ()
    "###
    );
}

#[test]
fn multiple_arguments_before_body() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f True False =
            case True of
                False -> ()
    "###
    );
}

#[test]
fn nested_branch_body_checked() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f flag other =
            case flag of
                True ->
                    case other of
                        True -> ()
                False -> ()
    "###
    );
}

#[test]
fn data_fields_list_missing_nonempty() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        f : Data -> unit
        f d =
            case d of
                Constr _ [] -> ()
                Map _ -> ()
                List _ -> ()
                I _ -> ()
                B _ -> ()
    "###
    );
}

#[test]
fn big_labeled_constructor_fields() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        type Packet = Packet { z : Int, a : Bytes } | Empty
        f value =
            case value of
                Empty -> ()
    "###
    );
}

#[test]
fn mixed_literal_does_not_prove_constructor_coverage() {
    assert_nitpick_error_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        type Token = A | B
        impl FromInt Token where
            fromInt _ = B
        f x =
            case x of
                A -> ()
                0 -> ()
    "###
    );
}

#[test]
fn literal_before_constructors_is_opaque() {
    assert_nitpick_snapshot!(
        r###"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        import Eq exposing (Eq)
        import Literal exposing (FromInt, FromString, FromBytes)
        type Token = A | B
        impl FromInt Token where
            fromInt _ = A
        f x =
            case x of
                0 -> ()
                A -> ()
                B -> ()
    "###
    );
}

#[test]
fn binop_operands_checked() {
    let errors = run_modules(
        &[(
            "Ops",
            "module Ops exposing ((<+>))\ninfix left 4 (<+>) = keep\nkeep a b = a\n",
        )],
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Builtin exposing (type bool(..))
        import Ops exposing ((<+>))
        f flag =
            (case flag of
                 True -> ()) <+> (case flag of
                                      False -> ())
    "#
        ),
    )
    .expect_err("both operator operands need checking");
    assert_diagnostics_snapshot!(errors);
}

#[test]
fn imported_qualified_union_missing_constructor() {
    let errors = run_modules(
        &[(
            "Shapes",
            "module Shapes exposing (type choice(..))\ntype choice = Choice unit | Other\n",
        )],
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Shapes as S
        f x =
            case x of
                S.Choice () -> ()
    "#
        ),
    )
    .expect_err("imported constructor coverage must retain Other");
    assert_diagnostics_snapshot!(errors);
}

#[test]
fn imported_labeled_constructor_subset() {
    let providers = &[(
        "Shapes",
        "module Shapes exposing (type choice(..))\ntype choice = Choice { left : bool, right : unit } | Other\n",
    )];
    let source = indoc::indoc!(
        r#"
        module Main exposing (..)
        import Shapes as S
        f x =
            case x of
                S.Choice { right } -> right
                S.Other -> ()
    "#
    );
    run_modules(providers, source).expect("omitted imported labels are wildcards");
    insta::with_settings!({ description => source_description(providers, source), omit_expression => true }, { insta::assert_debug_snapshot!(()); });
}

#[test]
fn same_constructor_names_in_distinct_imported_types() {
    let providers = &[
        (
            "A",
            "module A exposing (type choice(..))\ntype choice = Yes | No\n",
        ),
        (
            "B",
            "module B exposing (Choice(..))\ntype Choice = Yes | No\n",
        ),
    ];
    let source = indoc::indoc!(
        r#"
        module Main exposing (..)
        import A
        import B
        f : A.choice -> B.Choice -> unit
        f a b =
            case (a, b) of
                (A.Yes, B.Yes) -> ()
                (A.Yes, B.No) -> ()
                (A.No, B.Yes) -> ()
                (A.No, B.No) -> ()
    "#
    );
    run_modules(providers, source).expect("different columns keep separate union identities");
    insta::with_settings!({ description => source_description(providers, source), omit_expression => true }, { insta::assert_debug_snapshot!(()); });
}

#[test]
fn do_binding_rhs_before_continuation() {
    let errors = run(indoc::indoc!(
        r#"
        module Main exposing (..)
        import Builtin exposing (type bool(..))
        import Monad exposing (Monad)
        f flag value =
            do
                x <- (case flag of
                          True -> value)
                case flag of
                    False -> value
    "#
    ))
    .expect_err("both cases are incomplete");
    assert!(
        errors.summaries[0].contains("BadCase 6:"),
        "RHS must come first: {errors:?}"
    );
    assert_diagnostics_snapshot!(errors);
}

#[test]
fn trait_default_argument_checked() {
    assert_nitpick_error_snapshot!(
        r#"
        module Main exposing (..)
        import Builtin exposing (type bool(..))
        trait Choose 'a where
            choose : bool -> 'a -> unit
            choose True _ = ()
    "#
    );
}

#[test]
fn mixed_let_bindings_source_order() {
    assert_nitpick_error_snapshot!(
        r#"
        module Main exposing (..)
        import Builtin exposing (type bool(..))
        f flag =
            let
                a True = if later then () else ()
                True = flag
                later = (case flag of
                             False -> True)
            in
            a flag
    "#
    );
}

#[test]
fn string_escapes_round_trip() {
    let bump = Bump::new();
    let original = "a\"\\\n\r\t\0\u{1b}λ";
    let rendered = nash_nitpick::render::pattern_to_string(
        nash_nitpick::render::RenderContext::Unambiguous,
        nash_nitpick::Pattern::Literal(nash_nitpick::Literal::Str(original)),
    );
    let source = bump.alloc_str(&format!("module Main exposing (..)\nf {rendered} = ()\n"));
    let parsed = nash_parse::Parser::new(&bump, source).module().unwrap();
    let can = nash_can::canonicalize(&bump, nash_can::Context::default(), &parsed).unwrap();
    let nash_ast::Decls::Declare {
        definition: nash_ast::Def::Def { args, .. },
        ..
    } = can.module.decls
    else {
        panic!("expected definition")
    };
    let nash_ast::Pattern::Str(value) = args[0].value else {
        panic!("expected string pattern")
    };
    assert_eq!(value, original);
    insta::with_settings!({ description => &*source, omit_expression => true }, { insta::assert_snapshot!(rendered); });
}

#[test]
fn keyword_children_keep_pattern_coverage_checks() {
    for wrapper in [
        "assert (case x of True -> True)",
        "comptime (case x of True -> ())",
        "fail (case x of True -> \"stop\")",
        "todo (case x of True -> \"later\")",
        "trace (case x of True -> \"message\") ()",
        "trace \"message\" (case x of True -> ())",
    ] {
        let source = format!(
            "module Main exposing (..)\nimport Builtin exposing (type bool(..))\nf x = {wrapper}\n"
        );
        let errors = run(&source).expect_err("keyword child contains an incomplete match");
        assert_eq!(errors.summaries.len(), 1, "{wrapper}: {errors:?}");
        assert!(
            errors.summaries[0].contains("Incomplete BadCase"),
            "{wrapper}: {errors:?}"
        );
        assert_diagnostics_snapshot!(errors);
    }
}
