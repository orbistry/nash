//! Deep input compiles on the compiler stack.

use nash_driver::{Database, InMemorySource, build, build_graph};
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::Mutex;
use url::Url;

/// 4,000 levels overflow a 2 MiB thread in a debug build, in the import scan
/// and again in the compile.
#[tokio::test]
async fn nested_parentheses_have_no_limit() {
    let depth = 4_000;
    let source = format!(
        "module Main exposing (..)\nvalue : int\nvalue = {}1{}\n",
        "(".repeat(depth),
        ")".repeat(depth)
    );
    let files = InMemorySource::new();
    let uri = Url::parse("file:///project/src/Main.nash").unwrap();
    files.insert(uri.clone(), source);
    let mut modules = BTreeMap::from([(uri, None)]);
    modules.extend(nash_driver::bundled_base::modules());
    let db = Arc::new(Mutex::new(Database::new(files)));
    let graph = build_graph(db.clone(), &modules.keys().cloned().collect::<Vec<_>>())
        .await
        .unwrap();
    let report = build(db, &graph, &modules).await;
    assert!(report.is_success(), "{:?}", report.ordered_reports());
}
