use super::*;
use nash_ast::primitives;
use std::collections::BTreeMap;

mod source;

fn fixture<'a>(arena: &'a Arena, source: &str) -> (nash_can::CanResult<'a>, SolvedTypes<'a>) {
    fixture_in(arena, source, None)
}

fn fixture_in<'a>(
    arena: &'a Arena,
    source: &str,
    package: Option<nash_ast::PackageName<'a>>,
) -> (nash_can::CanResult<'a>, SolvedTypes<'a>) {
    let bump = arena.as_bump();
    let source = bump.alloc_str(source);
    let parsed = nash_parse::Parser::new(bump, source).module().unwrap();
    let interfaces = BTreeMap::from([("Builtin", nash_can::kinds::builtin_interface(bump))]);
    let canonical = nash_can::canonicalize(
        bump,
        nash_can::Context {
            package,
            interfaces: Some(&interfaces),
        },
        &parsed,
    )
    .unwrap();
    let (_, solved) = nash_solve::run(
        bump,
        &mut nash_constrain::UnionFind::new(),
        &canonical.module,
        &canonical.tables,
    )
    .unwrap();
    (canonical, solved)
}

fn evaluate(name: &str, source: &str) -> crate::harness::Evaluated {
    core_eval(name, source)
}

fn compiled_output<'a>(arena: &'a Arena, core: &'a nash_ir::core::Core<'a>) -> String {
    let assembled = crate::program::assemble_core(arena, core).unwrap();
    format!(
        "--- core\n{}\n--- uplc\n{}",
        nash_ir::pretty::pretty(core),
        nash_plutus::pretty::program(assembled.program)
    )
}

#[test]
fn source_identity_and_strict_local_capture() {
    let evaluation = evaluate(
        "source_identity_and_strict_local_capture",
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        identity x = x
        main : unit
        main =
            let
                captured = ()
                helper x = identity captured
            in
            helper ()
    "#
        ),
    );
    assert_eq!(evaluation.result, "(con unit ())");
}

#[test]
fn source_little_constructor_patterns_and_tuples() {
    let evaluation = evaluate(
        "source_little_constructor_patterns_and_tuples",
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        type option 'a = None | Some 'a
        unwrap fallback opt =
            case opt of
                None -> fallback
                Some x -> x
        main : unit
        main = unwrap () (Some ())
    "#
        ),
    );
    assert_eq!(evaluation.result, "(con unit ())");
}

#[test]
fn native_list_root_requires_an_explicit_element_instance() {
    let arena = Arena::new();
    let source = "module Main exposing (..)\nimport Primitive exposing (..)\nimport Builtin exposing (..)\nmain = []\n";
    let (module, solved) = fixture(&arena, source);
    let build = Build::new([Input {
        module: &module.module,
        types: &solved,
        tables: &module.tables,
    }]);
    let root = QualifiedName {
        home: module.module.name,
        name: "main",
    };
    assert!(matches!(
        build.compile(&arena, root, None, TraceConfig::default()),
        Err(Error::RootTypeArguments { .. })
    ));
    let unit = arena.alloc(Located::at_zero(Type::Named {
        reference: QualifiedName {
            home: primitives::primitive_home(),
            name: "unit",
        },
        args: &[],
    }));
    let compiled = build
        .compile(&arena, root, Some(&[unit]), TraceConfig::default())
        .unwrap();
    insta::with_settings!({description => source, omit_expression => true}, {
        insta::assert_snapshot!(compiled_output(&arena, compiled.core));
    });
    assert!(
        crate::harness::eval_core(&arena, compiled.core)
            .result
            .contains("list unit")
    );
}

struct Unit<'a> {
    canonical: nash_can::CanResult<'a>,
    solved: SolvedTypes<'a>,
}
fn with_base(source: &str, check: impl FnOnce(&Arena, &Build<'_, '_>, QualifiedName<'_>)) {
    with_base_modules(source, &[], check);
}

fn with_base_modules(
    source: &str,
    extra_modules: &[&str],
    check: impl FnOnce(&Arena, &Build<'_, '_>, QualifiedName<'_>),
) {
    let modules: Vec<_> = [
        include_str!("../../../nash-driver/base/src/Literal.nash"),
        include_str!("../../../nash-driver/base/src/Eq.nash"),
    ]
    .into_iter()
    .chain(extra_modules.iter().copied())
    .map(|source| (source, Some(primitives::BASE)))
    .chain(std::iter::once((source, None)))
    .collect();
    let mut settings = insta::Settings::clone_current();
    settings.set_description(source);
    settings.set_omit_expression(true);
    let _guard = settings.bind_to_scope();
    let arena = Arena::new();
    let bump = arena.as_bump();
    let mut interfaces = BTreeMap::from([("Builtin", nash_can::kinds::builtin_interface(bump))]);
    let mut units = Vec::new();
    for (source, package) in modules {
        let source = bump.alloc_str(source);
        let parsed = nash_parse::Parser::new(bump, source).module().unwrap();
        let canonical = nash_can::canonicalize(
            bump,
            nash_can::Context {
                package,
                interfaces: Some(&interfaces),
            },
            &parsed,
        )
        .unwrap();
        let (annotations, solved) = nash_solve::run(
            bump,
            &mut nash_constrain::UnionFind::new(),
            &canonical.module,
            &canonical.tables,
        )
        .unwrap();
        interfaces.insert(
            canonical.module.name.name,
            nash_can::from_module(bump, &canonical.module, &annotations),
        );
        units.push(Unit { canonical, solved });
    }
    let root = QualifiedName {
        home: units.last().unwrap().canonical.module.name,
        name: "main",
    };
    let build = Build::new(units.iter().map(|u| Input {
        module: &u.canonical.module,
        types: &u.solved,
        tables: &u.canonical.tables,
    }));
    check(&arena, &build, root);
}
fn core_eval(name: &str, source: &str) -> crate::harness::Evaluated {
    let mut evaluated = None;
    with_base(source, |arena, build, root| {
        let compiled = build
            .compile(arena, root, None, TraceConfig::default())
            .unwrap();
        let core =
            crate::recursion::rewrite(&nash_ir::build::Builder::new(arena), compiled.core).unwrap();
        let result = crate::harness::eval_core(arena, core);
        insta::assert_snapshot!(
            name,
            format!("--- core\n{}\n{result}", nash_ir::pretty::pretty(core))
        );
        evaluated = Some(result);
    });
    evaluated.unwrap()
}

#[test]
fn native_literal_implementations_and_default_methods_execute() {
    let result = core_eval(
        "native_literal_implementations_and_default_methods_execute",
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        import Eq exposing (..)
        same : int -> int -> bool
        same = eq
        main = if neq (Builtin.addInteger 20 21) 42 then same 42 42 else False
    "#
        ),
    );
    assert_eq!(result.result, "(con bool True)");
}

#[test]
fn literal_patterns_use_the_selected_conversion_and_eq_body() {
    let result = core_eval(
        "literal_patterns_use_the_selected_conversion_and_eq_body",
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        import Literal exposing (..)
        import Eq exposing (..)
        type box = Box int
        impl FromInt box where
            fromInt n = Box (Builtin.addInteger n 1)
        impl Eq box where
            eq (Box a) (Box b) = Builtin.equalsInteger (Builtin.addInteger a 1) b
        boxed : box
        boxed = 40
        main : bool
        main =
            case boxed of
                41 -> True
                _ -> False
    "#
        ),
    );
    assert_eq!(result.result, "(con bool True)");
}

