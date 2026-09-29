//! Remove recursive function groups using self-application and dispatchers.
//! Generated names share the builder's supply and avoid every input unique.
use nash_ir::{
    build::Builder,
    core::*,
    ty::{DispatchArm, RuntimeTy, TermTy, Ty},
};
use std::collections::{HashMap, HashSet};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("recursive values without function parameters are not supported")]
    RecursiveValue,
    #[error("recursive group repeats a binder")]
    DuplicateBinder,
    #[error("recursive group exceeds the constructor tag limit")]
    TooManyFunctions,
}

/// Parameters forwarded unchanged by every saturated self call. A first-class
/// use or partial application disables static lifting.
pub fn static_params(f: Name<'_>, params: &[Binder<'_>], body: &Core<'_>) -> Vec<u16> {
    let mut candidates = (0..params.len())
        .filter_map(|i| u16::try_from(i).ok())
        .collect::<Vec<_>>();
    let mut calls = 0;
    let mut uses = 0;
    visit(
        body,
        &HashSet::new(),
        &mut |node, shadowed| match &node.kind {
            CoreKind::App {
                func:
                    Core {
                        kind: CoreKind::Var(g),
                        ..
                    },
                args,
            } if *g == f && !shadowed.contains(g) => {
                calls += 1;
                if args.len() < params.len() {
                    candidates.clear();
                }
                candidates.retain(|&i| matches!(args.get(i as usize),Some(Core { kind: CoreKind::Var(v), .. }) if *v == params[i as usize].name && !shadowed.contains(v)));
            }
            CoreKind::Var(g) if *g == f && !shadowed.contains(g) => uses += 1,
            _ => {}
        },
    );
    if calls > 0 && calls == uses {
        candidates
    } else {
        Vec::new()
    }
}

pub fn rewrite<'a>(build: &Builder<'a>, core: &'a Core<'a>) -> Result<&'a Core<'a>, Error> {
    let mut used = HashSet::new();
    visit(core, &HashSet::new(), &mut |node, scope| {
        used.extend(scope.iter().map(|n| n.unique));
        if let CoreKind::Var(n) = &node.kind {
            used.insert(n.unique);
        }
    });
    Rewriter { build, used }.term(core, &HashMap::new())
}

#[derive(Clone)]
struct Replacement<'a> {
    value: &'a Core<'a>,
    /// Direct calls can omit arguments proved to be unchanged variables.
    packet: Option<(Binder<'a>, u16, &'a [DispatchArm<'a>])>,
    direct: Option<(&'a Core<'a>, Vec<u16>)>,
}
type Environment<'a> = HashMap<Name<'a>, Replacement<'a>>;
struct Rewriter<'b, 'a> {
    build: &'b Builder<'a>,
    used: HashSet<u32>,
}

// Continuations retain unfinished work on the heap, including nested groups.
enum Task<'a> {
    Visit(&'a Core<'a>, Environment<'a>),
    Finish(Continuation<'a>),
}
type Continuation<'a> = Box<
    dyn FnOnce(
            &mut Rewriter<'_, 'a>,
            &mut Vec<Task<'a>>,
            &mut Vec<&'a Core<'a>>,
        ) -> Result<Option<&'a Core<'a>>, Error>
        + 'a,
>;
struct Mutual<'a> {
    binders: &'a [RecBinder<'a>],
    outer: Environment<'a>,
    continuation: &'a Core<'a>,
    arms: &'a [DispatchArm<'a>],
    dispatcher: Binder<'a>,
    request: Binder<'a>,
    branches: Vec<Branch<'a>>,
}

