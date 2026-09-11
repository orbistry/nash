//! Remove recursive function groups using self-application and dispatchers.
//! Generated names share the builder's supply and avoid every input unique.
use nash_ir::{build::Builder, core::*, ty::Ty};
use std::collections::{HashMap, HashSet};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("recursive values without function parameters are not supported")]
    RecursiveValue,
    #[error("recursive group repeats a binder")]
    DuplicateBinder,
}

/// Parameters forwarded unchanged by every saturated self call. A first-class
/// use or partial application disables static lifting.
pub fn static_params(f: Name<'_>, params: &[Binder<'_>], body: &Core<'_>) -> Vec<u16> {
    let mut candidates = (0..params.len())
        .filter_map(|i| u16::try_from(i).ok())
        .collect::<Vec<_>>();
    let mut calls = 0;
    let mut uses = 0;
    visit(body, &HashSet::new(), &mut |node, shadowed| match node {
        Core::App {
            func: Core::Var(g),
            args,
        } if *g == f && !shadowed.contains(g) => {
            calls += 1;
            if args.len() < params.len() {
                candidates.clear();
            }
            candidates.retain(|&i| matches!(args.get(i as usize),Some(Core::Var(v)) if *v == params[i as usize].name && !shadowed.contains(v)));
        }
        Core::Var(g) if *g == f && !shadowed.contains(g) => uses += 1,
        _ => {}
    });
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
        if let Core::Var(n) = node {
            used.insert(n.unique);
        }
    });
    Rewriter { build, used }.term(core, &HashMap::new())
}

#[derive(Clone)]
struct Replacement<'a> {
    value: &'a Core<'a>,
    /// Direct calls can omit arguments proved to be unchanged variables.
    direct: Option<(&'a Core<'a>, Vec<u16>)>,
}
type Environment<'a> = HashMap<Name<'a>, Replacement<'a>>;
struct Rewriter<'b, 'a> {
    build: &'b Builder<'a>,
    used: HashSet<u32>,
}