#[test]
fn constrained_functions_are_first_class_specializations() {
    let result = core_eval(
        "constrained_functions_are_first_class_specializations",
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        import Eq exposing (..)
        pass f x = f x
        main = pass eq () ()
    "#
        ),
    );
    assert_eq!(result.result, "(con bool True)");
}

#[test]
fn trait_free_polymorphic_recursion_reuses_the_opaque_body() {
    with_base(
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        type option 'a = None | Some 'a
        plain : 'a -> unit
        plain x = plain (Some x)
        main = plain ()
    "#
        ),
        |arena, build, root| {
            let compiled = build
                .compile(arena, root, None, TraceConfig::default())
                .unwrap();
            insta::assert_snapshot!(compiled_output(arena, compiled.core));
            assert_eq!(
                compiled
                    .specializations
                    .iter()
                    .filter(|s| s.name.text == "plain")
                    .count(),
                1
            );
            crate::program::assemble_core(arena, compiled.core).unwrap();
        },
    );
}

#[test]
fn aggregate_destructuring_preserves_generalized_components() {
    let result = core_eval(
        "aggregate_destructuring_preserves_generalized_components",
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        main : (unit, bool)
        main =
            let
                (identity, second) = (\x -> x, \x -> x)
            in
            (identity (), second True)
    "#
        ),
    );
    assert!(result.result.contains("con unit"));
    assert!(result.result.contains("con bool True"));
}

#[test]
fn local_recursive_closure_keeps_its_capture() {
    let result = core_eval(
        "local_recursive_closure_keeps_its_capture",
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        run captured =
            let
                loop : int -> int
                loop n = if Builtin.equalsInteger n 0 then captured else loop (Builtin.subtractInteger n 1)
            in
            loop 3
        main : int
        main = run 42
    "#
        ),
    );
    assert_eq!(result.result, "(con integer 42)");
}

#[test]
fn closed_local_comptime_includes_its_reachable_helper() {
    let result = core_eval(
        "closed_local_comptime_includes_its_reachable_helper",
        indoc::indoc!(
            r#"
        module Main exposing (..)
        main : unit
        main =
            let
                helper x = x
            in
            comptime (helper ())
    "#
        ),
    );
    assert_eq!(result.result, "(con unit ())");
}

#[test]
fn unused_polymorphic_bottom_is_still_strict() {
    let result = core_eval(
        "unused_polymorphic_bottom_is_still_strict",
        indoc::indoc!(
            r#"
        module Main exposing (..)
        main : unit
        main =
            let
                unused = fail
            in
            ()
    "#
        ),
    );
    assert!(result.result.starts_with("error:"));
}

#[test]
fn comptime_rejects_runtime_captures() {
    with_base(
        indoc::indoc!(
            r#"
        module Main exposing (..)
        main : unit -> unit
        main x =
            let
                helper y = x
            in
            comptime (helper ())
    "#
        ),
        |arena, build, root| {
            assert!(matches!(
                build.compile(arena, root, None, TraceConfig::default()),
                Err(Error::ComptimeAssembly(_))
            ));
        },
    );
}

#[test]
fn source_recursive_static_arguments_are_marked() {
    with_base(
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        repeat : int -> int -> int
        repeat item n = if Builtin.equalsInteger n 0 then item else repeat item (Builtin.subtractInteger n 1)
        main : int
        main = repeat 42 3
    "#
        ),
        |arena, build, root| {
            let compiled = build
                .compile(arena, root, None, TraceConfig::default())
                .unwrap();
            let pretty = nash_ir::pretty::pretty(compiled.core);
            assert!(pretty.contains("static [0]"), "{pretty}");
            insta::assert_snapshot!(
                "source_recursion_static",
                compiled_output(arena, compiled.core)
            );
            let result = crate::program::assemble_core(arena, compiled.core)
                .unwrap()
                .program
                .eval(arena);
            assert_eq!(
                nash_plutus::pretty::term(result.term.unwrap()),
                "(con integer 42)"
            );
        },
    );
}

