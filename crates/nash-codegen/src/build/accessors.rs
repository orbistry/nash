//! Share pure projections after their first evaluation in the same scope.
//!
//! Prefix bindings follow source evaluation order. A trace, branch, lambda or
//! delay retains its own body scope, so a possibly failing decoder never moves
//! from an unselected or delayed path into the enclosing execution path.
use std::collections::{BTreeMap, HashMap, HashSet};

use nash_ir::{
    build::Builder,
    core::*,
    ty::{BigTy, ConstTy, TermTy, Ty},
};
use nash_plutus::{builtin::DefaultFunction as F, constant::Constant};

const DATA: Ty<'static> = Ty::Big(&BigTy::Data);
const DATA_LIST: Ty<'static> = Ty::Const(&ConstTy::List(DATA));
const CONSTR_PAIR: Ty<'static> = Ty::Const(&ConstTy::Pair(Ty::Const(&ConstTy::Int), DATA_LIST));

#[derive(Clone, Copy, PartialEq)]
enum Projection {
    Builtin(F),
    Field(u16, u16),
    DropList(u16),
}
impl Eq for Projection {}
impl std::hash::Hash for Projection {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            Self::Builtin(func) => {
                0u8.hash(state);
                (*func as usize).hash(state);
            }
            Self::DropList(count) => {
                2u8.hash(state);
                count.hash(state);
            }
            Self::Field(index, arity) => {
                1u8.hash(state);
                index.hash(state);
                arity.hash(state);
            }
        }
    }
}
#[derive(Clone, Default)]
struct Scope<'a> {
    aliases: HashMap<u32, u32>,
    types: HashMap<u32, Ty<'a>>,
    projections: HashMap<(Projection, u32), Binder<'a>>,
    tail_positions: HashMap<u32, (u32, usize)>,
    tails: BTreeMap<(u32, usize), Binder<'a>>,
    record_lengths: HashMap<u32, usize>,
    nonempty: HashSet<(u32, usize)>,
    /// Paths that lowering extracts as implicit case binders. Match the whole
    /// path before walking its children, so `sndPair (unConstrData value)` does
    /// not repeat the decoder when the fields binder is already in scope.
    case_paths: HashMap<(Vec<Projection>, u32), Binder<'a>>,
}
impl<'a> Scope<'a> {
    fn canonical(&self, name: u32) -> u32 {
        self.aliases.get(&name).copied().unwrap_or(name)
    }
    fn bind(&mut self, binder: Binder<'a>, value: Option<&Core<'a>>) {
        self.types.insert(binder.name.unique, binder.ty);
        if let Some(Core::Var(name)) = value {
            self.aliases
                .insert(binder.name.unique, self.canonical(name.unique));
        }
    }
    fn list_position(&self, name: u32) -> (u32, usize) {
        let name = self.canonical(name);
        self.tail_positions.get(&name).copied().unwrap_or((name, 0))
    }
    fn remember_tail(&mut self, binder: Binder<'a>, position: (u32, usize)) {
        self.tail_positions.insert(binder.name.unique, position);
        self.tails.insert(position, binder);
    }
    fn ty(&self, core: &Core<'a>) -> Ty<'a> {
        match core {
            Core::Var(name) => self
                .types
                .get(&name.unique)
                .copied()
                .or_else(|| self.types.get(&self.canonical(name.unique)).copied())
                .unwrap_or(Ty::Erased),
            Core::Lit(Constant::Integer(_)) => Ty::Const(&ConstTy::Int),
            Core::Lit(Constant::ByteString(_)) => Ty::Const(&ConstTy::Bytes),
            Core::Lit(Constant::String(_)) => Ty::Const(&ConstTy::String),
            Core::Lit(Constant::Boolean(_)) => Ty::Const(&ConstTy::Bool),
            Core::Lit(Constant::Unit) => Ty::Const(&ConstTy::Unit),
            Core::Lit(Constant::Data(_)) => DATA,
            _ => Ty::Erased,
        }
    }
    fn path(&self, core: &Core<'a>) -> Option<(Vec<Projection>, u32)> {
        match core {
            Core::Var(name) => Some((Vec::new(), self.canonical(name.unique))),
            Core::Builtin { func, args: [arg] } => {
                let (mut path, name) = self.path(arg)?;
                path.push(Projection::Builtin(*func));
                Some((path, name))
            }
            Core::Builtin {
                func: F::DropList,
                args: [Core::Lit(Constant::Integer(count)), list],
            } => {
                let count = u16::try_from(*count).ok()?;
                let (mut path, name) = self.path(list)?;
                path.push(Projection::DropList(count));
                Some((path, name))
            }
            Core::Field {
                record,
                index,
                arity,
            } => {
                let (mut path, name) = self.path(record)?;
                path.push(Projection::Field(*index, *arity));
                Some((path, name))
            }
            _ => None,
        }
    }
    fn case_binders(&mut self, kind: CaseKind, value: &Core<'a>, branch: &Branch<'a>) {
        let Some((base, name)) = self.path(value) else {
            return;
        };
        if let (CaseKind::List, Test::Cons, Core::Var(value)) = (kind, branch.test, value) {
            let (root, offset) = self.list_position(value.unique);
            self.nonempty.insert((root, offset));
            if let Some(tail) = branch.binders.get(1) {
                self.remember_tail(*tail, (root, offset + 1));
            }
        }
        let paths = match (kind, branch.test) {
            (CaseKind::Data, Test::DataConstr) => vec![vec![Projection::Builtin(F::UnConstrData)]],
            (CaseKind::Pair, Test::Pair) => vec![
                vec![Projection::Builtin(F::FstPair)],
                vec![Projection::Builtin(F::SndPair)],
            ],
            (CaseKind::Data, Test::DataMap) => vec![vec![Projection::Builtin(F::UnMapData)]],
            (CaseKind::Data, Test::DataList) => vec![vec![Projection::Builtin(F::UnListData)]],
            (CaseKind::Data, Test::DataI) => vec![vec![Projection::Builtin(F::UnIData)]],
            (CaseKind::Data, Test::DataB) => vec![vec![Projection::Builtin(F::UnBData)]],
            (CaseKind::List, Test::Cons) => vec![
                vec![Projection::Builtin(F::HeadList)],
                vec![Projection::Builtin(F::TailList)],
            ],
            (CaseKind::Tag, Test::Tag(_)) => (0..branch.binders.len())
                .map(|index| vec![Projection::Field(index as u16, branch.binders.len() as u16)])
                .collect(),
            _ => Vec::new(),
        };
        for (path, binder) in paths.into_iter().zip(branch.binders) {
            self.case_paths
                .insert((base.iter().copied().chain(path).collect(), name), *binder);
        }
    }
}
struct Parts<'a> {
    bindings: Vec<(Binder<'a>, &'a Core<'a>)>,
    value: &'a Core<'a>,
}
impl<'a> Parts<'a> {
    fn value(value: &'a Core<'a>) -> Self {
        Self {
            bindings: Vec::new(),
            value,
        }
    }
}
struct Share<'a, 'b> {
    build: &'b Builder<'a>,
}

pub(super) fn share<'a>(build: &Builder<'a>, core: &'a Core<'a>) -> &'a Core<'a> {
    let share = Share { build };
    share.wrap(share.term(core, &mut Scope::default()))
}

