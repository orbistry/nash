use bumpalo::Bump;
use nash_frontend::{Frontend, ModuleName, ModuleRole, SourceInput};
use nash_frontend_aiken::AikenFrontend;
use nash_region::{Position, Region};
use url::Url;

fn failure(source: &str, role: Option<ModuleRole>) -> nash_frontend::FrontendFailure {
    let arena = Bump::new();
    let uri = Url::parse("file:///math.ak").unwrap();
    let name = ModuleName::new("math");
    AikenFrontend
        .parse(
            &arena,
            SourceInput {
                source,
                uri: &uri,
                expected_module: &name,
                role,
                origin: nash_frontend::SourceOrigin::Root,
            },
        )
        .err()
        .expect("invalid source must fail")
}

#[test]
fn diagnostics_use_utf8_byte_columns_after_crlf() {
    let source =
        "use aiken/builtin\r\npub fn invalid() { trace @\"π\" builtin.unsupported_builtin(1) }\r\n";
    let diagnostic = failure(source, None).first;
    let line = source.lines().nth(1).unwrap();
    let name = "builtin.unsupported_builtin";
    let start = line.find(name).unwrap() + 1;
    assert_eq!(
        diagnostic.region,
        Some(Region::new(
            Position::new(2, start),
            Position::new(2, start + name.len()),
        )),
    );
}

#[test]
fn unknown_builtins_and_syntax_errors_have_located_diagnostics() {
    for source in [
        "use aiken/builtin.{unsupported_builtin}\npub fn identity(x) { x }",
        "use aiken/builtin\npub fn unsupported(x) { builtin.unsupported_builtin(x) }",
    ] {
        let failure = failure(source, None);
        assert_eq!(failure.first.code, "NAF2201", "{source}: {failure:?}");
        assert!(
            failure
                .first
                .region
                .is_some_and(|region| region.start.line >= 1 && region.start.column >= 1)
        );
    }
    assert_eq!(failure("pub fn broken(", None).first.code, "NAF2001");
}

#[test]
fn invalid_layouts_have_located_diagnostics() {
    for source in [
        "@list pub type Bad { First Second }",
        "pub type Bad { @tag(1) First Second }",
        "@list @tag(2) pub type Bad { Bad(Int) }",
        "pub type Bad { @list Bad(Int) }",
    ] {
        let error = failure(source, None);
        assert_eq!(error.first.code, "NAF2401");
        assert!(error.first.region.is_some());
    }
}