#[test]
fn generic_impl_context_default_and_superclass_evidence_are_closed() {
    let result = core_eval(
        "generic_impl_context_default_and_superclass_evidence_are_closed",
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        import Eq exposing (..)
        trait Eq 'a => Same 'a where
            same : 'a -> 'a -> bool
            same x y = eq x y
        impl Same int where
        impl Eq 'a => Same (list 'a) where
        use : Same 'a => 'a -> 'a -> bool
        use x y = if same x y then neq x y else True
        main = use [1, 2] [1, 3]
    "#
        ),
    );
    assert_eq!(result.result, "(con bool True)");
}

#[test]
fn implementation_method_only_type_variables_match_by_type() {
    let result = core_eval(
        "implementation_method_only_type_variables_match_by_type",
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        trait Keep 'a where
            keep : 'a -> 'b -> 'b
        impl Keep (list 'b) where
            keep _ value = value
        main : bool
        main = keep [()] True
    "#
        ),
    );
    assert_eq!(result.result, "(con bool True)");
}

#[test]
fn record_wire_order_access_update_and_accessor_execute() {
    let result = core_eval(
        "record_wire_order_access_update_and_accessor_execute",
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        type alias person = { age : int, active : bool }
        read : person -> int
        read = .age
        original : person
        original = { active = True, age = 1 }
        main : int
        main = read { original | age = 42 }
    "#
        ),
    );
    assert_eq!(result.result, "(con integer 42)");
}

#[test]
fn big_record_and_labeled_constructor_keep_distinct_layouts() {
    let result = core_eval(
        "big_record_and_labeled_constructor_keep_distinct_layouts",
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        import Eq exposing (..)
        type alias Record = { owner : Bytes, count : Int }
        type Datum = Datum { owner : Bytes, count : Int }
        record : Record
        record = { owner = #"aa", count = 1 }
        datum : Datum
        datum = Datum { owner = #"bb", count = 2 }
        main = neq record.count datum.count
    "#
        ),
    );
    assert_eq!(result.result, "(con bool True)");
}

#[test]
fn all_trace_configs_keep_compiler_and_user_messages_independent() {
    with_base(
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        main : unit
        main = trace "hello" (assert False)
    "#
        ),
        |arena, build, root| {
            for compiler in [false, true] {
                for user in [TraceLevel::Silent, TraceLevel::Compact, TraceLevel::Verbose] {
                    let compiled = build
                        .compile(arena, root, None, TraceConfig { user, compiler })
                        .unwrap();
                    let core = crate::recursion::rewrite(
                        &nash_ir::build::Builder::new(arena),
                        compiled.core,
                    )
                    .unwrap();
                    let result = crate::harness::eval_core(arena, core);
                    insta::assert_snapshot!(
                        format!("trace_config_{user:?}_{compiler}"),
                        format!("--- core\n{}\n{result}", nash_ir::pretty::pretty(core))
                    );
                    assert!(result.result.starts_with("error:"));
                    match user {
                        TraceLevel::Silent => assert!(result.logs.is_empty()),
                        TraceLevel::Compact => assert!(
                            result.logs.len() == 2
                                && result.logs.iter().all(|s| s.starts_with("Main:"))
                        ),
                        TraceLevel::Verbose => {
                            assert_eq!(result.logs, ["hello", "assertion failed"])
                        }
                    }
                }
            }
        },
    );
}

#[test]
fn repeated_trace_strings_are_hoisted_once() {
    with_base(
        indoc::indoc!(
            r#"
        module Main exposing (..)
        main : (unit, unit)
        main = (trace "hello" (), trace "hello" ())
    "#
        ),
        |arena, build, root| {
            let compiled = build
                .compile(arena, root, None, TraceConfig::default())
                .unwrap();
            let core = nash_ir::pretty::pretty(compiled.core);
            assert_eq!(core.matches("\"hello\"").count(), 1, "{core}");
            insta::assert_snapshot!(
                "source_trace_hoisting",
                compiled_output(arena, compiled.core)
            );
        },
    );
}

#[test]
fn unused_overloaded_value_does_not_choose_an_arbitrary_instance() {
    let result = core_eval(
        "unused_overloaded_value_does_not_choose_an_arbitrary_instance",
        indoc::indoc!(
            r#"
        module Main exposing (..)
        main : unit
        main =
            let
                unused = trace "no instance" 42
            in
            ()
    "#
        ),
    );
    assert_eq!(result.result, "(con unit ())");
    assert!(result.logs.is_empty());
}

#[test]
fn comptime_evaluates_arithmetic_and_rejects_nonconstants() {
    let result = core_eval(
        "comptime_evaluates_arithmetic_and_rejects_nonconstants",
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        main : int
        main = comptime (Builtin.addInteger 20 22)
    "#
        ),
    );
    assert_eq!(result.result, "(con integer 42)");
    with_base(
        indoc::indoc!(
            r#"
        module Main exposing (..)
        main : unit -> unit
        main = comptime (\x -> x)
    "#
        ),
        |arena, build, root| {
            assert!(matches!(
                build.compile(arena, root, None, TraceConfig::default()),
                Err(Error::ComptimeAssembly(_))
            ));
        },
    );
}

