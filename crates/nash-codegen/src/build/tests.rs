use super::*;
use nash_ast::primitives;
use std::collections::BTreeMap;

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

fn evaluate(source: &str) -> crate::harness::Evaluated {
    let arena = Arena::new();
    let (module, solved) = fixture(&arena, source);
    let build = Build::new([Input {
        module: &module.module,
        types: &solved,
        tables: &module.tables,
    }]);
    let compiled = build
        .compile(
            &arena,
            QualifiedName {
                home: module.module.name,
                name: "main",
            },
            None,
            TraceConfig::default(),
        )
        .unwrap();
    let core =
        crate::recursion::rewrite(&nash_ir::build::Builder::new(&arena), compiled.core).unwrap();
    crate::harness::eval_core(&arena, core)
}

#[test]
fn source_identity_and_strict_local_capture() {
    let evaluation = evaluate(indoc::indoc!(
        r#"
        module Main exposing (..)
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
    ));
    assert_eq!(evaluation.result, "(con unit ())");
}

#[test]
fn source_little_constructor_patterns_and_tuples() {
    let evaluation = evaluate(indoc::indoc!(
        r#"
        module Main exposing (..)
        import Builtin exposing (..)
        type option 'a = None | Some 'a
        unwrap fallback opt =
            case opt of
                None -> fallback
                Some x -> x
        main = unwrap () (Some ())
    "#
    ));
    assert_eq!(evaluation.result, "(con unit ())");
}

