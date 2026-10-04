use nash_driver::{Database, InMemorySource, build_graph, bundled_base, test_with};
use std::sync::Arc;
use tokio::sync::Mutex;
use url::Url;

async fn compile(source: &str) -> Result<Vec<nash_proof::ProofProgram>, String> {
    let memory = InMemorySource::new();
    let main = Url::parse("file:///proof-project/src/Main.nash").unwrap();
    memory.insert(main.clone(), source.into());
    let db = Arc::new(Mutex::new(Database::new(memory)));
    let mut modules = bundled_base::modules();
    modules.insert(main.clone(), None);
    let graph = build_graph(db.clone(), &modules.keys().cloned().collect::<Vec<_>>())
        .await
        .unwrap();
    let (report, programs) = test_with(db, &graph, &modules, move |solved| {
        nash_driver::build::compile_proofs_matching_with(
            solved,
            |uri| (*uri == main).then(nash_config::Build::default),
            |_, _| true,
        )
    })
    .await;
    if !report.is_success() {
        return Err(format!("{:#?}", report.ordered_reports()));
    }
    programs.unwrap().map_err(|e| e.message)
}

#[tokio::test]
async fn proof_domains_compile_to_closed_uplc_without_test_instrumentation() {
    let programs = compile(
        r#"module Main exposing (..)
proof
    import Proof
    prop "integer identity" =
        let x via Proof.int in
        do
            assert (x + 0 == x)
    prop "big integer identity" =
        let x via Proof.integer in
        do
            assert (x == x)
    prop "ledger identity" =
        let ctx via Proof.spendingV3 in
        do
            assert (ctx == ctx)
"#,
    )
    .await
    .unwrap();
    assert_eq!(programs.len(), 3);
    assert_eq!(programs[0].domains, vec![nash_proof::Domain::Int]);
    assert_eq!(programs[2].domains, vec![nash_proof::Domain::Spending(3)]);
    assert!(programs.iter().all(|p| !p.flat.is_empty()));
}

#[tokio::test]
async fn random_generator_is_not_a_symbolic_proof_domain() {
    let error = compile(
        r#"module Main exposing (..)
proof
    import Prop
    prop "random is not universal" =
        let x via Prop.int in
        do
            assert (x == x)
"#,
    )
    .await
    .unwrap_err();
    assert!(error.contains("invalid_domain"), "{error}");
}

#[tokio::test]
async fn proof_imports_do_not_leak_into_tests() {
    let error = compile(
        r#"module Main exposing (..)
tests
    test "private import" = do
        assert (Proof.int == 0)
proof
    import Proof
    test "closed" = do
        assert True
"#,
    )
    .await
    .unwrap_err();
    assert!(error.contains("Proof"), "{error}");
}

#[tokio::test]
async fn proof_budgets_and_mismatched_ledger_versions_are_rejected() {
    for (body, expected) in [
        (
            "test \"budget\" within (cpu 100) = do\n        assert True",
            "budget",
        ),
        (
            "prop \"wrong version\" =\n        let ctx via Proof.spendingV2 in\n        do\n            assert (ctx == ctx)",
            "version",
        ),
    ] {
        let source = format!("module Main exposing (..)\nproof\n    import Proof\n    {body}\n");
        let error = compile(&source).await.unwrap_err();
        assert!(error.contains(expected), "{error}");
    }
}