#[test]
fn recursive_function_rhs_preserves_strict_captures_once() {
    let result = core_eval(
        "recursive_function_rhs_preserves_strict_captures_once",
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        loop : bool -> unit
        loop =
            let
                captured = trace "capture" ()
            in
            \flag -> if flag then captured else loop True
        main = loop False
    "#
        ),
    );
    assert_eq!(result.result, "(con unit ())");
    assert_eq!(result.logs, ["capture"]);
}

#[test]
fn growing_native_layouts_stop_at_an_explicit_resource_limit() {
    with_base(
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        grow : list 'a -> unit
        grow xs = grow [xs]
        main = grow [()]
    "#
        ),
        |arena, build, root| {
            assert!(matches!(
                build.compile(arena, root, None, TraceConfig::default()),
                Err(Error::SpecializationLimit)
            ));
        },
    );
}

#[test]
fn empty_lists_key_the_native_element_layout_and_erase_big_nominal_names() {
    with_base(
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        empty _ = []
        ints : unit -> list int
        ints = empty
        bigInts : unit -> list Int
        bigInts = empty
        bigBytes : unit -> list Bytes
        bigBytes = empty
        main = (ints (), bigInts (), bigBytes ())
    "#
        ),
        |arena, build, root| {
            let compiled = build
                .compile(arena, root, None, TraceConfig::default())
                .unwrap();
            insta::assert_snapshot!(compiled_output(arena, compiled.core));
            assert_eq!(
                compiled
                    .specializations
                    .iter()
                    .filter(|s| s.name.text == "empty")
                    .count(),
                2
            );
            let core =
                crate::recursion::rewrite(&nash_ir::build::Builder::new(arena), compiled.core)
                    .unwrap();
            let result = crate::harness::eval_core(arena, core);
            assert!(result.result.contains("list integer"), "{}", result.result);
            assert_eq!(result.result.matches("list data").count(), 2);
        },
    );
}

#[test]
fn source_trait_specialization_core() {
    with_base(
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        import Eq exposing (..)
        main = neq () ()
    "#
        ),
        |arena, build, root| {
            let compiled = build
                .compile(arena, root, None, TraceConfig::default())
                .unwrap();
            insta::assert_snapshot!(
                "source_trait_default_specialization",
                compiled_output(arena, compiled.core)
            );
        },
    );
}

#[test]
fn source_identity_accepts_a_function_and_overapplication() {
    let result = core_eval(
        "source_identity_accepts_a_function_and_overapplication",
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        identity value = value
        main : int
        main = identity Builtin.addInteger 20 22
    "#
        ),
    );
    assert_eq!(result.result, "(con integer 42)");
}

#[test]
fn transparent_alias_method_variables_are_matched_after_expansion() {
    let result = core_eval(
        "transparent_alias_method_variables_are_matched_after_expansion",
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        type alias identity 'b = 'b
        trait Keep 'a where
            keep : 'a -> identity 'b -> identity 'b
        impl Keep unit where
            keep _ x = x
        raw : unit
        raw = ()
        main : unit
        main = keep () raw
    "#
        ),
    );
    assert_eq!(result.result, "(con unit ())");
}

#[test]
fn method_context_follows_the_renamed_method_only_variable() {
    let result = core_eval(
        "method_context_follows_the_renamed_method_only_variable",
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        import Eq exposing (..)
        trait Keep 'a where
            keep : Eq 'b => 'a -> 'b -> bool
        impl Keep (list 'b) where
            keep _ x = eq x x
        main = keep [()] 42
    "#
        ),
    );
    assert_eq!(result.result, "(con bool True)");
}

#[test]
fn captured_generic_evidence_is_local_to_each_outer_specialization() {
    let result = core_eval(
        "captured_generic_evidence_is_local_to_each_outer_specialization",
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        import Eq exposing (..)
        outer : Eq 'a => 'a -> bool
        outer x =
            let
                same y = eq x y
            in
            same x
        main = (outer 42, outer ())
    "#
        ),
    );
    assert_eq!(result.result.matches("con bool True").count(), 2);
}

#[test]
fn polymorphic_constants_evaluate_once_per_requested_evidence() {
    let result = core_eval(
        "polymorphic_constants_evaluate_once_per_requested_evidence",
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        value = trace "instance" 42
        small : int
        small = value
        big : Int
        big = value
        main = (small, small, big)
    "#
        ),
    );
    assert_eq!(result.logs, ["instance", "instance"]);
}

#[test]
fn generalized_destructuring_evaluates_its_aggregate_once() {
    let result = core_eval(
        "generalized_destructuring_evaluates_its_aggregate_once",
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        main : (unit, bool)
        main =
            let
                (first, second) = trace "aggregate" (\x -> x, \x -> x)
            in
            (first (), second True)
    "#
        ),
    );
    assert_eq!(result.logs, ["aggregate"]);
}

#[test]
fn separate_lexical_helpers_with_the_same_name_do_not_collide() {
    let result = core_eval(
        "separate_lexical_helpers_with_the_same_name_do_not_collide",
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        first x =
            let
                helper y = x
            in
            helper ()
        second x =
            let
                helper y = y
            in
            helper x
        main : (bool, unit)
        main = (first True, second ())
    "#
        ),
    );
    assert!(result.result.contains("con bool True"));
    assert!(result.result.contains("con unit"));
}

