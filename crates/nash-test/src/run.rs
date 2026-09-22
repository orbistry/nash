use crate::{
    eval::{self, Drawn, Logs, Ran},
    prng::Prng,
    shrink, *,
};
use nash_plutus::{arena::Arena, machine::PlutusVersion as MachineVersion, term::Term};
use rayon::prelude::*;

pub fn run_all(tests: Vec<TestProgram>, config: &Config) -> Vec<Outcome> {
    let run = || {
        tests
            .into_par_iter()
            .map(|test| run_one(test, config))
            .collect()
    };
    match rayon::ThreadPoolBuilder::new()
        .num_threads(config.jobs.max(1))
        .build()
    {
        Ok(pool) => pool.install(run),
        Err(_) => run(),
    }
}
fn version(v: PlutusVersion) -> MachineVersion {
    match v {
        PlutusVersion::V1 => MachineVersion::V1,
        PlutusVersion::V2 => MachineVersion::V2,
        PlutusVersion::V3 => MachineVersion::V3,
    }
}
fn exceeded(limit: Option<Budget>, used: ExBudget) -> Option<Failure> {
    limit
        .filter(|b| match b {
            Budget::Cpu(n) => i128::from(used.cpu) > *n,
            Budget::Mem(n) => i128::from(used.mem) > *n,
            Budget::Both { cpu, mem } => i128::from(used.cpu) > *cpu || i128::from(used.mem) > *mem,
        })
        .map(|limit| Failure::BudgetExceeded { limit, used })
}
fn record(out: &mut Outcome, logs: Logs) {
    out.assert = logs.assert_id.and_then(|id| {
        out.test
            .asserts
            .iter()
            .find(|s| s.id == id)
            .cloned()
            .map(|site| AssertReport {
                site,
                values: logs
                    .asserts
                    .iter()
                    .filter(|(i, _, _)| *i == id)
                    .map(|(_, index, v)| (*index, v.clone()))
                    .collect(),
            })
    });
    out.traces = logs.traces;
}
fn draw(
    version: MachineVersion,
    bytes: &[u8],
    prng: &Prng,
    machine_budget: ExBudget,
) -> Result<(Prng, Vec<String>), Failure> {
    match eval::run_draw_with_budget(version, bytes, prng, machine_budget) {
        Ok(Drawn::Some { prng, shown }) => Ok((prng, shown)),
        Ok(Drawn::None) => Err(Failure::Generator {
            message: "generator returned None on a seeded run".into(),
        }),
        Err(failure) => Err(failure),
    }
}
fn run_one(test: TestProgram, config: &Config) -> Outcome {
    run_one_with_budget(test, config, ExBudget::max())
}
fn run_one_with_budget(test: TestProgram, config: &Config, machine_budget: ExBudget) -> Outcome {
    let v = version(test.plutus_version);
    let mut out = Outcome {
        test,
        status: Status::Pass,
        budget: ExBudget::new(0, 0),
        iterations: 0,
        labels: Default::default(),
        traces: vec![],
        assert: None,
        counterexample: None,
        expected_failure: false,
    };
    // Own bytecode separately so result metadata can be updated throughout execution.
    let programs = out.test.programs.clone();
    let (run, draw_program) = match &programs {
        Programs::Unit { run } => (run, None),
        Programs::Prop { draw, run } => (run, Some(draw)),
    };
    let mut prng = Prng::from_seed(config.seed);
    let iterations = if draw_program.is_some() {
        config.max_success
    } else {
        1
    };
    for _ in 0..iterations {
        out.iterations += 1;
        let arena = Arena::new();
        let arg = draw_program.map(|_| Term::data(&arena, prng.to_data(&arena)));
        let ev = eval::evaluate_with_budget(&arena, v, run, arg, machine_budget);
        let exhaustion = ev.exhaustion_failure();
        out.budget.mem = out.budget.mem.max(ev.budget.mem);
        out.budget.cpu = out.budget.cpu.max(ev.budget.cpu);
        let logs = eval::split_logs(ev.logs);
        for label in &logs.labels {
            *out.labels.entry(label.clone()).or_insert(0) += 1;
        }
        if ev.invalid_program {
            out.status = Status::Fail(Failure::InvalidProgram {
                message: ev.term.unwrap_err(),
            });
            record(&mut out, logs);
            return out;
        }
        if let Some(failure) = exhaustion {
            out.status = Status::Fail(failure);
            record(&mut out, logs);
            return out;
        }
        let errored = ev.term.is_err();
        let counterexample = match out.test.expect {
            Expect::Pass | Expect::FailOnce => errored,
            Expect::Fail => !errored,
        };
        let Some(draw_program) = draw_program else {
            out.status = if let Some(failure) = exceeded(out.test.budget, ev.budget) {
                Status::Fail(failure)
            } else if counterexample {
                Status::Fail(Failure::Body)
            } else {
                Status::Pass
            };
            record(&mut out, logs);
            return out;
        };
        // Check the protocol before interpreting a successful run as a body outcome.
        let next = match ev.term {
            Ok(term) => match eval::decode_ran(term) {
                Ok(Ran::Some(p)) => Some(p),
                Ok(Ran::None) => {
                    out.status = Status::Fail(Failure::Generator {
                        message: "generator returned None on a seeded run".into(),
                    });
                    record(&mut out, logs);
                    return out;
                }
                Err(message) => {
                    out.status = Status::Fail(Failure::Generator { message });
                    record(&mut out, logs);
                    return out;
                }
            },
            Err(_) => None,
        };
        // An error may originate in a generator. Recovery distinguishes it from a body failure,
        // including iterations expected to fail and budget violations.
        let recovered = if next.is_none() || counterexample {
            match draw(v, draw_program, &prng, machine_budget) {
                Ok(p) => Some(p),
                Err(f) => {
                    out.status = Status::Fail(f);
                    record(&mut out, logs);
                    return out;
                }
            }
        } else {
            None
        };
        if let Some(f) = exceeded(out.test.budget, ev.budget) {
            out.status = Status::Fail(f);
            record(&mut out, logs);
            return out;
        }
        if counterexample {
            let (next, shown) = recovered.expect("counterexample recovers its inputs");
            let original = (shown, logs.clone());
            let expect = out.test.expect;
            let budget_limit = out.test.budget;
            let oracle = |choices: &[u64]| {
                let p = Prng::from_choices(choices);
                let shown = match eval::run_draw_with_budget(v, draw_program, &p, machine_budget) {
                    Ok(Drawn::Some { shown, .. }) => shown,
                    _ => return shrink::Status::Invalid,
                };
                let arena = Arena::new();
                let ev = eval::evaluate_with_budget(
                    &arena,
                    v,
                    run,
                    Some(Term::data(&arena, p.to_data(&arena))),
                    machine_budget,
                );
                if invalid_shrink_evaluation(&ev, budget_limit) {
                    return shrink::Status::Invalid;
                }
                let failed = match ev.term {
                    Err(_) => expect != Expect::Fail,
                    Ok(term) => match eval::decode_ran(term) {
                        Ok(Ran::Some(_)) => expect == Expect::Fail,
                        _ => return shrink::Status::Invalid,
                    },
                };
                if failed {
                    shrink::Status::Keep((shown, eval::split_logs(ev.logs)))
                } else {
                    shrink::Status::Ignore
                }
            };
            let mut ce = shrink::Counterexample {
                value: original,
                choices: next.choices(),
                cache: shrink::Cache::new(oracle),
                steps: 0,
            };
            if !ce.choices.is_empty() {
                eprintln!(
                    "  Simplifying counterexample from {} choices",
                    ce.choices.len()
                );
                let start = std::time::Instant::now();
                ce.simplify();
                eprintln!(
                    "  Simplified counterexample in {:?} after {} steps",
                    start.elapsed(),
                    ce.steps
                );
            }
            // Initial logs are retained even when no proposal improves the counterexample.
            let (shown, logs) = ce.value;
            drop(ce.cache);
            out.counterexample = Some(out.test.binder_texts.iter().cloned().zip(shown).collect());
            out.expected_failure = out.test.expect == Expect::FailOnce;
            out.status = if out.expected_failure {
                Status::Pass
            } else {
                Status::Fail(Failure::Body)
            };
            record(&mut out, logs);
            return out;
        }
        prng = next
            .or_else(|| recovered.map(|p| p.0))
            .expect("successful iteration recovers PRNG")
            .next_iteration();
        record(&mut out, logs);
    }
    if out.test.expect == Expect::FailOnce {
        out.status = Status::Fail(Failure::NoCounterexample);
    }
    out
}

