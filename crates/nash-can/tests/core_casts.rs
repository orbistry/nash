use bumpalo::Bump;
use nash_ast::{PackageName, primitives::CORE};
use nash_can::Context;

#[test]
fn casts_require_exact_core_package_for_every_import_route() {
    let mut diagnostics = Vec::new();
    for cast in [
        "castLift",
        "castLower",
        "castToData",
        "castFromDataShallow",
        "castValidateData",
    ] {
        for package in [
            Some(CORE),
            None,
            Some(PackageName {
                author: "other",
                project: "core",
            }),
            Some(PackageName {
                author: "nash",
                project: "other",
            }),
        ] {
            for (import, reference) in [
                ("import Builtin", "Builtin.castLift"),
                ("import Builtin as B", "B.castLift"),
                ("import Builtin exposing (..)", "castLift"),
                ("import Builtin exposing (castLift)", "castLift"),
            ] {
                let import = import.replace("castLift", cast);
                let reference = reference.replace("castLift", cast);
                let bump = Bump::new();
                let source = bump.alloc_str(&format!(
                    "module Main exposing (..)\n{import}\nlift : int -> Int\nlift = {reference}\n"
                ));
                let parsed = nash_parse::Parser::new(&bump, source).module().unwrap();
                let interfaces = std::collections::BTreeMap::from([(
                    "Builtin",
                    nash_can::kinds::builtin_interface(&bump),
                )]);
                let result = nash_can::canonicalize(
                    &bump,
                    Context {
                        package,
                        interfaces: Some(&interfaces),
                    },
                    &parsed,
                );
                assert_eq!(
                    result.is_ok(),
                    package == Some(CORE),
                    "{package:?}: {import}: {result:?}"
                );
                if package.is_none() && cast == "castLift" {
                    diagnostics.push(format!("{import}: {:?}", result.unwrap_err()));
                }
            }
        }
    }
    insta::assert_snapshot!(diagnostics.join("\n"));
}
