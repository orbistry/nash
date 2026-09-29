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
        struct State<'a> {
            core: &'a Core<'a>,
            inputs: Vec<&'a Core<'a>>,
            index: usize,
            values: Vec<&'a Core<'a>>,
            prefix: Vec<Prefix<'a>>,
        }
        enum Task<'a> {
            Visit(&'a Core<'a>),
            Step(State<'a>),
            Resume(State<'a>, bool),
        }
        let b = self.build;
        let mut pending = vec![Task::Visit(core)];
        let mut results = Vec::new();
        while let Some(task) = pending.pop() {
            match task {
                Task::Visit(core) => {
                    if matches!(
                        core.kind,
                        CoreKind::Var(_) | CoreKind::Lit(_) | CoreKind::Error
                    ) {
                        results.push(core);
                        continue;
                    }
                    let mut inputs = Vec::new();
                    core.push_children_reversed(&mut inputs);
                    inputs.reverse();
                    pending.push(Task::Step(State {
                        core,
                        inputs,
                        index: 0,
                        values: Vec::new(),
                        prefix: Vec::new(),
                    }));
                }
                Task::Resume(mut state, atom) => {
                    let mut value = results.pop().unwrap();
                    if atom {
                        value = self.atom(value, &mut state.prefix);
                    }
                    if let CoreKind::Let { binder, .. } = state.core.kind
                        && state.index == 0
                    {
                        value = self.peel(value, &mut state.prefix);
                        state.prefix.push(Prefix::Let(binder, value));
                    }
                    state.values.push(value);
                    state.index += 1;
                    pending.push(Task::Step(state));
                }
                Task::Step(mut state) => {
                    if let Some(&input) = state.inputs.get(state.index) {
                        let atom = match state.core.kind {
                            CoreKind::App { args, .. } => !args.is_empty(),
                            CoreKind::Builtin { .. }
                            | CoreKind::Constr { .. }
                            | CoreKind::Field { .. }
                            | CoreKind::Force(_) => true,
                            CoreKind::Case { .. } | CoreKind::Trace { .. } => state.index == 0,
                            _ => false,
                        };
                        if matches!(state.core.kind, CoreKind::App { .. })
                            && state.index > 1
                            && !is_atom(input)
                            && state.values.len() > 1
                        {
                            let mut ty = state.values[0].ty;
                            for arg in &state.values[1..] {
                                ty = self.applied_type(ty, arg);
                            }
                            let func = self.atom(
                                b.app(state.values[0], &state.values[1..], ty),
                                &mut state.prefix,
                            );
                            state.values.clear();
                            state.values.push(func);
                        }
                        pending.push(Task::Resume(state, atom));
                        pending.push(Task::Visit(input));
                    } else {
                        let core = state.core;
                        let result = match core.kind {
                            CoreKind::Lam { params: [], .. } | CoreKind::App { args: [], .. } => {
                                b.with_type(state.values[0], core.ty)
                            }
                            CoreKind::App { .. } => {
                                b.app(state.values[0], &state.values[1..], core.ty)
                            }
                            CoreKind::Let { .. } => b.with_type(state.values[1], core.ty),
                            _ => crate::traverse::rebuild(
                                b,
                                core,
                                &mut state.values.into_iter(),
                                &mut |_| None,
                            ),
                        };
                        results.push(self.finish(state.prefix, result));
                    }
                }
            }
        }
        results.pop().unwrap()
    }
}

#[cfg(test)]
mod tests;
