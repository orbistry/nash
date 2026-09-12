use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

struct Project(PathBuf);

impl Project {
    fn new(source: &[u8]) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "nash-cli-config-diagnostics-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("nash.jsonc"), source).unwrap();
        Self(root.canonicalize().unwrap())
    }

    fn check(&self) -> String {
        let output = Command::new(env!("CARGO_BIN_EXE_nash"))
            .env("NASH_PROXY_VERSION", env!("CARGO_PKG_VERSION"))
            .env("NO_COLOR", "1")
            .env("FORCE_HYPERLINK", "0")
            .args(["check"])
            .arg(&self.0)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        let text = String::from_utf8(output.stderr).unwrap();
        assert!(!text.contains('\x1b'), "{text}");
        text.replace(self.0.to_str().unwrap(), "<project>")
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

macro_rules! config_diagnostic {
    ($name:ident, $source:expr) => {
        #[test]
        fn $name() {
            let source = $source;
            let rendered = Project::new(source.as_bytes()).check();
            insta::with_settings!({
                description => format!("nash.jsonc (complete input):\n{source}"),
                omit_expression => true,
            }, {
                insta::assert_snapshot!(rendered);
            });
        }
    };
}

config_diagnostic!(parse_error, "{");
config_diagnostic!(empty_file, "");
config_diagnostic!(expected_object, "[]");
config_diagnostic!(missing_field, "{}");
config_diagnostic!(expected_string, r#"{"type":1}"#);
config_diagnostic!(invalid_type, r#"{"type":"oops"}"#);
config_diagnostic!(
    expected_array,
    r#"{"type":"application","sourceDirectories":false}"#
);
config_diagnostic!(
    package_name_missing_separator,
    r#"{"type":"application","dependencies":{"name":"1"}}"#
);
config_diagnostic!(
    package_name_invalid_author,
    r#"{"type":"application","dependencies":{"Bad/name":"1"}}"#
);
config_diagnostic!(
    package_name_invalid_project,
    r#"{"type":"application","dependencies":{"author/Bad":"1"}}"#
);
config_diagnostic!(
    expected_dependency,
    r#"{"type":"application","dependencies":{"author/name":1}}"#
);
config_diagnostic!(
    expected_bool,
    r#"{"type":"application","dependencies":{"author/name":{"workspace":1}}}"#
);
config_diagnostic!(
    workspace_must_be_true,
    r#"{"type":"application","dependencies":{"author/name":{"workspace":false}}}"#
);
config_diagnostic!(
    invalid_dependency,
    r#"{"type":"application","dependencies":{"author/name":{}}}"#
);
config_diagnostic!(
    workspace_dep_in_workspace,
    r#"{"type":"workspace","members":[],"dependencies":{"author/name":{"workspace":true}}}"#
);
config_diagnostic!(
    expected_array_or_object,
    r#"{"type":"package","name":"author/name","version":"1.0.0","summary":"test","license":"MIT","exposedModules":false}"#
);

#[test]
fn read_error() {
    // Invalid UTF-8 causes a real read_to_string error without OS-specific
    // permission behavior or error numbers.
    let rendered = Project::new(&[0xff]).check();
    insta::with_settings!({
        description => "nash.jsonc bytes (hex): ff",
        omit_expression => true,
    }, {
        insta::assert_snapshot!(rendered);
    });
}
