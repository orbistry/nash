//! Source-to-optimized snapshots reuse the normal Base fixture compiler.
use super::*;
use nash_config::OptimizationLevel;
use nash_ir::{anf, build::Builder, hygiene, pretty::pretty};
use nash_plutus::{builtin::DefaultFunction as F, term::Term};

macro_rules! boolean_case_snapshot {
    ($name:ident, $source:literal) => {
        #[test]
        fn $name() {
            let source = indoc::indoc!($source);
            with_base(source, |arena, build, root| {
                let compiled = build.compile(arena, root, None, TraceConfig::default()).expect("source compiles to Core");
                let b = Builder::new(arena);
                let after = crate::snapshot_optimizer::optimize(arena, compiled.core);
                // O0 includes only the recursion encoding required by lowering.
                let baseline_core = crate::recursion::rewrite(&b, compiled.core).unwrap();
                let baseline = crate::harness::eval_core_raw(arena, baseline_core);
                let optimized_core = crate::recursion::rewrite(&b, after).unwrap();
                let term = crate::lower::lower_optimized(arena, optimized_core).unwrap();
                let optimized = crate::harness::eval_named(arena, term);
                assert!(!optimized.result.starts_with("error:"), "{}", optimized.result);

                insta::with_settings!({description => source, omit_expression => true}, {
                    insta::assert_snapshot!(stringify!($name), format!(
                        "--- unoptimized Core\n{}\n--- unoptimized UPLC\n{}\n--- optimized Core\n{}\n--- optimized UPLC\n{}\n--- result\n{}\n--- logs\n{:?}",
                        pretty(compiled.core), baseline.uplc, pretty(after), optimized.uplc, optimized.result, optimized.logs));
                });
                anf::validate(after).unwrap();
                hygiene::validate(after, &[]).unwrap();
                assert_eq!(compiled.core.ty, after.ty);
                assert_eq!(baseline.observable, optimized.observable);
                assert_eq!(baseline.logs, optimized.logs);
            });
        }
    };
}
boolean_case_snapshot!(
    boolean_helpers,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Logic exposing ((&&), (||))
    main : bool
    main = (True && False) || (True && True)
"#
);
boolean_case_snapshot!(
    cold_trace,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin exposing (..)
    main : bool
    main = if True then False else trace "wrong" True
"#
);
boolean_case_snapshot!(
    retained_trace,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin exposing (..)
    main : bool
    main =
        let
            strict = trace "before" False
        in
        if True then strict else True
"#
);

boolean_case_snapshot!(
    builtin_pair_accessors,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin
    first : pair 'a 'b -> 'a
    first = .fst
    second : pair 'a 'b -> 'b
    second p = p.snd
    main : (Data, Data)
    main =
        let
            p = Builtin.trace "pair" (Builtin.mkPairData (I 20) (I 22))
        in
        (first p, second p)
"#
);
boolean_case_snapshot!(
    builtin_pair_accessor_alias,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin
    type alias decoded = pair int (list Data)
    fields : decoded -> list Data
    fields = .snd
    main : Data
    main = Builtin.headList (fields (Builtin.unConstrData (Builtin.constrData 0 [I 42])))
"#
);

boolean_case_snapshot!(
    recursive_forwarded_parameter,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin
    loop : int -> int -> int
    loop unused n =
        if Builtin.equalsInteger n 0 then
            42
        else
            loop unused (Builtin.subtractInteger n 1)
    main : int
    main = loop (Builtin.trace "entry" 7) 3
"#
);
boolean_case_snapshot!(
    mutually_forwarded_parameter,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin
    first : int -> int -> int
    first unused n =
        if Builtin.equalsInteger n 0 then
            42
        else
            second (Builtin.subtractInteger n 1) unused
    second : int -> int -> int
    second n unused = first unused n
    main : int
    main = first (Builtin.trace "entry" 7) 3
"#
);

