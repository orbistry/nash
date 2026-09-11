//! Dependency ordering, lexical closure assembly, and shared strings.
use super::*;
use std::collections::HashSet;

impl<'a> Engine<'a, '_, '_> {
    /// Emit SCCs in dependency order. Local groups stay inside their source scope.
    pub fn emit_group(
        &self,
        group: usize,
        root: &'a Core<'a>,
        reachable_only: bool,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        self.emit_specs(self.groups[group].specs.to_vec(), root, reachable_only)
    }
    fn emit_specs(
        &self,
        mut selected: Vec<usize>,
        root: &'a Core<'a>,
        reachable_only: bool,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        if reachable_only {
            let mut needed = names(root);
            loop {
                let old = needed.len();
                for &i in &selected {
                    if needed.contains(&self.specs[i].binder.name.unique) {
                        if let Some(value) = self.specs[i].value {
                            needed.extend(names(value));
                        } else {
                            return Err(Error::ComptimeAssembly(
                                "recursive dependency is still being compiled".into(),
                            ));
                        }
                    }
                }
                if needed.len() == old {
                    break;
                }
            }
            selected.retain(|&i| needed.contains(&self.specs[i].binder.name.unique));
        }
        let mut edges = Vec::new();
        for &i in &selected {
            let value = self.specs[i].value.ok_or_else(|| {
                Error::ComptimeAssembly("dependency is still being compiled".into())
            })?;
            let used = names(value);
            edges.push(
                selected
                    .iter()
                    .enumerate()
                    .filter_map(|(j, &s)| {
                        used.contains(&self.specs[s].binder.name.unique)
                            .then_some(j)
                    })
                    .collect::<Vec<_>>(),
            );
        }
        let components = components(&edges);
        let mut body = root;
        for component in components.into_iter().rev() {
            let recursive = component.len() > 1 || edges[component[0]].contains(&component[0]);
            if recursive {
                let definitions = component
                    .into_iter()
                    .map(|index| {
                        let spec = &self.specs[selected[index]];
                        (spec.binder, spec.value.unwrap())
                    })
                    .collect::<Vec<_>>();
                body = self.recursive_bindings(&definitions, body, &mut 4096)?;
            } else {
                let spec = &self.specs[selected[component[0]]];
                body = self.ir.let_(spec.binder, spec.value.unwrap(), body);
            }
        }
        Ok(body)
    }
    /// Evaluate the function-valued RHS once, then introduce recursion inside
    /// the selected branch. Delayed references stay inside the resulting lambdas.
    fn recursive_bindings(
        &self,
        definitions: &[(Binder<'a>, &'a Core<'a>)],
        root: &'a Core<'a>,
        remaining: &mut usize,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        if *remaining == 0 {
            return Err(Error::RecursionNormalizationLimit);
        }
        *remaining -= 1;
        let Some(index) = definitions
            .iter()
            .position(|(_, v)| !matches!(v, Core::Lam { .. }))
        else {
            let binders = definitions
                .iter()
                .map(|(binder, value)| {
                    let Core::Lam { params, body } = value else {
                        unreachable!()
                    };
                    RecBinder {
                        binder: *binder,
                        params,
                        body,
                        static_params: self.ir.arena.alloc_slice_copy(
                            &crate::recursion::static_params(binder.name, params, body),
                        ),
                    }
                })
                .collect::<Vec<_>>();
            return Ok(self.ir.let_rec(&binders, root));
        };
        let recursive_names = definitions
            .iter()
            .map(|(binder, _)| binder.name.unique)
            .collect::<HashSet<_>>();
        let replace = |value| {
            let mut changed = definitions.to_vec();
            changed[index].1 = value;
            changed
        };
        Ok(match definitions[index].1 {
            Core::Let {
                binder,
                value,
                body,
            } => {
                let mut changed = replace(*body);
                if matches!(value, Core::Lam { .. }) {
                    changed.push((*binder, *value));
                    self.recursive_bindings(&changed, root, remaining)?
                } else if names(value).is_disjoint(&recursive_names) {
                    self.ir.let_(
                        *binder,
                        value,
                        self.recursive_bindings(&changed, root, remaining)?,
                    )
                } else {
                    return Err(Error::RecursiveValue);
                }
            }
            Core::LetRec { binders, body } => {
                let mut changed = replace(*body);
                changed.extend(
                    binders
                        .iter()
                        .map(|b| (b.binder, self.ir.lam(b.params, b.body))),
                );
                self.recursive_bindings(&changed, root, remaining)?
            }
            Core::Trace { message, body } if names(message).is_disjoint(&recursive_names) => {
                self.ir.trace(
                    message,
                    self.recursive_bindings(&replace(*body), root, remaining)?,
                )
            }
            Core::Case {
                kind,
                scrutinee,
                branches,
                default,
            } if names(scrutinee).is_disjoint(&recursive_names) => {
                let branches = branches
                    .iter()
                    .map(|branch| {
                        Ok(Branch {
                            body: self.recursive_bindings(
                                &replace(branch.body),
                                root,
                                remaining,
                            )?,
                            ..*branch
                        })
                    })
                    .collect::<Result<Vec<_>, Error<'a>>>()?;
                let default = default
                    .map(|body| self.recursive_bindings(&replace(body), root, remaining))
                    .transpose()?;
                self.ir.case(*kind, scrutinee, &branches, default)
            }
            Core::Error => self.ir.error(),
            value if names(value).is_disjoint(&recursive_names) => {
                let binder = definitions[index].0;
                let mut rest = definitions.to_vec();
                rest.remove(index);
                self.ir.let_(
                    binder,
                    value,
                    self.recursive_bindings(&rest, root, remaining)?,
                )
            }
            _ => return Err(Error::RecursiveValue),
        })
    }

    pub fn closed_dependencies(&mut self, core: &'a Core<'a>) -> Result<&'a Core<'a>, Error<'a>> {
        let mut group = 0;
        while group < self.groups.len() {
            self.drain(group)?;
            group += 1;
        }
        self.emit_specs((0..self.specs.len()).collect(), core, true)
    }
}

pub(crate) fn names(core: &Core<'_>) -> HashSet<u32> {
    let mut result = HashSet::new();
    let mut pending = vec![(core, HashSet::new())];
    while let Some((core, mut bound)) = pending.pop() {
        match core {
            Core::Var(n) => {
                if !bound.contains(&n.unique) {
                    result.insert(n.unique);
                }
            }
            Core::Lit(_) | Core::Error => {}
            Core::Lam { params, body } => {
                bound.extend(params.iter().map(|p| p.name.unique));
                pending.push((body, bound));
            }
            Core::Delay(body) | Core::Force(body) => pending.push((body, bound)),
            Core::App { func, args } => {
                for arg in *args {
                    pending.push((arg, bound.clone()));
                }
                pending.push((func, bound));
            }
            Core::Let {
                binder,
                value,
                body,
            } => {
                pending.push((value, bound.clone()));
                bound.insert(binder.name.unique);
                pending.push((body, bound));
            }
            Core::LetRec { binders, body } => {
                bound.extend(binders.iter().map(|b| b.binder.name.unique));
                pending.push((body, bound.clone()));
                for b in *binders {
                    let mut inner = bound.clone();
                    inner.extend(b.params.iter().map(|p| p.name.unique));
                    pending.push((b.body, inner));
                }
            }
            Core::Case {
                scrutinee,
                branches,
                default,
                ..
            } => {
                pending.push((scrutinee, bound.clone()));
                for b in *branches {
                    let mut inner = bound.clone();
                    inner.extend(b.binders.iter().map(|b| b.name.unique));
                    pending.push((b.body, inner));
                }
                if let Some(d) = default {
                    pending.push((d, bound));
                }
            }
            Core::Constr { fields, .. } => {
                for f in *fields {
                    pending.push((f, bound.clone()));
                }
            }
            Core::Builtin { args, .. } => {
                for a in *args {
                    pending.push((a, bound.clone()));
                }
            }
            Core::Field { record, .. } => pending.push((record, bound)),
            Core::Cast { arg, .. } => pending.push((arg, bound)),
            Core::Trace { message, body } => {
                pending.push((message, bound.clone()));
                pending.push((body, bound));
            }
        }
    }
    result
}

fn components(edges: &[Vec<usize>]) -> Vec<Vec<usize>> {
    struct Tarjan<'e> {
        edges: &'e [Vec<usize>],
        next: usize,
        indices: Vec<Option<usize>>,
        low: Vec<usize>,
        stack: Vec<usize>,
        active: Vec<bool>,
        result: Vec<Vec<usize>>,
    }
    fn visit(t: &mut Tarjan<'_>, v: usize) {
        let index = t.next;
        t.next += 1;
        t.indices[v] = Some(index);
        t.low[v] = index;
        t.stack.push(v);
        t.active[v] = true;
        for &w in &t.edges[v] {
            if t.indices[w].is_none() {
                visit(t, w);
                t.low[v] = t.low[v].min(t.low[w]);
            } else if t.active[w] {
                t.low[v] = t.low[v].min(t.indices[w].unwrap());
            }
        }
        if t.low[v] == index {
            let mut component = Vec::new();
            loop {
                let w = t.stack.pop().unwrap();
                t.active[w] = false;
                component.push(w);
                if w == v {
                    break;
                }
            }
            component.sort_unstable();
            t.result.push(component);
        }
    }
    let n = edges.len();
    let mut t = Tarjan {
        edges,
        next: 0,
        indices: vec![None; n],
        low: vec![0; n],
        stack: Vec::new(),
        active: vec![false; n],
        result: Vec::new(),
    };
    for v in 0..n {
        if t.indices[v].is_none() {
            visit(&mut t, v);
        }
    }
    t.result
}

