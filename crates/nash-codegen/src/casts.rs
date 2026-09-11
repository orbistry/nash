//! Expand intrinsic casts and share shallow/full Data validation functions.
//! Ordinary source Lift implementations never enter this pass as Cast nodes.
use crate::ty_of::{TypeEnv, TypeError};
use nash_ir::{
    build::Builder,
    core::*,
    ty::{BigTy, ConstTy, TermTy, Ty},
};
use nash_plutus::{
    builtin::DefaultFunction as F,
    constant::{self, Constant},
};
use std::collections::{HashMap, HashSet};

const DATA: Ty<'static> = Ty::Big(&BigTy::Data);
const INT: Ty<'static> = Ty::Const(&ConstTy::Int);
const BYTES: Ty<'static> = Ty::Const(&ConstTy::Bytes);
const UNIT: Ty<'static> = Ty::Const(&ConstTy::Unit);
const DATA_LIST: Ty<'static> = Ty::Const(&ConstTy::List(DATA));
const DATA_PAIR: Ty<'static> = Ty::Const(&ConstTy::Pair(DATA, DATA));
const DATA_MAP: Ty<'static> = Ty::Const(&ConstTy::List(DATA_PAIR));
/// An operational compiler-resource limit, not a proof of type-level infinity.
const MAX_VALIDATION_HELPERS: usize = 256;

#[derive(Debug, thiserror::Error)]
pub enum Error<'a> {
    #[error("invalid intrinsic {kind:?} cast from {from} to {to}")]
    InvalidCast {
        kind: CastKind,
        from: Box<Ty<'a>>,
        to: Box<Ty<'a>>,
    },
    #[error("Data validation requires a concrete Big type, got {0}")]
    InvalidValidationType(Ty<'a>),
    #[error("{0}")]
    Type(TypeError<'a>),
    #[error("Data validation exceeded the {MAX_VALIDATION_HELPERS}-helper layout generation limit")]
    ValidationLayoutLimit,
}

pub fn expand<'a>(
    build: &Builder<'a>,
    types: &mut TypeEnv<'a, '_>,
    core: &'a Core<'a>,
) -> Result<&'a Core<'a>, Error<'a>> {
    expand_with_traces(build, types, core, true)
}

pub fn expand_with_traces<'a>(
    build: &Builder<'a>,
    types: &mut TypeEnv<'a, '_>,
    core: &'a Core<'a>,
    compiler_traces: bool,
) -> Result<&'a Core<'a>, Error<'a>> {
    let mut expander = Expander {
        build,
        types,
        compiler_traces,
        used: input_names(core),
        checkers: HashMap::new(),
        bindings: Vec::new(),
        pending: Vec::new(),
        each: None,
        each_map: None,
    };
    let core = expander.term(core)?;
    // Demand helpers through a worklist: growing type metadata reaches the
    // explicit limit without consuming a host call frame per instantiation.
    while let Some((full, ty, binder)) = expander.pending.pop() {
        let d = expander.fresh("data", DATA);
        let body = expander.checker_body(full, ty, d)?;
        expander.bindings.push(RecBinder {
            binder,
            params: build.arena.alloc_slice_copy(&[d]),
            static_params: &[],
            body,
        });
    }
    Ok(if expander.bindings.is_empty() {
        core
    } else {
        build.let_rec(&expander.bindings, core)
    })
}