boolean_case_snapshot!(
    staged_application_packing,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin exposing (..)
    choose : int -> int -> int -> int
    choose n =
        if Builtin.equalsInteger n 0 then
            \x y -> Builtin.addInteger x y
        else
            choose (Builtin.subtractInteger n 1)
    main : int
    main =
        let
            p = choose 3 20
        in
        p 22
"#
);
boolean_case_snapshot!(
    packed_application_trace_stages,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin exposing (..)
    choose : int -> int -> int -> int
    choose n =
        if Builtin.equalsInteger n 0 then
            \x -> trace "first" (\y -> trace "second" (Builtin.addInteger x y))
        else
            choose (Builtin.subtractInteger n 1)
    main : int
    main =
        let
            p = choose 3 20
        in
        p 22
"#
);
boolean_case_snapshot!(
    application_argument_trace_stays_after_first_stage,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin exposing (..)
    choose : int -> int -> int -> int
    choose n =
        if Builtin.equalsInteger n 0 then
            \x -> trace "first" (\y -> trace "second" (Builtin.addInteger x y))
        else
            choose (Builtin.subtractInteger n 1)
    main : int
    main =
        let
            p = choose 3 20
        in
        p (trace "argument" 22)
"#
);

#[test]
fn application_failure_precedes_later_argument() {
    let source = indoc::indoc!(
        r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        choose : int -> int -> int -> int
        choose n =
            if Builtin.equalsInteger n 0 then
                \x -> trace "first" fail
            else
                choose (Builtin.subtractInteger n 1)
        main : int
        main =
            let
                p = choose 3 20
            in
            p (trace "must not run" 22)
    "#
    );
    with_base(source, |arena, build, root| {
        let compiled = build
            .compile(arena, root, None, TraceConfig::default())
            .expect("source compiles to Core");
        let fixture = crate::harness::prepare_fixture(arena, compiled.core);
        assert!(fixture.evaluated.result.starts_with("error:"));
        insta::with_settings!({description => source, omit_expression => true}, {
            insta::assert_snapshot!(format!("{}\n--- result\n{}\n--- logs\n{:?}",
                fixture.code_snapshot(), fixture.evaluated.result, fixture.evaluated.logs));
        });
        fixture.assert_equivalent(arena);
    });
}

fn check_silent(arena: &Arena, name: &str, core: &Core<'_>, source: &str, fails: bool) {
    let after = crate::optimizer::optimize_silent(arena, core).unwrap();
    let before_program = crate::program::assemble_core(arena, core).unwrap();
    let after_program =
        crate::program::assemble_core_with_options(arena, core, OptimizationLevel::O2).unwrap();
    let baseline = crate::harness::eval_named(arena, before_program.named);
    let optimized = crate::harness::eval_named(arena, after_program.named);
    assert_eq!(optimized.result.starts_with("error:"), fails);
    insta::with_settings!({description => source, omit_expression => true}, {
        insta::assert_snapshot!(name, format!("--- unoptimized Core\n{}\n--- unoptimized UPLC\n{}\n--- optimized Core (O2)\n{}\n--- optimized UPLC (O2)\n{}\n--- O0 result\n{}\n--- O0 logs\n{:?}\n--- O2 result\n{}\n--- O2 logs\n{:?}", pretty(core), baseline.uplc, pretty(after), optimized.uplc, baseline.result, baseline.logs, optimized.result, optimized.logs));
    });
    // O2 intentionally drops failures in trace messages. Value failures remain
    // covered by the success/error guard and the evaluated snapshot.
    assert_eq!(core.ty, after.ty);
    nash_ir::hygiene::validate(after, &[]).unwrap();
    nash_ir::anf::validate(after).unwrap();
    assert_silent(after_program.named);
    let twice =
        crate::program::assemble_core_with_options(arena, after, OptimizationLevel::O2).unwrap();
    assert_eq!(
        nash_plutus::flat::encode(after_program.program).unwrap(),
        nash_plutus::flat::encode(twice.program).unwrap()
    );
}
fn assert_silent(root: &Term<'_, nash_plutus::binder::Name<'_>>) {
    let mut pending = vec![root];
    while let Some(term) = pending.pop() {
        match term {
            Term::Builtin(F::Trace) => panic!("O2 emitted a trace builtin"),
            Term::Lambda { body, .. } | Term::Delay(body) | Term::Force(body) => pending.push(body),
            Term::Apply { function, argument } => pending.extend([*function, *argument]),
            Term::Constr { fields, .. } => pending.extend(fields.iter().copied()),
            Term::Case { constr, branches } => {
                pending.push(constr);
                pending.extend(branches.iter().copied());
            }
            _ => {}
        }
    }
}

