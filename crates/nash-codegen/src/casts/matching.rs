//! Full Data shape validation without failing destructors or traces.
//! UTF-8 and curve decoding retain the pinned runtime failures on malformed bytes.
use super::{
    BYTES, DATA, DATA_LIST, DATA_MAP, DATA_PAIR, Error, INT, MAX_VALIDATION_HELPERS, input_names,
};
use crate::ty_of::TypeEnv;
use nash_ast::DataEncoding;
use nash_ir::{
    build::Builder,
    core::*,
    ty::{BigTy, ConstTy, TermTy, Ty},
};
use nash_plutus::{builtin::DefaultFunction as F, constant::Constant};
use std::collections::{HashMap, HashSet};

const BOOL: Ty<'static> = Ty::Const(&ConstTy::Bool);

/// Evaluate `value` once and return whether full Data validation would succeed.
/// Unsupported runtime types are compiler errors, not permissive predicates.
/// String and curve types guard the Data shape but retain pinned decoding failures.
pub(crate) fn data_matches<'a>(
    build: &Builder<'a>,
    types: &mut TypeEnv<'a, '_>,
    ty: Ty<'a>,
    value: &'a Core<'a>,
) -> Result<&'a Core<'a>, Error<'a>> {
    let mut matcher = Matcher {
        build,
        types,
        used: input_names(value),
        checkers: HashMap::new(),
        pending: Vec::new(),
        bindings: Vec::new(),
        each: None,
        each_map: None,
        yes: build.lit(Constant::bool(build.arena, true)),
        no: build.lit(Constant::bool(build.arena, false)),
    };
    let checker = matcher.checker(ty)?;
    let body = build.app(build.var(checker.name), &[value]);
    // Register before expanding: regular recursive layouts close their cycle,
    // while growing polymorphic layouts hit the same bound as cast validation.
    while let Some((ty, binder)) = matcher.pending.pop() {
        let data = matcher.fresh("data", DATA);
        let body = matcher.body(ty, data)?;
        matcher.bindings.push(RecBinder {
            binder,
            params: build.arena.alloc_slice_copy(&[data]),
            static_params: &[],
            body,
        });
    }
    Ok(build.let_rec(&matcher.bindings, body))
}

struct Matcher<'b, 'a, 'env> {
    build: &'b Builder<'a>,
    types: &'b mut TypeEnv<'a, 'env>,
    used: HashSet<u32>,
    checkers: HashMap<Ty<'a>, Binder<'a>>,
    pending: Vec<(Ty<'a>, Binder<'a>)>,
    bindings: Vec<RecBinder<'a>>,
    each: Option<Binder<'a>>,
    each_map: Option<Binder<'a>>,
    yes: &'a Core<'a>,
    no: &'a Core<'a>,
}

