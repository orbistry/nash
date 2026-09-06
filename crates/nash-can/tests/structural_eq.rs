use bumpalo::Bump;
use nash_ast::{PackageName, primitives::CORE};
use nash_can::Context;

#[test]
fn structural_eq_rejects_big_overrides_only_for_exact_core_trait() {
    let mut errors = Vec::new();
    for package in [
        CORE,
        PackageName {
            author: "other",
            project: "core",
        },
    ] {
        for head in ["Token", "Box"] {
            let bump = Bump::new();
            let builtins = std::collections::BTreeMap::from([(
                "Builtin",
                nash_can::kinds::builtin_interface(&bump),
            )]);
            let source = "module Eq exposing (Eq)\ntrait Eq 'a where\n    eq : 'a -> 'a -> bool\n";
            let parsed = nash_parse::Parser::new(&bump, source.as_bytes())
                .module()
                .unwrap();
            let canonical = nash_can::canonicalize(
                &bump,
                Context {
                    package: Some(package),
                    interfaces: Some(&builtins),
                },
                &parsed,
            )
            .unwrap();
            let mut interfaces = builtins;
            interfaces.insert(
                "Eq",
                nash_can::from_module(&bump, &canonical.module, &Default::default()),
            );
            let source = bump.alloc_str(&format!("module Main exposing (..)\nimport Eq exposing (Eq)\nimport Builtin\ntype Token = Token Int\ntype alias Box = {{ item : Int }}\nimpl Eq {head} where\n    eq _ _ = Builtin.True\n"));
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
            if package == CORE {
                let err = result.unwrap_err();
                assert!(matches!(
                    err.as_slice(),
                    [nash_can::Error::StructuralEqOverride { .. }]
                ));
                errors.push(format!("{head}: {err:?}"));
            } else {
                result.unwrap();
            }
        }
    }
    insta::assert_snapshot!(errors.join("\n"));
}