impl<'a> Rewriter<'_, 'a> {
    fn fresh(&mut self, text: &'a str, ty: Ty<'a>) -> Binder<'a> {
        loop {
            let name = self.build.fresh(text);
            if self.used.insert(name.unique) {
                return Binder { name, ty };
            }
        }
    }
    fn eta(&mut self, func: &'a Core<'a>, params: &[Binder<'a>], result: Ty<'a>) -> &'a Core<'a> {
        let params = params
            .iter()
            .map(|p| Binder {
                ty: p.ty,
                ..self.fresh(p.name.text, p.ty)
            })
            .collect::<Vec<_>>();
        let args = params
            .iter()
            .map(|p| self.build.var(p.name, p.ty))
            .collect::<Vec<_>>();
        self.build.lam(&params, self.build.app(func, &args, result))
    }
    fn term(&mut self, core: &'a Core<'a>, env: &Environment<'a>) -> Result<&'a Core<'a>, Error> {
        let mut tasks = vec![Task::Visit(core, env.clone())];
        let mut results = Vec::new();
        while let Some(task) = tasks.pop() {
            match task {
                Task::Visit(core, env) => self.schedule(core, env, &mut tasks, &mut results)?,
                Task::Finish(finish) => {
                    if let Some(result) = finish(self, &mut tasks, &mut results)? {
                        results.push(result);
                    }
                }
            }
        }
        Ok(results.pop().unwrap())
    }

    fn schedule(
        &mut self,
        core: &'a Core<'a>,
        env: Environment<'a>,
        tasks: &mut Vec<Task<'a>>,
        results: &mut Vec<&'a Core<'a>>,
    ) -> Result<(), Error> {
        let b = self.build;
        match core.kind {
            CoreKind::Var(n) => {
                results.push(b.with_type(env.get(&n).map_or(core, |r| r.value), core.ty));
                return Ok(());
            }
            CoreKind::Lit(_) | CoreKind::Error => {
                results.push(core);
                return Ok(());
            }
            CoreKind::LetRec { binders, body } => {
                return self.group(core.ty, binders, body, env, tasks);
            }
            _ => {}
        }
        let start = results.len();
        if let CoreKind::App {
            func: Core {
                kind: CoreKind::Var(n),
                ..
            },
            args,
        } = core.kind
        {
            if let Some((dispatcher, tag, arms)) = env.get(n).and_then(|r| r.packet)
                && args.len() >= arms[usize::from(tag)].params.len()
            {
                tasks.push(Task::Finish(Box::new(move |this, _, results| {
                    let b = this.build;
                    let args: Vec<_> = results.drain(start..).collect();
                    let arm = &arms[usize::from(tag)];
                    let arity = arm.params.len();
                    let mut fields = vec![b.var(dispatcher.name, dispatcher.ty)];
                    fields.extend_from_slice(&args[..arity]);
                    let packet_ty = Ty::Runtime(b.arena.alloc(RuntimeTy::Packet { arms, tag }));
                    let call = b.app(
                        b.var(dispatcher.name, dispatcher.ty),
                        &[b.constr(tag, &fields, packet_ty)],
                        arm.result,
                    );
                    let result = if args.len() == arity {
                        call
                    } else {
                        b.app(call, &args[arity..], core.ty)
                    };
                    Ok(Some(b.with_type(result, core.ty)))
                })));
                tasks.extend(args.iter().rev().map(|arg| Task::Visit(arg, env.clone())));
                return Ok(());
            }
            if let Some(Replacement {
                direct: Some((target, excluded)),
                ..
            }) = env.get(n)
            {
                let target = *target;
                tasks.push(Task::Finish(Box::new(move |this, _, results| {
                    let args: Vec<_> = results.drain(start..).collect();
                    Ok(Some(this.build.app(target, &args, core.ty)))
                })));
                tasks.extend(
                    args.iter()
                        .enumerate()
                        .rev()
                        .filter(|(i, _)| !excluded.iter().any(|x| usize::from(*x) == *i))
                        .map(|(_, arg)| Task::Visit(arg, env.clone())),
                );
                return Ok(());
            }
        }
        tasks.push(Task::Finish(Box::new(move |this, _, results| {
            let b = this.build;
            let mut children = results.drain(start..);
            let result = match core.kind {
                CoreKind::Lam { params, .. } => b.lam(params, children.next().unwrap()),
                CoreKind::App { .. } => {
                    let func = children.next().unwrap();
                    b.app(func, &children.by_ref().collect::<Vec<_>>(), core.ty)
                }
                CoreKind::Let { binder, .. } => {
                    b.let_(binder, children.next().unwrap(), children.next().unwrap())
                }
                CoreKind::Case {
                    kind,
                    branches,
                    default,
                    ..
                } => {
                    let scrutinee = children.next().unwrap();
                    let branches: Vec<_> = branches
                        .iter()
                        .map(|branch| Branch {
                            body: children.next().unwrap(),
                            ..*branch
                        })
                        .collect();
                    let default = default.map(|_| children.next().unwrap());
                    b.case(kind, scrutinee, &branches, default, core.ty)
                }
                CoreKind::Constr { tag, .. } => {
                    b.constr(tag, &children.by_ref().collect::<Vec<_>>(), core.ty)
                }
                CoreKind::Field { index, arity, .. } => b.alloc(
                    core.ty,
                    CoreKind::Field {
                        record: children.next().unwrap(),
                        index,
                        arity,
                    },
                ),
                CoreKind::Builtin { func, .. } => b.alloc(
                    core.ty,
                    CoreKind::Builtin {
                        func,
                        args: b
                            .arena
                            .alloc_slice_copy(&children.by_ref().collect::<Vec<_>>()),
                    },
                ),
                CoreKind::Trace { .. } => {
                    b.trace(children.next().unwrap(), children.next().unwrap())
                }
                CoreKind::Delay(_) => b.delay(children.next().unwrap()),
                CoreKind::Force(_) => b.force(children.next().unwrap(), core.ty),
                _ => unreachable!(),
            };
            Ok(Some(b.with_type(result, core.ty)))
        })));
        match core.kind {
            CoreKind::Lam { params, body } => tasks.push(Task::Visit(
                body,
                without(&env, params.iter().map(|p| p.name)),
            )),
            CoreKind::Let {
                binder,
                value,
                body,
            } => {
                tasks.push(Task::Visit(body, without(&env, [binder.name])));
                tasks.push(Task::Visit(value, env));
            }
            CoreKind::Case {
                scrutinee,
                branches,
                default,
                ..
            } => {
                if let Some(body) = default {
                    tasks.push(Task::Visit(body, env.clone()));
                }
                tasks.extend(branches.iter().rev().map(|branch| {
                    Task::Visit(
                        branch.body,
                        without(&env, branch.binders.iter().map(|p| p.name)),
                    )
                }));
                tasks.push(Task::Visit(scrutinee, env));
            }
            _ => {
                let mut children = Vec::new();
                core.push_children_reversed(&mut children);
                tasks.extend(
                    children
                        .into_iter()
                        .map(|child| Task::Visit(child, env.clone())),
                );
            }
        }
        Ok(())
    }

    fn group(
        &mut self,
        ty: Ty<'a>,
        binders: &'a [RecBinder<'a>],
        body: &'a Core<'a>,
        env: Environment<'a>,
        tasks: &mut Vec<Task<'a>>,
    ) -> Result<(), Error> {
        let b = self.build;
        // Retain the original type view even for empty groups.
        tasks.push(Task::Finish(Box::new(move |this, _, results| {
            Ok(Some(this.build.with_type(results.pop().unwrap(), ty)))
        })));
        if binders.is_empty() {
            tasks.push(Task::Visit(body, env));
            return Ok(());
        }
        if let [single] = binders
            && single.params.is_empty()
            && matches!(single.body.kind, CoreKind::Delay(_))
        {
            let self_ty = Ty::Runtime(b.arena.alloc(RuntimeTy::SelfFunction(single.binder.ty)));
            let self_arg = self.fresh("self", self_ty);
            let raw = b.app(
                b.var(self_arg.name, self_arg.ty),
                &[b.var(self_arg.name, self_arg.ty)],
                single.binder.ty,
            );
            let mut inner_env = env.clone();
            inner_env.insert(
                single.binder.name,
                Replacement {
                    value: raw,
                    packet: None,
                    direct: None,
                },
            );
            tasks.push(Task::Finish(Box::new(move |this, tasks, results| {
                let b = this.build;
                let inner_body = results.pop().unwrap();
                let inner = b.with_type(b.lam(&[self_arg], inner_body), self_ty);
                let knot = b.app(b.lam(&[self_arg], raw), &[inner], single.binder.ty);
                tasks.push(Task::Finish(Box::new(move |this, _, results| {
                    Ok(Some(this.build.let_(
                        single.binder,
                        knot,
                        results.pop().unwrap(),
                    )))
                })));
                tasks.push(Task::Visit(body, without(&env, [single.binder.name])));
                Ok(None)
            })));
            tasks.push(Task::Visit(single.body, inner_env));
            return Ok(());
        }
        if binders.iter().any(|r| r.params.is_empty()) {
            return Err(Error::RecursiveValue);
        }
        let names: HashSet<_> = binders.iter().map(|r| r.binder.name).collect();
        if names.len() != binders.len() {
            return Err(Error::DuplicateBinder);
        }
        let outer = without(&env, names);
        let outer_body = outer.clone();
        tasks.push(Task::Finish(Box::new(move |this, tasks, results| {
            let continuation = results.pop().unwrap();
            this.group_after_continuation(binders, outer, continuation, tasks)?;
            Ok(None)
        })));
        tasks.push(Task::Visit(body, outer_body));
        Ok(())
    }

    fn group_after_continuation(
        &mut self,
        binders: &'a [RecBinder<'a>],
        outer: Environment<'a>,
        continuation: &'a Core<'a>,
        tasks: &mut Vec<Task<'a>>,
    ) -> Result<(), Error> {
        let b = self.build;
        if let [single] = binders {
            // Recheck metadata: a stale static annotation must never discard work.
            let proven = static_params(single.binder.name, single.params, single.body);
            let statics = single
                .static_params
                .iter()
                .copied()
                .filter(|i| proven.contains(i))
                .collect::<Vec<_>>();
            let dynamic = single
                .params
                .iter()
                .enumerate()
                .filter(|(i, _)| !statics.iter().any(|x| usize::from(*x) == *i))
                .map(|(_, p)| *p)
                .collect::<Vec<_>>();
            let result_ty = single.body.ty;
            let worker_ty = if dynamic.is_empty() {
                Ty::Runtime(b.arena.alloc(RuntimeTy::Delay(result_ty)))
            } else {
                let params = dynamic.iter().map(|p| p.ty).collect::<Vec<_>>();
                Ty::Term(
                    b.arena
                        .alloc(TermTy::Fun(b.arena.alloc_slice_copy(&params), result_ty)),
                )
            };
            let self_ty = Ty::Runtime(b.arena.alloc(RuntimeTy::SelfFunction(worker_ty)));
            let self_arg = self.fresh("self", self_ty);
            let raw_self_call = b.app(
                b.var(self_arg.name, self_arg.ty),
                &[b.var(self_arg.name, self_arg.ty)],
                worker_ty,
            );
            let self_call = if dynamic.is_empty() {
                b.force(raw_self_call, result_ty)
            } else {
                raw_self_call
            };
            let value = if statics.is_empty() {
                self.eta(self_call, single.params, result_ty)
            } else {
                self_call
            };
            let mut inner_env = without(&outer, single.params.iter().map(|p| p.name));
            inner_env.insert(
                single.binder.name,
                Replacement {
                    value,
                    packet: None,
                    direct: Some((self_call, statics.clone())),
                },
            );
            tasks.push(Task::Finish(Box::new(move |this, _, results| {
                let b = this.build;
                let inner_body = results.pop().unwrap();

                let mut inner_params = vec![self_arg];
                inner_params.extend_from_slice(&dynamic);
                // If every parameter is static, delay the worker to keep knot creation lazy.
                let inner = b.lam(
                    &inner_params,
                    if dynamic.is_empty() {
                        b.delay(inner_body)
                    } else {
                        inner_body
                    },
                );
                let inner = b.with_type(inner, self_ty);
                let knot = b.app(b.lam(&[self_arg], raw_self_call), &[inner], worker_ty);
                let definition = if statics.is_empty() {
                    knot
                } else {
                    let applied = if dynamic.is_empty() {
                        b.force(knot, result_ty)
                    } else {
                        b.app(
                            knot,
                            &dynamic
                                .iter()
                                .map(|p| b.var(p.name, p.ty))
                                .collect::<Vec<_>>(),
                            result_ty,
                        )
                    };
                    b.lam(single.params, applied)
                };
                // Recursive calls into a fully static worker must force its delayed body.
                // Such calls cannot terminate by changing an argument, but can occur in
                // dead branches; represent them correctly without evaluating them early.
                Ok(Some(b.let_(single.binder, definition, continuation)))
            })));
            tasks.push(Task::Visit(single.body, inner_env));
            return Ok(());
        }
        if binders.len() > usize::from(u16::MAX) + 1 {
            return Err(Error::TooManyFunctions);
        }
        let arms = binders
            .iter()
            .map(|rb| {
                let params = rb.params.iter().map(|p| p.ty).collect::<Vec<_>>();
                DispatchArm {
                    params: b.arena.alloc_slice_copy(&params),
                    result: rb.body.ty,
                }
            })
            .collect::<Vec<_>>();
        let arms = b.arena.alloc_slice_copy(&arms);
        let dispatcher_ty = Ty::Runtime(b.arena.alloc(RuntimeTy::Dispatcher(arms)));
        let request_ty = Ty::Runtime(b.arena.alloc(RuntimeTy::Request(arms)));
        let dispatcher = self.fresh("dispatch", dispatcher_ty);
        let request = self.fresh("request", request_ty);
        self.mutual(
            Mutual {
                binders,
                outer,
                continuation,
                arms,
                dispatcher,
                request,
                branches: Vec::new(),
            },
            tasks,
        )
    }

    fn mutual(&mut self, mut state: Mutual<'a>, tasks: &mut Vec<Task<'a>>) -> Result<(), Error> {
        let b = self.build;
        let index = state.branches.len();
        if let Some(rb) = state.binders.get(index) {
            let self_arg = self.fresh("self", state.dispatcher.ty);
            let mut recursive_env = state.outer.clone();
            for (target, callee) in state.binders.iter().enumerate() {
                let alias = self.packet_alias(self_arg, target as u16, callee.params, state.arms);
                recursive_env.insert(
                    callee.binder.name,
                    Replacement {
                        value: alias,
                        packet: Some((self_arg, target as u16, state.arms)),
                        direct: None,
                    },
                );
            }
            let env = without(&recursive_env, rb.params.iter().map(|p| p.name));
            tasks.push(Task::Finish(Box::new(move |this, tasks, results| {
                let mut params = vec![self_arg];
                params.extend_from_slice(rb.params);
                state.branches.push(Branch {
                    test: Test::Tag(index as u16),
                    binders: this.build.arena.alloc_slice_copy(&params),
                    body: results.pop().unwrap(),
                });
                this.mutual(state, tasks)?;
                Ok(None)
            })));
            tasks.push(Task::Visit(rb.body, env));
        } else {
            let results_ty = Ty::Runtime(b.arena.alloc(RuntimeTy::Results(state.arms)));
            let dispatch = b.lam(
                &[state.request],
                b.case(
                    CaseKind::Tag,
                    b.var(state.request.name, state.request.ty),
                    &state.branches,
                    None,
                    results_ty,
                ),
            );
            let dispatch = b.with_type(dispatch, state.dispatcher.ty);
            let mut result = state.continuation;
            for (index, rb) in state.binders.iter().enumerate().rev() {
                let alias =
                    self.packet_alias(state.dispatcher, index as u16, rb.params, state.arms);
                result = b.let_(rb.binder, alias, result);
            }
            let result = b.let_(state.dispatcher, dispatch, result);
            tasks.push(Task::Finish(Box::new(move |_, _, _| Ok(Some(result)))));
        }
        Ok(())
    }

    fn packet_alias(
        &mut self,
        dispatcher: Binder<'a>,
        tag: u16,
        params: &[Binder<'a>],
        arms: &'a [DispatchArm<'a>],
    ) -> &'a Core<'a> {
        let params = params
            .iter()
            .map(|p| Binder {
                ty: p.ty,
                ..self.fresh(p.name.text, p.ty)
            })
            .collect::<Vec<_>>();
        let b = self.build;
        let mut fields = vec![b.var(dispatcher.name, dispatcher.ty)];
        fields.extend(params.iter().map(|p| b.var(p.name, p.ty)));
        let packet_ty = Ty::Runtime(b.arena.alloc(RuntimeTy::Packet { arms, tag }));
        b.lam(
            &params,
            b.app(
                b.var(dispatcher.name, dispatcher.ty),
                &[b.constr(tag, &fields, packet_ty)],
                arms[usize::from(tag)].result,
            ),
        )
    }
}
fn without<'a>(
    env: &Environment<'a>,
    names: impl IntoIterator<Item = Name<'a>>,
) -> Environment<'a> {
    let mut env = env.clone();
    for name in names {
        env.remove(&name);
    }
    env
}