pub(super) fn hoist_strings<'a>(build: &Builder<'a>, core: &'a Core<'a>) -> &'a Core<'a> {
    use nash_plutus::constant::Constant;
    struct Hoist<'a, 'b> {
        build: &'b Builder<'a>,
        strings: HashMap<&'a str, Binder<'a>>,
        bindings: Vec<(Binder<'a>, &'a Core<'a>)>,
    }
    impl<'a> Hoist<'a, '_> {
        fn term(&mut self, core: &'a Core<'a>) -> &'a Core<'a> {
            match core {
                Core::Lit(Constant::String(text)) => {
                    let binder = *self.strings.entry(text).or_insert_with(|| {
                        let binder = Binder {
                            name: self.build.fresh("message"),
                            ty: Ty::Const(&ConstTy::String),
                        };
                        self.bindings.push((binder, core));
                        binder
                    });
                    self.build.var(binder.name)
                }
                Core::Var(_) | Core::Lit(_) | Core::Error => core,
                Core::Lam { params, body } => {
                    let body = self.term(body);
                    self.build.lam(params, body)
                }
                Core::App { func, args } => {
                    let func = self.term(func);
                    let args = args.iter().map(|a| self.term(a)).collect::<Vec<_>>();
                    self.build.app(func, &args)
                }
                Core::Let {
                    binder,
                    value,
                    body,
                } => {
                    let value = self.term(value);
                    let body = self.term(body);
                    self.build.let_(*binder, value, body)
                }
                Core::LetRec { binders, body } => {
                    let binders = binders
                        .iter()
                        .map(|b| RecBinder {
                            body: self.term(b.body),
                            ..*b
                        })
                        .collect::<Vec<_>>();
                    let body = self.term(body);
                    self.build.let_rec(&binders, body)
                }
                Core::Case {
                    kind,
                    scrutinee,
                    branches,
                    default,
                } => {
                    let scrutinee = self.term(scrutinee);
                    let branches = branches
                        .iter()
                        .map(|b| Branch {
                            body: self.term(b.body),
                            ..*b
                        })
                        .collect::<Vec<_>>();
                    let default = default.map(|d| self.term(d));
                    self.build.case(*kind, scrutinee, &branches, default)
                }
                Core::Constr { tag, fields } => {
                    let fields = fields.iter().map(|f| self.term(f)).collect::<Vec<_>>();
                    self.build.constr(*tag, &fields)
                }
                Core::Field {
                    record,
                    index,
                    arity,
                } => {
                    let record = self.term(record);
                    self.build.field(record, *index, *arity)
                }
                Core::Builtin { func, args } => {
                    let args = args.iter().map(|a| self.term(a)).collect::<Vec<_>>();
                    self.build.builtin(*func, &args)
                }
                Core::Cast {
                    kind,
                    from,
                    to,
                    arg,
                } => {
                    let arg = self.term(arg);
                    self.build.cast(*kind, *from, *to, arg)
                }
                Core::Trace { message, body } => {
                    let message = self.term(message);
                    let body = self.term(body);
                    self.build.trace(message, body)
                }
                Core::Delay(body) => {
                    let body = self.term(body);
                    self.build.delay(body)
                }
                Core::Force(body) => {
                    let body = self.term(body);
                    self.build.force(body)
                }
            }
        }
    }
    let mut hoist = Hoist {
        build,
        strings: HashMap::new(),
        bindings: Vec::new(),
    };
    let mut core = hoist.term(core);
    for (binder, value) in hoist.bindings.into_iter().rev() {
        core = build.let_(binder, value, core);
    }
    core
}
