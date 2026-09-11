//! Execute serialized validator artifacts built from the real core workspace.
use std::{path::Path, sync::Arc};

use nash_codegen::build::TraceConfig;
use nash_driver::{
    Database, FileSystemSource, Project, build::build_validators, build_graph, build_with,
};
use nash_plutus::{arena::Arena, data::PlutusData, flat, syn, term::Term};
use tokio::sync::Mutex;

#[tokio::test]
async fn real_core_vesting_artifacts_execute_all_ledger_cases() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/vesting");
    let project = Project::load(root)
        .await
        .expect("load real vesting workspace");
    let db = Arc::new(Mutex::new(Database::new(FileSystemSource::new())));
    let origins = project
        .discover_modules(&*db.lock().await)
        .await
        .expect("discover core and app sources");
    for module in ["Lift", "Literal"] {
        assert!(
            origins.iter().any(|(uri, owner)| {
                uri.path().ends_with(&format!("/core/src/{module}.nash"))
                    && owner
                        .as_ref()
                        .is_some_and(|owner| owner.to_string() == "nash/core")
            }),
            "{module} must come from the actual nash/core package"
        );
    }
    let graph = build_graph(db.clone(), &origins.keys().cloned().collect::<Vec<_>>())
        .await
        .expect("build dependency graph");
    let (report, result) = build_with(db, &graph, &origins, |solved| {
        build_validators(solved, TraceConfig::default())
    })
    .await;
    assert!(report.is_success(), "{report:#?}");
    let outputs = result
        .expect("successful frontend calls backend")
        .expect("generate validator artifacts");
    assert_eq!(
        outputs
            .iter()
            .map(|output| output.module.as_str())
            .collect::<Vec<_>>(),
        ["Vesting", "VestingParam"]
    );

    for output in outputs {
        let arena = Arena::new();
        let program = syn::parse_program(&arena, &output.uplc)
            .into_result()
            .expect("serialized UPLC parses");
        assert_eq!(
            flat::encode(program).unwrap(),
            output.flat,
            "{} text and Flat agree",
            output.module
        );
        assert_eq!(
            flat::to_cbor(program).unwrap(),
            output.cbor,
            "{} text and CBOR agree",
            output.module
        );
        let parameterized = output.module == "VestingParam";
        for (name, deadline, redeemer, signer, expected) in [
            ("claim after deadline", 10, 0, &b""[..], true),
            ("claim before deadline", 30, 0, &b""[..], false),
            ("cancel signed by owner", 10, 1, &[0xaa][..], true),
            ("cancel unsigned", 10, 1, &b""[..], false),
        ] {
            let datum = PlutusData::constr(
                &arena,
                0,
                arena.alloc_slice_copy(&[
                    PlutusData::byte_string(&arena, &[0xaa]),
                    PlutusData::integer_from(&arena, deadline),
                ]),
            );
            let action = PlutusData::constr(&arena, redeemer, &[]);
            let context = PlutusData::constr(
                &arena,
                0,
                arena.alloc_slice_copy(&[
                    PlutusData::integer_from(&arena, 20),
                    PlutusData::byte_string(&arena, signer),
                ]),
            );
            let applied = if parameterized {
                program.apply(&arena, Term::integer_from(&arena, 5))
            } else {
                program
            };
            let evaluation = applied
                .apply(&arena, Term::data(&arena, datum))
                .apply(&arena, Term::data(&arena, action))
                .apply(&arena, Term::data(&arena, context))
                .eval(&arena);
            assert_eq!(
                evaluation.term.is_ok(),
                expected,
                "{}: {name}: {:?}",
                output.module,
                evaluation.term
            );
            if expected {
                assert_eq!(evaluation.term.unwrap(), Term::unit(&arena));
                assert!(evaluation.info.logs.is_empty(), "{}: {name}", output.module);
            } else {
                assert!(
                    matches!(
                        evaluation.term,
                        Err(nash_plutus::machine::MachineError::ExplicitErrorTerm)
                    ),
                    "{}: {name}: expected assertion failure",
                    output.module
                );
                assert_eq!(
                    evaluation.info.logs,
                    ["assertion failed"],
                    "{}: {name}",
                    output.module
                );
            }
            assert!(
                evaluation.info.consumed_budget.cpu > 0,
                "{}: {name}",
                output.module
            );
            assert!(
                evaluation.info.consumed_budget.mem > 0,
                "{}: {name}",
                output.module
            );
        }
    }
}