macro_rules! silent_case_snapshot {
    ($name:ident, $fails:literal, $source:literal) => {
        #[test]
        fn $name() {
            let source = indoc::indoc!($source);
            with_base(source, |arena, build, root| {
                let compiled = build
                    .compile(
                        arena,
                        root,
                        None,
                        TraceConfig {
                            user: TraceLevel::Silent,
                            compiler: false,
                        },
                    )
                    .unwrap();
                check_silent(arena, stringify!($name), compiled.core, source, $fails);
            });
        }
    };
}
silent_case_snapshot!(
    o2_source_and_builtin,
    false,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin
    main : int
    main = trace "source" (Builtin.trace "builtin" 42)
"#
);
silent_case_snapshot!(
    o2_first_class_and_partial,
    false,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin
    log : string -> int -> int
    log = Builtin.trace
    main : int
    main =
        let
            partial = log "partial"
        in
        partial (log "inner" 42)
"#
);
silent_case_snapshot!(
    o2_discards_message_failure,
    false,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin
    main : int
    main = Builtin.trace (fail) 42
"#
);
silent_case_snapshot!(
    o2_strict_value_failure,
    true,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin
    main : int
    main = Builtin.trace "failure" (fail)
"#
);
silent_case_snapshot!(
    o2_discards_partial_message_failure,
    false,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin
    partial : int -> int
    partial = Builtin.trace (fail)
    main : int
    main =
        let
            unused = partial
        in
        42
"#
);
silent_case_snapshot!(
    o2_source_message_is_omitted,
    false,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    main : int
    main = trace (fail) 42
"#
);

silent_case_snapshot!(
    o2_discards_aliased_message_failure,
    false,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin
    log : string -> int -> int
    log = Builtin.trace
    main : int
    main = log (fail) 42
"#
);

#[test]
fn o2_discards_diverging_message() {
    let source = indoc::indoc!(
        r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin
        loop : unit -> string
        loop _ = loop ()
        main : int
        main = Builtin.trace (loop ()) 42
    "#
    );
    with_base(source, |arena, build, root| {
        let core = build
            .compile(
                arena,
                root,
                None,
                TraceConfig {
                    user: TraceLevel::Silent,
                    compiler: false,
                },
            )
            .unwrap()
            .core;
        let before = crate::program::assemble_core(arena, core).unwrap();
        let after = crate::optimizer::optimize_silent(arena, core).unwrap();
        let compiled = crate::program::assemble_core_with_options(
            arena,
            core,
            nash_config::OptimizationLevel::O2,
        )
        .unwrap();
        let evaluated = crate::harness::eval_named(arena, compiled.named);
        assert!(!evaluated.result.starts_with("error:"));
        insta::with_settings!({description => source, omit_expression => true}, {
            insta::assert_snapshot!(format!("--- unoptimized Core\n{}\n--- unoptimized UPLC (not evaluated: diverging message)\n{}\n--- optimized Core (O2)\n{}\n--- optimized UPLC (O2)\n{}\n--- result\n{}\n--- logs\n{:?}", pretty(core), nash_plutus::pretty::term(before.named), pretty(after), evaluated.uplc, evaluated.result, evaluated.logs));
        });
        assert_eq!(core.ty, after.ty);
        hygiene::validate(after, &[]).unwrap();
        anf::validate(after).unwrap();
    });
}

silent_case_snapshot!(
    o2_retains_separate_message_binding,
    true,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin
    main : int
    main =
        let
            message = fail
        in
        Builtin.trace message 42
"#
);
silent_case_snapshot!(
    o2_retains_indirect_caller_work,
    true,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin
    call : (string -> int -> int) -> int
    call logger = logger (fail) 42
    main : int
    main = call Builtin.trace
"#
);
