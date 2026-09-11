//! Fixed-point analysis of the type variables inspected by generated code.
use nash_ast::{primitives, *};
use nash_region::Located;
use nash_solve::SolvedTypes;
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DemandKind {
    Native,
    Deep,
}
pub type Demands<'a> = HashMap<NodeId, BTreeMap<&'a str, DemandKind>>;
type Scope<'a> = HashMap<&'a str, Option<NodeId>>;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_slots_are_recursive_but_data_arguments_are_opaque() {
        let a = Located::at_zero(Type::Var("a"));
        let args = [&a];
        let list = Located::at_zero(Type::Named {
            reference: QualifiedName {
                home: primitives::builtin_home(),
                name: "list",
            },
            args: &args,
        });
        let data = Located::at_zero(Type::Named {
            reference: QualifiedName {
                home: primitives::literal_home(),
                name: "Option",
            },
            args: &args,
        });
        let mut out = BTreeMap::new();
        variables(&data, DemandKind::Native, &mut out);
        assert!(out.is_empty());
        variables(&list, DemandKind::Native, &mut out);
        assert_eq!(out, BTreeMap::from([("a", DemandKind::Native)]));
        variables(&data, DemandKind::Deep, &mut out);
        assert_eq!(out["a"], DemandKind::Deep);
    }
}

struct Owner<'a> {
    free: &'a [&'a str],
    parent: Option<NodeId>,
}
struct Call<'a> {
    caller: NodeId,
    target: NodeId,
    args: &'a [&'a Located<Type<'a>>],
}
struct MethodCall<'a> {
    caller: NodeId,
    trait_: QualifiedName<'a>,
    method: &'a str,
    args: &'a [&'a Located<Type<'a>>],
}
struct Analysis<'a> {
    demands: Demands<'a>,
    owners: HashMap<NodeId, Owner<'a>>,
    top: HashMap<QualifiedName<'a>, NodeId>,
    methods: HashMap<(QualifiedName<'a>, &'a str), Vec<NodeId>>,
    calls: Vec<Call<'a>>,
    method_calls: Vec<MethodCall<'a>>,
}