impl<'a> Share<'a, '_> {
    fn wrap(&self, parts: Parts<'a>) -> &'a Core<'a> {
        parts
            .bindings
            .into_iter()
            .rev()
            .fold(parts.value, |body, (binder, value)| {
                self.build.let_(binder, value, body)
            })
    }
    fn temporary(
        &self,
        value: &'a Core<'a>,
        scope: &mut Scope<'a>,
        bindings: &mut Vec<(Binder<'a>, &'a Core<'a>)>,
    ) -> &'a Core<'a> {
        let binder = Binder {
            name: self.build.fresh("evaluated"),
            ty: scope.ty(value),
        };
        scope.bind(binder, Some(value));
        bindings.push((binder, value));
        self.build.var(binder.name)
    }
    /// Before moving a later operand's prefix outside the application, evaluate
    /// each preceding operand that can have an effect at its original position.
    fn operands(
        &self,
        values: &[&'a Core<'a>],
        scope: &mut Scope<'a>,
    ) -> (Vec<(Binder<'a>, &'a Core<'a>)>, Vec<&'a Core<'a>>) {
        let mut bindings = Vec::new();
        let mut operands: Vec<&'a Core<'a>> = Vec::new();
        for value in values {
            let part = self.term(value, scope);
            if !part.bindings.is_empty() {
                for previous in &mut operands {
                    if !simple(previous) {
                        *previous = self.temporary(previous, scope, &mut bindings);
                    }
                }
                bindings.extend(part.bindings);
            }
            operands.push(part.value);
        }
        (bindings, operands)
    }
    fn projection(
        &self,
        projection: Projection,
        mut parts: Parts<'a>,
        scope: &mut Scope<'a>,
    ) -> Parts<'a> {
        if !matches!(parts.value, Core::Var(_)) {
            parts.value = self.temporary(parts.value, scope, &mut parts.bindings);
        }
        let Core::Var(name) = parts.value else {
            unreachable!()
        };
        let record_length = match (projection, scope.ty(parts.value)) {
            (Projection::Builtin(F::UnListData), Ty::Big(BigTy::Record(fields))) => {
                Some(fields.len())
            }
            _ => None,
        };
        let key = (projection, scope.canonical(name.unique));
        if let Some(binder) = scope
            .projections
            .get(&key)
            .or_else(|| scope.case_paths.get(&(vec![projection], key.1)))
            .copied()
        {
            if let Some(length) = record_length {
                scope
                    .record_lengths
                    .insert(scope.list_position(binder.name.unique).0, length);
            }
            parts.value = self.build.var(binder.name);
            return parts;
        }
        let position = scope.list_position(name.unique);
        let tail_position = match projection {
            Projection::Builtin(F::TailList) => Some((position.0, position.1 + 1)),
            Projection::DropList(count) => Some((position.0, position.1 + usize::from(count))),
            _ => None,
        };
        if projection == Projection::Builtin(F::HeadList) {
            scope.nonempty.insert(position);
        }
        let mut emitted_projection = projection;
        if let Projection::DropList(count) = projection
            && count != 0
        {
            let target = (position.0, position.1 + usize::from(count));
            let mut start = position;
            if let Some((&nearest, binder)) = scope.tails.range(position..=target).next_back() {
                start = nearest;
                parts.value = self.build.var(binder.name);
            }
            let remaining = target.1 - start.1;
            if remaining == 0 {
                return parts;
            }
            // A typed record supplies its field count without reading any fields.
            // Arbitrary lists still need a successful head read or Cons match.
            let nonempty = scope
                .record_lengths
                .get(&start.0)
                .is_some_and(|length| start.1 < *length)
                || scope.nonempty.contains(&start);
            emitted_projection = if remaining == 1 && nonempty {
                Projection::Builtin(F::TailList)
            } else {
                Projection::DropList(remaining as u16)
            };
        }
        let input = scope.ty(parts.value);
        let ty = match (projection, input) {
            (Projection::Builtin(F::UnListData), _) => DATA_LIST,
            (Projection::Builtin(F::UnConstrData), _) => CONSTR_PAIR,
            (Projection::Builtin(F::FstPair), Ty::Const(ConstTy::Pair(a, _))) => *a,
            (Projection::Builtin(F::SndPair), Ty::Const(ConstTy::Pair(_, b))) => *b,
            (Projection::Builtin(F::HeadList), Ty::Const(ConstTy::List(element))) => *element,
            (
                Projection::Builtin(F::TailList) | Projection::DropList(_),
                Ty::Const(ConstTy::List(_)),
            ) => input,
            (
                Projection::Field(index, _),
                Ty::Term(TermTy::Record(fields) | TermTy::Tuple(fields)),
            ) => fields
                .get(usize::from(index))
                .copied()
                .unwrap_or(Ty::Erased),
            _ => Ty::Erased,
        };
        let value = match emitted_projection {
            Projection::Builtin(func) => self.build.builtin(func, &[parts.value]),
            Projection::DropList(count) => self.build.builtin(
                F::DropList,
                &[self.build.int(i128::from(count)), parts.value],
            ),
            Projection::Field(index, arity) => self.build.field(parts.value, index, arity),
        };
        let binder = Binder {
            name: self.build.fresh("projection"),
            ty,
        };
        scope.bind(binder, None);
        scope.projections.insert(key, binder);
        if let Some(length) = record_length {
            scope.record_lengths.insert(binder.name.unique, length);
        }
        if let Some(position) = tail_position {
            scope.remember_tail(binder, position);
        }
        parts.bindings.push((binder, value));
        parts.value = self.build.var(binder.name);
        parts
    }
    fn term(&self, core: &'a Core<'a>, scope: &mut Scope<'a>) -> Parts<'a> {
        if let Some(binder) = scope
            .path(core)
            .and_then(|path| scope.case_paths.get(&path))
        {
            return Parts::value(self.build.var(binder.name));
        }
        match core {
            Core::Var(_) | Core::Lit(_) | Core::Error => Parts::value(core),
            Core::Lam { params, body } => {
                let mut inner = scope.clone();
                for param in *params {
                    inner.bind(*param, None);
                }
                Parts::value(
                    self.build
                        .lam(params, self.wrap(self.term(body, &mut inner))),
                )
            }
            Core::Let {
                binder,
                value,
                body,
            } => {
                let mut value = self.term(value, scope);
                scope.bind(*binder, Some(value.value));
                value.bindings.push((*binder, value.value));
                let body = self.term(body, scope);
                value.bindings.extend(body.bindings);
                value.value = body.value;
                value
            }
            Core::LetRec { binders, body } => {
                let mut inner = scope.clone();
                for binder in *binders {
                    inner.bind(binder.binder, None);
                }
                let binders = binders
                    .iter()
                    .map(|binder| {
                        let mut local = inner.clone();
                        for param in binder.params {
                            local.bind(*param, None);
                        }
                        RecBinder {
                            body: self.wrap(self.term(binder.body, &mut local)),
                            ..*binder
                        }
                    })
                    .collect::<Vec<_>>();
                Parts::value(
                    self.build
                        .let_rec(&binders, self.wrap(self.term(body, &mut inner))),
                )
            }
            Core::App {
                func: Core::Builtin { func, args: [] },
                args,
            } if args.len() <= func.arity() => self.term(self.build.builtin(*func, args), scope),
            Core::App { func, args } => {
                let all = std::iter::once(*func)
                    .chain(args.iter().copied())
                    .collect::<Vec<_>>();
                let (bindings, values) = self.operands(&all, scope);
                Parts {
                    bindings,
                    value: self.build.app(values[0], &values[1..]),
                }
            }
            Core::Builtin {
                func: F::DropList,
                args: [Core::Lit(Constant::Integer(count)), list],
            } if u16::try_from(*count).is_ok() => {
                let parts = self.term(list, scope);
                self.projection(
                    Projection::DropList(u16::try_from(*count).unwrap()),
                    parts,
                    scope,
                )
            }
            Core::Builtin { func, args: [arg] }
                if matches!(
                    func,
                    F::UnListData
                        | F::UnConstrData
                        | F::FstPair
                        | F::SndPair
                        | F::HeadList
                        | F::TailList
                ) =>
            {
                let parts = self.term(arg, scope);
                self.projection(Projection::Builtin(*func), parts, scope)
            }
            Core::Builtin { func, args } => {
                let (bindings, args) = self.operands(args, scope);
                Parts {
                    bindings,
                    value: self.build.builtin(*func, &args),
                }
            }
            Core::Constr { tag, fields } => {
                let (bindings, fields) = self.operands(fields, scope);
                Parts {
                    bindings,
                    value: self.build.constr(*tag, &fields),
                }
            }
            Core::Field {
                record,
                index,
                arity,
            } => {
                let record = self.term(record, scope);
                self.projection(Projection::Field(*index, *arity), record, scope)
            }
            Core::Case {
                kind,
                scrutinee,
                branches,
                default,
            } => {
                let mut scrutinee = self.term(scrutinee, scope);
                if let (CaseKind::Pair, [branch], None) = (kind, *branches, default)
                    && let Core::Var(selected) = branch.body
                    && let Some(index) = branch.binders.iter().position(|b| b.name == *selected)
                    && let Some((mut path, root)) = scope.path(scrutinee.value)
                {
                    path.push(Projection::Builtin(if index == 0 {
                        F::FstPair
                    } else {
                        F::SndPair
                    }));
                    if let Some(binder) = scope.case_paths.get(&(path, root)) {
                        scrutinee.value = self.build.var(binder.name);
                        return scrutinee;
                    }
                }
                let branches = branches
                    .iter()
                    .map(|branch| {
                        let mut inner = scope.clone();
                        for binder in branch.binders {
                            inner.bind(*binder, None);
                        }
                        inner.case_binders(*kind, scrutinee.value, branch);
                        Branch {
                            body: self.wrap(self.term(branch.body, &mut inner)),
                            ..*branch
                        }
                    })
                    .collect::<Vec<_>>();
                let default = default.map(|d| self.wrap(self.term(d, &mut scope.clone())));
                Parts {
                    bindings: scrutinee.bindings,
                    value: self.build.case(*kind, scrutinee.value, &branches, default),
                }
            }
            Core::Trace { message, body } => {
                let message = self.term(message, scope);
                Parts {
                    bindings: message.bindings,
                    value: self.build.trace(
                        message.value,
                        self.wrap(self.term(body, &mut scope.clone())),
                    ),
                }
            }
            Core::Delay(body) => Parts::value(
                self.build
                    .delay(self.wrap(self.term(body, &mut scope.clone()))),
            ),
            Core::Force(body) => {
                let body = self.term(body, scope);
                Parts {
                    bindings: body.bindings,
                    value: self.build.force(body.value),
                }
            }
        }
    }
}

fn simple(core: &Core<'_>) -> bool {
    matches!(
        core,
        Core::Var(_)
            | Core::Lit(_)
            | Core::Lam { .. }
            | Core::Delay(_)
            | Core::Builtin { args: [], .. }
    )
}
