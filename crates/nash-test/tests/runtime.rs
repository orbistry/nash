use nash_plutus::{
    arena::Arena,
    binder::DeBruijn,
    constant::Constant,
    flat,
    program::{Program, Version},
    term::Term,
    typ::Type,
};
use nash_region::{Position, Region};
use nash_test::{
    eval::split_logs,
    prng::Prng,
    shrink::{Cache, Counterexample, Status as ShrinkStatus},
    *,
};
use std::cell::Cell;

fn encode(arena: &Arena, term: &Term<'_, DeBruijn>) -> Vec<u8> {
    flat::encode(Program::new(arena, Version::plutus_v3(arena), term)).unwrap()
}
fn fixture(programs: Programs, expect: Expect) -> TestProgram {
    TestProgram {
        module: "Example".into(),
        name: "fixture".into(),
        expect,
        budget: None,
        region: Region::one(),
        programs,
        asserts: vec![],
        binder_texts: vec!["x".into()],
        plutus_version: PlutusVersion::V3,
        source: "x == 2".into(),
        source_path: "Example.nash".into(),
    }
}
fn unit(error: bool, expect: Expect) -> TestProgram {
    let arena = Arena::new();
    fixture(
        Programs::Unit {
            run: encode(
                &arena,
                if error {
                    Term::error(&arena)
                } else {
                    Term::unit(&arena)
                },
            ),
        },
        expect,
    )
}
fn prop(error: bool, none: bool, expect: Expect) -> TestProgram {
    let a = &Arena::new();
    let body = if error { Term::error(a) } else { Term::unit(a) };
    let prepare = if none {
        Term::constr(a, 1, a.alloc([])).lambda(a, DeBruijn::zero(a))
    } else {
        prepared_program(a, Term::var(a, DeBruijn::new(a, 1)), body, shown(a, "7"))
    };
    fixture(
        Programs::Prop {
            prepare: encode(a, prepare),
        },
        expect,
    )
}
fn shown<'a>(a: &'a Arena, text: &'a str) -> &'a Term<'a, DeBruijn> {
    Term::constant(
        a,
        Constant::proto_list(a, Type::string(a), a.alloc([Constant::string(a, text)])),
    )
}
// Body and display terms sit under the unit argument and the outer PRNG argument.
fn prepared_program<'a>(
    a: &'a Arena,
    next: &'a Term<'a, DeBruijn>,
    body: &'a Term<'a, DeBruijn>,
    show: &'a Term<'a, DeBruijn>,
) -> &'a Term<'a, DeBruijn> {
    Term::constr(
        a,
        0,
        a.alloc([Term::constr(
            a,
            0,
            a.alloc([
                next,
                body.lambda(a, DeBruijn::zero(a)),
                show.lambda(a, DeBruijn::zero(a)),
            ]),
        )]),
    )
    .lambda(a, DeBruijn::zero(a))
}

