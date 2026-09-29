use nash_driver::{Database, InMemorySource, build_graph_with_tests, bundled_base, test_with};
use std::sync::Arc;
use tokio::sync::Mutex;
use url::Url;

async fn run(source: &str, trace: nash_config::TraceLevel) -> String {
    let main = Url::parse("file:///app/src/Main.nash").unwrap();
    let memory = InMemorySource::new();
    memory.insert(main.clone(), source.into());
    let db = Arc::new(Mutex::new(Database::new(memory)));
    let mut modules = bundled_base::modules();
    modules.insert(main.clone(), None);
    let graph = build_graph_with_tests(
        db.clone(),
        &modules.keys().cloned().collect::<Vec<_>>(),
        std::slice::from_ref(&main),
    )
    .await
    .unwrap();
    let (report, programs) = test_with(db, &graph, &modules, move |solved| {
        nash_driver::build::compile_tests_with(solved, |uri| {
            (uri == &main).then_some(nash_config::Build {
                trace_level: trace,
                trace_level_explicit: true,
                ..Default::default()
            })
        })
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
    let programs = programs.unwrap().unwrap();
    assert!(!programs.is_empty());
    let results = nash_test::run_all(programs, &nash_test::Config::default());
    let mut output = String::new();
    for outcome in results {
        assert_eq!(
            outcome.status,
            nash_test::Status::Pass,
            "{}: {:?}",
            outcome.test.name,
            outcome.traces
        );
        output.push_str(&format!(
            "{}: {:?}; traces: {:?}\n",
            outcome.test.name, outcome.status, outcome.traces
        ));
        if let Some(values) = &outcome.counterexample {
            output.push_str(&format!(
                "counterexample: {values:?}\nreplay: {:?}\n",
                outcome.replay
            ));
        }
    }
    output
}

macro_rules! base_snapshot {
    ($name:ident, $file:literal) => {
        #[tokio::test]
        async fn $name() {
            let source = include_str!($file);
            let output = run(source, nash_config::TraceLevel::Verbose).await;
            insta::with_settings!({description => source, omit_expression => true}, {
                insta::assert_snapshot!(output);
            });
        }
    };
}

base_snapshot!(equality_and_ordering, "fixtures/base-traits/Equality.nash");
base_snapshot!(
    numeric_and_literal_traits,
    "fixtures/base-traits/Numeric.nash"
);
base_snapshot!(show_formats, "fixtures/base-traits/Show.nash");
base_snapshot!(semigroup_and_monoid, "fixtures/base-traits/Monoid.nash");
base_snapshot!(
    functor_applicative_monad,
    "fixtures/base-traits/Higher.nash"
);
base_snapshot!(lift_and_data, "fixtures/base-traits/Data.nash");
base_snapshot!(
    prelude_and_debug_syntax,
    "fixtures/base-traits/Prelude.nash"
);

#[tokio::test]
async fn debug_syntax_silent() {
    let source = include_str!("fixtures/base-traits/Prelude.nash");
    let output = run(source, nash_config::TraceLevel::Silent).await;
    insta::with_settings!({description => source, omit_expression => true}, {
        insta::assert_snapshot!(output);
    });
}

base_snapshot!(list_helpers, "fixtures/base-traits/Lists.nash");
base_snapshot!(type_helpers, "fixtures/base-traits/TypeHelpers.nash");
base_snapshot!(
    boolean_and_unit_literals,
    "fixtures/base-traits/BooleanUnitLiterals.nash"
);

base_snapshot!(generate_bounds, "fixtures/base-traits/PropBounds.nash");

base_snapshot!(
    data_conversions,
    "fixtures/base-traits/DataConversions.nash"
);
base_snapshot!(map_helpers, "fixtures/base-traits/Maps.nash");
base_snapshot!(representation_classes, "fixtures/base-traits/Classed.nash");

base_snapshot!(prop_helpers, "fixtures/base-traits/PropHelpers.nash");
base_snapshot!(cardano_value, "fixtures/base-traits/CardanoValue.nash");
base_snapshot!(cardano_helpers, "fixtures/base-traits/Cardano.nash");

base_snapshot!(integer_math, "fixtures/base-traits/IntegerMath.nash");
base_snapshot!(rational_math, "fixtures/base-traits/Rational.nash");
base_snapshot!(crypto_helpers, "fixtures/base-traits/Crypto.nash");

#[tokio::test]
async fn optimization_levels_preserve_test_and_property_traces() {
    let memory = InMemorySource::new();
    let mut modules = bundled_base::modules();
    let mut roots = Vec::new();
    for name in ["Baseline", "Optimized"] {
        let uri = Url::parse(&format!("file:///app/src/{name}.nash")).unwrap();
        memory.insert(
            uri.clone(),
            format!(
                r#"module {name} exposing (..)
import Prop exposing (constant)
coldFailure : unit -> bool
coldFailure _ = fail
tests
    test "pass" = do
        trace "before" ()
        assert (True || coldFailure ())
    test "failure" fail = do
        trace "before failure" ()
        assert False
    prop "property" =
        let
            x via trace "generator" (constant 7)
        in
        do
            trace "body" ()
            assert (x == 7)
"#
            ),
        );
        modules.insert(uri.clone(), None);
        roots.push(uri);
    }
    let db = Arc::new(Mutex::new(Database::new(memory)));
    let graph = build_graph_with_tests(
        db.clone(),
        &modules.keys().cloned().collect::<Vec<_>>(),
        &roots,
    )
    .await
    .unwrap();
    let (report, programs) = test_with(db, &graph, &modules, move |solved| {
        nash_driver::build::compile_tests_with(solved, |uri| {
            roots.contains(uri).then_some(nash_config::Build {
                optimize: if uri.path().ends_with("Baseline.nash") {
                    nash_config::OptimizationLevel::O0
                } else {
                    nash_config::OptimizationLevel::O1
                },
                trace_level: nash_config::TraceLevel::Verbose,
                trace_level_explicit: true,
                ..Default::default()
            })
        })
    })
    .await;
    assert!(report.is_success(), "{report:?}");
    let results = nash_test::run_all(programs.unwrap().unwrap(), &nash_test::Config::default());
    let render = |module: &str| {
        results
            .iter()
            .filter(|o| o.test.module == module)
            .map(|o| {
                format!(
                    "{}: {:?}; traces: {:?}; counterexample: {:?}",
                    o.test.name, o.status, o.traces, o.counterexample
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let before = render("Baseline");
    let after = render("Optimized");
    insta::assert_snapshot!(format!("--- O0\n{before}\n--- O1\n{after}"));
    assert_eq!(before, after);
}
