//! Explicit-only performance checks. This binary is not a Cargo test target.
#[path = "../../../crates/nash-codegen/tests/support/constant_fold.rs"]
mod constant_input;
#[path = "../../../crates/nash-codegen/tests/support/inverse.rs"]
mod inverse_input;
#[path = "../../../crates/nash-codegen/tests/support/pair_projection.rs"]
mod pair_input;
mod source;

#[path = "../../../crates/nash-codegen/tests/support/vesting.rs"]
mod vesting_input;

use nash_ir::{anf, build::Builder, core::Core, hygiene};
use nash_plutus::{
    arena::Arena,
    binder::DeBruijn,
    data::PlutusData,
    debruijn, flat,
    machine::{ExBudget, MachineError, PlutusVersion},
    pretty,
    program::{Program, Version},
    term::Term,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, io::Write, path::Path, process::Command, time::Duration};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const BUDGET: ExBudget = ExBudget {
    cpu: 100_000_000,
    mem: 2_000_000,
};
const SETTINGS: &str = "v1; Plutus V3/PV11; UPLC 1.1.0; bundled V3 default cost model; CPU=100000000; memory=2000000; raw Flat bytes before ledger application; O0 vs static lift/unused-parameters/ANF once/rules1+2+3+4+dead-bindings+recursive-reachability+representation-inverse+force-delay+known-bool+int-bytes/bound-constr+known-fields+list+data+idata-bdata-listdata-mapdata-constrdata+restricted-pair-cleanup/constant-fold+cleanup(calls128,cpu1000000,mem10000,bytes4096,nodes1024,depth64)/recursion/hygiene/lower+forced-builtin-sharing+constant-prefix-sharing";

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Measurement {
    cpu: i64,
    memory: i64,
    bytes: usize,
    result: String,
    logs: Vec<String>,
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Row {
    input: String,
    before: Measurement,
    after: Measurement,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Report {
    settings: String,
    sources: BTreeMap<String, String>,
    revision: String,
    rustc: String,
    rows: Vec<Row>,
}

fn main() {
    // Bounds both compilation/optimization and evaluation of experiments.
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(120));
        eprintln!("performance workload exceeded 120-second wall limit");
        std::process::exit(2);
    });
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("help");
    match (command, args.len()) {
        ("measure", 1) | ("check", 1..=2) | ("record", 2) | ("experiment", 2) => {}
        _ => {
            return Err("usage: nash-optimizer-perf measure | check [baseline.json] | record NEW.json | experiment MODULE.nash\nrecord refuses overwrites; review and copy explicitly to update baselines".into());
        }
    }
    let mut sources: BTreeMap<String, String> = source::SUPPORT
        .iter()
        .map(|(name, text, _)| ((*name).into(), (*text).into()))
        .collect();
    let rows = if command == "experiment" {
        let path = Path::new(&args[1]);
        if fs::metadata(path)?.len() > 65_536 {
            return Err("experiment source exceeds 64 KiB".into());
        }
        let text = fs::read_to_string(path)?;
        sources.insert("experiment".into(), text.clone());
        let arena = Arena::new();
        let core = source::compile(&arena, &text, &["main"])[0];
        vec![compare(
            &arena,
            format!("experiment source:\n{text}"),
            core,
            &[],
            None,
        )?]
    } else {
        sources.insert(
            "Workloads".into(),
            include_str!("../fixtures/Workloads.nash").into(),
        );
        sources.insert(
            "Vesting".into(),
            include_str!("../../../crates/nash-codegen/tests/fixtures/Vesting.nash").into(),
        );
        sources.insert(
            "VestingParam".into(),
            include_str!("../../../crates/nash-codegen/tests/fixtures/VestingParam.nash").into(),
        );
        sources.insert(
            "VestingContextInput".into(),
            include_str!("../../../crates/nash-codegen/tests/support/vesting.rs").into(),
        );
        sources.insert(
            "InverseInputs".into(),
            include_str!("../../../crates/nash-codegen/tests/support/inverse.rs").into(),
        );
        sources.insert(
            "ConstantInputs".into(),
            include_str!("../../../crates/nash-codegen/tests/support/constant_fold.rs").into(),
        );
        sources.insert(
            "PairInputs".into(),
            include_str!("../../../crates/nash-codegen/tests/support/pair_projection.rs").into(),
        );
        suite()?
    };
    let report = Report {
        settings: SETTINGS.into(),
        sources,
        revision: output(
            "jj",
            &["log", "-r", "@", "--no-graph", "-T", "commit_id"],
            Some(repo()),
        ),
        rustc: output("rustc", &["--version"], None),
        rows,
    };
    match command {
        "measure" | "experiment" => {
            println!("{}", serde_json::to_string_pretty(&report)?)
        }
        "record" => {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&args[1])?;
            writeln!(file, "{}", serde_json::to_string_pretty(&report)?)?;
            println!(
                "Wrote {}. Review before replacing the committed baseline.",
                args[1]
            );
        }
        "check" => {
            let default = Path::new(env!("CARGO_MANIFEST_DIR")).join("baseline.json");
            let path = args.get(1).map(Path::new).unwrap_or(&default);
            let baseline: Report = serde_json::from_str(&fs::read_to_string(path)?)?;
            if baseline.settings != report.settings || baseline.sources != report.sources {
                return Err("baseline settings or source inputs differ; review required".into());
            }
            if baseline.rows != report.rows {
                for (old, new) in baseline.rows.iter().zip(&report.rows) {
                    if old != new {
                        eprintln!("changed row:\nexpected {old:#?}\nactual {new:#?}");
                    }
                }
                return Err("performance baseline changed (including improvements or changed inputs); review required".into());
            }
            println!(
                "{} performance cases match the explicit baseline",
                report.rows.len()
            );
        }
        _ => unreachable!(),
    }
    Ok(())
}