/// Type names are local to each definition's solved scheme. Captured variables
/// flow to the enclosing owner; only quantified variables substitute at calls.
pub fn analyze<'a>(modules: &[(&Module<'a>, &SolvedTypes<'a>)]) -> Demands<'a> {
    let mut a = Analysis {
        demands: HashMap::new(),
        owners: HashMap::new(),
        top: HashMap::new(),
        methods: HashMap::new(),
        calls: vec![],
        method_calls: vec![],
    };
    for (module, _) in modules {
        for def in declarations(module.decls) {
            let (name, _) = definition(def);
            a.top.insert(
                QualifiedName {
                    home: module.name,
                    name: name.value,
                },
                NodeId::def(name),
            );
        }
        for trait_ in module.traits {
            for method in trait_.value.methods {
                if let Some(def) = method.default {
                    a.methods
                        .entry((
                            QualifiedName {
                                home: module.name,
                                name: trait_.value.name.value,
                            },
                            method.name.value,
                        ))
                        .or_default()
                        .push(NodeId::def(definition(def).0));
                }
            }
        }
        for impl_ in module.impls {
            for def in impl_.value.methods {
                let (name, _) = definition(def);
                a.methods
                    .entry((impl_.value.trait_, name.value))
                    .or_default()
                    .push(NodeId::def(name));
            }
        }
    }
    for (module, solved) in modules {
        let env = declarations(module.decls)
            .into_iter()
            .map(|def| {
                let (name, _) = definition(def);
                (name.value, Some(NodeId::def(name)))
            })
            .collect();
        for def in declarations(module.decls) {
            a.def(def, None, &env, solved);
        }
        for trait_ in module.traits {
            for method in trait_.value.methods {
                if let Some(def) = method.default {
                    a.def(def, None, &env, solved);
                }
            }
        }
        for impl_ in module.impls {
            for def in impl_.value.methods {
                a.def(def, None, &env, solved);
            }
        }
    }
    loop {
        let previous = a.demands.clone();
        for call in &a.calls {
            let Some(owner) = a.owners.get(&call.target) else {
                continue;
            };
            for (&name, &kind) in &previous[&call.target] {
                if let Some(index) = owner.free.iter().position(|v| *v == name)
                    && let Some(typ) = call.args.get(index)
                {
                    variables(typ, kind, a.demands.entry(call.caller).or_default());
                }
            }
        }
        for call in &a.method_calls {
            // A Given can select any applicable implementation at specialization.
            // Demand arguments only when a real method body inspects a layout.
            if let Some(kind) = a
                .methods
                .get(&(call.trait_, call.method))
                .into_iter()
                .flatten()
                .flat_map(|id| previous[id].values())
                .max()
                .copied()
            {
                for typ in call.args {
                    variables(typ, kind, a.demands.entry(call.caller).or_default());
                }
            }
        }
        for (id, owner) in &a.owners {
            if let Some(parent) = owner.parent {
                for (&name, &kind) in &previous[id] {
                    if !owner.free.contains(&name) {
                        insert(a.demands.entry(parent).or_default(), name, kind);
                    }
                }
            }
        }
        if previous == a.demands {
            return a.demands;
        }
    }
}
fn definition<'a>(def: &'a Def<'a>) -> (&'a Located<&'a str>, &'a Located<Expr<'a>>) {
    match def {
        Def::Def { name, body, .. } | Def::TypedDef { name, body, .. } => (name, body),
    }
}
fn declarations<'a>(mut decls: &'a Decls<'a>) -> Vec<&'a Def<'a>> {
    let mut defs = vec![];
    loop {
        match decls {
            Decls::Empty => return defs,
            Decls::Declare { definition, next } => {
                defs.push(*definition);
                decls = next;
            }
            Decls::DeclareRec {
                definition,
                following,
                next,
            } => {
                defs.push(*definition);
                defs.extend_from_slice(following);
                decls = next;
            }
        }
    }
}
fn insert<'a>(out: &mut BTreeMap<&'a str, DemandKind>, name: &'a str, kind: DemandKind) {
    out.entry(name)
        .and_modify(|old| *old = (*old).max(kind))
        .or_insert(kind);
}
fn variables<'a>(
    typ: &'a Located<Type<'a>>,
    kind: DemandKind,
    out: &mut BTreeMap<&'a str, DemandKind>,
) {
    match &typ.value {
        Type::Var(name) => insert(out, name, kind),
        Type::Named { reference, args }
            if kind == DemandKind::Deep
                || (reference.home == primitives::builtin_home()
                    && matches!(reference.name, "list" | "pair" | "array")) =>
        {
            for t in *args {
                variables(t, kind, out);
            }
        }
        Type::App { head, args } => {
            variables(head, kind, out);
            for t in *args {
                variables(t, kind, out);
            }
        }
        Type::Lambda { from, to } if kind == DemandKind::Deep => {
            variables(from, kind, out);
            variables(to, kind, out);
        }
        Type::Record { fields } if kind == DemandKind::Deep => {
            for f in *fields {
                variables(f.typ, kind, out);
            }
        }
        Type::Tuple {
            first,
            second,
            rest,
        } if kind == DemandKind::Deep => {
            variables(first, kind, out);
            variables(second, kind, out);
            for t in *rest {
                variables(t, kind, out);
            }
        }
        Type::Alias {
            target: AliasType::Filled { typ, .. },
            ..
        } => variables(typ, kind, out),
        Type::Alias {
            target: AliasType::Open(body),
            arguments,
            ..
        } => {
            let mut names = BTreeMap::new();
            variables(body, kind, &mut names);
            for (name, demand) in names {
                if let Some(arg) = arguments.iter().find(|a| a.name == name) {
                    variables(arg.typ, demand, out);
                } else {
                    insert(out, name, demand);
                }
            }
        }
        _ => {}
    }
}
fn bind<'a>(p: &'a Located<Pattern<'a>>, target: Option<NodeId>, env: &mut Scope<'a>) {
    match &p.value {
        Pattern::Var(name) => {
            env.insert(name, target);
        }
        Pattern::Record(names) => {
            for name in *names {
                env.insert(name, target);
            }
        }
        Pattern::Alias { pattern, name } => {
            bind(pattern, target, env);
            env.insert(name, target);
        }
        Pattern::Tuple {
            first,
            second,
            rest,
        } => {
            bind(first, target, env);
            bind(second, target, env);
            for p in *rest {
                bind(p, target, env);
            }
        }
        Pattern::List(ps) => {
            for p in *ps {
                bind(p, target, env);
            }
        }
        Pattern::Cons { head, tail } => {
            bind(head, target, env);
            bind(tail, target, env);
        }
        Pattern::Constructor(c) => {
            for arg in c.arguments {
                bind(arg.pattern, target, env);
            }
        }
        _ => {}
    }
}
impl<'a> Analysis<'a> {
    fn owner(&mut self, id: NodeId, parent: Option<NodeId>, solved: &SolvedTypes<'a>) {
        self.owners.insert(
            id,
            Owner {
                free: solved
                    .schemes
                    .get(&id)
                    .map_or(&[], |s| s.annotation.free_vars),
                parent,
            },
        );
        self.demands.entry(id).or_default();
    }
    fn def(
        &mut self,
        def: &'a Def<'a>,
        parent: Option<NodeId>,
        env: &Scope<'a>,
        solved: &SolvedTypes<'a>,
    ) {
        let (name, body) = definition(def);
        let id = NodeId::def(name);
        self.owner(id, parent, solved);
        let mut env = env.clone();
        env.insert(name.value, Some(id));
        match def {
            Def::Def { args, .. } => {
                for p in *args {
                    bind(p, None, &mut env);
                }
            }
            Def::TypedDef { args, .. } => {
                for p in *args {
                    bind(p.pattern, None, &mut env);
                }
            }
        }
        self.expr(body, id, &env, solved);
    }
    fn reference(
        &mut self,
        reference: QualifiedName<'a>,
        node: NodeId,
        owner: NodeId,
        solved: &SolvedTypes<'a>,
    ) {
        if reference.home == primitives::builtin_home()
            && primitives::BUILTINS
                .iter()
                .any(|b| b.name == reference.name && b.lowering.is_core_only())
        {
            let kind = if reference.name == "castToData" {
                // The checked Big value already is Data. Its nominal layout
                // cannot change this identity operation or its binder shape.
                DemandKind::Native
            } else {
                DemandKind::Deep
            };
            if let Some(instance) = solved.instances.get(&node) {
                for t in instance.type_args {
                    variables(t, kind, self.demands.entry(owner).or_default());
                }
            }
            if let Some(t) = solved.exprs.get(&node) {
                variables(t, kind, self.demands.entry(owner).or_default());
            }
        } else if let Some(&target) = self.top.get(&reference) {
            self.call(target, node, owner, solved);
        } else {
            let traits: Vec<_> = self
                .methods
                .keys()
                .filter(|(t, m)| t.home == reference.home && *m == reference.name)
                .map(|(t, _)| *t)
                .collect();
            for trait_ in traits {
                self.method(trait_, reference.name, node, owner, solved);
            }
        }
    }
    fn call(&mut self, target: NodeId, node: NodeId, caller: NodeId, solved: &SolvedTypes<'a>) {
        self.calls.push(Call {
            caller,
            target,
            args: solved.instances.get(&node).map_or(&[], |i| i.type_args),
        });
    }
    fn method(
        &mut self,
        trait_: QualifiedName<'a>,
        method: &'a str,
        node: NodeId,
        caller: NodeId,
        solved: &SolvedTypes<'a>,
    ) {
        self.method_calls.push(MethodCall {
            caller,
            trait_,
            method,
            args: solved.instances.get(&node).map_or(&[], |i| i.type_args),
        });
    }
    fn expr(
        &mut self,
        expr: &'a Located<Expr<'a>>,
        owner: NodeId,
        env: &Scope<'a>,
        solved: &SolvedTypes<'a>,
    ) {
        let node = NodeId::expr(expr);
        match &expr.value {
            Expr::VarLocal(name) => {
                if let Some(Some(target)) = env.get(name) {
                    self.call(*target, node, owner, solved);
                }
            }
            Expr::VarTopLevel(reference)
            | Expr::VarForeign { reference, .. }
            | Expr::VarOperator { reference, .. } => {
                self.reference(*reference, node, owner, solved)
            }
            Expr::VarMethod { trait_, method, .. } => {
                self.method(*trait_, method, node, owner, solved)
            }
            Expr::Str(_) | Expr::Bytes(_) | Expr::Int(_) => {
                let (name, method) = match expr.value {
                    Expr::Str(_) => ("FromString", "fromString"),
                    Expr::Bytes(_) => ("FromBytes", "fromBytes"),
                    _ => ("FromInt", "fromInt"),
                };
                self.method(
                    QualifiedName {
                        home: primitives::literal_home(),
                        name,
                    },
                    method,
                    node,
                    owner,
                    solved,
                );
            }
            Expr::List(items) => {
                if let Some(typ) = solved.exprs.get(&node) {
                    variables(
                        typ,
                        DemandKind::Native,
                        self.demands.entry(owner).or_default(),
                    );
                }
                for item in *items {
                    self.expr(item, owner, env, solved);
                }
            }
            Expr::Assert(e) | Expr::Comptime(e) => self.expr(e, owner, env, solved),
            Expr::Fail(e) | Expr::Todo(e) => {
                if let Some(e) = e {
                    self.expr(e, owner, env, solved);
                }
            }
            Expr::Trace { message, body } => {
                self.expr(message, owner, env, solved);
                self.expr(body, owner, env, solved);
            }
            Expr::Binop {
                reference,
                left,
                right,
                ..
            } => {
                self.reference(*reference, node, owner, solved);
                self.expr(left, owner, env, solved);
                self.expr(right, owner, env, solved);
            }
            Expr::Lambda { parameters, body } => {
                let mut env = env.clone();
                for p in *parameters {
                    bind(p, None, &mut env);
                }
                self.expr(body, owner, &env, solved);
            }
            Expr::Call {
                function,
                arguments,
            } => {
                self.expr(function, owner, env, solved);
                for arg in *arguments {
                    self.expr(arg, owner, env, solved);
                }
            }
            Expr::If {
                branches,
                final_else,
            } => {
                for b in *branches {
                    self.expr(b.condition, owner, env, solved);
                    self.expr(b.then_branch, owner, env, solved);
                }
                self.expr(final_else, owner, env, solved);
            }
            Expr::Let {
                definition: def,
                body,
            } => {
                self.def(def, Some(owner), env, solved);
                let mut env = env.clone();
                let (name, _) = definition(def);
                env.insert(name.value, Some(NodeId::def(name)));
                self.expr(body, owner, &env, solved);
            }
            Expr::LetRec { definitions, body } => {
                let mut env = env.clone();
                for d in *definitions {
                    let (name, _) = definition(d);
                    env.insert(name.value, Some(NodeId::def(name)));
                }
                for d in *definitions {
                    self.def(d, Some(owner), &env, solved);
                }
                self.expr(body, owner, &env, solved);
            }
            Expr::LetDestruct {
                pattern,
                value,
                body,
            } => {
                let id = NodeId::pattern(pattern);
                self.owner(id, Some(owner), solved);
                self.expr(value, id, env, solved);
                let mut env = env.clone();
                bind(pattern, Some(id), &mut env);
                self.expr(body, owner, &env, solved);
            }
            Expr::Case {
                scrutinee,
                branches,
            } => {
                self.expr(scrutinee, owner, env, solved);
                for b in *branches {
                    let mut env = env.clone();
                    bind(b.pattern, None, &mut env);
                    self.expr(b.body, owner, &env, solved);
                }
            }
            Expr::Access { record, .. } => self.expr(record, owner, env, solved),
            Expr::Update { base, fields, .. } => {
                self.expr(base, owner, env, solved);
                for f in *fields {
                    self.expr(f.value, owner, env, solved);
                }
            }
            Expr::Record { fields, .. } => {
                for f in *fields {
                    self.expr(f.value, owner, env, solved);
                }
            }
            Expr::Tuple {
                first,
                second,
                rest,
            } => {
                self.expr(first, owner, env, solved);
                self.expr(second, owner, env, solved);
                for e in *rest {
                    self.expr(e, owner, env, solved);
                }
            }
            Expr::Accessor(_) | Expr::VarConstructor { .. } | Expr::Unit => {}
        }
    }
}