#[tokio::test]
#[ignore = "requires NASH_PROOF_LEAN_PROJECT, Lean 4.24, the pinned libraries and Z3"]
async fn live_backend() {
    let project = std::env::var_os("NASH_PROOF_LEAN_PROJECT").expect("NASH_PROOF_LEAN_PROJECT");
    let mut programs = compile(
        r#"module Main exposing (..)
proof
    import Proof
    prop "identity" = let x via Proof.int in do
        assert (x + 0 == x)
    prop "not all integers are nonnegative" = let x via Proof.int in do
        assert (x >= 0)
    prop "division always fails" fail = let x via Proof.int in do
        assert (x / 0 == 0)
    prop "negative witness" fail once = let x via Proof.int in do
        assert (x >= 0)
"#,
    )
    .await
    .unwrap();
    let directory = std::env::temp_dir().join(format!(
        "nash-live-proofs-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let files = nash_proof::export(&programs, &directory, 65, 1000, 10).unwrap();
    let config = nash_proof::Config {
        lean_project: project.into(),
        fuel: 65,
        postcondition_fuel: 1000,
        solver_timeout: 10,
        wall_timeout: std::time::Duration::from_secs(60),
    };
    for ((program, source), expected) in programs.iter().zip(files).zip([
        nash_proof::Status::Verified,
        nash_proof::Status::Counterexample,
        nash_proof::Status::Verified,
        nash_proof::Status::Witness,
    ]) {
        let outcome = nash_proof::run(program, &source, &config).unwrap();
        assert_eq!(outcome.status, expected, "{}", outcome.diagnostics);
    }
    let limited = directory.join("limited");
    let files = nash_proof::export(&programs[..1], &limited, 1, 1000, 10).unwrap();
    let config = nash_proof::Config { fuel: 1, ..config };
    let outcome = nash_proof::run(&programs[0], &files[0], &config).unwrap();
    assert_eq!(
        outcome.status,
        nash_proof::Status::Counterexample,
        "{}",
        outcome.diagnostics
    );
    for (index, expect, status) in [
        (0, nash_source::Expect::Fail, nash_proof::Status::Verified),
        (
            1,
            nash_source::Expect::FailOnce,
            nash_proof::Status::Witness,
        ),
    ] {
        programs[0].expect = expect;
        let directory = directory.join(format!("exhaustion-{index}"));
        let files = nash_proof::export(&programs[..1], &directory, 1, 1000, 10).unwrap();
        let outcome = nash_proof::run(&programs[0], &files[0], &config).unwrap();
        assert_eq!(outcome.status, status, "{}", outcome.diagnostics);
    }
}

#[tokio::test]
async fn partial_correctness_compiles_computation_and_condition_separately() {
    let programs = compile(
        r#"module Main exposing (..)
proof
    import Proof
    prop "identity result" = let x via Proof.int in do
        Proof.returns (x + 0) (\result -> result == x)
    test "unit result" = do
        Proof.returns () (\_ -> True)
    prop "Data result" = let x via Proof.integer in do
        Proof.returns x (\result -> result == x)
"#,
    )
    .await
    .unwrap();
    assert_eq!(
        programs[0].postcondition.as_ref().unwrap().result,
        nash_proof::ReturnDomain::Int
    );
    assert_eq!(
        programs[1].postcondition.as_ref().unwrap().result,
        nash_proof::ReturnDomain::Unit
    );
    assert_eq!(
        programs[2].postcondition.as_ref().unwrap().result,
        nash_proof::ReturnDomain::Data
    );
    assert!(
        programs
            .iter()
            .all(|p| !p.postcondition.as_ref().unwrap().flat.is_empty())
    );
    assert_ne!(
        programs[0].flat,
        programs[0].postcondition.as_ref().unwrap().flat
    );
}

#[tokio::test]
async fn partial_correctness_rejects_failure_modifiers_and_function_results() {
    for (body, expected) in [
        (
            "test \"invalid fail\" fail = do\n        Proof.returns 1 (\\_ -> True)",
            "cannot use fail",
        ),
        (
            "test \"function result\" = do\n        Proof.returns (\\x -> x + 1) (\\_ -> True)",
            "result",
        ),
    ] {
        let source = format!("module Main exposing (..)\nproof\n    import Proof\n    {body}\n");
        let error = compile(&source).await.unwrap_err();
        assert!(error.contains(expected), "{error}");
    }
}

#[tokio::test]
#[ignore = "requires the pinned Lean project and Z3"]
async fn live_partial_correctness() {
    let programs = compile(
        r#"module Main exposing (..)
wrong x = x + 1
reject : int -> int
reject _ = fail
loop : int -> int
loop x = loop x
proof
    import Proof
    prop "correct identity" = let x via Proof.int in do
        Proof.returns x (\result -> result == x)
    prop "wrong returned result must refute" = let x via Proof.int in do
        Proof.returns (wrong x) (\result -> result == x)
    prop "rejection has no successful return" = let x via Proof.int in do
        Proof.returns (reject x) (\_ -> False)
    prop "exhaustion has no successful return" = let x via Proof.int in do
        Proof.returns (loop x) (\_ -> False)
    prop "undefined condition must refute" = let x via Proof.int in do
        Proof.returns x (\_ -> 1 / 0 == 0)
    prop "condition exhaustion must be inconclusive" = let x via Proof.int in do
        Proof.returns x (\result -> loop result == 0)
    test "unit return" = do
        Proof.returns () (\_ -> True)
    prop "Data return" = let x via Proof.integer in do
        Proof.returns x (\result -> result == x)
    prop "Boolean return" = let x via Proof.bool in do
        Proof.returns x (\result -> result == x)
    prop "Boolean false case must refute" = let x via Proof.bool in do
        Proof.returns x (\result -> result)
    prop "bytes return" = let x via Proof.bytes in do
        Proof.returns x (\result -> result == x)
    prop "string return" = let x via Proof.string in do
        Proof.returns x (\result -> result == x)
"#,
    )
    .await
    .unwrap();
    let directory = std::env::temp_dir().join(format!("nash-live-partial-{}", std::process::id()));
    let files = nash_proof::export(&programs, &directory, 120, 150, 10).unwrap();
    let config = nash_proof::Config {
        lean_project: std::env::var_os("NASH_PROOF_LEAN_PROJECT")
            .expect("Lean project")
            .into(),
        fuel: 120,
        postcondition_fuel: 150,
        solver_timeout: 10,
        wall_timeout: std::time::Duration::from_secs(60),
    };
    for ((program, source), expected) in programs.iter().zip(files).zip([
        nash_proof::Status::VerifiedPartial,
        nash_proof::Status::Counterexample,
        nash_proof::Status::VerifiedPartial,
        nash_proof::Status::VerifiedPartial,
        nash_proof::Status::Counterexample,
        nash_proof::Status::PostconditionExhausted,
        nash_proof::Status::VerifiedPartial,
        nash_proof::Status::VerifiedPartial,
        nash_proof::Status::VerifiedPartial,
        nash_proof::Status::Counterexample,
        nash_proof::Status::VerifiedPartial,
        nash_proof::Status::VerifiedPartial,
    ]) {
        let outcome = nash_proof::run(program, &source, &config).unwrap();
        assert_eq!(
            outcome.status, expected,
            "{}: {}",
            program.name, outcome.diagnostics
        );
    }
}

#[tokio::test]
async fn proof_aliases_and_independent_name_scopes_survive_canonicalization() {
    let programs = compile(
        r#"module Main exposing (..)
tests
    test "shared" = do
        assert True
proof
    import Proof as Universal exposing (returns)
    prop "shared" = let value via Universal.int in do
        returns value (\result -> result == value)
"#,
    )
    .await
    .unwrap();
    assert_eq!(programs.len(), 1);
    assert_eq!(programs[0].domains, vec![nash_proof::Domain::Int]);
    assert!(programs[0].postcondition.is_some());
}

#[tokio::test]
async fn proof_specific_errors_are_reported_before_codegen() {
    for (declarations, expected) in [
        (
            "test \"duplicate\" = do\n        assert True\n    test \"duplicate\" = do\n        assert True",
            "duplicate_proof",
        ),
        (
            "prop \"not a domain\" = let value via Proof.returns in do\n        assert True",
            "invalid_domain",
        ),
        (
            "prop \"wrong condition\" = let value via Proof.int in do\n        Proof.returns value (\\result -> result + 1)",
            "postcondition",
        ),
        (
            "prop \"invalid existential modifier\" fail once = let value via Proof.int in do\n        Proof.returns value (\\_ -> True)",
            "invalid_expectation",
        ),
        (
            "test \"non-unit execution\" = do\n        Proof.bool",
            "proof body",
        ),
    ] {
        let source =
            format!("module Main exposing (..)\nproof\n    import Proof\n    {declarations}\n");
        let error = compile(&source).await.unwrap_err();
        assert!(error.contains(expected), "expected {expected}: {error}");
    }
}
