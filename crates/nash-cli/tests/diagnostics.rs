use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

fn check(path: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nash"))
        .env("NASH_PROXY_VERSION", env!("CARGO_PKG_VERSION"))
        .env("NO_COLOR", "1")
        .args(["check"])
        .arg(path)
        .args(args)
        .output()
        .unwrap()
}

struct Project(PathBuf);
impl Project {
    fn new(files: &[(&str, &str)]) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "nash-cli-diagnostics-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(path.join("src")).unwrap();
        std::fs::write(
            path.join("nash.jsonc"),
            r#"{"type":"application","sourceDirectories":["src"]}"#,
        )
        .unwrap();
        for (name, text) in files {
            std::fs::write(path.join("src").join(format!("{name}.nash")), text).unwrap();
        }
        Self(path.canonicalize().unwrap())
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn normalized(text: &[u8], root: &Path) -> String {
    String::from_utf8(text.to_vec())
        .unwrap()
        .replace(root.to_str().unwrap(), "<project>")
}

#[test]
fn terminal_and_json_type_mismatch() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/type-mismatch")
        .canonicalize()
        .unwrap();
    let human = check(&root, &[]);
    assert_eq!(human.status.code(), Some(1));
    assert!(human.stdout.is_empty());
    let text = normalized(&human.stderr, &root);
    assert!(!text.contains('\u{1b}'));
    assert!(text.contains("TYPE MISMATCH"));
    insta::assert_snapshot!("type_mismatch_terminal", text);
    let json = check(&root, &["--report=json"]);
    assert_eq!(json.status.code(), Some(1));
    assert!(
        json.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&json.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(value["errors"][0]["problems"][0]["title"], "TYPE MISMATCH");
    insta::assert_snapshot!(
        "type_mismatch_json",
        normalized(
            serde_json::to_string_pretty(&value).unwrap().as_bytes(),
            &root
        )
    );
}

#[test]
fn warnings_keep_success_exit_and_can_be_hidden() {
    let project = Project::new(&[("Main", "module Main exposing (..)\nf unused = ()\n")]);
    let output = check(&project.0, &["--report=json"]);
    assert!(output.status.success());
    let errors: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(errors["errors"], serde_json::json!([]));
    let warnings: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(warnings["type"], "compile-warnings");
    assert_eq!(
        warnings["errors"][0]["problems"][0]["title"],
        "unused variable"
    );
    let hidden = check(&project.0, &["--report=json", "--no-warnings"]);
    assert!(hidden.status.success());
    assert!(hidden.stderr.is_empty());
}

#[test]
fn independent_errors_are_stable_and_dependents_are_blocked() {
    let project = Project::new(&[
        ("Z", "module Z exposing (..)\nx = missingZ\n"),
        ("A", "module A exposing (..)\nx = missingA\ny = missingB\n"),
        ("Main", "module Main exposing (..)\nimport Z\nx = Z.x\n"),
    ]);
    let first = check(&project.0, &["--report=json"]);
    assert_eq!(first.status.code(), Some(1));
    for _ in 0..3 {
        assert_eq!(check(&project.0, &["--report=json"]).stdout, first.stdout);
    }
    let json: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
    let modules = json["errors"].as_array().unwrap();
    assert_eq!(
        modules
            .iter()
            .map(|module| module["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["A", "Z"]
    );
    assert_eq!(modules[0]["problems"].as_array().unwrap().len(), 2);
    let human = check(&project.0, &[]);
    let text = String::from_utf8(human.stderr).unwrap();
    assert_eq!(text.matches("NAMING ERROR").count(), 3);
    assert!(text.contains("Skipped"));
    assert!(!text.contains("IMPORT PROBLEM"));
}

#[test]
fn forced_color_is_consistent_and_json_has_no_ansi() {
    let project = Project::new(&[("Main", "module Main exposing (..)\nx = missing\n")]);
    let human = check(&project.0, &["--color=always"]);
    assert!(String::from_utf8_lossy(&human.stderr).contains('\u{1b}'));
    let json = check(&project.0, &["--color=always", "--report=json"]);
    assert!(!String::from_utf8_lossy(&json.stdout).contains('\u{1b}'));
    serde_json::from_slice::<serde_json::Value>(&json.stdout).unwrap();
}

#[test]
fn documented_examples_run_through_the_real_core_package() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/diagnostic-examples")
        .canonicalize()
        .unwrap();
    let human = check(&root, &["--no-warnings"]);
    assert_eq!(human.status.code(), Some(1));
    let text = normalized(&human.stderr, &root);
    assert!(text.contains("This `map` call produces:"), "{text}");
    let docs = include_str!("../../../docs/diagnostics.md");
    let names = [
        "TYPE MISMATCH",
        "MISSING IMPL",
        "MISSING PATTERNS",
        "Compilation failed:",
    ];
    for pair in names.windows(2) {
        let start = text.find(&format!("{}\n", pair[0])).unwrap();
        let end = text[start..].find(pair[1]).unwrap() + start;
        let block = text[start..end]
            .trim_end()
            .replace("<project>/app/src/", "src/");
        assert!(docs.contains(&block), "documented output differs:\n{block}");
    }
    insta::assert_snapshot!("documented_examples_terminal", text);
    let output = check(&root, &["--report=json", "--no-warnings"]);
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let modules = json["errors"].as_array().unwrap();
    assert_eq!(
        modules
            .iter()
            .map(|module| (
                module["name"].as_str().unwrap(),
                module["problems"][0]["title"].as_str().unwrap()
            ))
            .collect::<Vec<_>>(),
        [
            ("Ledger", "TYPE MISMATCH"),
            ("Steps", "MISSING IMPL"),
            ("Tag", "MISSING PATTERNS")
        ]
    );
    insta::assert_snapshot!(
        "documented_examples_json",
        normalized(
            serde_json::to_string_pretty(&json).unwrap().as_bytes(),
            &root
        )
    );
}

#[tokio::test]
async fn mixed_errors_match_across_terminal_json_and_lsp() {
    use nash_driver::{Database, FileSystemSource, Project as DriverProject, build, build_graph};
    use std::sync::Arc;
    use tokio::sync::Mutex;
    let header = "module Main exposing (..)\ntrait Round 'a where\n    create : () -> 'a\n    discard : 'a -> ()\ntype higher 'f = Higher ('f ())\nidfa : 'f 'a -> 'f 'a\nidfa x = x\n";
    let definitions = [
        "mismatch : ()\nmismatch = \\x -> x\n",
        "missing = discard ()\n",
        "constraint : 'a -> ()\nconstraint x = discard x\n",
        "ambiguous = discard (create ())\n",
        "kind = idfa (Higher [])\n",
    ];
    for reverse in [false, true] {
        let mut definitions = definitions.to_vec();
        if reverse {
            definitions.reverse();
        }
        let source = format!("{header}{}", definitions.concat());
        let project = Project::new(&[("Main", &source)]);
        let output = check(&project.0, &["--report=json", "--no-warnings"]);
        assert_eq!(output.status.code(), Some(1));
        let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let problems = json["errors"][0]["problems"].as_array().unwrap();
        let mut titles = problems
            .iter()
            .map(|p| p["title"].as_str().unwrap())
            .collect::<Vec<_>>();
        titles.sort();
        assert_eq!(
            titles,
            [
                "AMBIGUOUS TYPE",
                "KIND MISMATCH",
                "MISSING CONSTRAINT",
                "MISSING IMPL",
                "TYPE MISMATCH"
            ]
        );
        let human = check(&project.0, &["--no-warnings"]);
        let text = String::from_utf8(human.stderr).unwrap();
        let mut previous = 0;
        for problem in problems {
            let title = problem["title"].as_str().unwrap();
            assert_eq!(text.matches(&format!("{title}\n")).count(), 1);
            let position = text.find(&format!("{title}\n")).unwrap();
            assert!(position >= previous);
            previous = position;
        }
        let db = Arc::new(Mutex::new(Database::new(FileSystemSource::new())));
        let loaded = DriverProject::load(&project.0).await.unwrap();
        let modules = loaded.discover_modules(&*db.lock().await).await.unwrap();
        let graph = build_graph(db.clone(), &modules.keys().cloned().collect::<Vec<_>>())
            .await
            .unwrap();
        let result = build(db, &graph, &modules).await;
        assert!(result.interfaces.is_empty());
        let reports = result.ordered_reports();
        let module = reports[0];
        let source = nash_report::Source::new(&module.source);
        let uri = format!("file://{}", module.path).parse().unwrap();
        let errors: Vec<_> = module
            .reports
            .iter()
            .filter(|report| report.severity == nash_report::Severity::Error)
            .collect();
        assert_eq!(errors.len(), problems.len());
        for (report, json) in errors.into_iter().zip(problems) {
            assert_eq!(report.title, json["title"]);
            assert_eq!(
                nash_report::json::encode_region(report.region),
                json["region"]
            );
            let location = format!(
                "[{}:{}:{}]",
                module.path, report.region.start.line, report.region.start.column
            );
            let rendered = nash_report::render_plain(report, &source, &module.path);
            assert!(
                rendered.contains(&location),
                "{rendered}\nExpected {location}"
            );
            let lsp = nash_language_server::diagnostics::to_lsp(report, &source, &uri);
            assert_eq!(serde_json::to_value(&lsp).unwrap()["code"], json["title"]);
            assert_eq!(
                u64::from(lsp.range.start.line) + 1,
                json["region"]["start"]["line"]
            );
            assert_eq!(
                u64::from(lsp.range.start.character) + 1,
                json["region"]["start"]["column"]
            );
            assert_eq!(
                u64::from(lsp.range.end.line) + 1,
                json["region"]["end"]["line"]
            );
            assert_eq!(
                u64::from(lsp.range.end.character) + 1,
                json["region"]["end"]["column"]
            );
        }
    }
}

#[test]
fn poisoned_tuple_child_keeps_independent_type_mismatch() {
    let project = Project::new(&[(
        "Main",
        "module Main exposing (..)\nbad : ((), ())\nbad = (().field, \\x -> x)\n",
    )]);
    let json = check(&project.0, &["--report=json", "--no-warnings"]);
    assert_eq!(json.status.code(), Some(1));
    let value: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    let problems = value["errors"][0]["problems"].as_array().unwrap();
    assert_eq!(problems.len(), 2, "{value:#}");
    let human = check(&project.0, &["--no-warnings"]);
    assert_eq!(human.status.code(), Some(1));
    insta::assert_snapshot!(
        "poisoned_tuple_terminal",
        normalized(&human.stderr, &project.0)
    );
    insta::assert_snapshot!(
        "poisoned_tuple_json",
        normalized(
            serde_json::to_string_pretty(&value).unwrap().as_bytes(),
            &project.0
        )
    );
}