#[cfg(test)]
mod graph_tests {
    use super::*;
    use bumpalo::Bump;
    use nash_solve::solved::{Instance, Scheme};
    fn var<'a>(b: &'a Bump, name: &'a str) -> &'a Located<Type<'a>> {
        b.alloc(Located::at_zero(Type::Var(name)))
    }
    fn named<'a>(
        b: &'a Bump,
        home: ModuleName<'a>,
        name: &'a str,
        arg: &'a Located<Type<'a>>,
    ) -> &'a Located<Type<'a>> {
        b.alloc(Located::at_zero(Type::Named {
            reference: QualifiedName { home, name },
            args: b.alloc_slice_copy(&[arg]),
        }))
    }
    fn expr<'a>(b: &'a Bump, e: Expr<'a>) -> &'a Located<Expr<'a>> {
        b.alloc(Located::at_zero(e))
    }
    fn def<'a>(
        b: &'a Bump,
        s: &mut SolvedTypes<'a>,
        name: &'a str,
        free: &'a [&'a str],
        body: &'a Located<Expr<'a>>,
    ) -> &'a Def<'a> {
        let name = b.alloc(Located::at_zero(name));
        let id = NodeId::def(name);
        let annotation = b.alloc(Annotation {
            context: &[],
            free_vars: free,
            typ: var(b, "result"),
        });
        s.schemes.insert(
            id,
            Scheme {
                annotation,
                binder: id,
            },
        );
        b.alloc(Def::Def {
            name,
            args: &[],
            body,
        })
    }
    fn module<'a>(b: &'a Bump, defs: &[&'a Def<'a>]) -> Module<'a> {
        let mut decls: &Decls<'a> = b.alloc(Decls::Empty);
        for d in defs.iter().rev() {
            decls = b.alloc(Decls::Declare {
                definition: d,
                next: decls,
            });
        }
        Module {
            traits: &[],
            impls: &[],
            kind: ModuleKind::Normal,
            name: ModuleName {
                package: None,
                name: "Main",
            },
            exports: Exports::Everything(nash_region::Region::one()),
            docs: b.alloc(Docs::NoDocs(nash_region::Region::one())),
            decls,
            unions: &[],
            aliases: &[],
            binops: &[],
        }
    }
    fn instance<'a>(
        b: &'a Bump,
        s: &mut SolvedTypes<'a>,
        e: &'a Located<Expr<'a>>,
        typ: &'a Located<Type<'a>>,
    ) {
        s.instances.insert(
            NodeId::expr(e),
            Instance {
                type_args: b.alloc_slice_copy(&[typ]),
                evidence: &[],
            },
        );
    }
    fn cast<'a>(
        b: &'a Bump,
        s: &mut SolvedTypes<'a>,
        home: ModuleName<'a>,
        typ: &'a Located<Type<'a>>,
    ) -> &'a Located<Expr<'a>> {
        let e = expr(
            b,
            Expr::VarTopLevel(QualifiedName {
                home,
                name: "castToData",
            }),
        );
        instance(b, s, e, typ);
        e
    }
    #[test]
    fn plain_recursive_option_does_not_demand_type_arguments() {
        let b = Bump::new();
        let mut s = SolvedTypes::default();
        let call = expr(&b, Expr::VarLocal("plain"));
        let option = named(&b, primitives::literal_home(), "option", var(&b, "a"));
        instance(&b, &mut s, call, option);
        let d = def(&b, &mut s, "plain", &["a"], call);
        let m = module(&b, &[d]);
        assert!(analyze(&[(&m, &s)])[&NodeId::def(definition(d).0)].is_empty());
    }
    #[test]
    fn nil_demand_propagates_across_helpers_with_scoped_names() {
        let b = Bump::new();
        let mut s = SolvedTypes::default();
        let nil = expr(&b, Expr::List(&[]));
        s.exprs.insert(
            NodeId::expr(nil),
            named(&b, primitives::builtin_home(), "list", var(&b, "element")),
        );
        let helper = def(&b, &mut s, "helper", &["element"], nil);
        let call = expr(&b, Expr::VarLocal("helper"));
        instance(
            &b,
            &mut s,
            call,
            named(&b, primitives::builtin_home(), "pair", var(&b, "caller")),
        );
        let caller = def(&b, &mut s, "caller", &["caller"], call);
        let opaque = expr(&b, Expr::VarLocal("caller"));
        instance(
            &b,
            &mut s,
            opaque,
            named(&b, primitives::literal_home(), "Option", var(&b, "opaque")),
        );
        let outer = def(&b, &mut s, "outer", &["opaque"], opaque);
        let m = module(&b, &[helper, caller, outer]);
        let demands = analyze(&[(&m, &s)]);
        assert_eq!(
            demands[&NodeId::def(definition(helper).0)],
            BTreeMap::from([("element", DemandKind::Native)])
        );
        assert_eq!(
            demands[&NodeId::def(definition(caller).0)],
            BTreeMap::from([("caller", DemandKind::Native)])
        );
        assert!(demands[&NodeId::def(definition(outer).0)].is_empty());
    }
    #[test]
    fn captured_demands_reach_parent_without_leaking_quantified_names() {
        let b = Bump::new();
        let mut s = SolvedTypes::default();
        let c = cast(&b, &mut s, primitives::builtin_home(), var(&b, "captured"));
        let inner = def(&b, &mut s, "inner", &["own"], c);
        let body = expr(
            &b,
            Expr::Let {
                definition: inner,
                body: expr(&b, Expr::Unit),
            },
        );
        let outer = def(&b, &mut s, "outer", &["captured"], body);
        let m = module(&b, &[outer]);
        let demands = analyze(&[(&m, &s)]);
        assert_eq!(
            demands[&NodeId::def(definition(outer).0)],
            BTreeMap::from([("captured", DemandKind::Native)])
        );
    }
    #[test]
    fn user_builtin_homonym_is_not_a_cast() {
        let b = Bump::new();
        let mut s = SolvedTypes::default();
        let c = cast(
            &b,
            &mut s,
            ModuleName {
                package: None,
                name: "Builtin",
            },
            var(&b, "a"),
        );
        let d = def(&b, &mut s, "plain", &["a"], c);
        let m = module(&b, &[d]);
        assert!(analyze(&[(&m, &s)])[&NodeId::def(definition(d).0)].is_empty());
    }
    #[test]
    fn representation_evidence_alone_does_not_demand_a_layout() {
        let b = Bump::new();
        let mut s = SolvedTypes::default();
        let typ = var(&b, "a");
        let call = expr(
            &b,
            Expr::VarTopLevel(QualifiedName {
                home: primitives::builtin_home(),
                name: "identity",
            }),
        );
        s.instances.insert(
            NodeId::expr(call),
            Instance {
                type_args: b.alloc_slice_copy(&[typ]),
                evidence: b.alloc_slice_fill_iter([Evidence::Repr {
                    trait_: primitives::ReprTrait::Big,
                    typ,
                }]),
            },
        );
        let d = def(&b, &mut s, "plain", &["a"], call);
        let m = module(&b, &[d]);
        assert!(analyze(&[(&m, &s)])[&NodeId::def(definition(d).0)].is_empty());
    }

    #[test]
    fn method_and_implicit_literal_calls_follow_casting_impls() {
        let b = Bump::new();
        let mut s = SolvedTypes::default();
        let c = cast(&b, &mut s, primitives::builtin_home(), var(&b, "implArg"));
        let method = def(&b, &mut s, "fromInt", &["implArg"], c);
        let literal = expr(&b, Expr::Int(4));
        instance(&b, &mut s, literal, var(&b, "a"));
        let d = def(&b, &mut s, "value", &["a"], literal);
        let mut m = module(&b, &[d]);
        let impl_ = b.alloc(Located::at_zero(Impl {
            variables: &["implArg"],
            trait_: QualifiedName {
                home: primitives::literal_home(),
                name: "FromInt",
            },
            context: &[],
            heads: &[],
            methods: b.alloc_slice_copy(&[method]),
        }));
        m.impls = b.alloc_slice_copy(&[&*impl_]);
        assert_eq!(
            analyze(&[(&m, &s)])[&NodeId::def(definition(d).0)],
            BTreeMap::from([("a", DemandKind::Native)])
        );
    }
}
