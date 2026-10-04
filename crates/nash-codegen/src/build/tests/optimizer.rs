//! Source-to-optimized snapshots reuse the normal Base fixture compiler.
use super::*;
use nash_ir::{anf, build::Builder, hygiene, pretty::pretty};

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
