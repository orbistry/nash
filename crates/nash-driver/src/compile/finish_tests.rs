use super::*;

#[test]
fn finish_receives_original_solved_nodes_and_tables() {
    let uri = Url::parse("file:///project/src/Main.nash").unwrap();
    let (report, output) = build_sync_with_edges_and(
        vec![(
            uri.clone(),
            None,
            Ok(
                "module Main exposing (..)\ntrait Keep 'a where\n    keep : 'a -> 'a\nid x = x\n"
                    .into(),
            ),
        )],
        &HashMap::new(),
        |solved| {
            assert_eq!(solved.modules.len(), 1);
            let module = &solved.modules[0];
            assert!(!module.types.exprs.is_empty());
            assert!(!module.types.patterns.is_empty());
            assert!(module.tables.traits.keys().any(|name| name.name == "Keep"));
            (module.uri.clone(), module.module.name.name.to_owned())
        },
    );
    assert!(report.is_success());
    assert_eq!(output, Some((uri, "Main".into())));
}

#[test]
fn failed_frontend_never_calls_finish() {
    let uri = Url::parse("file:///project/src/Main.nash").unwrap();
    let (report, output) = build_sync_with_edges_and(
        vec![(
            uri,
            None,
            Ok("module Main exposing (..)\nmain = unknown\n".into()),
        )],
        &HashMap::new(),
        |_| panic!("failed build must not generate code"),
    );
    assert!(!report.is_success());
    assert!(output.is_none());
}