struct Expander<'b, 'a, 'env> {
    build: &'b Builder<'a>,
    types: &'b mut TypeEnv<'a, 'env>,
    compiler_traces: bool,
    used: HashSet<u32>,
    checkers: HashMap<(bool, Ty<'a>), Binder<'a>>,
    bindings: Vec<RecBinder<'a>>,
    pending: Vec<(bool, Ty<'a>, Binder<'a>)>,
    each: Option<Binder<'a>>,
    each_map: Option<Binder<'a>>,
}
impl<'a> Expander<'_, 'a, '_> {
    fn fresh(&mut self, text: &'a str, ty: Ty<'a>) -> Binder<'a> {
        loop {
            let name = self.build.fresh(text);
            if self.used.insert(name.unique) {
                return Binder { name, ty };
            }
        }
    }
    fn fun_ty(&self, args: &[Ty<'a>], result: Ty<'a>) -> Ty<'a> {
        Ty::Term(
            self.build
                .arena
                .alloc(TermTy::Fun(self.build.arena.alloc_slice_copy(args), result)),
        )
    }
    fn fail(&self, full: bool, ty: Ty<'a>, reason: &str) -> &'a Core<'a> {
        let b = self.build;
        if self.compiler_traces {
            let message = b.arena.as_bump().alloc_str(&format!(
                "{}: {ty}: {reason}",
                if full { "validateData" } else { "fromData" }
            ));
            b.trace(b.lit(Constant::string(b.arena, message)), b.error())
        } else {
            b.error()
        }
    }
    fn term(&mut self, core: &'a Core<'a>) -> Result<&'a Core<'a>, Error<'a>> {
        let b = self.build;
        Ok(match core {
            Core::Cast {
                kind,
                from,
                to,
                arg,
            } => {
                let arg = self.term(arg)?;
                self.cast(*kind, *from, *to, arg)?
            }
            Core::Var(_) | Core::Lit(_) | Core::Error => core,
            Core::Lam { params, body } => b.lam(params, self.term(body)?),
            Core::App { func, args } => {
                let func = self.term(func)?;
                let args = args
                    .iter()
                    .map(|a| self.term(a))
                    .collect::<Result<Vec<_>, _>>()?;
                b.app(func, &args)
            }
            Core::Let {
                binder,
                value,
                body,
            } => b.let_(*binder, self.term(value)?, self.term(body)?),
            Core::LetRec { binders, body } => {
                let binders = binders
                    .iter()
                    .map(|rb| {
                        Ok(RecBinder {
                            body: self.term(rb.body)?,
                            ..*rb
                        })
                    })
                    .collect::<Result<Vec<_>, Error<'a>>>()?;
                b.let_rec(&binders, self.term(body)?)
            }
            Core::Case {
                kind,
                scrutinee,
                branches,
                default,
            } => {
                let scrutinee = self.term(scrutinee)?;
                let branches = branches
                    .iter()
                    .map(|branch| {
                        Ok(Branch {
                            body: self.term(branch.body)?,
                            ..*branch
                        })
                    })
                    .collect::<Result<Vec<_>, Error<'a>>>()?;
                let default = default.map(|body| self.term(body)).transpose()?;
                b.case(*kind, scrutinee, &branches, default)
            }
            Core::Constr { tag, fields } => {
                let fields = fields
                    .iter()
                    .map(|f| self.term(f))
                    .collect::<Result<Vec<_>, _>>()?;
                b.constr(*tag, &fields)
            }
            Core::Field {
                record,
                index,
                arity,
            } => b.arena.alloc(Core::Field {
                record: self.term(record)?,
                index: *index,
                arity: *arity,
            }),
            Core::Builtin { func, args } => {
                let args = args
                    .iter()
                    .map(|a| self.term(a))
                    .collect::<Result<Vec<_>, _>>()?;
                b.arena.alloc(Core::Builtin {
                    func: *func,
                    args: b.arena.alloc_slice_copy(&args),
                })
            }
            Core::Trace { message, body } => b.trace(self.term(message)?, self.term(body)?),
            Core::Delay(body) => b.delay(self.term(body)?),
            Core::Force(body) => b.force(self.term(body)?),
        })
    }
    fn cast(
        &mut self,
        kind: CastKind,
        from: Ty<'a>,
        to: Ty<'a>,
        arg: &'a Core<'a>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        let b = self.build;
        match kind {
            CastKind::ToData if matches!(from, Ty::Big(_)) && to == DATA => return Ok(arg),
            CastKind::FromDataShallow | CastKind::ValidateData
                if from == DATA && matches!(to, Ty::Big(_)) =>
            {
                if to == DATA {
                    return Ok(arg);
                }
                let check = self.checker(kind == CastKind::ValidateData, to)?;
                return Ok(b.app(b.var(check.name), &[arg]));
            }
            CastKind::Lift | CastKind::Lower => {
                if from == to && matches!(from, Ty::Big(_)) {
                    return Ok(arg);
                }
                let (small, big) = if kind == CastKind::Lift {
                    (from, to)
                } else {
                    (to, from)
                };
                let funcs = match (small, big) {
                    (Ty::Const(ConstTy::Int), Ty::Big(BigTy::Int)) => Some((F::IData, F::UnIData)),
                    (Ty::Const(ConstTy::Bytes), Ty::Big(BigTy::Bytes)) => {
                        Some((F::BData, F::UnBData))
                    }
                    (Ty::Const(ConstTy::List(a)), Ty::Big(BigTy::List(b)))
                        if a == b && matches!(a, Ty::Big(_)) =>
                    {
                        Some((F::ListData, F::UnListData))
                    }
                    (
                        Ty::Const(ConstTy::List(Ty::Const(ConstTy::Pair(k, v)))),
                        Ty::Big(BigTy::Map(bk, bv)),
                    ) if k == bk
                        && v == bv
                        && matches!(k, Ty::Big(_))
                        && matches!(v, Ty::Big(_)) =>
                    {
                        Some((F::MapData, F::UnMapData))
                    }
                    _ => None,
                };
                if let Some((lift, lower)) = funcs {
                    return Ok(b.builtin(if kind == CastKind::Lift { lift } else { lower }, &[arg]));
                }
            }
            _ => {}
        }
        Err(Error::InvalidCast {
            kind,
            from: Box::new(from),
            to: Box::new(to),
        })
    }
    fn checker(&mut self, full: bool, ty: Ty<'a>) -> Result<Binder<'a>, Error<'a>> {
        if !matches!(ty, Ty::Big(_)) {
            return Err(Error::InvalidValidationType(ty));
        }
        if let Some(checker) = self.checkers.get(&(full, ty)) {
            return Ok(*checker);
        }
        // Cache lookup comes first: regular recursive layouts close their cycle.
        if self.checkers.len() >= MAX_VALIDATION_HELPERS {
            return Err(Error::ValidationLayoutLimit);
        }
        let binder = self.fresh(
            if full { "validateData" } else { "fromData" },
            self.fun_ty(&[DATA], ty),
        );
        self.checkers.insert((full, ty), binder);
        self.pending.push((full, ty, binder));
        Ok(binder)
    }
    fn checker_body(
        &mut self,
        full: bool,
        ty: Ty<'a>,
        d: Binder<'a>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        let b = self.build;
        let Ty::Big(big) = ty else {
            return Err(Error::InvalidValidationType(ty));
        };
        let (shape, params, body, reason) = match big {
            BigTy::Data => return Ok(b.var(d.name)),
            BigTy::Int => (
                Test::DataI,
                vec![self.fresh("integer", INT)],
                b.var(d.name),
                "expected I",
            ),
            BigTy::Bytes => (
                Test::DataB,
                vec![self.fresh("bytes", BYTES)],
                b.var(d.name),
                "expected B",
            ),
            BigTy::List(elem) => {
                let xs = self.fresh("items", DATA_LIST);
                let body = if full {
                    let checker = self.checker(true, *elem)?;
                    let each = self.each();
                    let checked = self.fresh("checked", UNIT);
                    b.let_(
                        checked,
                        b.app(b.var(each.name), &[b.var(checker.name), b.var(xs.name)]),
                        b.var(d.name),
                    )
                } else {
                    b.var(d.name)
                };
                (Test::DataList, vec![xs], body, "expected List")
            }
            BigTy::Map(key, val) => {
                let xs = self.fresh("entries", DATA_MAP);
                let body = if full {
                    let key = self.checker(true, *key)?;
                    let val = self.checker(true, *val)?;
                    let each = self.each_map();
                    let checked = self.fresh("checked", UNIT);
                    b.let_(
                        checked,
                        b.app(
                            b.var(each.name),
                            &[b.var(key.name), b.var(val.name), b.var(xs.name)],
                        ),
                        b.var(d.name),
                    )
                } else {
                    b.var(d.name)
                };
                (Test::DataMap, vec![xs], body, "expected Map")
            }
            BigTy::Record(fields) => {
                let xs = self.fresh("fields", DATA_LIST);
                let body = self.fields(full, ty, fields, xs, b.var(d.name))?;
                (Test::DataList, vec![xs], body, "expected List")
            }
            BigTy::Adt(adt) => {
                let layouts = self.types.layout(*adt).map_err(Error::Type)?;
                let tag = self.fresh("tag", INT);
                let xs = self.fresh("fields", DATA_LIST);
                let branches = layouts
                    .iter()
                    .enumerate()
                    .map(|(index, fields)| {
                        Ok(Branch {
                            test: Test::Int(constant::integer_from(b.arena, index as i128)),
                            binders: &[],
                            body: self.fields(full, ty, fields, xs, b.var(d.name))?,
                        })
                    })
                    .collect::<Result<Vec<_>, Error<'a>>>()?;
                let body = b.case(
                    CaseKind::Int,
                    b.var(tag.name),
                    &branches,
                    Some(self.fail(full, ty, "invalid constructor tag")),
                );
                (Test::DataConstr, vec![tag, xs], body, "expected Constr")
            }
        };
        Ok(b.case(
            CaseKind::Data,
            b.var(d.name),
            &[Branch {
                test: shape,
                binders: b.arena.alloc_slice_copy(&params),
                body,
            }],
            Some(self.fail(full, ty, reason)),
        ))
    }
    fn fields(
        &mut self,
        full: bool,
        ty: Ty<'a>,
        fields: &[Ty<'a>],
        xs: Binder<'a>,
        ok: &'a Core<'a>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        let b = self.build;
        let tails = (0..fields.len())
            .map(|_| self.fresh("rest", DATA_LIST))
            .collect::<Vec<_>>();
        let end = tails.last().copied().unwrap_or(xs);
        let fail = self.fail(full, ty, "wrong field count");
        let mut body = b.if_(b.builtin(F::NullList, &[b.var(end.name)]), ok, fail);
        for index in (0..fields.len()).rev() {
            let current = if index == 0 { xs } else { tails[index - 1] };
            body = b.let_(
                tails[index],
                b.builtin(F::TailList, &[b.var(current.name)]),
                body,
            );
            if full {
                let checker = self.checker(true, fields[index])?;
                let checked = self.fresh("checked", fields[index]);
                body = b.let_(
                    checked,
                    b.app(
                        b.var(checker.name),
                        &[b.builtin(F::HeadList, &[b.var(current.name)])],
                    ),
                    body,
                );
            }
            body = b.if_(b.builtin(F::NullList, &[b.var(current.name)]), fail, body);
        }
        Ok(body)
    }
    fn each(&mut self) -> Binder<'a> {
        if let Some(each) = self.each {
            return each;
        }
        let b = self.build;
        let check_ty = self.fun_ty(&[DATA], DATA);
        let each = self.fresh("validateEach", self.fun_ty(&[check_ty, DATA_LIST], UNIT));
        self.each = Some(each);
        let check = self.fresh("check", check_ty);
        let xs = self.fresh("items", DATA_LIST);
        let checked = self.fresh("checked", DATA);
        let next = b.app(
            b.var(each.name),
            &[b.var(check.name), b.builtin(F::TailList, &[b.var(xs.name)])],
        );
        let body = b.if_(
            b.builtin(F::NullList, &[b.var(xs.name)]),
            b.lit(Constant::unit(b.arena)),
            b.let_(
                checked,
                b.app(
                    b.var(check.name),
                    &[b.builtin(F::HeadList, &[b.var(xs.name)])],
                ),
                next,
            ),
        );
        self.bindings.push(RecBinder {
            binder: each,
            params: b.arena.alloc_slice_copy(&[check, xs]),
            static_params: &[0],
            body,
        });
        each
    }
    fn each_map(&mut self) -> Binder<'a> {
        if let Some(each) = self.each_map {
            return each;
        }
        let b = self.build;
        let check_ty = self.fun_ty(&[DATA], DATA);
        let each = self.fresh(
            "validateEntries",
            self.fun_ty(&[check_ty, check_ty, DATA_MAP], UNIT),
        );
        self.each_map = Some(each);
        let key = self.fresh("checkKey", check_ty);
        let val = self.fresh("checkValue", check_ty);
        let xs = self.fresh("entries", DATA_MAP);
        let pair = self.fresh("entry", DATA_PAIR);
        let checked_key = self.fresh("checkedKey", DATA);
        let checked_val = self.fresh("checkedValue", DATA);
        let next = b.app(
            b.var(each.name),
            &[
                b.var(key.name),
                b.var(val.name),
                b.builtin(F::TailList, &[b.var(xs.name)]),
            ],
        );
        let check_val = b.let_(
            checked_val,
            b.app(
                b.var(val.name),
                &[b.builtin(F::SndPair, &[b.var(pair.name)])],
            ),
            next,
        );
        let check_key = b.let_(
            checked_key,
            b.app(
                b.var(key.name),
                &[b.builtin(F::FstPair, &[b.var(pair.name)])],
            ),
            check_val,
        );
        let step = b.let_(pair, b.builtin(F::HeadList, &[b.var(xs.name)]), check_key);
        let body = b.if_(
            b.builtin(F::NullList, &[b.var(xs.name)]),
            b.lit(Constant::unit(b.arena)),
            step,
        );
        self.bindings.push(RecBinder {
            binder: each,
            params: b.arena.alloc_slice_copy(&[key, val, xs]),
            static_params: &[0, 1],
            body,
        });
        each
    }
}

