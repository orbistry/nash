use std::{
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

struct Project(PathBuf);
impl Project {
    fn new(source: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "nash-test-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let project = Self(root);
        let core = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../core")
            .canonicalize()
            .unwrap();
        project.write(
            "nash.jsonc",
            &serde_json::json!({"type":"application","dependencies":{"nash/core":{"path":core}}})
                .to_string(),
        );
        project.write("src/Main.nash", source);
        project
    }
    fn write(&self, name: &str, text: &str) {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    fn run(&self, command: &str, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_nash"))
            .env("NASH_PROXY_VERSION", env!("CARGO_PKG_VERSION"))
            .env("NO_COLOR", "1")
            .env("FORCE_HYPERLINK", "0")
            .arg(command)
            .arg(&self.0)
            .args(args)
            .output()
            .unwrap()
    }
    fn json(&self, args: &[&str]) -> (Output, serde_json::Value) {
        let mut flags = vec!["--json", "--seed", "42"];
        flags.extend_from_slice(args);
        let output = self.run("test", &flags);
        let value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
            panic!(
                "{error}\nstdout:{}\nstderr:{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )
        });
        (output, value)
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn success(output: &Output) {
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

const UNITS: &str = "module Main exposing (..)\nimport Builtin exposing (..)\n\nprivate x = x\n\ntests\n    test \"passes\" = do\n        assert (private True)\n    test \"expected\" fail = do\n        assert False\n    test \"fails\" = do\n        assert False\n";

#[test]
fn unit_outcomes_filters_and_check_without_execution() {
    let project = Project::new(UNITS);
    success(&project.run("check", &[]));
    let (failed, json) = project.json(&[]);
    assert_eq!(failed.status.code(), Some(1));
    let text = json.to_string();
    for name in ["passes", "expected", "fails"] {
        assert!(text.contains(name));
    }
    success(&project.run(
        "t",
        &["--seed", "42", "--match", "Main.{passes}", "--exact"],
    ));
    success(&project.run(
        "test",
        &["--match", "passes", "--match", "expected", "--exact"],
    ));
    let filtered = project.run("test", &["--match", "fail", "--exact"]);
    success(&filtered);
}

const PROPS: &str = "module Main exposing (..)\nimport Prelude exposing (..)\nimport Literal\n\ntests\n    import Fuzz\n    import Test exposing (label)\n    prop \"bounded\" =\n        let x via Fuzz.choice 20 in\n        do\n            label \"draw\"\n            assert (x <= 20)\n    prop \"counterexample\" fail once =\n        let x via Fuzz.choice 20 in\n        do\n            assert (x < 10)\n    prop \"always fails\" fail =\n        let x via Fuzz.choice 20 in\n        do\n            assert (x < 0)\n";

#[test]
fn properties_replay_and_parallelism() {
    let project = Project::new(PROPS);
    let (one, a) = project.json(&["--jobs", "1"]);
    success(&one);
    let (many, b) = project.json(&["--jobs", "8"]);
    success(&many);
    assert_eq!(a, b, "fixed seed must give identical structured outcomes");
    let normalized = serde_json::to_string_pretty(&a)
        .unwrap()
        .replace(
            &project
                .0
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .to_string(),
            "<project>",
        )
        .replace(&project.0.to_string_lossy().to_string(), "<project>");
    insta::with_settings!({description => PROPS, omit_expression => true}, { insta::assert_snapshot!(normalized); });
}

#[test]
fn generator_failure_is_not_an_expected_body_failure() {
    let source = "module Main exposing (..)\nimport Builtin exposing (..)\n\ntests\n    import Fuzz exposing (type fuzzer(..))\n    import Option exposing (type option(..))\n    prop \"broken\" fail =\n        let x via Fuzzer (\\_ -> None) in\n        do\n            assert False\n";
    let project = Project::new(source);
    let (output, json) = project.json(&[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(json.to_string().to_lowercase().contains("fuzzer"));
}

#[test]
fn budget_failure_does_not_pass_under_fail() {
    let source = "module Main exposing (..)\nimport Builtin exposing (..)\n\ntests\n    test \"budget\" fail within (cpu 1, mem 1) = do\n        assert False\n";
    let project = Project::new(source);
    let (output, json) = project.json(&[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(json.to_string().to_lowercase().contains("budget"));
}

#[test]
fn invalid_counts_are_rejected() {
    let project = Project::new(UNITS);
    for flag in ["--jobs", "--max-success"] {
        let output = project.run("test", &[flag, "0"]);
        assert_eq!(output.status.code(), Some(2));
    }
}

#[test]
fn tracing_defaults_overrides_and_reserved_labels() {
    let source = "module Main exposing (..)\n\ntests\n    import Test exposing (label)\n    test \"trace\" = do\n        trace \"hello\" ()\n        label \"seen\"\n";
    let project = Project::new(source);
    let (output, verbose) = project.json(&[]);
    success(&output);
    assert_eq!(verbose["tests"][0]["traces"], serde_json::json!(["hello"]));
    let (output, overridden_silent) = project.json(&["--trace-level", "silent"]);
    success(&output);
    assert_eq!(
        overridden_silent["tests"][0]["traces"],
        serde_json::json!([])
    );
    assert_eq!(overridden_silent["tests"][0]["labels"]["seen"], 1);
    let config_path = project.0.join("nash.jsonc");
    let mut config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
    config["traceLevel"] = "silent".into();
    std::fs::write(config_path, config.to_string()).unwrap();
    let (output, silent) = project.json(&[]);
    success(&output);
    assert_eq!(silent["tests"][0]["traces"], serde_json::json!([]));
    assert_eq!(silent["tests"][0]["labels"]["seen"], 1);
    let (output, overridden) = project.json(&["--trace-level", "verbose"]);
    success(&output);
    assert_eq!(
        overridden["tests"][0]["traces"],
        serde_json::json!(["hello"])
    );
}

#[test]
fn properties_run_on_each_ledger_target() {
    let project = Project::new(PROPS);
    for target in ["v1", "v2", "v3"] {
        let (output, report) = project.json(&["--plutus-version", target]);
        success(&output);
        let tests = report["tests"].as_array().unwrap();
        assert_eq!(tests.len(), 3);
        assert!(tests.iter().all(|test| test["status"] == "pass"));
    }
}

#[test]
fn zero_capture_assertion_has_source_report() {
    let source = "module Main exposing (..)\nimport Builtin exposing (..)\n\ntests\n    test \"constant\" = do\n        assert False\n";
    let project = Project::new(source);
    let (output, report) = project.json(&["--trace-level", "silent"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(report["tests"][0]["assert"]["source"], "False");
    assert_eq!(
        report["tests"][0]["assert"]["values"],
        serde_json::json!([])
    );
}

#[test]
fn filtering_skips_codegen_for_unselected_properties() {
    let source = format!("{PROPS}    test \"unit\" = do\n        ()\n");
    let project = Project::new(&source);
    let (output, report) = project.json(&["--plutus-version", "v1", "--match", "unit", "--exact"]);
    success(&output);
    assert_eq!(report["tests"].as_array().unwrap().len(), 1);
    assert_eq!(report["tests"][0]["name"], "unit");
}

#[test]
fn workspace_test_settings_and_cli_override() {
    let project = Project::new("");
    project.write("nash.jsonc", &serde_json::json!({"type":"workspace","members":["a","b",PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../core").canonicalize().unwrap()]}).to_string());
    project.write(
        "a/nash.jsonc",
        r#"{"type":"application","plutusVersion":"v1","traceLevel":"silent"}"#,
    );
    project.write(
        "b/nash.jsonc",
        r#"{"type":"application","plutusVersion":"v3","traceLevel":"verbose"}"#,
    );
    project.write("a/src/Alpha.nash", "module Alpha exposing (..)\nimport Literal\ntests\n    test \"alpha\" = do\n        trace \"alpha\" ()\n");
    project.write("b/src/Beta.nash", "module Beta exposing (..)\nimport Literal\ntests\n    test \"beta\" = do\n        trace \"beta\" ()\n");
    let (output, report) = project.json(&[]);
    success(&output);
    let tests = report["tests"].as_array().unwrap();
    assert_eq!(tests.len(), 2);
    assert_eq!(
        tests.iter().find(|t| t["name"] == "alpha").unwrap()["traces"],
        serde_json::json!([])
    );
    assert_eq!(
        tests.iter().find(|t| t["name"] == "beta").unwrap()["traces"],
        serde_json::json!(["beta"])
    );
    let (output, report) = project.json(&["--trace-level", "silent", "--plutus-version", "v2"]);
    success(&output);
    for test in report["tests"].as_array().unwrap() {
        assert_eq!(test["traces"], serde_json::json!([]));
    }
}

#[test]
fn dependency_tests_are_not_checked_or_executed() {
    let source = "validator module Main exposing (main)\nmain : Data -> unit\nmain _ = ()\ntests\n    import Dep\n    test \"root\" = do\n        Dep.value\n";
    let project = Project::new(source);
    project.write(
        "nash.jsonc",
        r#"{"type":"application","dependencies":{"test/dep":{"path":"dep"}}}"#,
    );
    project.write("dep/nash.jsonc", r#"{"type":"package","name":"test/dep","version":"1.0.0","summary":"","license":"MIT","exposedModules":["Dep"],"testDependencies":{"test/absent":{"path":"missing"}}}"#);
    project.write("dep/src/Dep.nash", "module Dep exposing (value)\nvalue = ()\ntests\n    import Missing\n    test \"dependency\" = do\n        undefinedTestName\n");
    success(&project.run("check", &[]));
    let (output, report) = project.json(&[]);
    success(&output);
    assert_eq!(report["tests"].as_array().unwrap().len(), 1);
    assert_eq!(report["tests"][0]["name"], "root");
    success(&project.run("build", &[]));
    assert!(project.0.join("build/Main.uplc").exists());
}
