mod snapshot_support;
use snapshot_support::SnapshotInputs;

use bumpalo::Bump;
use nash_ast::{PackageName, primitives::BASE};
use nash_can::Context;

#[test]
fn real_builtins_are_available_through_every_import_route() {
    let snapshot_inputs = SnapshotInputs::default();
    for builtin in ["iData", "unIData", "bData", "unBData"] {
        for package in [
            Some(BASE),
            None,
            Some(PackageName {
                author: "other",
                project: "base",
            }),
            Some(PackageName {
                author: "nash",
                project: "other",
            }),
        ] {
            for (import, reference) in [
                ("import Builtin", "Builtin.iData"),
                ("import Builtin as B", "B.iData"),
                ("import Builtin exposing (..)", "iData"),
                ("import Builtin exposing (iData)", "iData"),
            ] {
                let import = import.replace("iData", builtin);
                let reference = reference.replace("iData", builtin);
                let bump = Bump::new();
                let source = bump.alloc_str(&format!(
                    "module Main exposing (..)\n{import}\noperation = {reference}\n"
                ));
                let parsed = nash_parse::Parser::new(&bump, snapshot_inputs.record(source))
                    .module()
                    .unwrap();
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
                assert!(result.is_ok(), "{package:?}: {import}: {result:?}");
            }
        }
    }
}
