use nash_driver::{Database, FileSystemSource, Project};
use std::path::{Path, PathBuf};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        Self(std::env::temp_dir().join(format!(
            "nash-driver-tests-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        )))
    }
    fn write(&self, path: &str, source: &str) {
        let path = self.0.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, source).unwrap();
    }
    fn root(&self) -> &Path {
        &self.0
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn path_test_dependencies_are_scoped_and_dependency_tests_are_not_discovered() {
    let fixture = Fixture::new();
    fixture.write(
        "app/nash.jsonc",
        r#"{"type":"application","testDependencies":{"sample/helpers":{"path":"../helpers"}}}"#,
    );
    fixture.write("helpers/nash.jsonc", r#"{"type":"package","name":"sample/helpers","version":"1.0.0","summary":"Helpers","license":"MIT","exposedModules":["Helper"],"testDependencies":{"sample/missing":{"path":"../missing"}}}"#);
    fixture.write(
        "helpers/src/Helper.nash",
        "module Helper exposing (identity)\nidentity x = x\n",
    );
    fixture.write("app/src/Main.nash", "module Main exposing (..)\ntests\n    import Helper\n    test \"works\" = do\n        Helper.identity ()\n");
    let project = Project::load(fixture.root().join("app")).await.unwrap();
    let db = Database::new(FileSystemSource::new());
    let production = project.discover_modules_production(&db).await.unwrap();
    assert_eq!(
        production.len(),
        1 + nash_driver::bundled_base::SOURCES.len()
    );
    let testing = project.discover_modules(&db).await.unwrap();
    assert_eq!(testing.len(), 2 + nash_driver::bundled_base::SOURCES.len());
    assert_eq!(
        testing
            .values()
            .filter_map(Option::as_ref)
            .next()
            .unwrap()
            .to_string(),
        "sample/helpers"
    );
    fixture.write(
        "app/src/Main.nash",
        "module Main exposing (..)\nimport Helper\nvalue = Helper.identity ()\n",
    );
    let error = project.discover_modules(&db).await.unwrap_err();
    assert!(
        error.to_string().contains("outside its tests block"),
        "{error}"
    );
}

#[tokio::test]
async fn workspace_test_dependency_paths_are_relative_to_workspace_root() {
    let fixture = Fixture::new();
    fixture.write("nash.jsonc", r#"{"type":"workspace","members":["app"],"dependencies":{"sample/helpers":{"path":"helpers"}}}"#);
    fixture.write(
        "app/nash.jsonc",
        r#"{"type":"application","testDependencies":{"sample/helpers":{"workspace":true}}}"#,
    );
    fixture.write("app/src/Main.nash", "module Main exposing (..)\ntests\n    import Helper\n    test \"works\" = do\n        Helper.identity ()\n");
    fixture.write("helpers/nash.jsonc", r#"{"type":"package","name":"sample/helpers","version":"1.0.0","summary":"Helpers","license":"MIT","exposedModules":["Helper"]}"#);
    fixture.write(
        "helpers/src/Helper.nash",
        "module Helper exposing (identity)\nidentity x = x\n",
    );
    let project = Project::load(fixture.root()).await.unwrap();
    let db = Database::new(FileSystemSource::new());
    assert_eq!(project.discover_own_modules(&db).await.unwrap().len(), 1);
    assert_eq!(
        project
            .discover_modules_production(&db)
            .await
            .unwrap()
            .len(),
        1 + nash_driver::bundled_base::SOURCES.len()
    );
    assert_eq!(
        project.discover_modules(&db).await.unwrap().len(),
        2 + nash_driver::bundled_base::SOURCES.len()
    );
}

#[tokio::test]
async fn unavailable_test_dependency_reports_resolution_error_only_for_checks() {
    let fixture = Fixture::new();
    fixture.write(
        "nash.jsonc",
        r#"{"type":"application","testDependencies":{"sample/helpers":"1.0.0 <= v < 2.0.0"}}"#,
    );
    fixture.write("src/Main.nash", "module Main exposing (..)\nvalue = ()\n");
    let project = Project::load(fixture.root()).await.unwrap();
    let db = Database::new(FileSystemSource::new());
    assert_eq!(
        project
            .discover_modules_production(&db)
            .await
            .unwrap()
            .len(),
        1 + nash_driver::bundled_base::SOURCES.len()
    );
    let error = project.discover_modules(&db).await.unwrap_err().to_string();
    assert!(
        error.contains("sample/helpers") && error.contains("local path"),
        "{error}"
    );
}