impl<'a> Rewriter<'_, 'a> {
    fn fresh(&mut self, text: &'a str) -> Binder<'a> {
        loop {
            let name = self.build.fresh(text);
            if self.used.insert(name.unique) {
                return Binder {
                    name,
                    ty: Ty::Erased,
                };
            }
        }
    }
    fn eta(&mut self, func: &'a Core<'a>, params: &[Binder<'a>]) -> &'a Core<'a> {
        let params = params
            .iter()
            .map(|p| Binder {
                ty: p.ty,
                ..self.fresh(p.name.text)
            })
            .collect::<Vec<_>>();
        let args = params
            .iter()
            .map(|p| self.build.var(p.name))
            .collect::<Vec<_>>();
        self.build.lam(&params, self.build.app(func, &args))
    }
    fn term(&mut self, core: &'a Core<'a>, env: &Environment<'a>) -> Result<&'a Core<'a>, Error> {
        let b = self.build;
        Ok(match core {
            Core::Var(n) => env.get(n).map_or(core, |r| r.value),
            Core::Lit(_) | Core::Error => core,
            Core::Lam { params, body } => b.lam(
                params,
                self.term(body, &without(env, params.iter().map(|p| p.name)))?,
            ),
            Core::App { func, args } => {
                if let Core::Var(n) = func
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
                    b.app(target, &args)
                } else {
                    let func = self.term(func, env)?;
                    let args = args
                        .iter()
                        .map(|arg| self.term(arg, env))
                        .collect::<Result<Vec<_>, _>>()?;
                    b.app(func, &args)
                }
            }
            Core::Let {
                binder,
                value,
                body,
            } => b.let_(
                *binder,
                self.term(value, env)?,
                self.term(body, &without(env, [binder.name]))?,
            ),
            Core::LetRec { binders, body } => self.group(binders, body, env)?,
            Core::Case {
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
                b.case(*kind, scrutinee, &branches, default)
            }
            Core::Constr { tag, fields } => {
                let fields = fields
                    .iter()
                    .map(|f| self.term(f, env))
                    .collect::<Result<Vec<_>, _>>()?;
                b.constr(*tag, &fields)
            }
            Core::Field {
                record,
                index,
                arity,
            } => b.arena.alloc(Core::Field {
                record: self.term(record, env)?,
                index: *index,
                arity: *arity,
            }),
            Core::Builtin { func, args } => {
                let args = args
                    .iter()
                    .map(|a| self.term(a, env))
                    .collect::<Result<Vec<_>, _>>()?;
                b.arena.alloc(Core::Builtin {
                    func: *func,
                    args: b.arena.alloc_slice_copy(&args),
                })
            }
            Core::Cast {
                kind,
                from,
                to,
                arg,
            } => b.cast(*kind, *from, *to, self.term(arg, env)?),
            Core::Trace { message, body } => {
                b.trace(self.term(message, env)?, self.term(body, env)?)
            }
            Core::Delay(body) => b.delay(self.term(body, env)?),
            Core::Force(body) => b.force(self.term(body, env)?),
        })
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
        if binders.iter().any(|r| r.params.is_empty()) {
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
            let self_arg = self.fresh("self");
            let raw_self_call = b.app(b.var(self_arg.name), &[b.var(self_arg.name)]);
            let self_call = if dynamic.is_empty() {
                b.force(raw_self_call)
            } else {
                raw_self_call
            };
            let value = if statics.is_empty() {
                self.eta(self_call, single.params)
            } else {
                self_call
            };
            let mut inner_env = without(&outer, single.params.iter().map(|p| p.name));
            inner_env.insert(
                single.binder.name,
                Replacement {
                    value,
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
            let knot = b.app(b.lam(&[self_arg], raw_self_call), &[inner]);
            let definition = if statics.is_empty() {
                knot
            } else {
                let applied = if dynamic.is_empty() {
                    b.force(knot)
                } else {
                    b.app(
                        knot,
                        &dynamic.iter().map(|p| b.var(p.name)).collect::<Vec<_>>(),
                    )
                };
                b.lam(single.params, applied)
            };
            // Recursive calls into a fully static worker must force its delayed body.
            // Such calls cannot terminate by changing an argument, but can occur in
            // dead branches; represent them correctly without evaluating them early.
            return Ok(b.let_(single.binder, definition, continuation));
        }
        let cycle = self.fresh("cycle");
        let select = self.fresh("select");
        let mut recursive_env = outer.clone();
        let mut aliases = Vec::new();
        for (index, rb) in binders.iter().enumerate() {
            let picks = binders
                .iter()
                .map(|_| self.fresh("pick"))
                .collect::<Vec<_>>();
            let selector = b.lam(&picks, b.var(picks[index].name));
            let call = b.app(b.var(cycle.name), &[b.var(cycle.name), selector]);
            let alias = self.eta(call, rb.params);
            recursive_env.insert(
                rb.binder.name,
                Replacement {
                    value: alias,
                    direct: None,
                },
            );
            aliases.push(alias);
        }
        let arms = binders
            .iter()
            .map(|rb| {
                let body = self.term(
                    rb.body,
                    &without(&recursive_env, rb.params.iter().map(|p| p.name)),
                )?;
                Ok(b.lam(rb.params, body))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let dispatcher = b.lam(&[cycle, select], b.app(b.var(select.name), &arms));
        let mut result = continuation;
        for (rb, value) in binders.iter().zip(aliases).rev() {
            result = b.let_(rb.binder, value, result);
        }
        Ok(b.let_(cycle, dispatcher, result))
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
    match core {
        Core::Lam { params, body } => {
            visit(body, &extended(params.iter().map(|p| p.name).collect()), f)
        }
        Core::App { func, args } => {
            visit(func, scope, f);
            for arg in *args {
                visit(arg, scope, f);
            }
        }
        Core::Let {
            binder,
            value,
            body,
        } => {
            visit(value, scope, f);
            visit(body, &extended(vec![binder.name]), f);
        }
        Core::LetRec { binders, body } => {
            let group = extended(binders.iter().map(|b| b.binder.name).collect());
            for rb in *binders {
                let mut inner = group.clone();
                inner.extend(rb.params.iter().map(|p| p.name));
                visit(rb.body, &inner, f);
            }
            visit(body, &group, f);
        }
        Core::Case {
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
        Core::Constr { fields: args, .. } | Core::Builtin { args, .. } => {
            for arg in *args {
                visit(arg, scope, f);
            }
        }
        Core::Field { record, .. } => visit(record, scope, f),
        Core::Cast { arg, .. } => visit(arg, scope, f),
        Core::Trace { message, body } => {
            visit(message, scope, f);
            visit(body, scope, f);
        }
        Core::Delay(body) | Core::Force(body) => visit(body, scope, f),
        Core::Var(_) | Core::Lit(_) | Core::Error => {}
    }
}

#[cfg(test)]
#[path = "recursion_tests.rs"]
mod tests;
