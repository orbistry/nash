const BASE_MODULES: &[&str] = &[
    include_str!("../../../../nash-driver/base/src/Lift.nash"),
    include_str!("../../../../nash-driver/base/src/Bool.nash"),
    include_str!("../../../../nash-driver/base/src/Unit.nash"),
    include_str!("../../../../nash-driver/base/src/Ordering.nash"),
    include_str!("../../../../nash-driver/base/src/Functor.nash"),
    include_str!("../../../../nash-driver/base/src/Applicative.nash"),
    include_str!("../../../../nash-driver/base/src/Monad.nash"),
    include_str!("../../../../nash-driver/base/src/Option.nash"),
    include_str!("../../../../nash-driver/base/src/Data.nash"),
];

macro_rules! case {
    ($name:ident, $source:literal, $expected:expr) => {
        case!(
            $name,
            $source,
            $expected,
            crate::build::TraceConfig::default()
        );
    };
    ($name:ident, $source:literal, $expected:expr, $trace:expr) => {
        #[test]
        fn $name() {
            crate::build::tests::with_base_modules(
                indoc::indoc!($source),
                crate::build::tests::source::BASE_MODULES,
                |arena, build, root| {
                    let compiled = build
                        .compile(arena, root, None, $trace)
                        .expect("source compiles to Core");
                    let core = crate::recursion::rewrite(
                        &nash_ir::build::Builder::new(arena),
                        compiled.core,
                    )
                    .expect("recursion rewrites");
                    let evaluated = crate::harness::eval_core(arena, core);
                    let expected: Result<&str, ()> = $expected;
                    match expected {
                        Ok(result) => assert_eq!(evaluated.result, result),
                        Err(()) => assert!(
                            evaluated.result.starts_with("error:"),
                            "expected evaluation failure, got {}",
                            evaluated.result
                        ),
                    }
                    insta::assert_snapshot!(
                        stringify!($name),
                        format!("--- core\n{}\n{evaluated}", nash_ir::pretty::pretty(core))
                    );
                },
            );
        }
    };
}

macro_rules! validator_case {
    ($name:ident, $source:literal) => {
        #[test]
        fn $name() {
            crate::build::tests::with_base_modules(
                indoc::indoc!($source),
                crate::build::tests::source::BASE_MODULES,
                |arena, build, root| {
                    let compiled = build
                        .compile(arena, root, None, crate::build::TraceConfig::default())
                        .expect("validator compiles to Core");
                    let assembled = crate::program::assemble_core(arena, compiled.core)
                        .expect("validator lowers to closed UPLC");
                    insta::assert_snapshot!(
                        stringify!($name),
                        format!(
                            "--- core\n{}\n--- uplc\n{}",
                            nash_ir::pretty::pretty(compiled.core),
                            nash_plutus::pretty::program(assembled.program)
                        )
                    );
                },
            );
        }
    };
}

macro_rules! traced_case {
    ($name:ident, $silent_name:ident, $source:literal, $expected:expr) => {
        case!($name, $source, $expected);
        case!(
            $silent_name,
            $source,
            $expected,
            crate::build::TraceConfig {
                user: crate::build::TraceLevel::Silent,
                compiler: false,
            }
        );
    };
}

mod builtins;
mod collections;
mod lists;
mod patterns;
