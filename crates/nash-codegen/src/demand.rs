//! Fixed-point analysis of the type variables inspected by generated code.
use nash_ast::{primitives, *};
use nash_region::Located;
use nash_solve::SolvedTypes;
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DemandKind {
    /// Encoded list storage distinguishes pairs from ordinary Data, not payloads.
    DataListElement,
    Native,
    Encoding,
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
    external: HashSet<QualifiedName<'a>>,
    literal_traits: [QualifiedName<'a>; 3],
}

fn literal_traits<'a>(tables: &nash_can::environment::Tables<'a>) -> [QualifiedName<'a>; 3] {
    ["FromInt", "FromString", "FromBytes"].map(|name| {
        tables.core_trait(QualifiedName {
            home: primitives::literal_home(),
            name,
        })
    })
}

/// Type names are local to each definition's solved scheme. Captured variables
/// flow to the enclosing owner; only quantified variables substitute at calls.
pub fn analyze<'a>(
    modules: &[(
        &Module<'a>,
        &SolvedTypes<'a>,
        &nash_can::environment::Tables<'a>,
    )],
) -> Demands<'a> {
    let mut a = Analysis {
        demands: HashMap::new(),
        owners: HashMap::new(),
        top: HashMap::new(),
        methods: HashMap::new(),
        calls: vec![],
        method_calls: vec![],
        literal_traits: literal_traits(&nash_can::environment::Tables::default()),
        external: modules
            .iter()
            .flat_map(|(module, _, _)| {
                module.unions.iter().filter_map(|union| {
                    union.value.data_layout.map(|_| QualifiedName {
                        home: module.name,
                        name: union.value.name.value,
                    })
                })
            })
            .collect(),
    };
    for primitive in primitives::PRIMITIVES {
        if primitives::data_union(primitive.name).is_some() {
            a.external.insert(QualifiedName {
                home: primitives::builtin_home(),
                name: primitive.name,
            });
        }
    }
    for (module, _, _) in modules {
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
    for (module, solved, tables) in modules {
        a.literal_traits = literal_traits(tables);
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
                    operation_variables(
                        typ,
                        kind,
                        &a.external,
                        a.demands.entry(call.caller).or_default(),
                    );
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
                    operation_variables(
                        typ,
                        kind,
                        &a.external,
                        a.demands.entry(call.caller).or_default(),
                    );
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
        // Declaration IDs have no scheme-local variable names. Instantiated
        // generic declarations appear as ordinary Type::Var in solved schemes.
        Type::DeclaredHole(_) => {}
        Type::Var(name) => insert(out, name, kind),
        Type::Named { reference, args }
            if kind == DemandKind::Deep
                || (kind == DemandKind::Native
                    && reference.home == primitives::builtin_home()
                    && matches!(reference.name, "list" | "pair" | "array")) =>
        {
            for t in *args {
                variables(t, kind, out);
            }
        }
        Type::App { head, args } => {
            variables(head, kind, out);
            if kind != DemandKind::DataListElement {
                for t in *args {
                    variables(t, kind, out);
                }
            }
        }
        Type::Lambda { from, to } if kind == DemandKind::Deep => {
            variables(from, kind, out);
            variables(to, kind, out);
        }
        Type::Function { arguments, result } if kind == DemandKind::Deep => {
            for argument in *arguments {
                variables(argument, kind, out);
            }
            variables(result, kind, out);
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
        Pattern::Pair { first, second } => {
            bind(first, target, env);
            bind(second, target, env);
        }
        Pattern::DataList { elements, tail } => {
            for p in *elements {
                bind(p, target, env);
            }
            if let Some(p) = tail {
                bind(p, target, env);
            }
        }
        Pattern::DataTuple {
            first,
            second,
            rest,
        }
        | Pattern::Tuple {
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
fn operation_variables<'a>(
    typ: &'a Located<Type<'a>>,
    kind: DemandKind,
    external: &HashSet<QualifiedName<'a>>,
    out: &mut BTreeMap<&'a str, DemandKind>,
) {
    if kind == DemandKind::Encoding {
        encoding_variables(typ, external, out);
    } else {
        variables(typ, kind, out);
    }
}

/// Encoding already-encoded values only inspects their outer storage shape.
/// Payload layout demands belong to the operations that construct those values.
fn encoding_variables<'a>(
    typ: &'a Located<Type<'a>>,
    external: &HashSet<QualifiedName<'a>>,
    out: &mut BTreeMap<&'a str, DemandKind>,
) {
    match &typ.value {
        Type::DeclaredHole(_) => {}
        Type::Var(name) => insert(out, name, DemandKind::Encoding),
        Type::Named { reference, args }
            if reference.home == primitives::builtin_home() && reference.name == "data_list" =>
        {
            variables(args[0], DemandKind::DataListElement, out);
        }
        Type::Named { reference, .. }
            if external.contains(reference)
                || reference
                    .name
                    .chars()
                    .next()
                    .is_some_and(char::is_uppercase)
                || (reference.home == primitives::builtin_home()
                    && matches!(reference.name, "data_pair" | "data_tuple")) => {}
        Type::Named { reference, args }
            if reference.home == primitives::builtin_home()
                && matches!(reference.name, "list" | "pair") =>
        {
            variables(typ, DemandKind::Native, out);
            for argument in *args {
                encoding_variables(argument, external, out);
            }
        }
        Type::Tuple {
            first,
            second,
            rest,
        } => {
            encoding_variables(first, external, out);
            encoding_variables(second, external, out);
            for field in *rest {
                encoding_variables(field, external, out);
            }
        }
        Type::Record { fields } => {
            for field in *fields {
                encoding_variables(field.typ, external, out);
            }
        }
        Type::Alias {
            target: AliasType::Filled { typ, .. },
            ..
        } => {
            encoding_variables(typ, external, out);
        }
        Type::Alias {
            target: AliasType::Open(body),
            arguments,
            ..
        } => {
            let mut names = BTreeMap::new();
            encoding_variables(body, external, &mut names);
            for (name, kind) in names {
                if let Some(argument) = arguments.iter().find(|a| a.name == name) {
                    operation_variables(argument.typ, kind, external, out);
                } else {
                    insert(out, name, kind);
                }
            }
        }
        _ => variables(typ, DemandKind::Deep, out),
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
                    self.pattern(p, id, solved);
                    bind(p, None, &mut env);
                }
            }
            Def::TypedDef { args, .. } => {
                for p in *args {
                    bind(p.pattern, None, &mut env);
                    self.pattern(p.pattern, id, solved);
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
                .any(|b| b.name == reference.name && b.lowering.requires_layout())
        {
            let kind = if reference.name == "castToData" {
                // The checked Big value already is Data. Its nominal layout
                // cannot change this identity operation or its binder shape.
                DemandKind::Native
            } else if matches!(
                reference.name,
                "dataListHead" | "dataListCons" | "dataPairFirst" | "dataPairSecond" | "enumerate"
            ) {
                DemandKind::Encoding
            } else {
                DemandKind::Deep
            };
            if let Some(instance) = solved.instances.get(&node) {
                for (index, t) in instance.type_args.iter().enumerate() {
                    let inspected = match reference.name {
                        "dataPairFirst" | "enumerate" => index == 0,
                        "dataPairSecond" => index == 1,
                        _ => true,
                    };
                    if inspected {
                        operation_variables(
                            t,
                            kind,
                            &self.external,
                            self.demands.entry(owner).or_default(),
                        );
                    }
                }
            }
            if kind != DemandKind::Encoding
                && let Some(t) = solved.exprs.get(&node)
            {
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
    fn pattern(
        &mut self,
        pattern: &'a Located<Pattern<'a>>,
        owner: NodeId,
        solved: &SolvedTypes<'a>,
    ) {
        let mut pending = vec![pattern];
        while let Some(pattern) = pending.pop() {
            if (matches!(
                &pattern.value,
                Pattern::Pair { .. } | Pattern::DataList { .. } | Pattern::DataTuple { .. }
            ) || matches!(&pattern.value, Pattern::Constructor(c) if c.union.data_layout.is_some()))
                && let Some(typ) = solved.patterns.get(&NodeId::pattern(pattern))
            {
                variables(
                    typ,
                    DemandKind::Deep,
                    self.demands.entry(owner).or_default(),
                );
            }
            match &pattern.value {
                Pattern::Pair { first, second } => pending.extend([*first, *second]),
                Pattern::DataList { elements, tail } => {
                    pending.extend_from_slice(elements);
                    pending.extend(tail.iter().copied());
                }
                Pattern::DataTuple {
                    first,
                    second,
                    rest,
                }
                | Pattern::Tuple {
                    first,
                    second,
                    rest,
                } => {
                    pending.extend([*first, *second]);
                    pending.extend_from_slice(rest);
                }
                Pattern::List(elements) => pending.extend_from_slice(elements),
                Pattern::Cons { head, tail } => pending.extend([*head, *tail]),
                Pattern::Alias { pattern, .. } => pending.push(pattern),
                Pattern::Constructor(c) => {
                    pending.extend(c.arguments.iter().map(|arg| arg.pattern))
                }
                _ => {}
            }
        }
    }
    fn data_runtime(&self, typ: &Located<Type<'a>>) -> bool {
        match &typ.value {
            Type::DeclaredHole(_) => false,
            Type::Named { reference, .. } => {
                self.external.contains(reference)
                    || (reference.home == primitives::builtin_home()
                        && matches!(reference.name, "data_pair" | "data_list" | "data_tuple"))
            }
            Type::Alias { target, .. } => match target {
                AliasType::Open(body) | AliasType::Filled { body, .. } => self.data_runtime(body),
            },
            _ => false,
        }
    }
    fn encoding(&mut self, typ: &'a Located<Type<'a>>, owner: NodeId) {
        encoding_variables(typ, &self.external, self.demands.entry(owner).or_default());
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
            Expr::Convert { kind, value, .. } => {
                let kind = solved.conversions.get(&node).unwrap_or(kind);
                let target = if *kind == ConversionKind::ToData {
                    NodeId::expr(value)
                } else {
                    node
                };
                if !matches!(
                    kind,
                    ConversionKind::Identity
                        | ConversionKind::ViewData
                        | ConversionKind::FromDataBytesView
                ) && let Some(typ) = solved.exprs.get(&target)
                {
                    if *kind == ConversionKind::ToData {
                        self.encoding(typ, owner);
                    } else {
                        variables(
                            typ,
                            DemandKind::Deep,
                            self.demands.entry(owner).or_default(),
                        );
                    }
                }
                self.expr(value, owner, env, solved);
            }
            Expr::Equal { left, right, .. } => {
                for operand in [*left, *right] {
                    if let Some(typ) = solved.exprs.get(&NodeId::expr(operand)) {
                        self.encoding(typ, owner);
                    }
                    self.expr(operand, owner, env, solved);
                }
            }
            Expr::Format { value } => {
                if let Some(typ) = solved.exprs.get(&NodeId::expr(value))
                    && !matches!(
                        &typ.value,
                        Type::Named { reference, .. }
                            if reference.home == primitives::builtin_home()
                                && reference.name == "string"
                    )
                {
                    self.encoding(typ, owner);
                }
                self.expr(value, owner, env, solved);
            }
            Expr::TupleIndex { tuple, .. } => {
                for target in [NodeId::expr(tuple), node] {
                    if let Some(typ) = solved.exprs.get(&target) {
                        variables(
                            typ,
                            DemandKind::Native,
                            self.demands.entry(owner).or_default(),
                        );
                    }
                }
                self.expr(tuple, owner, env, solved);
            }
            Expr::Pair { first, second } => {
                for field in [first, second] {
                    if let Some(typ) = solved.exprs.get(&NodeId::expr(field)) {
                        self.encoding(typ, owner);
                    }
                }
                self.expr(first, owner, env, solved);
                self.expr(second, owner, env, solved);
            }
            Expr::DataList { elements, tail } => {
                if let Some(typ) = solved.exprs.get(&node) {
                    self.encoding(typ, owner);
                }
                for element in *elements {
                    if let Some(typ) = solved.exprs.get(&NodeId::expr(element)) {
                        self.encoding(typ, owner);
                    }
                    self.expr(element, owner, env, solved);
                }
                if let Some(tail) = tail {
                    self.expr(tail, owner, env, solved);
                }
            }
            Expr::VarConstructor { reference, .. } => {
                if self.external.contains(&QualifiedName {
                    home: reference.home,
                    name: reference.union,
                }) && let Some(typ) = solved.exprs.get(&node)
                {
                    let mut typ = *typ;
                    loop {
                        match &typ.value {
                            Type::Lambda { from, to } => {
                                self.encoding(from, owner);
                                typ = to;
                            }
                            Type::Function { arguments, result } => {
                                for argument in *arguments {
                                    self.encoding(argument, owner);
                                }
                                typ = result;
                            }
                            _ => break,
                        }
                    }
                }
            }
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
                let (index, method) = match expr.value {
                    Expr::Str(_) => (1, "fromString"),
                    Expr::Bytes(_) => (2, "fromBytes"),
                    _ => (0, "fromInt"),
                };
                self.method(self.literal_traits[index], method, node, owner, solved);
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
            Expr::Assert(e)
            | Expr::Comptime(e)
            | Expr::Callable { value: e, .. }
            | Expr::ModuleConstantCheck { value: e }
            | Expr::TypeScope { value: e }
            | Expr::RunnableCheck { function: e, .. } => self.expr(e, owner, env, solved),
            Expr::Fail(e) | Expr::Todo(e) => {
                if let Some(e) = e {
                    self.expr(e, owner, env, solved);
                }
            }
            Expr::Trace { message, body } => {
                self.expr(message, owner, env, solved);
                self.expr(body, owner, env, solved);
            }
            Expr::TraceLabel {
                label,
                arguments,
                body,
                ..
            } => {
                self.expr(label, owner, env, solved);
                for argument in *arguments {
                    self.expr(argument, owner, env, solved);
                }
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
            Expr::Lambda { parameters, body } | Expr::Function { parameters, body } => {
                let mut env = env.clone();
                for p in *parameters {
                    self.pattern(p, owner, solved);
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
            Expr::SurfaceCall {
                function,
                arguments,
                ..
            } => {
                self.expr(function, owner, env, solved);
                for argument in *arguments {
                    self.expr(argument.value, owner, env, solved);
                }
            }
            Expr::Pipe {
                input,
                function,
                arguments,
                ..
            } => {
                self.expr(input, owner, env, solved);
                self.expr(function, owner, env, solved);
                if let Some(arguments) = arguments {
                    for argument in *arguments {
                        self.expr(argument.value, owner, env, solved);
                    }
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
            Expr::LetValue {
                pattern,
                value,
                body,
                uses,
                ..
            } => {
                if *uses != 0 {
                    self.pattern(pattern, owner, solved);
                    self.expr(value, owner, env, solved);
                    let mut env = env.clone();
                    bind(pattern, None, &mut env);
                    self.expr(body, owner, &env, solved);
                } else {
                    self.expr(body, owner, env, solved);
                }
            }
            Expr::LetDestruct {
                pattern,
                value,
                body,
            } => {
                let id = NodeId::pattern(pattern);
                self.owner(id, Some(owner), solved);
                self.pattern(pattern, id, solved);
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
                    self.pattern(b.pattern, owner, solved);
                    let mut env = env.clone();
                    bind(b.pattern, None, &mut env);
                    self.expr(b.body, owner, &env, solved);
                }
            }
            Expr::Match {
                value,
                pattern,
                body,
                fallback,
                ..
            } => {
                if let Some(kind) = solved.conversions.get(&node) {
                    if *kind == ConversionKind::ToData {
                        if let Some(typ) = solved.exprs.get(&NodeId::expr(value)) {
                            self.encoding(typ, owner);
                        }
                    } else if !matches!(
                        kind,
                        ConversionKind::Identity
                            | ConversionKind::ViewData
                            | ConversionKind::FromDataBytesView
                    ) && let Some(typ) = solved.patterns.get(&NodeId::pattern(pattern))
                    {
                        variables(
                            typ,
                            DemandKind::Deep,
                            self.demands.entry(owner).or_default(),
                        );
                    }
                }
                self.pattern(pattern, owner, solved);
                self.expr(value, owner, env, solved);
                self.expr(fallback, owner, env, solved);
                let mut env = env.clone();
                bind(pattern, None, &mut env);
                self.expr(body, owner, &env, solved);
            }
            Expr::FieldOrModule { module, .. }
                if !solved
                    .field_selections
                    .get(&node)
                    .copied()
                    .expect("solved field/module selection") =>
            {
                self.expr(
                    module.expect("selected module candidate"),
                    owner,
                    env,
                    solved,
                );
            }
            Expr::Access { record, .. } | Expr::FieldOrModule { record, .. } => {
                if let Some(typ) = solved.exprs.get(&NodeId::expr(record))
                    && self.data_runtime(typ)
                {
                    variables(
                        typ,
                        DemandKind::Deep,
                        self.demands.entry(owner).or_default(),
                    );
                }
                self.expr(record, owner, env, solved);
            }
            Expr::Update { base, fields, .. } | Expr::RecordUpdate { base, fields, .. } => {
                if let Some(typ) = solved.exprs.get(&node)
                    && self.data_runtime(typ)
                {
                    variables(
                        typ,
                        DemandKind::Deep,
                        self.demands.entry(owner).or_default(),
                    );
                }
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
            Expr::DataTuple {
                first,
                second,
                rest,
            }
            | Expr::Tuple {
                first,
                second,
                rest,
            } => {
                if matches!(&expr.value, Expr::DataTuple { .. })
                    && let Some(typ) = solved.exprs.get(&node)
                {
                    variables(
                        typ,
                        DemandKind::Deep,
                        self.demands.entry(owner).or_default(),
                    );
                }
                self.expr(first, owner, env, solved);
                self.expr(second, owner, env, solved);
                for e in *rest {
                    self.expr(e, owner, env, solved);
                }
            }
            Expr::Accessor(_) => {
                if let Some(typ) = solved.exprs.get(&node)
                    && let Type::Lambda { from, .. } = &typ.value
                    && self.data_runtime(from)
                {
                    variables(
                        typ,
                        DemandKind::Deep,
                        self.demands.entry(owner).or_default(),
                    );
                }
            }
            Expr::Unit | Expr::Constant(_) => {}
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
        assert!(
            analyze(&[(&m, &s, &Default::default())])[&NodeId::def(definition(d).0)].is_empty()
        );
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
        let demands = analyze(&[(&m, &s, &Default::default())]);
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
        let demands = analyze(&[(&m, &s, &Default::default())]);
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
        assert!(
            analyze(&[(&m, &s, &Default::default())])[&NodeId::def(definition(d).0)].is_empty()
        );
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
        assert!(
            analyze(&[(&m, &s, &Default::default())])[&NodeId::def(definition(d).0)].is_empty()
        );
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
            analyze(&[(&m, &s, &Default::default())])[&NodeId::def(definition(d).0)],
            BTreeMap::from([("a", DemandKind::Native)])
        );
    }
}