impl<'a> Matcher<'_, 'a, '_> {
    fn fresh(&mut self, text: &'a str, ty: Ty<'a>) -> Binder<'a> {
        loop {
            let name = self.build.fresh(text);
            if self.used.insert(name.unique) {
                return Binder { name, ty };
            }
        }
    }

    fn fun_ty(&self, args: &[Ty<'a>]) -> Ty<'a> {
        Ty::Term(
            self.build
                .arena
                .alloc(TermTy::Fun(self.build.arena.alloc_slice_copy(args), BOOL)),
        )
    }

    fn checker(&mut self, ty: Ty<'a>) -> Result<Binder<'a>, Error<'a>> {
        if let Some(checker) = self.checkers.get(&ty) {
            return Ok(*checker);
        }
        if self.checkers.len() >= MAX_VALIDATION_HELPERS {
            return Err(Error::ValidationLayoutLimit);
        }
        let checker = self.fresh("dataMatches", self.fun_ty(&[DATA]));
        self.checkers.insert(ty, checker);
        self.pending.push((ty, checker));
        Ok(checker)
    }

    fn data_case(
        &self,
        data: Binder<'a>,
        test: Test<'a>,
        binders: &[Binder<'a>],
        body: &'a Core<'a>,
    ) -> &'a Core<'a> {
        self.build.case(
            CaseKind::Data,
            self.build.var(data.name),
            &[Branch {
                test,
                binders: self.build.arena.alloc_slice_copy(binders),
                body,
            }],
            Some(self.no),
        )
    }

    fn body(&mut self, ty: Ty<'a>, data: Binder<'a>) -> Result<&'a Core<'a>, Error<'a>> {
        let b = self.build;
        Ok(match ty {
            Ty::Big(BigTy::Data) => self.yes,
            Ty::Big(BigTy::Int) | Ty::Const(ConstTy::Int) => {
                let integer = self.fresh("integer", INT);
                self.data_case(data, Test::DataI, &[integer], self.yes)
            }
            Ty::Big(BigTy::Bytes) | Ty::Const(ConstTy::Bytes | ConstTy::String) => {
                let bytes = self.fresh("bytes", BYTES);
                let body = if matches!(ty, Ty::Const(ConstTy::String)) {
                    let decoded = self.fresh("string", ty);
                    // Pinned soft casts retain DecodeUtf8 failures after the
                    // byte-data shape check, including nested string fields.
                    b.let_(
                        decoded,
                        b.builtin(F::DecodeUtf8, &[b.var(bytes.name)]),
                        self.yes,
                    )
                } else {
                    self.yes
                };
                self.data_case(data, Test::DataB, &[bytes], body)
            }
            Ty::Const(ConstTy::BlsG1 | ConstTy::BlsG2) => {
                let bytes = self.fresh("bytes", BYTES);
                let decoded = self.fresh("point", ty);
                let uncompress = if matches!(ty, Ty::Const(ConstTy::BlsG1)) {
                    F::Bls12_381_G1_Uncompress
                } else {
                    F::Bls12_381_G2_Uncompress
                };
                // Pinned if/is rejects other Data shapes, but malformed curve
                // bytes abort in uncompress rather than selecting the fallback.
                let body = b.let_(
                    decoded,
                    b.builtin(uncompress, &[b.var(bytes.name)]),
                    self.yes,
                );
                self.data_case(data, Test::DataB, &[bytes], body)
            }
            Ty::Const(ConstTy::Bool | ConstTy::Unit) => {
                let tag = self.fresh("tag", INT);
                let fields = self.fresh("fields", DATA_LIST);
                let empty = b.builtin(F::NullList, &[b.var(fields.name)]);
                let valid_tag = b.builtin(F::EqualsInteger, &[b.var(tag.name), b.int(0)]);
                let valid_tag = if matches!(ty, Ty::Const(ConstTy::Bool)) {
                    b.if_(
                        valid_tag,
                        self.yes,
                        b.builtin(F::EqualsInteger, &[b.var(tag.name), b.int(1)]),
                    )
                } else {
                    valid_tag
                };
                let body = b.if_(valid_tag, empty, self.no);
                self.data_case(data, Test::DataConstr, &[tag, fields], body)
            }
            Ty::Big(BigTy::Map(key, value))
            | Ty::Const(
                ConstTy::List(Ty::Const(ConstTy::DataPair(key, value)))
                | ConstTy::DataList(Ty::Const(ConstTy::DataPair(key, value))),
            ) => {
                let entries = self.fresh("entries", DATA_MAP);
                let body = if *key == DATA && *value == DATA {
                    self.yes
                } else {
                    let key = self.checker(*key)?;
                    let value = self.checker(*value)?;
                    let each = self.each_map();
                    b.app(
                        b.var(each.name),
                        &[b.var(key.name), b.var(value.name), b.var(entries.name)],
                    )
                };
                self.data_case(data, Test::DataMap, &[entries], body)
            }
            Ty::Big(BigTy::List(element))
            | Ty::Const(ConstTy::List(element) | ConstTy::DataList(element)) => {
                let items = self.fresh("items", DATA_LIST);
                let body = if *element == DATA {
                    self.yes
                } else {
                    let element = self.checker(*element)?;
                    let each = self.each();
                    b.app(b.var(each.name), &[b.var(element.name), b.var(items.name)])
                };
                self.data_case(data, Test::DataList, &[items], body)
            }
            Ty::Const(ConstTy::DataPair(first, second) | ConstTy::Pair(first, second))
                if matches!(ty, Ty::Const(ConstTy::DataPair(..)))
                    || (matches!(first, Ty::Big(_)) && matches!(second, Ty::Big(_))) =>
            {
                let fields = self.fresh("fields", DATA_LIST);
                let body = self.fields(&[*first, *second], fields)?;
                self.data_case(data, Test::DataList, &[fields], body)
            }
            Ty::Big(BigTy::Record(types))
            | Ty::Const(ConstTy::DataTuple(types))
            | Ty::Term(TermTy::Tuple(types) | TermTy::Record(types)) => {
                let fields = self.fresh("fields", DATA_LIST);
                let body = self.fields(types, fields)?;
                self.data_case(data, Test::DataList, &[fields], body)
            }
            Ty::Big(BigTy::Adt(adt)) => {
                let layout = self.types.layout(*adt).map_err(Error::Type)?;
                match layout.data_layout.map(|layout| layout.encoding) {
                    Some(DataEncoding::Transparent) => {
                        let inner = self.checker(layout.fields[0][0])?;
                        b.app(b.var(inner.name), &[b.var(data.name)])
                    }
                    Some(DataEncoding::List) => {
                        let fields = self.fresh("fields", DATA_LIST);
                        let body = self.fields(layout.fields[0], fields)?;
                        self.data_case(data, Test::DataList, &[fields], body)
                    }
                    _ => {
                        let tag = self.fresh("tag", INT);
                        let fields = self.fresh("fields", DATA_LIST);
                        let branches = layout
                            .fields
                            .iter()
                            .enumerate()
                            .map(|(index, types)| {
                                Ok(Branch {
                                    test: Test::Int(nash_plutus::constant::integer_from(
                                        b.arena,
                                        layout.data_layout.map_or(index as i128, |layout| {
                                            i128::from(layout.tags[index])
                                        }),
                                    )),
                                    binders: &[],
                                    body: self.fields(types, fields)?,
                                })
                            })
                            .collect::<Result<Vec<_>, Error<'a>>>()?;
                        let body = b.case(CaseKind::Int, b.var(tag.name), &branches, Some(self.no));
                        self.data_case(data, Test::DataConstr, &[tag, fields], body)
                    }
                }
            }
            _ => return Err(Error::InvalidValidationType(ty)),
        })
    }

    fn fields(&mut self, types: &[Ty<'a>], items: Binder<'a>) -> Result<&'a Core<'a>, Error<'a>> {
        let b = self.build;
        let tails = (0..types.len())
            .map(|_| self.fresh("rest", DATA_LIST))
            .collect::<Vec<_>>();
        let end = tails.last().copied().unwrap_or(items);
        let mut body = b.builtin(F::NullList, &[b.var(end.name)]);
        for index in (0..types.len()).rev() {
            let current = if index == 0 { items } else { tails[index - 1] };
            body = b.let_(
                tails[index],
                b.builtin(F::TailList, &[b.var(current.name)]),
                body,
            );
            if types[index] != DATA {
                let checker = self.checker(types[index])?;
                body = b.if_(
                    b.app(
                        b.var(checker.name),
                        &[b.builtin(F::HeadList, &[b.var(current.name)])],
                    ),
                    body,
                    self.no,
                );
            }
            // Both HeadList and TailList occur only inside the nonempty branch.
            body = b.if_(
                b.builtin(F::NullList, &[b.var(current.name)]),
                self.no,
                body,
            );
        }
        Ok(body)
    }

    fn each(&mut self) -> Binder<'a> {
        if let Some(each) = self.each {
            return each;
        }
        let b = self.build;
        let check_ty = self.fun_ty(&[DATA]);
        let each = self.fresh("dataMatchesEach", self.fun_ty(&[check_ty, DATA_LIST]));
        self.each = Some(each);
        let check = self.fresh("check", check_ty);
        let items = self.fresh("items", DATA_LIST);
        let next = b.app(
            b.var(each.name),
            &[
                b.var(check.name),
                b.builtin(F::TailList, &[b.var(items.name)]),
            ],
        );
        let step = b.if_(
            b.app(
                b.var(check.name),
                &[b.builtin(F::HeadList, &[b.var(items.name)])],
            ),
            next,
            self.no,
        );
        let body = b.if_(b.builtin(F::NullList, &[b.var(items.name)]), self.yes, step);
        self.bindings.push(RecBinder {
            binder: each,
            params: b.arena.alloc_slice_copy(&[check, items]),
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
        let check_ty = self.fun_ty(&[DATA]);
        let each = self.fresh(
            "dataMatchesEntries",
            self.fun_ty(&[check_ty, check_ty, DATA_MAP]),
        );
        self.each_map = Some(each);
        let key = self.fresh("checkKey", check_ty);
        let value = self.fresh("checkValue", check_ty);
        let items = self.fresh("entries", DATA_MAP);
        let entry = self.fresh("entry", DATA_PAIR);
        let next = b.app(
            b.var(each.name),
            &[
                b.var(key.name),
                b.var(value.name),
                b.builtin(F::TailList, &[b.var(items.name)]),
            ],
        );
        let value_ok = b.if_(
            b.app(
                b.var(value.name),
                &[b.builtin(F::SndPair, &[b.var(entry.name)])],
            ),
            next,
            self.no,
        );
        let key_ok = b.if_(
            b.app(
                b.var(key.name),
                &[b.builtin(F::FstPair, &[b.var(entry.name)])],
            ),
            value_ok,
            self.no,
        );
        let step = b.let_(entry, b.builtin(F::HeadList, &[b.var(items.name)]), key_ok);
        let body = b.if_(b.builtin(F::NullList, &[b.var(items.name)]), self.yes, step);
        self.bindings.push(RecBinder {
            binder: each,
            params: b.arena.alloc_slice_copy(&[key, value, items]),
            static_params: &[0, 1],
            body,
        });
        each
    }
}
