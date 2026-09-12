use super::*;
use miette::{GraphicalReportHandler, GraphicalTheme};

fn uri() -> Url {
    Url::parse("file:///project/src/Main.nash").unwrap()
}

macro_rules! driver_error_snapshot {
    ($name:ident, $fixture:expr, $error:expr) => {
        #[test]
        fn $name() {
            let mut rendered = String::new();
            GraphicalReportHandler::new_themed(GraphicalTheme::unicode_nocolor())
                .with_width(80)
                .render_report(&mut rendered, &$error)
                .unwrap();
            assert!(!rendered.contains('\x1b'));
            assert!(rendered.contains('×'));
            insta::with_settings!({ description => $fixture, omit_expression => true }, {
                insta::assert_snapshot!(rendered);
            });
        }
    };
}

driver_error_snapshot!(
    conflicting_module_owners,
    "Workspace ownership: packages first/lib and second/lib both own /project/src/Main.nash.",
    DriverError::ConflictingModuleOwners {
        uri: Box::new(uri()),
        first: "first/lib".into(),
        second: "second/lib".into(),
    }
);
driver_error_snapshot!(
    file_not_found,
    "Source lookup: /project/src/Main.nash does not exist.",
    DriverError::FileNotFound { uri: uri() }
);
driver_error_snapshot!(
    read_error,
    "Source read: /project/src/Main.nash returns a deterministic NotFound error.",
    DriverError::ReadError {
        path: "/project/src/Main.nash".into(),
        source: std::io::Error::new(std::io::ErrorKind::NotFound, "fixture file missing"),
    }
);
driver_error_snapshot!(
    write_error,
    "Artifact write: /project/build/Main.flat returns a deterministic PermissionDenied error.",
    DriverError::WriteError {
        path: "/project/build/Main.flat".into(),
        source: std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "fixture directory is read-only"
        ),
    }
);
driver_error_snapshot!(
    invalid_file_uri,
    "Source lookup: an HTTPS URL was supplied where a file URI was required.",
    DriverError::InvalidFileUri {
        uri: Url::parse("https://example.test/Main.nash").unwrap()
    }
);
driver_error_snapshot!(
    config_error,
    "nash.jsonc:\n\n{\n\nConfiguration parser returned: unclosed object.",
    DriverError::ConfigError(nash_config::ConfigError::ParseError {
        path: "/project/nash.jsonc".into(),
        message: "unclosed object".into(),
    })
);
driver_error_snapshot!(
    project_not_found,
    "Project discovery: /project and its parents contain no nash.jsonc.",
    DriverError::ProjectNotFound {
        path: "/project".into()
    }
);
driver_error_snapshot!(
    member_not_found,
    "nash.jsonc:\n\n{\"type\":\"workspace\",\"members\":[\"packages/missing\"]}",
    DriverError::MemberNotFound {
        pattern: "packages/missing".into()
    }
);
driver_error_snapshot!(
    import_cycle,
    "A.nash:\n\nmodule A exposing (..)\nimport B\n\nB.nash:\n\nmodule B exposing (..)\nimport A\n",
    DriverError::ImportCycle {
        cycle: "A -> B -> A".into()
    }
);
driver_error_snapshot!(
    module_not_found,
    "Main.nash:\n\nmodule Main exposing (..)\nimport Missing\n",
    DriverError::ModuleNotFound {
        module: "Missing".into()
    }
);
driver_error_snapshot!(
    invalid_module_path,
    "Module discovery: source path /project/src is a directory rather than a Nash source file.",
    DriverError::InvalidModulePath {
        path: "/project/src".into()
    }
);
