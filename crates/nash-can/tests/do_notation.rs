mod snapshot_support;
use snapshot_support::SnapshotInputs;

use bumpalo::Bump;
use indoc::indoc;

const MONAD_SOURCE: &str = "module Monad exposing (Monad)\ntrait Monad 'm where\n    bind : 'm 'a -> ('a -> 'm 'b) -> 'm 'b\n";

fn monad(bump: &Bump, core: bool) -> nash_can::Interface<'_> {
    let module = nash_parse::Parser::new(bump, MONAD_SOURCE)
        .module()
        .unwrap();
    let canonical = nash_can::canonicalize(
        bump,
        nash_can::Context {
            package: core.then_some(nash_ast::primitives::CORE),
            interfaces: None,
        },
        &module,
    )
    .unwrap();
    nash_can::from_module(bump, &canonical.module, &Default::default())
}

#[test]
fn do_scopes_statements_and_uses_the_core_method() {
    let snapshot_inputs = SnapshotInputs::default();
    snapshot_inputs.record(MONAD_SOURCE);
    let bump = Bump::new();
    let interfaces = std::collections::BTreeMap::from([("Monad", monad(&bump, true))]);
    let source = indoc!(
        r#"
        module Main exposing (..)
        import Monad as M
        bind x = x
        run m pure = do
            (x, y) <- m
            let
                saved = (x, y)
            m
            pure saved
        single x = do
            x
    "#
    );
    let module = nash_parse::Parser::new(&bump, snapshot_inputs.record(source))
        .module()
        .unwrap();
    let canonical = nash_can::canonicalize(
        &bump,
        nash_can::Context {
            package: None,
            interfaces: Some(&interfaces),
        },
        &module,
    )
    .unwrap();
    assert!(canonical.warnings.is_empty(), "{:?}", canonical.warnings);
    insta::with_settings!({description => snapshot_inputs.description(), omit_expression => true}, {
        insta::assert_debug_snapshot!(canonical.module.decls);
    });
}

#[test]
fn do_rejects_missing_core_and_refutable_patterns() {
    let snapshot_inputs = SnapshotInputs::default();
    snapshot_inputs.record(MONAD_SOURCE);
    let bump = Bump::new();
    let mut results = Vec::new();
    for (core, pattern, rhs) in [
        (false, "x", "m"),
        (true, "Some x", "m"),
        (true, "(x, Some y)", "m"),
        (true, "x", "x"),
    ] {
        let interfaces = std::collections::BTreeMap::from([("Monad", monad(&bump, core))]);
        let source = bump.alloc_str(&format!("module Main exposing (..)\nimport Monad\ntype option 'a = Some 'a\nrun m = do\n    {pattern} <- {rhs}\n    m\n"));
        let module = nash_parse::Parser::new(&bump, snapshot_inputs.record(source))
            .module()
            .unwrap();
        let errors = nash_can::canonicalize(
            &bump,
            nash_can::Context {
                package: None,
                interfaces: Some(&interfaces),
            },
            &module,
        )
        .unwrap_err();
        assert!(
            matches!(errors.as_slice(), [nash_can::Error::DoWithoutMonad { .. }] if !core)
                || matches!(errors.as_slice(), [nash_can::Error::RefutableBindPattern { .. }] if core && rhs == "m")
                || matches!(errors.as_slice(), [nash_can::Error::NotFoundVar { .. }] if rhs == "x")
        );
        results.push(snapshot_support::errors(source, &errors));
    }
    insta::with_settings!({info => &"diagnostic", description => snapshot_inputs.description(), omit_expression => true}, {
        insta::assert_snapshot!(results.join("\n"));
    });
}
