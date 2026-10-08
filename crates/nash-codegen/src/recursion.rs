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
        let b = self.build;
        let result = match &core.kind {
            CoreKind::Var(n) => env.get(n).map_or(core, |r| r.value),
            CoreKind::Lit(_) | CoreKind::Error => core,
            CoreKind::Lam { params, body } => b.lam(
                params,
                self.term(body, &without(env, params.iter().map(|p| p.name)))?,
            ),
            CoreKind::App { func, args } => {
                if let CoreKind::Var(n) = &func.kind
                    && let Some((dispatcher, tag, arms)) = env.get(n).and_then(|r| r.packet)
                    && args.len() >= arms[usize::from(tag)].params.len()
                {
                    let args = args
                        .iter()
                        .map(|arg| self.term(arg, env))
                        .collect::<Result<Vec<_>, _>>()?;
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
                    if args.len() == arity {
                        call
                    } else {
                        b.app(call, &args[arity..], core.ty)
                    }
                } else if let CoreKind::Var(n) = &func.kind
                    && let Some(Replacement {
                        direct: Some((target, excluded)),
                        ..
                    }) = env.get(n)
                {
                    let args = args
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| !excluded.iter().any(|x| usize::from(*x) == *i))
                        .map(|(_, arg)| self.term(arg, env))
                        .collect::<Result<Vec<_>, _>>()?;
                    b.app(target, &args, core.ty)
                } else {
                    let func = self.term(func, env)?;
                    let args = args
                        .iter()
                        .map(|arg| self.term(arg, env))
                        .collect::<Result<Vec<_>, _>>()?;
                    b.app(func, &args, core.ty)
                }
            }
            CoreKind::Let {
                binder,
                value,
                body,
            } => b.let_(
                *binder,
                self.term(value, env)?,
                self.term(body, &without(env, [binder.name]))?,
            ),
            CoreKind::LetRec { binders, body } => self.group(binders, body, env)?,
            CoreKind::Case {
                kind,
                scrutinee,
                branches,
                default,
            } => {
                let scrutinee = self.term(scrutinee, env)?;
                let branches = branches
                    .iter()
                    .map(|branch| {
                        Ok(Branch {
                            body: self.term(
                                branch.body,
                                &without(env, branch.binders.iter().map(|p| p.name)),
                            )?,
                            ..*branch
                        })
                    })
                    .collect::<Result<Vec<_>, Error>>()?;
                let default = default.map(|d| self.term(d, env)).transpose()?;
                b.case(*kind, scrutinee, &branches, default, core.ty)
            }
            CoreKind::Constr { tag, fields } => {
                let fields = fields
                    .iter()
                    .map(|f| self.term(f, env))
                    .collect::<Result<Vec<_>, _>>()?;
                b.constr(*tag, &fields, core.ty)
            }
            CoreKind::Field {
                record,
                index,
                arity,
            } => b.alloc(
                core.ty,
                CoreKind::Field {
                    record: self.term(record, env)?,
                    index: *index,
                    arity: *arity,
                },
            ),
            CoreKind::Builtin { func, args } => {
                let args = args
                    .iter()
                    .map(|a| self.term(a, env))
                    .collect::<Result<Vec<_>, _>>()?;
                b.alloc(
                    core.ty,
                    CoreKind::Builtin {
                        func: *func,
                        args: b.arena.alloc_slice_copy(&args),
                    },
                )
            }
            CoreKind::Trace { message, body } => {
                b.trace(self.term(message, env)?, self.term(body, env)?)
            }
            CoreKind::Delay(body) => b.delay(self.term(body, env)?),
            CoreKind::Force(body) => b.force(self.term(body, env)?, core.ty),
        };
        Ok(b.with_type(result, core.ty))
    }
    fn group(
        &mut self,
        binders: &'a [RecBinder<'a>],
        body: &'a Core<'a>,
        env: &Environment<'a>,
    ) -> Result<&'a Core<'a>, Error> {
        let b = self.build;
        if binders.is_empty() {
            return self.term(body, env);
        }
        // A fully static worker is an explicit delayed recursive value. Knot
        // creation returns the delay; each original call still forces its body.
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
            let inner_body = self.term(single.body, &inner_env)?;
            let inner = b.with_type(b.lam(&[self_arg], inner_body), self_ty);
            let knot = b.app(b.lam(&[self_arg], raw), &[inner], single.binder.ty);
            let continuation = self.term(body, &without(env, [single.binder.name]))?;
            return Ok(b.let_(single.binder, knot, continuation));
        }
        if binders
            .iter()
            .any(|r| r.params.is_empty() && !matches!(r.body.kind, CoreKind::Delay(_)))
        {
            return Err(Error::RecursiveValue);
        }
        let names = binders
            .iter()
            .map(|r| r.binder.name)
            .collect::<HashSet<_>>();
        if names.len() != binders.len() {
            return Err(Error::DuplicateBinder);
        }
        let outer = without(env, names.iter().copied());
        let continuation = self.term(body, &outer)?;
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
            let inner_body = self.term(single.body, &inner_env)?;
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
            return Ok(b.let_(single.binder, definition, continuation));
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
                    result: if let ([], CoreKind::Delay(body)) = (rb.params, rb.body.kind) {
                        body.ty
                    } else {
                        rb.body.ty
                    },
                }
            })
            .collect::<Vec<_>>();
        let arms = b.arena.alloc_slice_copy(&arms);
        let dispatcher_ty = Ty::Runtime(b.arena.alloc(RuntimeTy::Dispatcher(arms)));
        let request_ty = Ty::Runtime(b.arena.alloc(RuntimeTy::Request(arms)));
        let results_ty = Ty::Runtime(b.arena.alloc(RuntimeTy::Results(arms)));
        let dispatcher = self.fresh("dispatch", dispatcher_ty);
        let request = self.fresh("request", request_ty);
        let mut branches = Vec::new();
        for (index, rb) in binders.iter().enumerate() {
            let self_arg = self.fresh("self", dispatcher_ty);
            let mut recursive_env = outer.clone();
            for (target, callee) in binders.iter().enumerate() {
                let alias = self.packet_alias(self_arg, target as u16, callee.params, arms);
                recursive_env.insert(
                    callee.binder.name,
                    Replacement {
                        value: alias,
                        packet: Some((self_arg, target as u16, arms)),
                        direct: None,
                    },
                );
            }
            let member = if let ([], CoreKind::Delay(body)) = (rb.params, rb.body.kind) {
                body
            } else {
                rb.body
            };
            let body = self.term(
                member,
                &without(&recursive_env, rb.params.iter().map(|p| p.name)),
            )?;
            let mut params = vec![self_arg];
            params.extend_from_slice(rb.params);
            branches.push(Branch {
                test: Test::Tag(index as u16),
                binders: b.arena.alloc_slice_copy(&params),
                body,
            });
        }
        let dispatch = b.lam(
            &[request],
            b.case(
                CaseKind::Tag,
                b.var(request.name, request.ty),
                &branches,
                None,
                results_ty,
            ),
        );
        let dispatch = b.with_type(dispatch, dispatcher_ty);
        let mut result = continuation;
        for (index, rb) in binders.iter().enumerate().rev() {
            let alias = self.packet_alias(dispatcher, index as u16, rb.params, arms);
            result = b.let_(rb.binder, alias, result);
        }
        Ok(b.let_(dispatcher, dispatch, result))
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
        let call = b.app(
            b.var(dispatcher.name, dispatcher.ty),
            &[b.constr(tag, &fields, packet_ty)],
            arms[usize::from(tag)].result,
        );
        if params.is_empty() {
            b.delay(call)
        } else {
            b.lam(&params, call)
        }
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
    f(core, scope);
    let extended = |names: Vec<Name<'a>>| {
        let mut s = scope.clone();
        s.extend(names);
        s
    };
    match &core.kind {
        CoreKind::Lam { params, body } => {
            visit(body, &extended(params.iter().map(|p| p.name).collect()), f)
        }
        CoreKind::App { func, args } => {
            visit(func, scope, f);
            for arg in *args {
                visit(arg, scope, f);
            }
        }
        CoreKind::Let {
            binder,
            value,
            body,
        } => {
            visit(value, scope, f);
            visit(body, &extended(vec![binder.name]), f);
        }
        CoreKind::LetRec { binders, body } => {
            let group = extended(binders.iter().map(|b| b.binder.name).collect());
            for rb in *binders {
                let mut inner = group.clone();
                inner.extend(rb.params.iter().map(|p| p.name));
                visit(rb.body, &inner, f);
            }
            visit(body, &group, f);
        }
        CoreKind::Case {
            scrutinee,
            branches,
            default,
            ..
        } => {
            visit(scrutinee, scope, f);
            for b in *branches {
                visit(
                    b.body,
                    &extended(b.binders.iter().map(|p| p.name).collect()),
                    f,
                );
            }
            if let Some(d) = default {
                visit(d, scope, f);
            }
        }
        CoreKind::Constr { fields: args, .. } | CoreKind::Builtin { args, .. } => {
            for arg in *args {
                visit(arg, scope, f);
            }
        }
        CoreKind::Field { record, .. } => visit(record, scope, f),
        CoreKind::Trace { message, body } => {
            visit(message, scope, f);
            visit(body, scope, f);
        }
        CoreKind::Delay(body) | CoreKind::Force(body) => visit(body, scope, f),
        CoreKind::Var(_) | CoreKind::Lit(_) | CoreKind::Error => {}
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