/// Walk with lexical binders, so static analysis does not confuse shadowing
/// with forwarding a parameter. Core uses unique names even for equal text.
fn visit<'a>(
    core: &Core<'a>,
    scope: &HashSet<Name<'a>>,
    f: &mut impl FnMut(&Core<'a>, &HashSet<Name<'a>>),
) {
    let mut pending = vec![(core, scope.clone())];
    let mut children = Vec::new();
    while let Some((core, scope)) = pending.pop() {
        f(core, &scope);
        let extended = |names: Vec<Name<'a>>| {
            let mut inner = scope.clone();
            inner.extend(names);
            inner
        };
        match core.kind {
            CoreKind::Lam { params, body } => {
                pending.push((body, extended(params.iter().map(|p| p.name).collect())))
            }
            CoreKind::Let {
                binder,
                value,
                body,
            } => {
                pending.push((body, extended(vec![binder.name])));
                pending.push((value, scope));
            }
            CoreKind::LetRec { binders, body } => {
                let group = extended(binders.iter().map(|r| r.binder.name).collect());
                pending.push((body, group.clone()));
                for rb in binders.iter().rev() {
                    let mut inner = group.clone();
                    inner.extend(rb.params.iter().map(|p| p.name));
                    pending.push((rb.body, inner));
                }
            }
            CoreKind::Case {
                scrutinee,
                branches,
                default,
                ..
            } => {
                if let Some(body) = default {
                    pending.push((body, scope.clone()));
                }
                pending.extend(branches.iter().rev().map(|branch| {
                    (
                        branch.body,
                        extended(branch.binders.iter().map(|p| p.name).collect()),
                    )
                }));
                pending.push((scrutinee, scope));
            }
            _ => {
                children.clear();
                core.push_children_reversed(&mut children);
                pending.extend(children.iter().map(|child| (*child, scope.clone())));
            }
        }
    }
}