#[test]
fn shared_identity_binders_erase_the_first_instance_type() {
    with_base(
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        ident x = x
        main : (int, bytes)
        main = (ident 42, ident #"aa")
    "#
        ),
        |arena, build, root| {
            let compiled = build
                .compile(arena, root, None, TraceConfig::default())
                .unwrap();
            insta::assert_snapshot!(compiled_output(arena, compiled.core));
            assert_eq!(
                compiled
                    .specializations
                    .iter()
                    .filter(|s| s.name.text == "ident")
                    .count(),
                1
            );
            let pretty = nash_ir::pretty::pretty(compiled.core);
            assert!(pretty.contains(": 'erased -> 'erased"), "{pretty}");
            assert!(
                pretty.contains("x#") && pretty.contains(": 'erased -> x#"),
                "{pretty}"
            );
            assert_eq!(compiled.root_type.to_string(), "(int, bytes)");
        },
    );
}

#[test]
fn higher_kinded_default_method_accepts_a_nominal_record_alias() {
    let result = core_eval(
        "higher_kinded_default_method_accepts_a_nominal_record_alias",
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        type alias box 'a = { value : 'a }
        trait Pass 'f where
            pass : 'f 'a -> 'f 'a
            pass x = x
            other : 'f 'a -> 'f 'a
        impl Pass box where
            other x = x
        boxed : box unit
        boxed = { value = () }
        main = (pass boxed).value
    "#
        ),
    );
    assert_eq!(result.result, "(con unit ())");
}

#[test]
fn source_data_encoding_preserves_nominal_constructors() {
    let result = core_eval(
        "source_data_encoding_preserves_nominal_constructors",
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        type First = First
        type Second = Second
        encodeFirst : First -> Data
        encodeFirst value =
            case value of
                First -> Builtin.constrData 0 []
        encodeSecond : Second -> Data
        encodeSecond value =
            case value of
                Second -> Builtin.constrData 1 []
        main = Builtin.equalsData (encodeFirst First) (encodeSecond Second)
    "#
        ),
    );
    assert_eq!(result.result, "(con bool False)");
}

#[test]
fn conditional_recursive_function_initializes_only_the_selected_branch_once() {
    let result = core_eval(
        "conditional_recursive_function_initializes_only_the_selected_branch_once",
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        loop : bool -> unit
        loop =
            if trace "condition" True then
                trace "selected" (\flag -> if flag then () else loop True)
            else
                trace "unselected" (\flag -> loop flag)
        main = loop False
    "#
        ),
    );
    assert_eq!(result.result, "(con unit ())");
    assert_eq!(result.logs, ["condition", "selected"]);
}

#[test]
fn case_recursive_function_retains_branch_captures() {
    let result = core_eval(
        "case_recursive_function_retains_branch_captures",
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        type choice = First unit | Second
        loop : bool -> unit
        loop =
            case trace "choose" (First ()) of
                First captured -> trace "selected" (\flag -> if flag then captured else loop True)
                Second -> \flag -> loop flag
        main = loop False
    "#
        ),
    );
    assert_eq!(result.result, "(con unit ())");
    assert_eq!(result.logs, ["choose", "selected"]);
}

#[test]
fn repeated_big_record_fields_share_the_list_decoder() {
    with_base(
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        type alias Record = { first : Int, second : Int }
        main : Record -> (Int, Int)
        main d = (d.first, d.second)
    "#
        ),
        |arena, build, root| {
            let compiled = build
                .compile(arena, root, None, TraceConfig::default())
                .unwrap();
            insta::assert_snapshot!(compiled_output(arena, compiled.core));
            let pretty = nash_ir::pretty::pretty(compiled.core);
            assert_eq!(pretty.matches("unListData").count(), 1, "{pretty}");
        },
    );
}

#[test]
fn big_constructor_pattern_and_access_share_the_constructor_decoder() {
    with_base(
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        type Datum = Datum { owner : Bytes, deadline : Int }
        main : Datum -> (Bytes, Int)
        main d = case d of Datum { owner } -> (owner, d.deadline)
    "#
        ),
        |arena, build, root| {
            let compiled = build
                .compile(arena, root, None, TraceConfig::default())
                .unwrap();
            insta::assert_snapshot!(compiled_output(arena, compiled.core));
            let pretty = nash_ir::pretty::pretty(compiled.core);
            assert_eq!(pretty.matches("unConstrData").count(), 1, "{pretty}");
            assert_eq!(pretty.matches("sndPair").count(), 0, "{pretty}");
            let assembled = crate::program::assemble_core(arena, compiled.core).unwrap();
            let uplc = nash_plutus::pretty::program(assembled.program);
            assert_eq!(uplc.matches("unConstrData").count(), 1, "{uplc}");
            assert_eq!(uplc.matches("sndPair").count(), 0, "{uplc}");
            assert_eq!(uplc.matches("fstPair").count(), 0, "{uplc}");
        },
    );
}

#[test]
fn accessor_sharing_retains_trace_before_a_malformed_record_failure() {
    with_base(
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        type alias Record = { first : Int, second : Int }
        main : Record -> (unit, Int)
        main d = (trace "before" (), d.first)
    "#
        ),
        |arena, build, root| {
            let compiled = build
                .compile(arena, root, None, TraceConfig::default())
                .unwrap();
            insta::assert_snapshot!(compiled_output(arena, compiled.core));
            let compiled = crate::program::assemble_core(arena, compiled.core).unwrap();
            let malformed = nash_plutus::data::PlutusData::integer_from(arena, 0);
            let result = compiled
                .program
                .apply(arena, nash_plutus::term::Term::data(arena, malformed))
                .eval(arena);
            assert!(result.term.is_err());
            assert_eq!(result.info.logs, ["before"]);
        },
    );
}