fn repo() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
}
fn output(program: &str, args: &[&str], cwd: Option<&Path>) -> String {
    let mut cmd = Command::new(program);
    cmd.args(args);
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    cmd.output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_else(|| "unavailable".into())
}

fn accepted<'a>(arena: &'a Arena, core: &'a Core<'a>) -> &'a Core<'a> {
    let b = Builder::new(arena);
    let core = nash_codegen::optimizer::optimize(arena, core);
    anf::validate(core).expect("ANF before recursion rewriting");
    let core = nash_codegen::recursion::rewrite(&b, core).expect("recursion rewrite");
    let core = hygiene::freshen(&b, core);
    hygiene::validate(core, &[]).expect("unique closed binders");
    core
}

fn compare<'a>(
    arena: &'a Arena,
    input: String,
    core: &'a Core<'a>,
    args: &[&'a Term<'a, DeBruijn>],
    expected_result: Option<&str>,
) -> Result<Row> {
    let before =
        nash_codegen::recursion::rewrite(&Builder::new(arena), core).expect("baseline recursion");
    let after = accepted(arena, core);
    assert_eq!(core.ty, after.ty);
    let before = measure(arena, before, args, false)?;
    let after = measure(arena, after, args, true)?;
    if before.result != after.result || before.logs != after.logs {
        return Err(format!("semantic mismatch for {input}: {before:?} versus {after:?}").into());
    }
    if let Some(expected) = expected_result
        && before.result != expected
    {
        return Err(format!(
            "unexpected outcome for {input}: {} (expected {expected})",
            before.result
        )
        .into());
    }
    Ok(Row {
        input,
        before,
        after,
    })
}

fn measure<'a>(
    arena: &'a Arena,
    core: &'a Core<'a>,
    args: &[&'a Term<'a, DeBruijn>],
    sharing: bool,
) -> Result<Measurement> {
    let named = if sharing {
        nash_codegen::lower::lower_with_constant_sharing(arena, core)
    } else {
        nash_codegen::lower::lower(arena, core)
    }
    .expect("lowering");
    let term = debruijn::to_debruijn(arena, named).expect("closed UPLC");
    let mut program = Program::new(arena, Version::plutus_v3(arena), term);
    let bytes = flat::encode(program).expect("Flat encoding").len();
    for arg in args {
        program = program.apply(arena, arg);
    }
    let eval = program.eval_version_budget(arena, PlutusVersion::V3, BUDGET);
    let result = match eval.term {
        Err(MachineError::OutOfExError(_)) => {
            return Err("evaluation exceeded explicit budget".into());
        }
        Err(error) => format!("error: {error:?}"),
        Ok(term) if ground(term) => pretty::term(term),
        Ok(_) => {
            return Err(
                "workload must return a ground value; apply returned functions in main".into(),
            );
        }
    };
    Ok(Measurement {
        cpu: eval.info.consumed_budget.cpu,
        memory: eval.info.consumed_budget.mem,
        bytes,
        result,
        logs: eval.info.logs,
    })
}
fn ground(term: &Term<'_, DeBruijn>) -> bool {
    match term {
        Term::Constant(_) => true,
        Term::Constr { fields, .. } => fields.iter().all(|t| ground(t)),
        _ => false,
    }
}