fn run(test: TestProgram) -> Outcome {
    run_all(
        vec![test],
        &Config {
            seed: 42,
            max_success: 5,
            jobs: 1,
        },
    )
    .remove(0)
}
#[test]
fn units_and_budget_are_independent_of_expected_failure() {
    assert_eq!(run(unit(false, Expect::Pass)).status, Status::Pass);
    assert_eq!(
        run(unit(true, Expect::Pass)).status,
        Status::Fail(Failure::Body)
    );
    assert_eq!(run(unit(true, Expect::Fail)).status, Status::Pass);
    assert_eq!(
        run(unit(false, Expect::Fail)).status,
        Status::Fail(Failure::Body)
    );
    let mut t = unit(true, Expect::Fail);
    t.budget = Some(Budget::Cpu(0));
    assert!(matches!(
        run(t).status,
        Status::Fail(Failure::BudgetExceeded { .. })
    ));
}
#[test]
fn property_modifiers_and_generator_none() {
    let o = run(prop(false, false, Expect::Pass));
    assert_eq!(o.status, Status::Pass);
    assert_eq!(o.iterations, 5);
    let o = run(prop(true, false, Expect::Fail));
    assert_eq!(o.status, Status::Pass);
    assert_eq!(o.iterations, 5);
    let o = run(prop(true, false, Expect::FailOnce));
    assert_eq!(o.status, Status::Pass);
    assert!(o.expected_failure);
    assert_eq!(o.counterexample, Some(vec![("x".into(), "7".into())]));
    assert_eq!(
        run(prop(false, false, Expect::FailOnce)).status,
        Status::Fail(Failure::NoCounterexample)
    );
    for expect in [Expect::Pass, Expect::Fail, Expect::FailOnce] {
        assert!(matches!(
            run(prop(false, true, expect)).status,
            Status::Fail(Failure::Generator { .. })
        ));
    }
}
#[test]
fn invalid_bytecode_is_not_an_expected_body_failure() {
    let test = fixture(Programs::Unit { run: vec![] }, Expect::Fail);
    assert!(matches!(
        run(test).status,
        Status::Fail(Failure::InvalidProgram { .. })
    ));
}
#[test]
fn parallel_jobs_preserve_order_and_outcomes() {
    let tests = vec![
        unit(false, Expect::Pass),
        prop(true, false, Expect::Fail),
        prop(true, false, Expect::FailOnce),
    ];
    let mut c = Config::default();
    let a = run_all(tests.clone(), &c);
    c.jobs = 8;
    assert_eq!(a, run_all(tests, &c));
}
#[test]
fn prng_roundtrip_preserves_nonempty_history_and_rejects_bad_terms() {
    let a = &Arena::new();
    let mut terms = Vec::new();
    for p in [
        Prng::from_seed(42),
        Prng::Seeded {
            seed: [3; 32],
            choices: vec![1, u64::MAX, 4],
        },
        Prng::from_choices(&[0, 7, u64::MAX]),
    ] {
        let term = p.to_term(a);
        assert_eq!(Prng::from_term(term).unwrap(), p);
        terms.push(nash_plutus::pretty::term(term));
    }
    insta::with_settings!({omit_expression => true}, {
        insta::assert_snapshot!("prng_native_terms", terms.join("\n\n"));
    });
    assert!(Prng::from_term(Term::integer_from(a, 0)).is_err());
    let Prng::Seeded { seed, .. } = Prng::from_seed(42) else {
        unreachable!()
    };
    assert_eq!(
        seed.iter().map(|b| format!("{b:02x}")).collect::<String>(),
        "e7da4748680e2763d6ca394c222f99b27c75b2c9c9563ab56f963293f6cddb9b"
    );
}
#[test]
fn logs_preserve_malformed_payload_and_nul_values() {
    let logs = split_logs(vec![
        "\0label\0a".into(),
        "\0assert\x003".into(),
        "\0assert\x003\x002\0a\0b".into(),
        "\0assert\0bad".into(),
        "trace".into(),
    ]);
    assert_eq!(logs.labels, ["a"]);
    assert_eq!(logs.assert_id, Some(3));
    assert_eq!(logs.asserts, [(3, 2, "a\0b".into())]);
    assert_eq!(logs.traces, ["\0assert\0bad", "trace"]);
}
fn simplify(start: Vec<u64>, oracle: impl FnMut(&[u64]) -> ShrinkStatus<u64>) -> (Vec<u64>, usize) {
    let mut ce = Counterexample {
        value: 0,
        choices: start,
        cache: Cache::new(oracle),
        steps: 0,
    };
    ce.simplify();
    (ce.choices, ce.steps)
}
#[test]
fn shrink_int_pair_and_list() {
    assert_eq!(
        simplify(vec![48213], |c| match c.first() {
            None => ShrinkStatus::Invalid,
            Some(n) if *n >= 1000 => ShrinkStatus::Keep(*n),
            _ => ShrinkStatus::Ignore,
        })
        .0,
        [1000]
    );
    assert_eq!(
        simplify(vec![9, 3], |c| if c.len() < 2 {
            ShrinkStatus::Invalid
        } else if c[0] > c[1] {
            ShrinkStatus::Keep(c[0])
        } else {
            ShrinkStatus::Ignore
        })
        .0,
        [1, 0]
    );
    let oracle = |c: &[u64]| {
        let Some(&n) = c.first() else {
            return ShrinkStatus::Invalid;
        };
        if n as usize > c.len() - 1 {
            return ShrinkStatus::Invalid;
        }
        let sum = c[1..=n as usize].iter().sum::<u64>();
        if sum > 100 {
            ShrinkStatus::Keep(sum)
        } else {
            ShrinkStatus::Ignore
        }
    };
    let a = simplify(vec![5, 10, 90, 20, 5, 1], oracle);
    let b = simplify(vec![5, 10, 90, 20, 5, 1], oracle);
    assert_eq!(a, b);
    assert_eq!(a.0, [1, 101]);
}
#[test]
fn cache_keeps_replay_lengths_distinct() {
    let calls = Cell::new(0);
    let mut cache = Cache::new(|c: &[u64]| {
        calls.set(calls.get() + 1);
        if c.len() < 2 {
            ShrinkStatus::Invalid
        } else {
            ShrinkStatus::Keep(c.len())
        }
    });
    assert_eq!(cache.get(&[1]), ShrinkStatus::Invalid);
    assert_eq!(cache.get(&[1, 2, 3]), ShrinkStatus::Keep(3));
    assert_eq!(cache.get(&[1, 2]), ShrinkStatus::Keep(2));
    assert_eq!(cache.get(&[1, 2, 4]), ShrinkStatus::Keep(3));
    assert_eq!(cache.get(&[1, 2]), ShrinkStatus::Keep(2));
    assert_eq!(calls.get(), 4);
    assert_eq!(cache.size(), 4);
}
#[test]
fn unicode_assert_columns_and_json() {
    let source = "é == x";
    let site = AssertSite {
        id: 0,
        region: Region::new(Position::new(1, 1), Position::new(1, 8)),
        captures: vec![
            Capture {
                index: 0,
                region: Region::new(Position::new(1, 1), Position::new(1, 3)),
                shown: true,
            },
            Capture {
                index: 1,
                region: Region::new(Position::new(1, 7), Position::new(1, 8)),
                shown: false,
            },
        ],
    };
    let report = AssertReport {
        site,
        values: vec![(0, "é".into())],
    };
    assert_eq!(
        report::terminal::render_assert(source, &report),
        "× assert (é == x)\n          │    │\n          │    ?\n          é\n"
    );
    let mut outcome = run(unit(true, Expect::Pass));
    outcome.test.source = source.into();
    outcome.assert = Some(report);
    let json: serde_json::Value =
        serde_json::from_str(&report::json::render(42, 5, &[outcome])).unwrap();
    assert_eq!(json["tests"][0]["assert"]["values"][1]["column"], 5);
    insta::with_settings!({description => source, omit_expression => true}, {
        insta::assert_snapshot!(serde_json::to_string_pretty(&json).unwrap());
    });
}
fn lazy_if<'a>(
    a: &'a Arena,
    c: &'a Term<'a, DeBruijn>,
    yes: &'a Term<'a, DeBruijn>,
    no: &'a Term<'a, DeBruijn>,
) -> &'a Term<'a, DeBruijn> {
    Term::if_then_else(a)
        .force(a)
        .apply(a, c)
        .apply(a, yes.delay(a))
        .apply(a, no.delay(a))
        .force(a)
}
fn traced<'a>(
    a: &'a Arena,
    message: &'a str,
    term: &'a Term<'a, DeBruijn>,
) -> &'a Term<'a, DeBruijn> {
    Term::trace(a)
        .force(a)
        .apply(a, Term::string(a, message))
        .apply(a, term.delay(a))
        .force(a)
}
#[test]
fn actual_cek_property_shrinks_and_retains_failure_logs() {
    let a = &Arena::new();
    let p = Term::var(a, DeBruijn::new(a, 2));
    let seeded = Term::integer_from(a, 25)
        .lambda(a, DeBruijn::zero(a))
        .lambda(a, DeBruijn::zero(a));
    let replayed = Term::head_list(a)
        .force(a)
        .apply(a, Term::var(a, DeBruijn::new(a, 1)))
        .lambda(a, DeBruijn::zero(a));
    let n = Term::case(a, p, a.alloc([seeded, replayed]));
    let enough = Term::less_than_equals_integer(a)
        .apply(a, Term::integer_from(a, 10))
        .apply(a, n);
    let output = Prng::Seeded {
        seed: [0; 32],
        choices: vec![25],
    };
    let next = output.to_term(a);
    let run = lazy_if(
        a,
        enough,
        traced(a, "failure remains", Term::error(a)),
        Term::unit(a),
    );
    let exact = Term::equals_integer(a)
        .apply(a, n)
        .apply(a, Term::integer_from(a, 10));
    let show = |s| {
        a.alloc(Term::Constant(a.alloc(Constant::ProtoList(
            Type::string(a),
            a.alloc([Constant::string(a, s)]),
        )))) as &Term<'_, DeBruijn>
    };
    let shown = lazy_if(a, exact, show("10"), show("25"));
    let prepare = prepared_program(a, next, run, shown);
    let t = fixture(
        Programs::Prop {
            prepare: encode(a, prepare),
        },
        Expect::Pass,
    );
    let out = self::run(t);
    assert_eq!(out.status, Status::Fail(Failure::Body));
    assert_eq!(out.counterexample, Some(vec![("x".into(), "10".into())]));
    assert_eq!(out.traces, ["failure remains"]);
}
#[test]
fn unchanged_counterexample_preserves_assert_marker_and_traces() {
    let a = &Arena::new();
    let mut t = prop(true, false, Expect::Pass);
    if let Programs::Prop { prepare } = &mut t.programs {
        *prepare = encode(
            a,
            prepared_program(
                a,
                Term::var(a, DeBruijn::new(a, 1)),
                traced(
                    a,
                    "\0assert\x000",
                    traced(a, "original trace", Term::error(a)),
                ),
                shown(a, "7"),
            ),
        );
    }
    t.asserts.push(AssertSite {
        id: 0,
        region: Region::one(),
        captures: vec![],
    });
    let out = run(t);
    assert!(out.assert.is_some());
    assert_eq!(out.traces, ["original trace"]);
}
#[test]
fn generator_errors_remain_generator_failures_for_fail_properties() {
    let a = &Arena::new();
    let mut t = prop(true, false, Expect::Fail);
    if let Programs::Prop { prepare } = &mut t.programs {
        *prepare = encode(a, Term::error(a).lambda(a, DeBruijn::zero(a)));
    }
    assert!(matches!(
        run(t).status,
        Status::Fail(Failure::Generator { .. })
    ));
}
#[test]
fn coverage_counts_duplicate_labels_and_uses_requested_denominator() {
    let a = &Arena::new();
    let mut t = prop(false, false, Expect::Pass);
    let p = Term::var(a, DeBruijn::new(a, 1));
    if let Programs::Prop { prepare } = &mut t.programs {
        *prepare = encode(
            a,
            prepared_program(
                a,
                p,
                traced(a, "\0label\0a", traced(a, "\0label\0a", Term::unit(a))),
                shown(a, "7"),
            ),
        );
    }
    let out = run(t);
    assert_eq!(out.labels["a"], 10);

    let Programs::Prop { prepare: bytes } = &out.test.programs else {
        unreachable!()
    };
    let program: &nash_plutus::program::Program<'_, DeBruijn> =
        nash_plutus::flat::decode(a, bytes).unwrap();
    insta::with_settings!({description => nash_plutus::pretty::program(program), omit_expression => true}, {
        insta::assert_snapshot!("coverage_labels", report::terminal::render(std::slice::from_ref(&out), Coverage::Labels, 42, std::time::Duration::ZERO));
        insta::assert_snapshot!("coverage_tests", report::terminal::render(&[out], Coverage::Tests, 42, std::time::Duration::ZERO));
    });
}

