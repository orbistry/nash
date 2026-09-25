use nash_driver::{Database, InMemorySource, build_graph, build_with, bundled_base};
use std::sync::Arc;
use tokio::sync::Mutex;
use url::Url;

pub async fn compile_validator(source: &str) -> nash_driver::build::ValidatorOutput {
    let memory = InMemorySource::new();
    let uri = Url::parse("file:///project/src/Main.nash").unwrap();
    memory.insert(uri.clone(), source.into());
    let mut origins = bundled_base::modules();
    origins.insert(uri, None);
    let db = Arc::new(Mutex::new(Database::new(memory)));
    let graph = build_graph(db.clone(), &origins.keys().cloned().collect::<Vec<_>>())
        .await
        .unwrap();
    let (report, result) = build_with(db, &graph, &origins, |solved| {
        nash_driver::build::build_validators(solved, nash_config::Build::default())
    })
    .await;
    assert!(
        report.is_success(),
        "{}",
        report
            .ordered_reports()
            .iter()
            .map(|reports| {
                let view = nash_report::Source::new(&reports.source);
                reports
                    .reports
                    .iter()
                    .map(|report| nash_report::render_plain(report, &view, &reports.path))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .collect::<Vec<_>>()
            .join("\n")
    );
    result.unwrap().unwrap().remove(0)
}
