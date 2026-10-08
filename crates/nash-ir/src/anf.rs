//! A-normalization with explicit left-to-right application stages.
//!
//! Input binders must be globally unique. Lambda, delay, recursive function and
//! branch bodies remain in their original execution scopes. This pass does not
//! discard work or perform beta reduction.
use crate::{
    build::Builder,
    core::*,
    ty::{RuntimeTy, TermTy, Ty},
};
use std::collections::HashSet;

/// Values whose evaluation cannot execute a user body. A zero-parameter lambda
/// is not a value: lowering evaluates its body directly.
pub fn is_atom(core: &Core<'_>) -> bool {
    matches!(
        core.kind,
        CoreKind::Var(_) | CoreKind::Lit(_) | CoreKind::Delay(_)
    ) || matches!(core.kind, CoreKind::Lam { params, .. } if !params.is_empty())
        || matches!(core.kind, CoreKind::Builtin { args, .. } if args.is_empty())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnfError {
    NonAtomicOperand,
    NestedBindingValue,
    EmptyLambda,
    EmptyApplication,
}

/// Check ANF shape, including bodies inside values. Binding scope and uniqueness
/// are checked separately by `hygiene::validate`; this is not a type checker.
pub fn validate(core: &Core<'_>) -> Result<(), Vec<AnfError>> {
    let mut errors = Vec::new();
    core.walk(&mut |node| {
        let mut operand = |value| {
            if !is_atom(value) {
                errors.push(AnfError::NonAtomicOperand);
            }
        };
        match node.kind {
            CoreKind::App { func, args } => {
                operand(func);
                for arg in args {
                    operand(arg);
                }
                if args.is_empty() {
                    errors.push(AnfError::EmptyApplication);
                }
            }
            CoreKind::Builtin { args, .. } => {
                for arg in args {
                    operand(arg);
                }
            }
            CoreKind::Constr { fields, .. } => {
                for field in fields {
                    operand(field);
                }
            }
            CoreKind::Case { scrutinee, .. } => operand(scrutinee),
            CoreKind::Field { record, .. } => operand(record),
            CoreKind::Force(body) => operand(body),
            CoreKind::Trace { message, .. } => operand(message),
            CoreKind::Let { value, .. }
                if matches!(value.kind, CoreKind::Let { .. } | CoreKind::LetRec { .. }) =>
            {
                errors.push(AnfError::NestedBindingValue)
            }
            CoreKind::Lam { params: [], .. } => errors.push(AnfError::EmptyLambda),
            _ => {}
        }
    });
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Normalize a typed Core tree. Use the enclosing program's Builder when this
/// tree will be reinserted into a larger scope. Existing IDs in this tree are
/// avoided even when the supplied Builder has a fresh name supply.
pub fn normalize<'a>(build: &Builder<'a>, core: &'a Core<'a>) -> &'a Core<'a> {
    let report = crate::analysis::occurrences(core);
    let used = report
        .bindings
        .iter()
        .map(|b| b.name.unique)
        .chain(report.uses.iter().map(|u| u.name.unique))
        .collect();
    Normalizer { build, used }.term(core)
}

enum Prefix<'a> {
    Let(Binder<'a>, &'a Core<'a>),
    Rec(&'a [RecBinder<'a>]),
}
struct Normalizer<'b, 'a> {
    build: &'b Builder<'a>,
    used: HashSet<u32>,
}
impl<'a> Normalizer<'_, 'a> {
    fn finish(&self, prefix: Vec<Prefix<'a>>, mut body: &'a Core<'a>) -> &'a Core<'a> {
        for binding in prefix.into_iter().rev() {
            body = match binding {
                Prefix::Let(binder, value) => self.build.let_(binder, value, body),
                Prefix::Rec(binders) => self.build.let_rec(binders, body),
            };
        }
        body
    }
    fn peel(&self, mut value: &'a Core<'a>, prefix: &mut Vec<Prefix<'a>>) -> &'a Core<'a> {
        let ty = value.ty;
        loop {
            match value.kind {
                CoreKind::Let {
                    binder,
                    value: rhs,
                    body,
                } => {
                    prefix.push(Prefix::Let(binder, rhs));
                    value = body;
                }
                CoreKind::LetRec { binders, body } => {
                    prefix.push(Prefix::Rec(binders));
                    value = body;
                }
                _ => return self.build.with_type(value, ty),
            }
        }
    }
    fn atom(&mut self, value: &'a Core<'a>, prefix: &mut Vec<Prefix<'a>>) -> &'a Core<'a> {
        let value = self.term(value);
        let value = self.peel(value, prefix);
        if is_atom(value) {
            return value;
        }
        let name = loop {
            let name = self.build.fresh("anf");
            if self.used.insert(name.unique) {
                break name;
            }
        };
        prefix.push(Prefix::Let(Binder { name, ty: value.ty }, value));
        self.build.var(name, value.ty)
    }
    fn applied_type(&self, ty: Ty<'a>, arg: &'a Core<'a>) -> Ty<'a> {
        match ty {
            Ty::Term(TermTy::Fun(params, result)) if !params.is_empty() => {
                if params.len() == 1 {
                    *result
                } else {
                    Ty::Term(self.build.arena.alloc(TermTy::Fun(&params[1..], *result)))
                }
            }
            Ty::Runtime(RuntimeTy::SelfFunction(result)) => *result,
            Ty::Runtime(RuntimeTy::Dispatcher(arms)) => {
                if let Ty::Runtime(RuntimeTy::Packet { tag, .. }) = arg.ty {
                    arms[usize::from(*tag)].result
                } else {
                    Ty::Runtime(self.build.arena.alloc(RuntimeTy::Results(arms)))
                }
            }
            _ => panic!("ANF intermediate application requires a function type, got {ty}"),
        }
    }
    fn term(&mut self, core: &'a Core<'a>) -> &'a Core<'a> {
        let b = self.build;
        let mut prefix = Vec::new();
        let kind = match core.kind {
            CoreKind::Var(_) | CoreKind::Lit(_) | CoreKind::Error => return core,
            CoreKind::Lam { params, body } => {
                let body = self.term(body);
                if params.is_empty() {
                    return b.with_type(body, core.ty);
                }
                CoreKind::Lam { params, body }
            }
            CoreKind::Delay(body) => CoreKind::Delay(self.term(body)),
            CoreKind::Force(body) => CoreKind::Force(self.atom(body, &mut prefix)),
            CoreKind::App { func, args } => {
                if args.is_empty() {
                    return b.with_type(self.term(func), core.ty);
                }
                let mut func = self.atom(func, &mut prefix);
                let mut pending: Vec<&'a Core<'a>> = Vec::new();
                for arg in args {
                    // Applying the prefix may fail, trace or diverge. It must
                    // run before evaluating the next non-value argument.
                    if !is_atom(arg) && !pending.is_empty() {
                        let mut ty = func.ty;
                        for previous in &pending {
                            ty = self.applied_type(ty, previous);
                        }
                        func = self.atom(b.app(func, &pending, ty), &mut prefix);
                        pending.clear();
                    }
                    pending.push(self.atom(arg, &mut prefix));
                }
                return self.finish(prefix, b.app(func, &pending, core.ty));
            }
            CoreKind::Let {
                binder,
                value,
                body,
            } => {
                let value = self.term(value);
                let value = self.peel(value, &mut prefix);
                prefix.push(Prefix::Let(binder, value));
                let body = b.with_type(self.term(body), core.ty);
                return self.finish(prefix, body);
            }
            CoreKind::LetRec { binders, body } => {
                let binders: Vec<_> = binders
                    .iter()
                    .map(|rec| RecBinder {
                        body: self.term(rec.body),
                        ..*rec
                    })
                    .collect();
                CoreKind::LetRec {
                    binders: b.arena.alloc_slice_copy(&binders),
                    body: self.term(body),
                }
            }
            CoreKind::Builtin { func, args } => {
                // A known builtin cannot execute until saturation. Earlier
                // applications only collect arguments, so operands can be named
                // left-to-right without introducing partial builtin bindings.
                let args: Vec<_> = args.iter().map(|arg| self.atom(arg, &mut prefix)).collect();
                CoreKind::Builtin {
                    func,
                    args: b.arena.alloc_slice_copy(&args),
                }
            }
            CoreKind::Constr { tag, fields } => {
                let fields: Vec<_> = fields
                    .iter()
                    .map(|field| self.atom(field, &mut prefix))
                    .collect();
                CoreKind::Constr {
                    tag,
                    fields: b.arena.alloc_slice_copy(&fields),
                }
            }
            CoreKind::Field {
                record,
                index,
                arity,
            } => CoreKind::Field {
                record: self.atom(record, &mut prefix),
                index,
                arity,
            },
            CoreKind::Case {
                kind,
                scrutinee,
                branches,
                default,
            } => {
                let scrutinee = self.atom(scrutinee, &mut prefix);
                let branches: Vec<_> = branches
                    .iter()
                    .map(|branch| Branch {
                        body: self.term(branch.body),
                        ..*branch
                    })
                    .collect();
                CoreKind::Case {
                    kind,
                    scrutinee,
                    branches: b.arena.alloc_slice_copy(&branches),
                    default: default.map(|body| self.term(body)),
                }
            }
            CoreKind::Trace { message, body } => CoreKind::Trace {
                message: self.atom(message, &mut prefix),
                body: self.term(body),
            },
        };
        self.finish(prefix, b.alloc(core.ty, kind))
    }
}

#[cfg(test)]
mod tests;