#[test]
fn terminal_and_json_reports_snapshot() {
    let test = unit(false, Expect::Pass);
    let arena = Arena::new();
    let Programs::Unit { run: bytes } = &test.programs else {
        unreachable!()
    };
    let program: &nash_plutus::program::Program<'_, DeBruijn> =
        nash_plutus::flat::decode(&arena, bytes).unwrap();
    let input = nash_plutus::pretty::program(program);
    let mut outcome = run(test);
    outcome.budget = ExBudget::new(1200, 345100);
    let mut settings = insta::Settings::clone_current();
    settings.set_description(&input);
    settings.set_omit_expression(true);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(report::terminal::render(
        std::slice::from_ref(&outcome),
        Coverage::Labels,
        42,
        std::time::Duration::from_millis(810)
    ));
    outcome.status = Status::Fail(Failure::BudgetExceeded {
        limit: Budget::Both {
            cpu: i128::MAX,
            mem: 0,
        },
        used: outcome.budget,
    });
    let json: serde_json::Value =
        serde_json::from_str(&report::json::render(42, 100, &[outcome])).unwrap();
    assert_eq!(
        json["tests"][0]["failure"]["limit"]["cpu"],
        i128::MAX.to_string()
    );
    insta::assert_snapshot!(
        "budget_failure_json",
        serde_json::to_string_pretty(&json).unwrap()
    );
}