#[cfg(test)]
#[path = "recursion_tests.rs"]
mod tests;

#[cfg(test)]
mod metadata_tests {
    use super::*;
    use nash_ir::ty::ConstTy;
    use nash_plutus::arena::Arena;

    fn binder<'a>(b: &Builder<'a>, text: &'a str, ty: Ty<'a>) -> Binder<'a> {
        Binder {
            name: b.fresh(text),
            ty,
        }
    }

    fn function_ty<'a>(b: &Builder<'a>, params: &[Ty<'a>], result: Ty<'a>) -> Ty<'a> {
        Ty::Term(
            b.arena
                .alloc(TermTy::Fun(b.arena.alloc_slice_copy(params), result)),
        )
    }

    #[test]
    fn single_worker_types_follow_dynamic_and_static_parameters() {
        let arena = Arena::new();
        let b = Builder::new(&arena);
        let int = Ty::Const(&ConstTy::Int);
        for statics in [&[][..], &[0][..], &[0, 1][..]] {
            let f = binder(&b, "f", function_ty(&b, &[int, int], int));
            let x = binder(&b, "x", int);
            let y = binder(&b, "y", int);
            let source = b.let_rec(
                &[RecBinder {
                    binder: f,
                    params: arena.alloc_slice_copy(&[x, y]),
                    static_params: statics,
                    body: b.app(
                        b.var(f.name, f.ty),
                        &[b.var(x.name, x.ty), b.var(y.name, y.ty)],
                        int,
                    ),
                }],
                b.int(0),
            );
            let result = rewrite(&b, source).unwrap();
            assert_eq!(result.ty, int);
            let mut workers = 0;
            result.walk(&mut |node| {
                if let Ty::Runtime(RuntimeTy::SelfFunction(worker)) = node.ty {
                    workers += 1;
                    if statics.len() == 2 {
                        assert_eq!(*worker, Ty::Runtime(&RuntimeTy::Delay(int)));
                    } else {
                        assert_eq!(*worker, function_ty(&b, &vec![int; 2 - statics.len()], int));
                    }
                }
                if let CoreKind::Force(_) = node.kind {
                    assert_eq!(node.ty, int);
                }
            });
            assert!(workers > 0);
        }
    }

    #[test]
    fn mutual_dispatch_retains_heterogeneous_result_and_packet_types() {
        let arena = Arena::new();
        let b = Builder::new(&arena);
        let int = Ty::Const(&ConstTy::Int);
        let boolean = Ty::Const(&ConstTy::Bool);
        let result_function = function_ty(&b, &[boolean], boolean);
        let f = binder(&b, "f", function_ty(&b, &[int], int));
        let g = binder(&b, "g", function_ty(&b, &[int], result_function));
        let x = binder(&b, "x", int);
        let y = binder(&b, "y", int);
        let z = binder(&b, "z", boolean);
        // f overapplies g: the dispatcher call returns a function; only the
        // following application returns bool. This distinction must survive.
        let call_g = b.app(
            b.var(g.name, g.ty),
            &[b.var(x.name, x.ty), b.error(boolean)],
            boolean,
        );
        let unused = binder(&b, "unused", boolean);
        let source = b.let_rec(
            &[
                RecBinder {
                    binder: f,
                    params: arena.alloc_slice_copy(&[x]),
                    static_params: &[],
                    body: b.let_(unused, call_g, b.int(0)),
                },
                RecBinder {
                    binder: g,
                    params: arena.alloc_slice_copy(&[y]),
                    static_params: &[],
                    body: b.lam(&[z], b.var(z.name, z.ty)),
                },
            ],
            b.var(g.name, g.ty),
        );
        let result = rewrite(&b, source).unwrap();
        assert_eq!(result.ty, g.ty);
        let mut packet_calls = 0;
        let mut dispatchers = 0;
        result.walk(&mut |node| {
            if let Ty::Runtime(RuntimeTy::Dispatcher(arms)) = node.ty {
                dispatchers += 1;
                assert_eq!(arms[0].result, int);
                assert_eq!(arms[1].result, result_function);
            }
            if let CoreKind::App { func, args } = &node.kind
                && let Ty::Runtime(RuntimeTy::Dispatcher(arms)) = func.ty
                && let Ty::Runtime(RuntimeTy::Packet { tag, .. }) = args[0].ty
            {
                packet_calls += 1;
                assert_eq!(node.ty, arms[usize::from(*tag)].result);
            }
        });
        assert!(dispatchers > 0);
        assert!(packet_calls >= 3);
    }
}