#[test]
fn accessor_sharing_keeps_unselected_branch_decoding_lazy() {
    with_base(
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        type alias Record = { first : Int, second : Int }
        main : Record -> unit
        main d = if False then (\x -> ()) d.first else ()
    "#
        ),
        |arena, build, root| {
            let compiled = build
                .compile(arena, root, None, TraceConfig::default())
                .unwrap();
            insta::assert_snapshot!(compiled_output(arena, compiled.core));
            let compiled = crate::program::assemble_core(arena, compiled.core).unwrap();
            let malformed = nash_plutus::data::PlutusData::integer_from(arena, 0);
            let result = compiled
                .program
                .apply(arena, nash_plutus::term::Term::data(arena, malformed))
                .eval(arena);
            assert!(result.term.is_ok(), "{:?}", result.term);
            assert!(result.info.logs.is_empty());
        },
    );
}

// Keep snapshots at the source boundary; direct Core fixtures retain focused
// assertions for representations that source compilation can normalize away.
macro_rules! source_codegen_snapshot {
    ($name:ident, $source:literal, $expected:literal) => {
        #[test]
        fn $name() {
            with_base(indoc::indoc!($source), |arena, build, root| {
                let compiled = build
                    .compile(arena, root, None, TraceConfig::default())
                    .unwrap();
                let rewritten =
                    crate::recursion::rewrite(&nash_ir::build::Builder::new(arena), compiled.core)
                        .unwrap();
                let evaluated = crate::harness::eval_core(arena, rewritten);
                assert_eq!(evaluated.result, $expected);
                insta::assert_snapshot!(
                    stringify!($name),
                    format!(
                        "--- core\n{}\n{evaluated}",
                        nash_ir::pretty::pretty(rewritten)
                    )
                );
            });
        }
    };
}

source_codegen_snapshot!(
    source_let_application,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin exposing (..)
    main : int
    main =
        let
            x = 1
        in
        (\y -> Builtin.addInteger x y) 2
"#,
    "(con integer 3)"
);

source_codegen_snapshot!(
    source_lazy_boolean_branch,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin exposing (..)
    main : int
    main = if True then 42 else fail
"#,
    "(con integer 42)"
);

source_codegen_snapshot!(
    source_reachable_binding_chain,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin exposing (..)
    x : int
    x = 40
    y : int
    y = Builtin.addInteger x 2
    unused : int
    unused = fail
    main : int
    main = y
"#,
    "(con integer 42)"
);

source_codegen_snapshot!(
    source_static_second_parameter,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin exposing (..)
    count : int -> int -> int
    count n step =
        if Builtin.equalsInteger n 0 then 0
        else Builtin.addInteger step (count (Builtin.subtractInteger n 1) step)
    main : int
    main = count 3 7
"#,
    "(con integer 21)"
);

source_codegen_snapshot!(
    source_shared_default_leaf,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin exposing (..)
    main : bool
    main =
        case (False, False) of
            (True, True) -> True
            _ -> trace "default" False
"#,
    "(con bool False)"
);

#[test]
fn source_trace_precedes_failure() {
    with_base(
        indoc::indoc!(
            r#"
        module Main exposing (..)
        main : unit
        main = trace "before failure" fail
    "#
        ),
        |arena, build, root| {
            let compiled = build
                .compile(arena, root, None, TraceConfig::default())
                .unwrap();
            let evaluated = crate::harness::eval_core(arena, compiled.core);
            assert!(evaluated.result.starts_with("error:"));
            assert_eq!(evaluated.logs, ["before failure"]);
            insta::assert_snapshot!(format!(
                "--- core\n{}\n{evaluated}",
                nash_ir::pretty::pretty(compiled.core)
            ));
        },
    );
}

#[test]
fn native_case_branches_evaluate_scrutinee_once_and_remain_lazy() {
    with_base(
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        choose : bool -> int
        choose flag =
            if trace "condition" flag then trace "true" 42
            else trace "false" 7
        main = (choose True, choose False)
        "#
        ),
        |arena, build, root| {
            let compiled = build
                .compile(arena, root, None, TraceConfig::default())
                .unwrap();
            let core =
                crate::recursion::rewrite(&nash_ir::build::Builder::new(arena), compiled.core)
                    .unwrap();
            let result = crate::harness::eval_core(arena, core);
            assert_eq!(result.logs, ["condition", "true", "condition", "false"]);
            assert!(result.uplc.contains("(case"));
            assert!(!result.uplc.contains("ifThenElse"));
            insta::assert_snapshot!(format!(
                "--- core\n{}\n{result}",
                nash_ir::pretty::pretty(core)
            ));
        },
    );
}