#[test]
fn native_list_root_requires_an_explicit_element_instance() {
    let arena = Arena::new();
    let (module, solved) = fixture(
        &arena,
        "module Main exposing (..)\nimport Builtin exposing (..)\nmain = []\n",
    );
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
            home: primitives::builtin_home(),
            name: "unit",
        },
        args: &[],
    }));
    let compiled = build
        .compile(&arena, root, Some(&[unit]), TraceConfig::default())
        .unwrap();
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
fn with_core(source: &str, check: impl FnOnce(&Arena, &Build<'_, '_>, QualifiedName<'_>)) {
    let mut settings = insta::Settings::clone_current();
    settings.set_description(
        [
            include_str!("../../../../core/src/Literal.nash"),
            include_str!("../../../../core/src/Eq.nash"),
            source,
        ]
        .join("\n"),
    );
    settings.set_omit_expression(true);
    let _guard = settings.bind_to_scope();
    let arena = Arena::new();
    let bump = arena.as_bump();
    let mut interfaces = BTreeMap::from([("Builtin", nash_can::kinds::builtin_interface(bump))]);
    let mut units = Vec::new();
    for (source, package) in [
        (
            include_str!("../../../../core/src/Literal.nash"),
            Some(primitives::CORE),
        ),
        (
            include_str!("../../../../core/src/Eq.nash"),
            Some(primitives::CORE),
        ),
        (source, None),
    ] {
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
fn core_eval(source: &str) -> crate::harness::Evaluated {
    let mut evaluated = None;
    with_core(source, |arena, build, root| {
        let compiled = build
            .compile(arena, root, None, TraceConfig::default())
            .unwrap();
        let core =
            crate::recursion::rewrite(&nash_ir::build::Builder::new(arena), compiled.core).unwrap();
        evaluated = Some(crate::harness::eval_core(arena, core));
    });
    evaluated.unwrap()
}

#[test]
fn native_literal_implementations_and_default_methods_execute() {
    let result = core_eval(indoc::indoc!(
        r#"
        module Main exposing (..)
        import Builtin exposing (..)
        import Eq exposing (..)
        same : int -> int -> bool
        same = eq
        main = if neq (Builtin.addInteger 20 21) 42 then same 42 42 else False
    "#
    ));
    assert_eq!(result.result, "(con bool True)");
}

#[test]
fn literal_patterns_use_the_selected_conversion_and_eq_body() {
    let result = core_eval(indoc::indoc!(
        r#"
        module Main exposing (..)
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
        main =
            case boxed of
                41 -> True
                _ -> False
    "#
    ));
    assert_eq!(result.result, "(con bool True)");
}

#[test]
fn constrained_functions_are_first_class_specializations() {
    let result = core_eval(indoc::indoc!(
        r#"
        module Main exposing (..)
        import Builtin exposing (..)
        import Eq exposing (..)
        pass f x = f x
        main = pass eq () ()
    "#
    ));
    assert_eq!(result.result, "(con bool True)");
}

#[test]
fn trait_free_polymorphic_recursion_reuses_the_opaque_body() {
    with_core(
        indoc::indoc!(
            r#"
        module Main exposing (..)
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
    let result = core_eval(indoc::indoc!(
        r#"
        module Main exposing (..)
        import Builtin exposing (..)
        main =
            let
                (identity, second) = (\x -> x, \x -> x)
            in
            (identity (), second True)
    "#
    ));
    assert!(result.result.contains("con unit"));
    assert!(result.result.contains("con bool True"));
}

#[test]
fn local_recursive_closure_keeps_its_capture() {
    let result = core_eval(indoc::indoc!(
        r#"
        module Main exposing (..)
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
    ));
    assert_eq!(result.result, "(con integer 42)");
}

#[test]
fn closed_local_comptime_includes_its_reachable_helper() {
    let result = core_eval(indoc::indoc!(
        r#"
        module Main exposing (..)
        main =
            let
                helper x = x
            in
            comptime (helper ())
    "#
    ));
    assert_eq!(result.result, "(con unit ())");
}

#[test]
fn unused_polymorphic_bottom_is_still_strict() {
    let result = core_eval(indoc::indoc!(
        r#"
        module Main exposing (..)
        main =
            let
                unused = fail
            in
            ()
    "#
    ));
    assert!(result.result.starts_with("error:"));
}

#[test]
fn comptime_rejects_runtime_captures() {
    with_core(
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
    with_core(
        indoc::indoc!(
            r#"
        module Main exposing (..)
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
            insta::assert_snapshot!("source_recursion_static", pretty);
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
    let result = core_eval(indoc::indoc!(
        r#"
        module Main exposing (..)
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
    ));
    assert_eq!(result.result, "(con bool True)");
}

#[test]
fn implementation_method_only_type_variables_match_by_type() {
    let result = core_eval(indoc::indoc!(
        r#"
        module Main exposing (..)
        import Builtin exposing (..)
        trait Keep 'a where
            keep : 'a -> 'b -> 'b
        impl Keep (list 'b) where
            keep _ value = value
        main = keep [()] True
    "#
    ));
    assert_eq!(result.result, "(con bool True)");
}

#[test]
fn record_wire_order_access_update_and_accessor_execute() {
    let result = core_eval(indoc::indoc!(
        r#"
        module Main exposing (..)
        import Builtin exposing (..)
        type alias person = { age : int, active : bool }
        read : person -> int
        read = .age
        original : person
        original = { active = True, age = 1 }
        main : int
        main = read { original | age = 42 }
    "#
    ));
    assert_eq!(result.result, "(con integer 42)");
}

#[test]
fn big_record_and_labeled_constructor_keep_distinct_layouts() {
    let result = core_eval(indoc::indoc!(
        r#"
        module Main exposing (..)
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
    ));
    assert_eq!(result.result, "(con bool True)");
}

#[test]
fn all_trace_configs_keep_compiler_and_user_messages_independent() {
    with_core(
        indoc::indoc!(
            r#"
        module Main exposing (..)
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
    with_core(
        indoc::indoc!(
            r#"
        module Main exposing (..)
        main = (trace "hello" (), trace "hello" ())
    "#
        ),
        |arena, build, root| {
            let compiled = build
                .compile(arena, root, None, TraceConfig::default())
                .unwrap();
            let core = nash_ir::pretty::pretty(compiled.core);
            assert_eq!(core.matches("\"hello\"").count(), 1, "{core}");
            insta::assert_snapshot!("source_trace_hoisting", core);
        },
    );
}

#[test]
fn unused_overloaded_value_does_not_choose_an_arbitrary_instance() {
    let result = core_eval(indoc::indoc!(
        r#"
        module Main exposing (..)
        main =
            let
                unused = trace "no instance" 42
            in
            ()
    "#
    ));
    assert_eq!(result.result, "(con unit ())");
    assert!(result.logs.is_empty());
}

#[test]
fn comptime_evaluates_arithmetic_and_rejects_nonconstants() {
    let result = core_eval(indoc::indoc!(
        r#"
        module Main exposing (..)
        import Builtin exposing (..)
        main : int
        main = comptime (Builtin.addInteger 20 22)
    "#
    ));
    assert_eq!(result.result, "(con integer 42)");
    with_core(
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
    let result = core_eval(indoc::indoc!(
        r#"
        module Main exposing (..)
        import Builtin exposing (..)
        loop : bool -> unit
        loop =
            let
                captured = trace "capture" ()
            in
            \flag -> if flag then captured else loop True
        main = loop False
    "#
    ));
    assert_eq!(result.result, "(con unit ())");
    assert_eq!(result.logs, ["capture"]);
}

#[test]
fn growing_native_layouts_stop_at_an_explicit_resource_limit() {
    with_core(
        indoc::indoc!(
            r#"
        module Main exposing (..)
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
    with_core(
        indoc::indoc!(
            r#"
        module Main exposing (..)
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
    with_core(
        indoc::indoc!(
            r#"
        module Main exposing (..)
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
                nash_ir::pretty::pretty(compiled.core)
            );
        },
    );
}

#[test]
fn builtin_identity_accepts_a_function_and_overapplication() {
    let result = core_eval(indoc::indoc!(
        r#"
        module Main exposing (..)
        import Builtin exposing (..)
        main : int
        main = Builtin.identity Builtin.addInteger 20 22
    "#
    ));
    assert_eq!(result.result, "(con integer 42)");
}

#[test]
fn transparent_alias_method_variables_are_matched_after_expansion() {
    let result = core_eval(indoc::indoc!(
        r#"
        module Main exposing (..)
        import Builtin exposing (..)
        type alias identity 'b = 'b
        trait Keep 'a where
            keep : 'a -> identity 'b -> identity 'b
        impl Keep unit where
            keep _ x = x
        main = keep () ()
    "#
    ));
    assert_eq!(result.result, "(con unit ())");
}

#[test]
fn method_context_follows_the_renamed_method_only_variable() {
    let result = core_eval(indoc::indoc!(
        r#"
        module Main exposing (..)
        import Builtin exposing (..)
        import Eq exposing (..)
        trait Keep 'a where
            keep : Eq 'b => 'a -> 'b -> bool
        impl Keep (list 'b) where
            keep _ x = eq x x
        main = keep [()] 42
    "#
    ));
    assert_eq!(result.result, "(con bool True)");
}

#[test]
fn captured_generic_evidence_is_local_to_each_outer_specialization() {
    let result = core_eval(indoc::indoc!(
        r#"
        module Main exposing (..)
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
    ));
    assert_eq!(result.result.matches("con bool True").count(), 2);
}

#[test]
fn polymorphic_constants_evaluate_once_per_requested_evidence() {
    let result = core_eval(indoc::indoc!(
        r#"
        module Main exposing (..)
        import Builtin exposing (..)
        value = trace "instance" 42
        small : int
        small = value
        big : Int
        big = value
        main = (small, small, big)
    "#
    ));
    assert_eq!(result.logs, ["instance", "instance"]);
}

#[test]
fn generalized_destructuring_evaluates_its_aggregate_once() {
    let result = core_eval(indoc::indoc!(
        r#"
        module Main exposing (..)
        import Builtin exposing (..)
        main =
            let
                (first, second) = trace "aggregate" (\x -> x, \x -> x)
            in
            (first (), second True)
    "#
    ));
    assert_eq!(result.logs, ["aggregate"]);
}

#[test]
fn separate_lexical_helpers_with_the_same_name_do_not_collide() {
    let result = core_eval(indoc::indoc!(
        r#"
        module Main exposing (..)
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
        main = (first True, second ())
    "#
    ));
    assert!(result.result.contains("con bool True"));
    assert!(result.result.contains("con unit"));
}

#[test]
fn shared_identity_binders_erase_the_first_instance_type() {
    with_core(
        indoc::indoc!(
            r#"
        module Main exposing (..)
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
    let result = core_eval(indoc::indoc!(
        r#"
        module Main exposing (..)
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
    ));
    assert_eq!(result.result, "(con unit ())");
}

#[test]
fn to_data_identity_shares_big_nominal_instances() {
    let arena = Arena::new();
    let (module, solved) = fixture_in(
        &arena,
        indoc::indoc!(
            r#"
        module Main exposing (..)
        import Builtin exposing (..)
        type First = First
        type Second = Second
        toData : Big 'a => 'a -> Data
        toData = Builtin.castToData
        main = (toData First, toData Second)
    "#
        ),
        Some(primitives::CORE),
    );
    let build = Build::new([Input {
        module: &module.module,
        types: &solved,
        tables: &module.tables,
    }]);
    let compiled = build
        .compile(
            &arena,
            QualifiedName {
                home: module.module.name,
                name: "main",
            },
            None,
            TraceConfig::default(),
        )
        .unwrap();
    assert_eq!(
        compiled
            .specializations
            .iter()
            .filter(|s| s.name.text == "toData")
            .count(),
        1
    );
    let pretty = nash_ir::pretty::pretty(compiled.core);
    assert!(pretty.contains(": Data -> Data"), "{pretty}");
    let core =
        crate::recursion::rewrite(&nash_ir::build::Builder::new(&arena), compiled.core).unwrap();
    assert_eq!(
        crate::harness::eval_core(&arena, core)
            .result
            .matches("con data")
            .count(),
        2
    );
}

#[test]
fn conditional_recursive_function_initializes_only_the_selected_branch_once() {
    let result = core_eval(indoc::indoc!(
        r#"
        module Main exposing (..)
        import Builtin exposing (..)
        loop : bool -> unit
        loop =
            if trace "condition" True then
                trace "selected" (\flag -> if flag then () else loop True)
            else
                trace "unselected" (\flag -> loop flag)
        main = loop False
    "#
    ));
    assert_eq!(result.result, "(con unit ())");
    assert_eq!(result.logs, ["condition", "selected"]);
}

#[test]
fn case_recursive_function_retains_branch_captures() {
    let result = core_eval(indoc::indoc!(
        r#"
        module Main exposing (..)
        import Builtin exposing (..)
        type choice = First unit | Second
        loop : bool -> unit
        loop =
            case trace "choose" (First ()) of
                First captured -> trace "selected" (\flag -> if flag then captured else loop True)
                Second -> \flag -> loop flag
        main = loop False
    "#
    ));
    assert_eq!(result.result, "(con unit ())");
    assert_eq!(result.logs, ["choose", "selected"]);
}

#[test]
fn repeated_big_record_fields_share_the_list_decoder() {
    with_core(
        indoc::indoc!(
            r#"
        module Main exposing (..)
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
            let pretty = nash_ir::pretty::pretty(compiled.core);
            assert_eq!(pretty.matches("unListData").count(), 1, "{pretty}");
        },
    );
}

#[test]
fn big_constructor_pattern_and_access_share_the_constructor_decoder() {
    with_core(
        indoc::indoc!(
            r#"
        module Main exposing (..)
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
            let pretty = nash_ir::pretty::pretty(compiled.core);
            assert_eq!(pretty.matches("unConstrData").count(), 0, "{pretty}");
            assert_eq!(pretty.matches("sndPair").count(), 0, "{pretty}");
            let assembled = crate::program::assemble_core(arena, compiled.core).unwrap();
            let uplc = nash_plutus::pretty::program(assembled.program);
            assert_eq!(uplc.matches("unConstrData").count(), 1, "{uplc}");
            assert_eq!(uplc.matches("sndPair").count(), 1, "{uplc}");
        },
    );
}

#[test]
fn accessor_sharing_retains_trace_before_a_malformed_record_failure() {
    with_core(
        indoc::indoc!(
            r#"
        module Main exposing (..)
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
    with_core(
        indoc::indoc!(
            r#"
        module Main exposing (..)
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
