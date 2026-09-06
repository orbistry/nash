use bumpalo::Bump;
use nash_can::Context;

#[test]
fn twin_imports_preserve_privacy_and_explicit_exposure() {
    let mut diagnostics = Vec::new();
    for (exports, imports, bare, qualified) in [
        ("type status(..), Status", "..", true, false),
        ("type status, Status(..)", "..", false, true),
        ("..", "Status(..)", false, true),
    ] {
        let bump = Bump::new();
        let source = bump.alloc_str(&format!("module Status exposing ({exports})\ntype status = Ready | Waiting\ntype Status = Ready | Waiting\n"));
        let parsed = nash_parse::Parser::new(&bump, source.as_bytes())
            .module()
            .unwrap();
        let canonical = nash_can::canonicalize(&bump, Context::default(), &parsed).unwrap();
        let interface = nash_can::from_module(&bump, &canonical.module, &Default::default());
        let interfaces = std::collections::BTreeMap::from([("Status", interface)]);
        for (constructor, expected) in [("Ready", bare), ("S.Ready", qualified)] {
            let source = bump.alloc_str(&format!("module Main exposing (..)\nimport Status as S exposing ({imports})\nvalue = {constructor}\n"));
            let parsed = nash_parse::Parser::new(&bump, source.as_bytes())
                .module()
                .unwrap();
            let result = nash_can::canonicalize(
                &bump,
                Context {
                    package: None,
                    interfaces: Some(&interfaces),
                },
                &parsed,
            );
            assert_eq!(
                result.is_ok(),
                expected,
                "exports={exports}, imports={imports}, constructor={constructor}: {result:?}"
            );
            if let Err(errors) = result {
                assert!(matches!(
                    errors.as_slice(),
                    [nash_can::Error::NotFoundCtor { .. }]
                ));
                diagnostics.push(format!(
                    "exports={exports}; imports={imports}; {constructor}: {errors:?}"
                ));
            }
        }
    }
    insta::assert_snapshot!(diagnostics.join("\n"));
}

#[test]
fn twin_exception_rejects_unrelated_and_malformed_duplicates() {
    let mut diagnostics = Vec::new();
    for declarations in [
        "type status = Ready\ntype Other = Ready\n",
        "type status = Ready\ntype Status = Ready\ntype Other = Ready\n",
        "type status = Ready | Ready\ntype Status = Ready | Ready\n",
        "type status = Ready | Waiting\ntype Status = Waiting | Ready\n",
        "type status = Ready\ntype Status 'a = Ready\n",
        "type status = Ready\ntype Status = Ready Int\n",
        "type status = Ready\ntype Status = Ready\ntype alias Ready = { value : Int }\n",
    ] {
        let bump = Bump::new();
        let source = bump.alloc_str(&format!("module Status exposing (..)\n{declarations}"));
        let parsed = nash_parse::Parser::new(&bump, source.as_bytes())
            .module()
            .unwrap();
        let errors = nash_can::canonicalize(&bump, Context::default(), &parsed).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| matches!(error, nash_can::Error::DuplicateCtor { .. })),
            "{errors:?}"
        );
        diagnostics.push(format!("{declarations}{errors:?}"));
    }
    insta::assert_snapshot!(diagnostics.join("\n"));
}