#[test]
fn native_case_dispatches_lists_data_and_sparse_literals() {
    with_base(
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        first : list int -> int
        first xs =
            case xs of
                [] -> Builtin.subtractInteger 0 1
                x :: _ -> x
        decode : Data -> int
        decode data =
            case data of
                I n -> n
                _ -> Builtin.subtractInteger 0 2
        select : int -> int
        select n =
            case n of
                7 -> 10
                100 -> 20
                _ -> 30
        main = (first [], first [42], decode (I 9), decode (B #""), select 7, select 100, select (Builtin.subtractInteger 0 7))
        "#
        ),
        |arena, build, root| {
            let compiled = build
                .compile(arena, root, None, TraceConfig::default())
                .unwrap();
            let core =
                crate::recursion::rewrite(&nash_ir::build::Builder::new(arena), compiled.core)
                    .unwrap();
            let result = crate::harness::eval_core(arena, core);
            assert!(!result.result.starts_with("error:"), "{}", result.result);
            assert!(result.uplc.contains("(case"));
            assert!(result.uplc.contains("chooseData"));
            assert!(!result.uplc.contains("ifThenElse"));
            assert!(!result.uplc.contains("chooseList"));
            insta::assert_snapshot!(format!(
                "--- core\n{}\n{result}",
                nash_ir::pretty::pretty(core)
            ));
        },
    );
}

#[test]
fn pair_wildcard_uses_native_case_without_projection_builtins() {
    with_base(
        indoc::indoc!(
            r#"
            module Main exposing (..)
            main : pair int (list Data) -> int
            main pair(tag, _) = tag
        "#
        ),
        |arena, build, root| {
            let compiled = build
                .compile(arena, root, None, TraceConfig::default())
                .unwrap();
            let program = crate::program::assemble_core(arena, compiled.core).unwrap();
            let uplc = nash_plutus::pretty::program(program.program);
            assert!(uplc.contains("(case"), "{uplc}");
            assert!(!uplc.contains("fstPair"), "{uplc}");
            assert!(!uplc.contains("sndPair"), "{uplc}");
            insta::assert_snapshot!(compiled_output(arena, compiled.core));
        },
    );
}

#[test]
fn consecutive_big_fields_reuse_previous_tails() {
    with_base(
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Primitive exposing (..)
        type alias Record = { a : Int, b : Int, c : Int, d : Int }
        select : Record -> (Int, Int, Int, Int, Int)
        select record = (record.a, record.b, record.c, record.d, record.c)
        main = select { a = 10, b = 20, c = 30, d = 40 }
        "#
        ),
        |arena, build, root| {
            let compiled = build
                .compile(arena, root, None, TraceConfig::default())
                .unwrap();
            let pretty = nash_ir::pretty::pretty(compiled.core);
            assert_eq!(pretty.matches("tailList").count(), 3, "{pretty}");
            assert!(!pretty.contains("dropList"), "{pretty}");
            let result = crate::harness::eval_core(arena, compiled.core);
            assert_eq!(
                result.result,
                "(constr 0\n  (con data (I 10))\n  (con data (I 20))\n  (con data (I 30))\n  (con data (I 40))\n  (con data (I 30)))"
            );
            insta::assert_snapshot!(format!("--- core\n{pretty}\n{result}"));
        },
    );
}