#[test]
fn multiline_assert_uses_source_rows_display_width_and_indented_values() {
    let source = "  assert (identity \"界\" == identity \"e\u{301}\" ++ z\n      && identity \"名\" == value)";
    let mut settings = insta::Settings::clone_current();
    settings.set_description(source);
    settings.set_omit_expression(true);
    let _guard = settings.bind_to_scope();
    let position = |offset: usize| {
        let prefix = &source[..offset];
        Position::new(
            prefix.bytes().filter(|&b| b == b'\n').count() + 1,
            prefix.rsplit('\n').next().unwrap().len() + 1,
        )
    };
    let capture = |index, name: &str| {
        let offset = source.find(name).unwrap();
        Capture {
            index,
            region: Region::new(position(offset), position(offset + name.len())),
            shown: true,
        }
    };
    let report = AssertReport {
        site: AssertSite {
            id: 0,
            region: Region::new(
                position(source.find("identity").unwrap()),
                position(source.len() - 1),
            ),
            captures: vec![
                capture(0, "identity \"界\""),
                capture(1, "identity \"e\u{301}\""),
                capture(2, "identity \"名\""),
                capture(3, "value"),
                capture(4, "z"),
            ],
        },
        values: vec![
            (0, "left\ncontinued".into()),
            (1, "right".into()),
            (2, "name".into()),
            (3, "first\nsecond".into()),
            (4, "zero".into()),
        ],
    };
    insta::assert_snapshot!(
        "multiline_assert_display_columns",
        report::terminal::render_assert(source, &report)
    );
    let mut outcome = run(unit(true, Expect::Pass));
    outcome.test.source = source.into();
    outcome.assert = Some(report);
    let json: serde_json::Value =
        serde_json::from_str(&report::json::render(42, 5, &[outcome])).unwrap();
    insta::assert_snapshot!(
        "multiline_assert_json",
        serde_json::to_string_pretty(&json["tests"][0]["assert"]).unwrap()
    );
    let values = json["tests"][0]["assert"]["values"].as_array().unwrap();
    assert_eq!(
        values
            .iter()
            .map(|v| (v["row"].as_u64().unwrap(), v["column"].as_u64().unwrap()))
            .collect::<Vec<_>>(),
        [(0, 0), (0, 17), (1, 9), (1, 26), (0, 33)]
    );
}

#[test]
fn preparation_runs_once_and_success_does_not_show_values() {
    let arena = &Arena::new();
    let next = Prng::from_seed(42).to_term(arena);
    let prepared = prepared_program(arena, next, Term::unit(arena), Term::error(arena));
    let program =
        traced(arena, "generate", prepared.apply(arena, next)).lambda(arena, DeBruijn::zero(arena));
    let test = fixture(
        Programs::Prop {
            prepare: encode(arena, program),
        },
        Expect::Pass,
    );
    let outcome = run(test);
    assert_eq!(outcome.status, Status::Pass);
    assert_eq!(outcome.traces, ["generate"]);
    insta::with_settings!({description => nash_plutus::pretty::term(program), omit_expression => true}, {
        insta::assert_snapshot!(report::terminal::render(&[outcome], Coverage::Labels, 42, std::time::Duration::ZERO));
    });
}