fn invalid_shrink_evaluation(ev: &eval::Evaluated<'_>, budget: Option<Budget>) -> bool {
    ev.invalid_program || ev.exhausted.is_some() || exceeded(budget, ev.budget).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nash_plutus::{
        binder::DeBruijn,
        flat,
        program::{Program, Version},
    };
    use nash_region::Region;

    fn bytes(arena: &Arena, term: &Term<'_, DeBruijn>) -> Vec<u8> {
        flat::encode(Program::new(arena, Version::plutus_v3(arena), term)).unwrap()
    }
    fn fixture(programs: Programs, expect: Expect, budget: Option<Budget>) -> TestProgram {
        TestProgram {
            module: "Budget".into(),
            name: "machine exhaustion".into(),
            expect,
            budget,
            region: Region::one(),
            programs,
            asserts: vec![],
            binder_texts: vec![],
            plutus_version: PlutusVersion::V3,
            source: String::new(),
            source_path: "Budget.nash".into(),
        }
    }
    #[test]
    fn machine_exhaustion_fails_every_modifier_even_without_within() {
        let arena = &Arena::new();
        let unit = bytes(arena, Term::unit(arena));
        let property = bytes(
            arena,
            Term::unit(arena).lambda(arena, DeBruijn::zero(arena)),
        );
        let machine_limit = ExBudget::new(0, 0);
        for expect in [Expect::Pass, Expect::Fail, Expect::FailOnce] {
            for budget in [
                None,
                Some(Budget::Both {
                    cpu: i128::MAX,
                    mem: i128::MAX,
                }),
            ] {
                for programs in [
                    Programs::Unit { run: unit.clone() },
                    Programs::Prop {
                        run: property.clone(),
                        draw: property.clone(),
                    },
                ] {
                    let outcome = run_one_with_budget(
                        fixture(programs, expect, budget),
                        &Config::default(),
                        machine_limit,
                    );
                    let Status::Fail(Failure::BudgetExceeded { limit, used }) = outcome.status
                    else {
                        panic!("unexpected outcome: {:?}", outcome.status);
                    };
                    assert_eq!(limit, Budget::Both { cpu: 0, mem: 0 });
                    assert!(used.cpu > 0 || used.mem > 0);
                    assert!(outcome.counterexample.is_none());
                    assert!(!outcome.expected_failure);
                }
            }
        }
    }
    #[test]
    fn actual_exhaustion_cannot_be_a_shrink_counterexample() {
        let arena = &Arena::new();
        let run = bytes(arena, Term::error(arena));
        let exhausted =
            eval::evaluate_with_budget(arena, MachineVersion::V3, &run, None, ExBudget::new(0, 0));
        assert!(exhausted.term.is_err());
        assert_eq!(exhausted.exhausted, Some(ExBudget::new(0, 0)));
        assert!(invalid_shrink_evaluation(&exhausted, None));
        assert!(invalid_shrink_evaluation(
            &exhausted,
            Some(Budget::Cpu(i128::MAX))
        ));
        let body_error = eval::evaluate(arena, MachineVersion::V3, &run, None);
        assert!(body_error.term.is_err());
        assert_eq!(body_error.exhausted, None);
        assert!(!invalid_shrink_evaluation(&body_error, None));
    }
    #[test]
    fn draw_exhaustion_preserves_budget_failure() {
        let arena = &Arena::new();
        let program = bytes(
            arena,
            Term::unit(arena).lambda(arena, DeBruijn::zero(arena)),
        );
        let failure = draw(
            MachineVersion::V3,
            &program,
            &Prng::from_seed(42),
            ExBudget::new(0, 0),
        )
        .unwrap_err();
        assert!(matches!(
            failure,
            Failure::BudgetExceeded {
                limit: Budget::Both { cpu: 0, mem: 0 },
                ..
            }
        ));
    }
}