fn suite() -> Result<Vec<Row>> {
    let mut rows = Vec::new();
    let arena = Arena::new();
    let names = [
        "listTraversal",
        "staticRecursion",
        "dataMatch",
        "dataMiss",
        "decoding",
        "validationPass",
        "validationFail",
        "booleanHelpers",
        "constantPrefixTwice",
        "constantPrefixCold",
        "constantPrefixLoop",
    ];
    let cores = source::compile(&arena, include_str!("../fixtures/Workloads.nash"), &names);
    let expected = [
        "(con integer 36)",
        "(con integer 42)",
        "(con integer 42)",
        "(con integer 0)",
        "(con integer 42)",
        "(con unit ())",
        "error: ExplicitErrorTerm",
        "(con bool True)",
        "(con integer 197)",
        "(con integer 42)",
        "(con integer 1528)",
    ];
    for ((name, core), expected) in names.into_iter().zip(cores).zip(expected) {
        rows.push(compare(
            &arena,
            format!("Workloads.{name} (fixtures/Workloads.nash)"),
            core,
            &[],
            Some(expected),
        )?);
    }
    for (source, parameter) in [
        (
            include_str!("../../../crates/nash-codegen/tests/fixtures/Vesting.nash"),
            false,
        ),
        (
            include_str!("../../../crates/nash-codegen/tests/fixtures/VestingParam.nash"),
            true,
        ),
    ] {
        let arena = Arena::new();
        let core = source::compile(&arena, source, &["main"])[0];
        for (scenario, deadline, redeemer, signer, minimum, success) in [
            ("claim after deadline", 10, 0, &b""[..], 5, true),
            ("claim before deadline", 30, 0, &b""[..], 5, false),
            ("cancel signed", 10, 1, &[0xaa][..], 5, true),
            ("cancel unsigned", 10, 1, &b""[..], 5, false),
            ("boundary zero minimum", 18, 0, &b""[..], 0, true),
            ("boundary five minimum", 18, 0, &b""[..], 5, !parameter),
        ] {
            let datum = PlutusData::constr(
                &arena,
                0,
                arena.alloc_slice_copy(&[
                    PlutusData::byte_string(&arena, &[0xaa]),
                    PlutusData::integer_from(&arena, deadline),
                ]),
            );
            let action = PlutusData::constr(&arena, redeemer, &[]);
            let context = vesting_input::context(&arena, datum, action, 20, signer);
            let mut args = Vec::new();
            if parameter {
                args.push(Term::integer_from(&arena, minimum));
            }
            args.push(Term::data(&arena, context));
            let input = format!(
                "Vesting{}: {scenario}; deadline={deadline}, action={redeemer}, signer={signer:?}, minimum={}, V3 context, lower_time=20, owner=[170]",
                if parameter { "Param" } else { "" },
                if parameter {
                    minimum.to_string()
                } else {
                    "none".into()
                }
            );
            rows.push(compare(
                &arena,
                input,
                core,
                &args,
                Some(if success {
                    "(con unit ())"
                } else {
                    "error: ExplicitErrorTerm"
                }),
            )?);
        }
    }
    let arena = Arena::new();
    for (name, core, _) in inverse_input::cases(&Builder::new(&arena)) {
        rows.push(compare(
            &arena,
            format!("representation cancellation: {name}"),
            core,
            &[],
            None,
        )?);
    }
    {
        let arena = Arena::new();
        for (name, core, _) in constant_input::cases(&Builder::new(&arena)) {
            rows.push(compare(
                &arena,
                format!("constant folding: {name}"),
                core,
                &[],
                None,
            )?);
        }
    }
    let arena = Arena::new();
    for (name, core, _) in pair_input::cases(&Builder::new(&arena)) {
        rows.push(compare(
            &arena,
            format!("pair projection: {name}"),
            core,
            &[],
            None,
        )?);
    }
    Ok(rows)
}