fn input_names(core: &Core<'_>) -> HashSet<u32> {
    let mut names = HashSet::new();
    let mut pending = vec![core];
    while let Some(node) = pending.pop() {
        match node {
            Core::Var(n) => {
                names.insert(n.unique);
            }
            Core::Lam { params, body } => {
                names.extend(params.iter().map(|p| p.name.unique));
                pending.push(body);
            }
            Core::App { func, args } => {
                pending.push(func);
                pending.extend_from_slice(args);
            }
            Core::Let {
                binder,
                value,
                body,
            } => {
                names.insert(binder.name.unique);
                pending.extend([*value, *body]);
            }
            Core::LetRec { binders, body } => {
                for rb in *binders {
                    names.insert(rb.binder.name.unique);
                    names.extend(rb.params.iter().map(|p| p.name.unique));
                    pending.push(rb.body);
                }
                pending.push(body);
            }
            Core::Case {
                scrutinee,
                branches,
                default,
                ..
            } => {
                pending.push(scrutinee);
                for branch in *branches {
                    names.extend(branch.binders.iter().map(|p| p.name.unique));
                    pending.push(branch.body);
                }
                pending.extend(default.iter().copied());
            }
            Core::Constr { fields, .. } => pending.extend_from_slice(fields),
            Core::Builtin { args, .. } => pending.extend_from_slice(args),
            Core::Field { record, .. } => pending.push(record),
            Core::Cast { arg, .. } => pending.push(arg),
            Core::Trace { message, body } => pending.extend([*message, *body]),
            Core::Delay(body) | Core::Force(body) => pending.push(body),
            Core::Lit(_) | Core::Error => {}
        }
    }
    names
}

#[cfg(test)]
#[path = "casts_tests.rs"]
mod tests;
