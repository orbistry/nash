use crate::{Budget, Failure, prng::Prng};
use nash_plutus::{
    arena::Arena,
    binder::DeBruijn,
    constant::Constant,
    flat,
    machine::{ExBudget, MachineError, PlutusVersion},
    program::Program,
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
pub enum Drawn {
    Some { prng: Prng, shown: Vec<String> },
    None,
}
#[derive(Debug)]
pub enum Ran {
    Some(Prng),
    None,
}
pub fn decode_ran(term: &Term<'_, DeBruijn>) -> Result<Ran, String> {
    match term {
        Term::Constr {
            tag: 0,
            fields: [Term::Constant(Constant::Data(p))],
        } => Ok(Ran::Some(Prng::from_data(p)?)),
        Term::Constr { tag: 1, fields: [] } => Ok(Ran::None),
        _ => Err("malformed property run result".into()),
    }
}
pub fn run_draw(version: PlutusVersion, bytes: &[u8], prng: &Prng) -> Result<Drawn, Failure> {
    run_draw_with_budget(version, bytes, prng, ExBudget::max())
}
pub(crate) fn run_draw_with_budget(
    version: PlutusVersion,
    bytes: &[u8],
    prng: &Prng,
    budget: ExBudget,
) -> Result<Drawn, Failure> {
    let arena = Arena::new();
    let ev = evaluate_with_budget(
        &arena,
        version,
        bytes,
        Some(Term::data(&arena, prng.to_data(&arena))),
        budget,
    );
    if let Some(failure) = ev.exhaustion_failure() {
        return Err(failure);
    }
    ev.term
        .and_then(decode_drawn)
        .map_err(|message| Failure::Generator { message })
}
fn decode_drawn(term: &Term<'_, DeBruijn>) -> Result<Drawn, String> {
    match term {
        Term::Constr {
            tag: 0,
            fields:
                [
                    Term::Constr {
                        tag: 0,
                        fields:
                            [
                                Term::Constant(Constant::Data(p)),
                                Term::Constant(Constant::ProtoList(_, items)),
                            ],
                    },
                ],
        } => Ok(Drawn::Some {
            prng: Prng::from_data(p)?,
            shown: items
                .iter()
                .map(|c| match c {
                    Constant::String(s) => Ok((*s).to_string()),
                    _ => Err("non-string draw value".into()),
                })
                .collect::<Result<_, String>>()?,
        }),
        Term::Constr { tag: 1, fields: [] } => Ok(Drawn::None),
        _ => Err("malformed property draw result".into()),
    }
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
