use bumpalo::Bump;
use nash_frontend::{Frontend, ModuleName, ModuleRole, Severity, SourceInput};
use nash_frontend_nash::NashFrontend;
use nash_region::Position;
use nash_source::ModuleKind;
use url::Url;

#[test]
fn native_module_and_owned_dependencies_share_module_identity() {
    let uri = Url::parse("file:///src/Example/Main.nash").unwrap();
    let name = ModuleName::new("Example.Main");
    let inspected = {
        let source = String::from(
            "module Example.Main exposing (identity)\nimport Example.Helper\nidentity x = x\n",
        );
        let input = SourceInput {
            source: &source,
            uri: &uri,
            expected_module: &name,
            role: None,
            origin: nash_frontend::SourceOrigin::Root,
        };
        let arena = Bump::new();
        let parsed = NashFrontend.parse(&arena, input).unwrap();
        assert_eq!(parsed.module.name.unwrap().value, "Example.Main");
        assert_eq!(parsed.module.values[0].value.name.value, "identity");
        NashFrontend.inspect(input).unwrap()
    };
    assert_eq!(inspected.dependencies.len(), 1);
    assert_eq!(inspected.dependencies[0].module.as_str(), "Example.Helper");
    assert_eq!(inspected.dependencies[0].region.start, Position::new(2, 8));
}

#[test]
fn missing_header_reports_required_project_module_name() {
    let uri = Url::parse("file:///src/Example/Main.nash").unwrap();
    let name = ModuleName::new("Example.Main");
    let failure = NashFrontend
        .inspect(SourceInput {
            source: "identity x = x\n",
            uri: &uri,
            expected_module: &name,
            role: None,
            origin: nash_frontend::SourceOrigin::Root,
        })
        .unwrap_err();
    let diagnostic = failure.diagnostics().next().unwrap();
    assert_eq!(diagnostic.code, "nash::syntax::missing_module_name");
    assert!(
        diagnostic
            .help
            .iter()
            .any(|help| help.contains("Example.Main"))
    );
    assert!(diagnostic.primary_label.is_none());
}

#[test]
fn header_mismatch_uses_full_name_and_suggests_project_identity() {
    let uri = Url::parse("file:///src/Example/Main.nash").unwrap();
    let name = ModuleName::new("Example.Main");
    let failure = NashFrontend
        .inspect(SourceInput {
            source: "module Other.Main exposing (..)\nidentity x = x\n",
            uri: &uri,
            expected_module: &name,
            role: None,
            origin: nash_frontend::SourceOrigin::Root,
        })
        .unwrap_err();
    let diagnostic = failure.diagnostics().next().unwrap();
    assert_eq!(diagnostic.code, "nash::syntax::module_name_mismatch");
    assert_eq!(diagnostic.suggestions, ["Example.Main"]);
    let region = diagnostic.region.unwrap();
    assert_eq!(region.start, Position::new(1, 8));
    assert_eq!(region.end, Position::new(1, 18));
}

#[test]
fn syntax_failure_retains_opening_and_boundary_after_source_is_dropped() {
    let failure = {
        let uri = Url::parse("file:///src/Main.nash").unwrap();
        let name = ModuleName::new("Main");
        let source = String::from("module Main exposing (..)\nvalue = \"abc");
        NashFrontend
            .inspect(SourceInput {
                source: &source,
                uri: &uri,
                expected_module: &name,
                role: None,
                origin: nash_frontend::SourceOrigin::Root,
            })
            .unwrap_err()
    };
    let diagnostic = failure.diagnostics().next().unwrap();
    assert_eq!(diagnostic.code, "nash::syntax::unclosed_delimiter");
    assert_eq!(diagnostic.severity, Severity::Error);
    assert_eq!(diagnostic.region.unwrap().start, Position::new(2, 13));
    assert_eq!(diagnostic.labels.len(), 1);
    assert_eq!(diagnostic.labels[0].region.start, Position::new(2, 9));
    assert!(diagnostic.primary_label.as_ref().unwrap().contains('"'));
    assert!(diagnostic.help.iter().any(|help| help.contains('"')));
}

#[test]
fn native_validator_header_is_authoritative_without_explicit_role() {
    let uri = Url::parse("file:///src/Main.nash").unwrap();
    let name = ModuleName::new("Main");
    let input = SourceInput {
        source: "validator module Main exposing (main)\nmain x = x\n",
        uri: &uri,
        expected_module: &name,
        role: None,
        origin: nash_frontend::SourceOrigin::Root,
    };
    let arena = Bump::new();
    let parsed = NashFrontend.parse(&arena, input).unwrap();
    assert!(matches!(parsed.module.kind, ModuleKind::Validator(_)));
    let failure = NashFrontend
        .inspect(SourceInput {
            role: Some(ModuleRole::Library),
            ..input
        })
        .unwrap_err();
    let diagnostic = failure.diagnostics().next().unwrap();
    assert_eq!(diagnostic.code, "nash::syntax::module_role_mismatch");
    assert_eq!(diagnostic.region.unwrap().start, Position::new(1, 1));

    let failure = NashFrontend
        .inspect(SourceInput {
            source: "module Main exposing (main)\nmain x = x\n",
            role: Some(ModuleRole::Validator),
            ..input
        })
        .unwrap_err();
    assert_eq!(
        failure.diagnostics().next().unwrap().code,
        "nash::syntax::module_role_mismatch"
    );
}

#[test]
fn project_only_roles_are_rejected_with_a_located_native_diagnostic() {
    let uri = Url::parse("file:///src/Main.nash").unwrap();
    let name = ModuleName::new("Main");
    for role in [ModuleRole::Environment, ModuleRole::Configuration] {
        let failure = NashFrontend
            .inspect(SourceInput {
                source: "module Main exposing (..)\nvalue = ()\n",
                uri: &uri,
                expected_module: &name,
                role: Some(role),
                origin: nash_frontend::SourceOrigin::Root,
            })
            .unwrap_err();
        let diagnostic = failure.diagnostics().next().unwrap();
        assert_eq!(diagnostic.code, "nash::syntax::module_role_mismatch");
        assert_eq!(diagnostic.region.unwrap().start.line, 1);
    }
}
