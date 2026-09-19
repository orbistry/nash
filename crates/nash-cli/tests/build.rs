use nash_plutus::{arena::Arena, data::PlutusData, syn, term::Term};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

struct Project(PathBuf);
impl Project {
    fn new(config: &str, source: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "nash-build-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let project = Self(root);
        project.write("nash.jsonc", config);
        project.write("src/Main.nash", source);
        project
    }
    fn traced(config: &str) -> Self {
        let project = Self::new(r#"{"type":"workspace","members":["core","app"]}"#, "");
        project.write("app/nash.jsonc", config);
        project.write("app/src/Main.nash", TRACED);
        project.write("core/nash.jsonc", r#"{"type":"package","name":"nash/core","version":"0.1.0","summary":"core","license":"Apache-2.0","exposedModules":["Literal"]}"#);
        project.write("core/src/Literal.nash", LITERAL);
        project.write("core/src/Check.nash", CHECK);
        project
    }
    fn write(&self, path: &str, contents: &str) {
        let path = self.0.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
    fn build(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_nash"))
            .env("NASH_PROXY_VERSION", env!("CARGO_PKG_VERSION"))
            .env("NO_COLOR", "1")
            .env("FORCE_HYPERLINK", "0")
            .arg("build")
            .arg(&self.0)
            .args(args)
            .output()
            .unwrap()
    }
    fn read(&self, path: &str) -> String {
        std::fs::read_to_string(self.0.join(path)).unwrap()
    }
    fn artifacts(&self) -> BTreeMap<String, Vec<u8>> {
        std::fs::read_dir(self.0.join("build"))
            .unwrap()
            .map(|entry| {
                let path = entry.unwrap().path();
                (
                    path.file_name().unwrap().to_str().unwrap().to_owned(),
                    std::fs::read(path).unwrap(),
                )
            })
            .collect()
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn build_hash(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr)
        .lines()
        .find_map(|line| line.trim().strip_prefix("hash ").map(str::to_owned))
        .expect("build reports a script hash")
}
fn logs(source: &str) -> Vec<String> {
    let arena = Arena::new();
    let named = syn::parse_program(&arena, source).unwrap();
    let program = named.apply(
        &arena,
        Term::data(&arena, PlutusData::constr(&arena, 2, &[])),
    );
    program.eval(&arena).info.logs
}
const UNIT: &str = "validator module Main exposing (main)\nmain : Data -> unit\nmain _ = ()\n";
const LITERAL: &str = include_str!("../../../core/src/Literal.nash");
const CHECK: &str = "module Check exposing (Expected, validate)\nimport Builtin\nimport Literal exposing (..)\ntype Expected = First | Second\nvalidate : Expected -> Int\nvalidate value =\n    case value of\n        First -> Builtin.iData 0\n        Second -> Builtin.iData 1\n";
const TRACED: &str = "validator module Main exposing (main)\nimport Builtin exposing (..)\nimport Literal exposing (..)\nimport Check\nmain : Check.Expected -> Int\nmain d = trace \"user trace\" (Check.validate d)\n";

#[test]
fn configuration_and_cli_overrides_are_independent() {
    let config = r#"{"type":"application","plutusVersion":"v2","traceLevel":"verbose","compilerTraces":true}"#;
    let project = Project::traced(config);
    let built = project.build(&[]);
    success(&built);
    let configured = project.read("build/Main.uplc");
    assert!(configured.starts_with("(program 1.1.0"));
    let configured_logs = logs(&configured);
    assert_eq!(configured_logs.len(), 2, "{configured_logs:?}");
    assert_eq!(configured_logs[0], "user trace");
    assert!(String::from_utf8_lossy(&built.stderr).contains("hash "));

    success(&project.build(&["--trace-level", "silent"]));
    let compiler_only = project.read("build/Main.uplc");
    assert_eq!(logs(&compiler_only), configured_logs[1..]);
    success(&project.build(&["--compiler-traces=false"]));
    let user_only = project.read("build/Main.uplc");
    assert_eq!(logs(&user_only), ["user trace"]);
    success(&project.build(&[
        "--plutus-version",
        "v3",
        "--trace-level",
        "compact",
        "--compiler-traces=false",
    ]));
    let compact = project.read("build/Main.uplc");
    assert!(compact.starts_with("(program 1.1.0"));
    let compact_logs = logs(&compact);
    assert_eq!(compact_logs.len(), 1);
    assert_ne!(compact_logs[0], "user trace");
    success(&project.build(&["--trace-level", "silent", "--compiler-traces=false"]));
    let silent = project.read("build/Main.uplc");
    assert!(logs(&silent).is_empty());
    insta::with_settings!({ description => format!("{config}\n\n{LITERAL}\n{CHECK}\n{TRACED}"), omit_expression => true }, {
        insta::assert_snapshot!(format!("--- configured\n{configured}\n--- compiler only\n{compiler_only}\n--- user only\n{user_only}\n--- compact V3\n{compact}\n--- silent\n{silent}"));
    });
}

#[test]
fn defaults_and_bare_compiler_trace_flag() {
    let project = Project::traced(r#"{"type":"application"}"#);
    success(&project.build(&[]));
    let default = project.read("build/Main.uplc");
    assert!(default.starts_with("(program 1.1.0"));
    assert!(logs(&default).is_empty());
    success(&project.build(&["--compiler-traces"]));
    assert_eq!(logs(&project.read("build/Main.uplc")).len(), 1);
}

#[test]
fn workspace_members_use_their_config_and_cli_overrides() {
    let project = Project::new(r#"{"type":"workspace","members":["one","two"]}"#, "");
    project.write(
        "one/nash.jsonc",
        r#"{"type":"application","plutusVersion":"v1","sourceDirectories":["../shared"]}"#,
    );
    project.write(
        "two/nash.jsonc",
        r#"{"type":"application","plutusVersion":"v3"}"#,
    );
    project.write("shared/First.nash", &UNIT.replace("Main", "First"));
    project.write("two/src/Second.nash", &UNIT.replace("Main", "Second"));
    let configured = project.build(&[]);
    success(&configured);
    let hashes: Vec<_> = String::from_utf8_lossy(&configured.stderr)
        .lines()
        .filter_map(|line| line.trim().strip_prefix("hash ").map(str::to_owned))
        .collect();
    assert_eq!(hashes.len(), 2);
    assert_ne!(hashes[0], hashes[1]);
    assert!(
        project
            .read("build/First.uplc")
            .starts_with("(program 1.1.0")
    );
    assert!(
        project
            .read("build/Second.uplc")
            .starts_with("(program 1.1.0")
    );
    let overridden = project.build(&["--plutus-version", "v2"]);
    success(&overridden);
    let overridden_hashes: Vec<_> = String::from_utf8_lossy(&overridden.stderr)
        .lines()
        .filter_map(|line| line.trim().strip_prefix("hash ").map(str::to_owned))
        .collect();
    assert_eq!(overridden_hashes.len(), 2);
    assert_eq!(overridden_hashes[0], overridden_hashes[1]);
    assert!(hashes.iter().all(|hash| hash != &overridden_hashes[0]));
    for name in ["First", "Second"] {
        assert!(
            project
                .read(&format!("build/{name}.uplc"))
                .starts_with("(program 1.1.0")
        );
    }
}

#[test]
fn package_build_settings_apply() {
    let project = Project::new(
        r#"{"type":"package","name":"test/validators","version":"0.1.0","summary":"validators","license":"MIT","exposedModules":["Main"],"plutusVersion":"v1"}"#,
        UNIT,
    );
    let configured = project.build(&[]);
    success(&configured);
    let cbor = hex::decode(project.read("build/Main.cbor")).unwrap();
    assert_eq!(
        build_hash(&configured),
        hex::encode(nash_plutus::script::script_hash(
            nash_plutus::machine::PlutusVersion::V1,
            &cbor
        ))
    );
    assert!(
        project
            .read("build/Main.uplc")
            .starts_with("(program 1.1.0")
    );
}

#[test]
fn failures_preserve_outputs_and_zero_validators_remove_only_owned_files() {
    let project = Project::new(r#"{"type":"application"}"#, UNIT);
    success(&project.build(&[]));
    project.write("build/unrelated.cbor", "keep me");
    let prior = project.artifacts();
    project.write(
        "src/Main.nash",
        "validator module Main exposing (main)\nmain _ = missing\n",
    );
    assert!(!project.build(&[]).status.success());
    assert_eq!(project.artifacts(), prior);
    let invalid =
        "validator module Main exposing (main)\nmain : Data -> unit\nmain value = value\n";
    project.write("src/Main.nash", invalid);
    let failed = project.build(&["--plutus-version", "v1"]);
    assert!(!failed.status.success());
    assert_eq!(project.artifacts(), prior);
    let message = String::from_utf8(failed.stderr)
        .unwrap()
        .replace(project.0.to_str().unwrap(), "<project>");
    assert!(message.contains("Type mismatch"), "{message}");
    insta::with_settings!({description => invalid, omit_expression => true}, { insta::assert_snapshot!(message); });
    project.write("src/Main.nash", UNIT);
    success(&project.build(&["--plutus-version", "v2"]));
    project.write("src/Main.nash", "module Main exposing (..)\nmain = ()\n");
    success(&project.build(&[]));
    let remaining = project.artifacts();
    assert_eq!(remaining.len(), 2);
    assert_eq!(remaining["unrelated.cbor"], b"keep me");
    assert!(remaining.contains_key(".nash-artifacts"));
}

#[test]
fn ledger_targets_support_native_terms_and_keep_distinct_hashes() {
    let source = "validator module Main exposing (main)\nimport Builtin\ntype wrapped = Wrapped int\nmain : int -> unit\nmain n = case Wrapped n of Wrapped x -> assert (Builtin.equalsInteger n x)\n";
    let project = Project::new(r#"{"type":"application"}"#, source);
    let mut hashes = std::collections::BTreeSet::new();
    let mut bytecode = None;
    for (version, ledger) in [
        ("v1", nash_plutus::machine::PlutusVersion::V1),
        ("v2", nash_plutus::machine::PlutusVersion::V2),
        ("v3", nash_plutus::machine::PlutusVersion::V3),
    ] {
        let output = project.build(&["--plutus-version", version]);
        success(&output);
        let uplc = project.read("build/Main.uplc");
        assert!(uplc.starts_with("(program 1.1.0"));
        assert!(uplc.contains("(constr"));
        assert!(uplc.contains("(case"));
        let arena = Arena::new();
        let program = syn::parse_program(&arena, &uplc).unwrap();
        assert!(
            program
                .apply(&arena, Term::integer_from(&arena, 42))
                .eval_version(&arena, ledger)
                .term
                .is_ok()
        );
        let cbor = hex::decode(project.read("build/Main.cbor")).unwrap();
        if let Some(prior) = &bytecode {
            assert_eq!(&cbor, prior);
        }
        bytecode = Some(cbor);
        hashes.insert(build_hash(&output));
    }
    assert_eq!(hashes.len(), 3);
}

#[test]
fn optimizer_options_are_rejected() {
    let project = Project::new(r#"{"type":"application","optimize":0}"#, UNIT);
    assert!(!project.build(&[]).status.success());
    project.write("nash.jsonc", r#"{"type":"application"}"#);
    assert!(!project.build(&["--optimize", "0"]).status.success());
    assert!(!project.0.join("build").exists());
}

#[test]
fn validator_settings_apply_to_dependency_code() {
    let config = r#"{"type":"application","plutusVersion":"v2","traceLevel":"verbose"}"#;
    let project = Project::traced(config);
    let helper_config = r#"{"type":"package","name":"nash/core","version":"0.1.0","summary":"core","license":"Apache-2.0","exposedModules":["Literal","Helper"],"plutusVersion":"v1","traceLevel":"silent"}"#;
    project.write("core/nash.jsonc", helper_config);
    let helper = "module Helper exposing (run)\nimport Builtin\nimport Literal exposing (..)\nrun : Data -> bytes\nrun d = trace \"helper trace\" (Builtin.serialiseData d)\n";
    let main = "validator module Main exposing (main)\nimport Helper\nmain : Data -> bytes\nmain = Helper.run\n";
    project.write("core/src/Helper.nash", helper);
    project.write("app/src/Main.nash", main);
    success(&project.build(&[]));
    let program = project.read("build/Main.uplc");
    assert!(program.starts_with("(program 1.1.0"));
    assert_eq!(logs(&program), ["helper trace"]);
    let overridden = project.build(&["--plutus-version", "v1"]);
    success(&overridden);
    assert_eq!(project.read("build/Main.uplc"), program);
    success(&project.build(&["--trace-level", "silent"]));
    assert!(logs(&project.read("build/Main.uplc")).is_empty());
    insta::with_settings!({description => format!("{config}\n{helper_config}\n\n{LITERAL}\n{CHECK}\n{helper}\n{main}"), omit_expression => true}, {
        insta::assert_snapshot!(program);
    });
}
