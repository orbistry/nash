use crate::{Budget, Failure, prng::Prng};
use nash_plutus::{
    arena::Arena,
    binder::DeBruijn,
    constant::Constant,
    flat,
    machine::{ExBudget, MachineError, PlutusVersion},
    program::{Program, Version},
    term::Term,
};
pub struct Evaluated<'a> {
    pub term: Result<&'a Term<'a, DeBruijn>, String>,
    pub budget: ExBudget,
    pub logs: Vec<String>,
    pub invalid_program: bool,
    /// Machine limit when evaluation exhausted its execution budget.
    pub exhausted: Option<ExBudget>,
}
pub fn evaluate<'a>(
    arena: &'a Arena,
    version: PlutusVersion,
    bytes: &[u8],
    arg: Option<&'a Term<'a, DeBruijn>>,
) -> Evaluated<'a> {
    evaluate_with_budget(arena, version, bytes, arg, ExBudget::max())
}
impl Evaluated<'_> {
    pub(crate) fn exhaustion_failure(&self) -> Option<Failure> {
        self.exhausted.map(|limit| Failure::BudgetExceeded {
            limit: Budget::Both {
                cpu: i128::from(limit.cpu),
                mem: i128::from(limit.mem),
            },
            used: self.budget,
        })
    }
}
pub(crate) fn evaluate_with_budget<'a>(
    arena: &'a Arena,
    version: PlutusVersion,
    bytes: &[u8],
    arg: Option<&'a Term<'a, DeBruijn>>,
    budget: ExBudget,
) -> Evaluated<'a> {
    let program: &Program<'_, DeBruijn> = match flat::decode(arena, bytes) {
        Ok(p) => p,
        Err(e) => {
            return Evaluated {
                term: Err(e.to_string()),
                budget: ExBudget::new(0, 0),
                logs: vec![],
                invalid_program: true,
                exhausted: None,
            };
        }
    };
    let program = match arg {
        Some(a) => program.apply(arena, a),
        None => program,
    };
    let result = program.eval_version_budget(arena, version, budget);
    let exhausted = matches!(&result.term, Err(MachineError::OutOfExError(_))).then_some(budget);
    Evaluated {
        term: result.term.map_err(|e| e.to_string()),
        budget: result.info.consumed_budget,
        logs: result.info.logs,
        invalid_program: false,
        exhausted,
    }
}
#[derive(Debug)]
pub struct Prepared<'a> {
    pub prng: Prng,
    body: &'a Term<'a, DeBruijn>,
    show: &'a Term<'a, DeBruijn>,
    budget: ExBudget,
    logs: Vec<String>,
}

pub fn prepare<'a>(
    arena: &'a Arena,
    version: PlutusVersion,
    bytes: &[u8],
    prng: &Prng,
    budget: ExBudget,
) -> Result<Option<Prepared<'a>>, Failure> {
    let ev = evaluate_with_budget(arena, version, bytes, Some(prng.to_term(arena)), budget);
    if let Some(failure) = ev.exhaustion_failure() {
        return Err(failure);
    }
    if ev.invalid_program {
        return Err(Failure::InvalidProgram {
            message: ev.term.unwrap_err(),
        });
    }
    let term = ev.term.map_err(|message| Failure::Generator { message })?;
    match term {
        Term::Constr {
            tag: 0,
            fields:
                [
                    Term::Constr {
                        tag: 0,
                        fields: [prng, body, show],
                    },
                ],
        } => {
            let prng = Prng::from_term(prng).map_err(|message| Failure::Generator { message })?;
            Ok(Some(Prepared {
                prng,
                body,
                show,
                budget: ev.budget,
                logs: ev.logs,
            }))
        }
        Term::Constr { tag: 1, fields: [] } => Ok(None),
        _ => Err(Failure::Generator {
            message: "malformed prepared property".into(),
        }),
    }
}

fn call<'a>(
    arena: &'a Arena,
    version: PlutusVersion,
    thunk: &'a Term<'a, DeBruijn>,
    budget: ExBudget,
) -> Evaluated<'a> {
    let result = Program::new(
        arena,
        Version::plutus_v3(arena),
        thunk.apply(arena, Term::unit(arena)),
    )
    .eval_version_budget(arena, version, budget);
    let exhausted = matches!(&result.term, Err(MachineError::OutOfExError(_))).then_some(budget);
    Evaluated {
        term: result.term.map_err(|e| e.to_string()),
        budget: result.info.consumed_budget,
        logs: result.info.logs,
        invalid_program: false,
        exhausted,
    }
}

pub fn run_prepared<'a>(
    arena: &'a Arena,
    version: PlutusVersion,
    prepared: &Prepared<'a>,
    budget: ExBudget,
) -> Evaluated<'a> {
    let mut ev = call(arena, version, prepared.body, budget - prepared.budget);
    ev.budget = ExBudget::new(
        ev.budget.cpu + prepared.budget.cpu,
        ev.budget.mem + prepared.budget.mem,
    );
    ev.exhausted = ev.exhausted.map(|_| budget);
    let mut logs = prepared.logs.clone();
    logs.append(&mut ev.logs);
    ev.logs = logs;
    ev
}

pub fn show_prepared(
    arena: &Arena,
    version: PlutusVersion,
    prepared: &Prepared<'_>,
    budget: ExBudget,
) -> Result<Vec<String>, Failure> {
    let ev = call(arena, version, prepared.show, budget);
    if let Some(failure) = ev.exhaustion_failure() {
        return Err(failure);
    }
    let shown = ev.term.and_then(|term| match term {
        Term::Constant(Constant::ProtoList(_, items)) => items
            .iter()
            .map(|c| match c {
                Constant::String(s) => Ok((*s).to_string()),
                _ => Err("non-string draw value".into()),
            })
            .collect(),
        _ => Err("malformed property display result".into()),
    });
    shown.map_err(|message| Failure::Generator { message })
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Logs {
    pub labels: Vec<String>,
    pub asserts: Vec<(u32, u32, String)>,
    pub assert_id: Option<u32>,
    pub traces: Vec<String>,
}
pub fn split_logs(logs: Vec<String>) -> Logs {
    let mut out = Logs::default();
    for line in logs {
        if let Some(rest) = line.strip_prefix("\0label\0") {
            out.labels.push(rest.into());
            continue;
        }
        if let Some(rest) = line.strip_prefix("\0assert\0") {
            let mut parts = rest.splitn(3, '\0');
            if let Some(id) = parts.next().and_then(|s| s.parse().ok()) {
                match (parts.next(), parts.next()) {
                    (None, None) => {
                        out.assert_id.get_or_insert(id);
                        continue;
                    }
                    (Some(index), Some(value)) => {
                        if let Ok(index) = index.parse() {
                            out.assert_id.get_or_insert(id);
                            out.asserts.push((id, index, value.into()));
                            continue;
                        }
                    }
                    _ => {}
                }
            }
        }
        out.traces.push(line);
    }
    out
}
