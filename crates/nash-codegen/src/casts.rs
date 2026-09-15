//! Expand intrinsic casts and share shallow/full Data validation functions.
//! Ordinary source Lift implementations never enter this pass as Cast nodes.
use crate::ty_of::{TypeEnv, TypeError};
use nash_ast::DataEncoding;
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

mod matching;
pub(crate) use matching::data_matches;

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
    #[error("Data conversion requires a concrete supported runtime type, got {0}")]
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
        mappers: HashMap::new(),
        pending: Vec::new(),
        each: None,
        each_map: None,
    };
    let core = expander.term(core)?;
    // Demand helpers through a worklist: growing type metadata reaches the
    // explicit limit without consuming a host call frame per instantiation.
    while let Some((full, check_only, ty, binder)) = expander.pending.pop() {
        let d = expander.fresh("data", DATA);
        let body = expander.checker_body(full, check_only, ty, d)?;
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
    checkers: HashMap<(bool, bool, Ty<'a>), Binder<'a>>,
    bindings: Vec<RecBinder<'a>>,
    pending: Vec<(bool, bool, Ty<'a>, Binder<'a>)>,
    mappers: HashMap<(Ty<'a>, Ty<'a>), Binder<'a>>,
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
    fn data_case(
        &self,
        full: bool,
        ty: Ty<'a>,
        data: Binder<'a>,
        branch: Branch<'a>,
        reason: &str,
    ) -> &'a Core<'a> {
        self.build.case(
            CaseKind::Data,
            self.build.var(data.name),
            &[branch],
            Some(self.fail(full, ty, reason)),
        )
    }

    fn list(&self, element: Ty<'a>, values: &[&'a Core<'a>]) -> Result<&'a Core<'a>, Error<'a>> {
        let b = self.build;
        let typ = element
            .plutus_type(b.arena)
            .ok_or(Error::InvalidValidationType(element))?;
        let mut out = b.lit(Constant::proto_list(b.arena, typ, &[]));
        for value in values.iter().rev() {
            out = b.builtin(F::MkCons, &[value, out]);
        }
        Ok(out)
    }

    fn encode(&mut self, ty: Ty<'a>, value: &'a Core<'a>) -> Result<&'a Core<'a>, Error<'a>> {
        let b = self.build;
        match ty {
            Ty::Big(_) => return Ok(value),
            Ty::Const(ConstTy::Int) => return Ok(b.builtin(F::IData, &[value])),
            Ty::Const(ConstTy::Bytes) => return Ok(b.builtin(F::BData, &[value])),
            Ty::Const(ConstTy::String) => {
                return Ok(b.builtin(F::BData, &[b.builtin(F::EncodeUtf8, &[value])]));
            }
            Ty::Const(ConstTy::BlsG1) => {
                return Ok(b.builtin(F::BData, &[b.builtin(F::Bls12_381_G1_Compress, &[value])]));
            }
            Ty::Const(ConstTy::BlsG2) => {
                return Ok(b.builtin(F::BData, &[b.builtin(F::Bls12_381_G2_Compress, &[value])]));
            }
            _ => {}
        }
        let input = self.fresh("value", ty);
        let v = b.var(input.name);
        let body = match ty {
            Ty::Const(ConstTy::Bool) => b.builtin(
                F::ConstrData,
                &[b.if_(v, b.int(1), b.int(0)), self.list(DATA, &[])?],
            ),
            Ty::Const(ConstTy::Unit) => {
                b.builtin(F::ConstrData, &[b.int(0), self.list(DATA, &[])?])
            }
            Ty::Const(ConstTy::DataPair(_, _)) => b.builtin(
                F::ListData,
                &[self.list(
                    DATA,
                    &[b.builtin(F::FstPair, &[v]), b.builtin(F::SndPair, &[v])],
                )?],
            ),
            Ty::Const(ConstTy::Pair(first, second)) => {
                let first = self.encode(*first, b.builtin(F::FstPair, &[v]))?;
                let second = self.encode(*second, b.builtin(F::SndPair, &[v]))?;
                b.builtin(F::ListData, &[self.list(DATA, &[first, second])?])
            }
            Ty::Const(
                ConstTy::List(Ty::Const(ConstTy::DataPair(_, _)))
                | ConstTy::DataList(Ty::Const(ConstTy::DataPair(_, _))),
            ) => b.builtin(F::MapData, &[v]),
            Ty::Const(ConstTy::DataList(_) | ConstTy::DataTuple(_)) => b.builtin(F::ListData, &[v]),
            Ty::Const(ConstTy::List(element)) => {
                let items = if matches!(element, Ty::Big(_)) {
                    v
                } else {
                    let item = self.fresh("item", *element);
                    let encoded = self.encode(*element, b.var(item.name))?;
                    self.map(*element, DATA, b.lam(&[item], encoded), v)?
                };
                b.builtin(F::ListData, &[items])
            }
            Ty::Term(TermTy::Tuple(fields) | TermTy::Record(fields)) => {
                let arity =
                    u16::try_from(fields.len()).map_err(|_| Error::InvalidValidationType(ty))?;
                let values = fields
                    .iter()
                    .enumerate()
                    .map(|(i, field)| self.encode(*field, b.field(v, i as u16, arity)))
                    .collect::<Result<Vec<_>, _>>()?;
                b.builtin(F::ListData, &[self.list(DATA, &values)?])
            }
            _ => return Err(Error::InvalidValidationType(ty)),
        };
        Ok(b.let_(input, value, body))
    }

    /// Share a typed list mapper; element conversions are passed as its static argument.
    fn map(
        &mut self,
        from: Ty<'a>,
        to: Ty<'a>,
        function: &'a Core<'a>,
        values: &'a Core<'a>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        let b = self.build;
        let mapper = if let Some(mapper) = self.mappers.get(&(from, to)) {
            *mapper
        } else {
            let from_list = Ty::Const(b.arena.alloc(ConstTy::List(from)));
            let to_list = Ty::Const(b.arena.alloc(ConstTy::List(to)));
            let function_ty = self.fun_ty(&[from], to);
            let mapper = self.fresh("mapData", self.fun_ty(&[function_ty, from_list], to_list));
            self.mappers.insert((from, to), mapper);
            let f = self.fresh("convert", function_ty);
            let xs = self.fresh("items", from_list);
            let body = b.if_(
                b.builtin(F::NullList, &[b.var(xs.name)]),
                self.list(to, &[])?,
                b.builtin(
                    F::MkCons,
                    &[
                        b.app(b.var(f.name), &[b.builtin(F::HeadList, &[b.var(xs.name)])]),
                        b.app(
                            b.var(mapper.name),
                            &[b.var(f.name), b.builtin(F::TailList, &[b.var(xs.name)])],
                        ),
                    ],
                ),
            );
            self.bindings.push(RecBinder {
                binder: mapper,
                params: b.arena.alloc_slice_copy(&[f, xs]),
                static_params: &[0],
                body,
            });
            mapper
        };
        Ok(b.app(b.var(mapper.name), &[function, values]))
    }

    fn decode_value(
        &mut self,
        full: bool,
        check_only: bool,
        ty: Ty<'a>,
        d: Binder<'a>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        let b = self.build;
        let result = |value: Binder<'a>| {
            if check_only {
                b.var(d.name)
            } else {
                b.var(value.name)
            }
        };
        let (shape, params, body, reason) = match ty {
            Ty::Const(ConstTy::Int) => {
                let value = self.fresh("integer", INT);
                (Test::DataI, vec![value], result(value), "expected I")
            }
            Ty::Const(ConstTy::Bytes | ConstTy::String) => {
                let value = self.fresh("bytes", BYTES);
                let body = if matches!(ty, Ty::Const(ConstTy::String)) {
                    let decoded = b.builtin(F::DecodeUtf8, &[b.var(value.name)]);
                    if check_only {
                        let checked = self.fresh("checked", ty);
                        b.let_(checked, decoded, b.var(d.name))
                    } else {
                        decoded
                    }
                } else {
                    result(value)
                };
                (Test::DataB, vec![value], body, "expected B")
            }
            Ty::Const(ConstTy::BlsG1 | ConstTy::BlsG2) => {
                let value = self.fresh("bytes", BYTES);
                let uncompress = if matches!(ty, Ty::Const(ConstTy::BlsG1)) {
                    F::Bls12_381_G1_Uncompress
                } else {
                    F::Bls12_381_G2_Uncompress
                };
                // Like the pinned cast, bytes of an invalid compressed point
                // fail inside uncompress; checking the Data shape alone is not
                // enough to validate a curve value.
                let decoded = b.builtin(uncompress, &[b.var(value.name)]);
                let body = if check_only {
                    let checked = self.fresh("checked", ty);
                    b.let_(checked, decoded, b.var(d.name))
                } else {
                    decoded
                };
                (Test::DataB, vec![value], body, "expected B")
            }
            Ty::Const(ConstTy::Unit) if !full => {
                return Ok(b.lit(Constant::unit(b.arena)));
            }
            Ty::Const(ConstTy::Bool) if !full => {
                let tag = self.fresh("tag", INT);
                let xs = self.fresh("fields", DATA_LIST);
                let body = b.builtin(F::EqualsInteger, &[b.int(1), b.var(tag.name)]);
                (Test::DataConstr, vec![tag, xs], body, "expected Constr")
            }
            Ty::Const(ConstTy::Bool | ConstTy::Unit) => {
                let tag = self.fresh("tag", INT);
                let xs = self.fresh("fields", DATA_LIST);
                let boolean = matches!(ty, Ty::Const(ConstTy::Bool));
                let branches = (0..if boolean { 2 } else { 1 })
                    .map(|tag| Branch {
                        test: Test::Int(constant::integer_from(b.arena, tag)),
                        binders: &[],
                        body: if check_only {
                            b.var(d.name)
                        } else if boolean {
                            b.lit(Constant::bool(b.arena, tag == 1))
                        } else {
                            b.lit(Constant::unit(b.arena))
                        },
                    })
                    .collect::<Vec<_>>();
                let body = b.case(
                    CaseKind::Int,
                    b.var(tag.name),
                    &branches,
                    Some(self.fail(full, ty, "invalid constructor tag")),
                );
                let body = self.fields(full, ty, &[], xs, body)?;
                (Test::DataConstr, vec![tag, xs], body, "expected Constr")
            }
            Ty::Const(
                ConstTy::List(Ty::Const(ConstTy::DataPair(key, value)))
                | ConstTy::DataList(Ty::Const(ConstTy::DataPair(key, value))),
            ) => {
                let xs = self.fresh("entries", DATA_MAP);
                let body = if full {
                    let key = self.validator(*key)?;
                    let value = self.validator(*value)?;
                    let each = self.each_map();
                    let checked = self.fresh("checked", UNIT);
                    b.let_(
                        checked,
                        b.app(
                            b.var(each.name),
                            &[b.var(key.name), b.var(value.name), b.var(xs.name)],
                        ),
                        result(xs),
                    )
                } else {
                    result(xs)
                };
                (Test::DataMap, vec![xs], body, "expected Map")
            }
            Ty::Const(ConstTy::DataList(element)) => {
                let xs = self.fresh("items", DATA_LIST);
                let body = if full {
                    let checker = self.validator(*element)?;
                    let each = self.each();
                    let checked = self.fresh("checked", UNIT);
                    b.let_(
                        checked,
                        b.app(b.var(each.name), &[b.var(checker.name), b.var(xs.name)]),
                        result(xs),
                    )
                } else {
                    result(xs)
                };
                (Test::DataList, vec![xs], body, "expected List")
            }
            Ty::Const(ConstTy::List(element)) => {
                let xs = self.fresh("items", DATA_LIST);
                let body = if check_only || matches!(element, Ty::Big(_)) {
                    if full {
                        let checker = self.validator(*element)?;
                        let each = self.each();
                        let checked = self.fresh("checked", UNIT);
                        b.let_(
                            checked,
                            b.app(b.var(each.name), &[b.var(checker.name), b.var(xs.name)]),
                            result(xs),
                        )
                    } else {
                        result(xs)
                    }
                } else {
                    let checker = self.checker(full, *element)?;
                    self.map(DATA, *element, b.var(checker.name), b.var(xs.name))?
                };
                (Test::DataList, vec![xs], body, "expected List")
            }
            Ty::Const(ConstTy::DataPair(first, second) | ConstTy::Pair(first, second))
                if matches!(ty, Ty::Const(ConstTy::DataPair(..)))
                    || (matches!(first, Ty::Big(_)) && matches!(second, Ty::Big(_))) =>
            {
                let xs = self.fresh("fields", DATA_LIST);
                let pair = if check_only {
                    b.var(d.name)
                } else {
                    b.builtin(
                        F::MkPairData,
                        &[
                            b.builtin(F::HeadList, &[b.var(xs.name)]),
                            b.builtin(F::HeadList, &[b.builtin(F::TailList, &[b.var(xs.name)])]),
                        ],
                    )
                };
                let body = if !full && matches!(ty, Ty::Const(ConstTy::DataPair(..))) {
                    pair
                } else {
                    self.fields(full, ty, &[*first, *second], xs, pair)?
                };
                (Test::DataList, vec![xs], body, "expected List")
            }
            Ty::Const(ConstTy::DataTuple(fields)) => {
                let xs = self.fresh("fields", DATA_LIST);
                let body = if full {
                    self.fields(true, ty, fields, xs, result(xs))?
                } else {
                    result(xs)
                };
                (Test::DataList, vec![xs], body, "expected List")
            }
            Ty::Term(TermTy::Tuple(fields) | TermTy::Record(fields)) => {
                let xs = self.fresh("fields", DATA_LIST);
                let body = if check_only {
                    self.fields(full, ty, fields, xs, b.var(d.name))?
                } else {
                    let values = fields
                        .iter()
                        .map(|ty| self.fresh("field", *ty))
                        .collect::<Vec<_>>();
                    let body =
                        b.constr(0, &values.iter().map(|p| b.var(p.name)).collect::<Vec<_>>());
                    self.sequence_fields(full, ty, fields, xs, Some(&values), body)?
                };
                (Test::DataList, vec![xs], body, "expected List")
            }
            _ => return Err(Error::InvalidValidationType(ty)),
        };
        Ok(self.data_case(
            full,
            ty,
            d,
            Branch {
                test: shape,
                binders: b.arena.alloc_slice_copy(&params),
                body,
            },
            reason,
        ))
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
            Core::Var(_) | Core::Lit(_) | Core::Evaluated { .. } | Core::Error => core,
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
            CastKind::ToData if to == DATA => return self.encode(from, arg),
            CastKind::FromDataShallow | CastKind::ValidateData if from == DATA => {
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
        self.converter(full, false, ty)
    }
    fn validator(&mut self, ty: Ty<'a>) -> Result<Binder<'a>, Error<'a>> {
        self.converter(true, true, ty)
    }
    fn converter(
        &mut self,
        full: bool,
        check_only: bool,
        ty: Ty<'a>,
    ) -> Result<Binder<'a>, Error<'a>> {
        let check_only = check_only && !matches!(ty, Ty::Big(_));
        if matches!(
            ty,
            Ty::Erased | Ty::Constructor(_) | Ty::Term(TermTy::Fun(..))
        ) {
            return Err(Error::InvalidValidationType(ty));
        }
        if let Some(checker) = self.checkers.get(&(full, check_only, ty)) {
            return Ok(*checker);
        }
        // Cache lookup comes first: regular recursive layouts close their cycle.
        if self.checkers.len() >= MAX_VALIDATION_HELPERS {
            return Err(Error::ValidationLayoutLimit);
        }
        let binder = self.fresh(
            if full { "validateData" } else { "fromData" },
            self.fun_ty(&[DATA], if check_only { DATA } else { ty }),
        );
        self.checkers.insert((full, check_only, ty), binder);
        self.pending.push((full, check_only, ty, binder));
        Ok(binder)
    }
    fn checker_body(
        &mut self,
        full: bool,
        check_only: bool,
        ty: Ty<'a>,
        d: Binder<'a>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        let b = self.build;
        let big = match ty {
            Ty::Big(big) => big,
            _ => return self.decode_value(full, check_only, ty, d),
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
                    let checker = self.validator(*elem)?;
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
                    let key = self.validator(*key)?;
                    let val = self.validator(*val)?;
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
                if layouts
                    .data_layout
                    .is_some_and(|layout| layout.encoding == DataEncoding::Transparent)
                {
                    let checker = self.converter(full, true, layouts.fields[0][0])?;
                    let checked = self.fresh("checked", DATA);
                    return Ok(b.let_(
                        checked,
                        b.app(b.var(checker.name), &[b.var(d.name)]),
                        b.var(d.name),
                    ));
                }
                if !full && layouts.data_layout.is_some() {
                    return Ok(b.var(d.name));
                }
                let tag = self.fresh("tag", INT);
                let xs = self.fresh("fields", DATA_LIST);
                if layouts
                    .data_layout
                    .is_some_and(|layout| layout.encoding == DataEncoding::List)
                {
                    let body = self.fields(full, ty, layouts.fields[0], xs, b.var(d.name))?;
                    return Ok(self.data_case(
                        full,
                        ty,
                        d,
                        Branch {
                            test: Test::DataList,
                            binders: b.arena.alloc_slice_copy(&[xs]),
                            body,
                        },
                        "expected List",
                    ));
                }
                let branches = layouts
                    .fields
                    .iter()
                    .enumerate()
                    .map(|(index, fields)| {
                        Ok(Branch {
                            test: Test::Int(constant::integer_from(
                                b.arena,
                                layouts
                                    .data_layout
                                    .map_or(index as i128, |layout| i128::from(layout.tags[index])),
                            )),
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
        self.sequence_fields(full, ty, fields, xs, None, ok)
    }

    fn sequence_fields(
        &mut self,
        full: bool,
        ty: Ty<'a>,
        fields: &[Ty<'a>],
        xs: Binder<'a>,
        decoded: Option<&[Binder<'a>]>,
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
            if full || decoded.is_some() {
                let value = b.builtin(F::HeadList, &[b.var(current.name)]);
                let checked =
                    decoded.map_or_else(|| self.fresh("checked", DATA), |values| values[index]);
                let value = if !full && matches!(fields[index], Ty::Big(_)) {
                    value
                } else {
                    let checker = if decoded.is_some() {
                        self.checker(full, fields[index])?
                    } else {
                        self.validator(fields[index])?
                    };
                    b.app(b.var(checker.name), &[value])
                };
                body = b.let_(checked, value, body);
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
        let check_ty = self.fun_ty(&[DATA], Ty::Erased);
        let each = self.fresh("validateEach", self.fun_ty(&[check_ty, DATA_LIST], UNIT));
        self.each = Some(each);
        let check = self.fresh("check", check_ty);
        let xs = self.fresh("items", DATA_LIST);
        let checked = self.fresh("checked", Ty::Erased);
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
        let check_ty = self.fun_ty(&[DATA], Ty::Erased);
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
            Core::Lit(_) | Core::Evaluated { .. } | Core::Error => {}
        }
    }
    names
}

#[cfg(test)]
#[path = "casts_tests.rs"]
mod tests;