source_codegen_snapshot!(
    little_constructor_record_accesses_each_field,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin exposing (..)
    type entry = Entry { count : int, enabled : bool, label : bytes }
    main =
        let
            item = Entry { label = #"aabb", enabled = True, count = 42 }
        in
        if item.enabled then addInteger item.count (lengthOfByteString item.label) else 0
    "#,
    "(con integer 44)"
);

source_codegen_snapshot!(
    sparse_big_fields_drop_from_previous_tail,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin
    type alias Record = { a : Int, b : Int, c : Int, d : Int, e : Int, f : Int }
    select : Record -> int
    select r = Builtin.addInteger (Builtin.unIData r.b)
        (Builtin.addInteger (Builtin.unIData r.e) (Builtin.unIData r.f))
    main = select { a = 1, b = 2, c = 3, d = 4, e = 5, f = 6 }
    "#,
    "(con integer 13)"
);

source_codegen_snapshot!(
    reverse_big_fields_reuse_only_available_tails,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin
    type alias Record = { a : Int, b : Int, c : Int, d : Int }
    select : Record -> int
    select r = Builtin.addInteger (Builtin.unIData r.d)
        (Builtin.addInteger (Builtin.unIData r.b) (Builtin.unIData r.d))
    main = select { a = 1, b = 2, c = 3, d = 4 }
    "#,
    "(con integer 10)"
);

source_codegen_snapshot!(
    constructor_pattern_tail_reused_by_field_access,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin
    type Record = Record { a : Int, b : Int, c : Int, d : Int }
    select : Record -> int
    select r =
        case r of
            Record { b } -> Builtin.addInteger (Builtin.unIData b) (Builtin.unIData r.c)
    main = select (Record { a = 1, b = 2, c = 3, d = 4 })
    "#,
    "(con integer 5)"
);

source_codegen_snapshot!(
    explicit_drops_preserve_saturation_after_cached_tail,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin
    select : list int -> bool
    select xs =
        let
            first = Builtin.dropList 2 xs
            second = Builtin.dropList 3 xs
        in
        if Builtin.nullList first then Builtin.nullList second else False
    main = select [1]
    "#,
    "(con bool True)"
);

source_codegen_snapshot!(
    cached_drop_does_not_suppress_tail_failure,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin
    select : list int -> bool
    select xs =
        let
            dropped = Builtin.dropList 1 xs
        in
        if Builtin.nullList dropped then
            Builtin.nullList (Builtin.tailList xs)
        else False
    main = select []
    "#,
    "error: Runtime(EmptyList([]))"
);

source_codegen_snapshot!(
    big_record_update_first_shares_unchanged_suffix,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin
    type alias Record = { a : Int, b : Int, c : Int, d : Int }
    replace : Record -> Record
    replace r = { r | a = (trace "replace" 10) }
    total : Record -> int
    total r = Builtin.addInteger (Builtin.unIData r.a)
        (Builtin.addInteger (Builtin.unIData r.b)
            (Builtin.addInteger (Builtin.unIData r.c) (Builtin.unIData r.d)))
    main = total (replace { a = 1, b = 2, c = 3, d = 4 })
    "#,
    "(con integer 19)"
);

source_codegen_snapshot!(
    big_record_update_middle_shares_unchanged_suffix,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin
    type alias Record = { a : Int, b : Int, c : Int, d : Int }
    replace : Record -> Record
    replace r = { r | c = (trace "replace" 30) }
    total : Record -> int
    total r = Builtin.addInteger (Builtin.unIData r.a)
        (Builtin.addInteger (Builtin.unIData r.b)
            (Builtin.addInteger (Builtin.unIData r.c) (Builtin.unIData r.d)))
    main = total (replace { a = 1, b = 2, c = 3, d = 4 })
    "#,
    "(con integer 37)"
);

source_codegen_snapshot!(
    big_record_update_last_rebuilds_fields,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin
    type alias Record = { a : Int, b : Int, c : Int, d : Int }
    replace : Record -> Record
    replace r = { r | d = (trace "replace" 40) }
    total : Record -> int
    total r = Builtin.addInteger (Builtin.unIData r.a)
        (Builtin.addInteger (Builtin.unIData r.b)
            (Builtin.addInteger (Builtin.unIData r.c) (Builtin.unIData r.d)))
    main = total (replace { a = 1, b = 2, c = 3, d = 4 })
    "#,
    "(con integer 46)"
);

source_codegen_snapshot!(
    big_record_update_multiple_shares_unchanged_suffix,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin
    type alias Record = { a : Int, b : Int, c : Int, d : Int }
    replace : Record -> Record
    replace r = { r | c = (trace "third" 30), a = (trace "first" 10) }
    total : Record -> int
    total r = Builtin.addInteger (Builtin.unIData r.a)
        (Builtin.addInteger (Builtin.unIData r.b)
            (Builtin.addInteger (Builtin.unIData r.c) (Builtin.unIData r.d)))
    main = total (replace { a = 1, b = 2, c = 3, d = 4 })
    "#,
    "(con integer 46)"
);

source_codegen_snapshot!(
    big_record_update_all_rebuilds_fields,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin
    type alias Record = { a : Int, b : Int, c : Int, d : Int }
    replace : Record -> Record
    replace r = { r | a = 10, b = 20, c = 30, d = 40 }
    total : Record -> int
    total r = Builtin.addInteger (Builtin.unIData r.a)
        (Builtin.addInteger (Builtin.unIData r.b)
            (Builtin.addInteger (Builtin.unIData r.c) (Builtin.unIData r.d)))
    main = total (replace { a = 1, b = 2, c = 3, d = 4 })
    "#,
    "(con integer 100)"
);

source_codegen_snapshot!(
    big_record_update_all_does_not_decode_base,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin
    type alias Record = { a : Int, b : Int }
    replace : Record -> Record
    replace r = { r | a = (trace "first" 10), b = (trace "second" 20) }
    main = Builtin.unIData (replace (Primitive.coerce ())).b
    "#,
    "(con integer 20)"
);

source_codegen_snapshot!(
    big_record_update_preserves_opaque_suffix,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin
    type alias Record = { a : Int, b : Int }
    replace : Record -> Record
    replace r = { r | a = (trace "first" 10) }
    original : Record
    original = Primitive.coerce (Builtin.listData [Builtin.iData 1, Builtin.iData 2, Builtin.iData 3])
    main = Builtin.equalsData (Primitive.coerce (replace original))
        (Primitive.coerce (Builtin.listData [Builtin.iData 10, Builtin.iData 2, Builtin.iData 3]))
    "#,
    "(con bool True)"
);

source_codegen_snapshot!(
    big_record_update_in_unselected_branch_stays_lazy,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin
    type alias Record = { a : Int, b : Int }
    choose : bool -> Record -> int
    choose flag r =
        if flag then Builtin.unIData { r | a = (trace "unused" 10) }.b
        else 42
    main = choose False (Primitive.coerce ())
    "#,
    "(con integer 42)"
);

source_codegen_snapshot!(
    big_record_update_traces_before_missing_tail_failure,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin
    type alias Record = { a : Int, b : Int }
    empty : list Data
    empty = []
    original : Record
    original = Primitive.coerce (Builtin.listData empty)
    main : Record
    main = { original | a = (trace "before tail" 10) }
    "#,
    "error: Runtime(EmptyList([]))"
);

source_codegen_snapshot!(
    big_record_update_does_not_validate_unchanged_suffix,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin
    type alias Record = { a : Int, b : Int }
    original : Record
    original = Primitive.coerce (Builtin.listData [Builtin.iData 1])
    main = Builtin.equalsData
        (Primitive.coerce { original | a = 10 })
        (Primitive.coerce (Builtin.listData [Builtin.iData 10]))
    "#,
    "(con bool True)"
);

source_codegen_snapshot!(
    zero_drop_still_checks_its_list_argument,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin
    wrong : list int
    wrong = Primitive.coerce ()
    main : unit
    main =
        let
            xs = Builtin.dropList 0 wrong
        in
        ()
    "#,
    "error: Runtime(ExpectedList(Unit))"
);

source_codegen_snapshot!(
    record_layout_proves_adjacent_update_tails_without_field_reads,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin
    type alias Record = { a : Int, b : Int, c : Int, d : Int }
    update : Record -> int
    update r =
        let
            first = { r | a = 10 }
        in
        let
            second = { r | a = 10, b = 20 }
        in
        let
            third = { r | a = 10, b = 20, c = 30 }
        in
        Builtin.addInteger (Builtin.unIData first.a)
            (Builtin.addInteger (Builtin.unIData second.b) (Builtin.unIData third.c))
    main = update { a = 1, b = 2, c = 3, d = 4 }
    "#,
    "(con integer 60)"
);

source_codegen_snapshot!(
    record_layout_proves_access_after_cached_update_tail,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin
    type alias Record = { a : Int, b : Int, c : Int, d : Int }
    select : Record -> int
    select r =
        let
            updated = { r | a = 10 }
        in
        Builtin.addInteger (Builtin.unIData r.c) (Builtin.unIData updated.a)
    main = select { a = 1, b = 2, c = 3, d = 4 }
    "#,
    "(con integer 13)"
);
