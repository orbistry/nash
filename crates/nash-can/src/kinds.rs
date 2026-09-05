//! Kind inference: Haskell98-style, over type declaration SCCs.
//! See docs/kinds.md.

use bumpalo::Bump;
use nash_ast::{BaseKind, Kind, KindScheme, KindSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KindVar(u32);

/// Inference-time kind. `Var` is a union-find index.
#[derive(Clone, Copy, Debug)]
pub enum K<'a> {
    Base(BaseKind),
    Var(KindVar),
    Arrow(&'a K<'a>, &'a K<'a>),
}

#[derive(Clone, Copy, Debug)]
enum Node<'a> {
    Unbound(KindSet),
    Bound(&'a K<'a>),
    Link(KindVar),
}

#[derive(Debug)]
pub enum Mismatch<'a> {
    /// Neither side is a variable and the shapes differ, or a bound excludes the binding.
    Shapes {
        expected: &'a K<'a>,
        actual: &'a K<'a>,
    },
    Infinite(KindVar),
}

pub struct Infer<'a> {
    bump: &'a Bump,
    nodes: Vec<Node<'a>>,
}

impl<'a> Infer<'a> {
    pub fn new(bump: &'a Bump) -> Self {
        Infer {
            bump,
            nodes: Vec::new(),
        }
    }

    pub fn fresh(&mut self, bound: KindSet) -> KindVar {
        self.nodes.push(Node::Unbound(bound));
        KindVar(
            (self.nodes.len() - 1)
                .try_into()
                .expect("kind variable count exceeds u32"),
        )
    }

    pub fn fresh_k(&mut self, bound: KindSet) -> &'a K<'a> {
        let var = self.fresh(bound);
        self.bump.alloc(K::Var(var))
    }

    fn find(&mut self, var: KindVar) -> KindVar {
        match self.nodes[var.0 as usize] {
            Node::Link(next) => {
                let root = self.find(next);
                self.nodes[var.0 as usize] = Node::Link(root);
                root
            }
            _ => var,
        }
    }

    /// Resolve one level: a bound variable becomes its binding.
    fn head(&mut self, kind: &'a K<'a>) -> &'a K<'a> {
        match kind {
            K::Var(var) => {
                let root = self.find(*var);
                match self.nodes[root.0 as usize] {
                    Node::Bound(bound) => self.head(bound),
                    _ => self.bump.alloc(K::Var(root)),
                }
            }
            _ => kind,
        }
    }

    pub fn unify(&mut self, expected: &'a K<'a>, actual: &'a K<'a>) -> Result<(), Mismatch<'a>> {
        let expected = self.head(expected);
        let actual = self.head(actual);
        match (expected, actual) {
            (K::Var(a), K::Var(b)) if a == b => Ok(()),
            (K::Var(a), K::Var(b)) => {
                let (Node::Unbound(sa), Node::Unbound(sb)) =
                    (self.nodes[a.0 as usize], self.nodes[b.0 as usize])
                else {
                    unreachable!("head returns unbound roots")
                };
                let joined = sa.intersect(sb);
                if joined.is_empty() {
                    return Err(Mismatch::Shapes { expected, actual });
                }
                self.nodes[a.0 as usize] = Node::Link(*b);
                self.nodes[b.0 as usize] = Node::Unbound(joined);
                Ok(())
            }
            (K::Var(var), other) | (other, K::Var(var)) => self.bind(*var, other, expected, actual),
            (K::Base(a), K::Base(b)) if a == b => Ok(()),
            (K::Arrow(a1, r1), K::Arrow(a2, r2)) => {
                self.unify(a1, a2)?;
                self.unify(r1, r2)
            }
            _ => Err(Mismatch::Shapes { expected, actual }),
        }
    }

    fn bind(
        &mut self,
        var: KindVar,
        kind: &'a K<'a>,
        expected: &'a K<'a>,
        actual: &'a K<'a>,
    ) -> Result<(), Mismatch<'a>> {
        let Node::Unbound(bound) = self.nodes[var.0 as usize] else {
            unreachable!("head returns unbound roots")
        };
        let allowed = match kind {
            K::Base(base) => bound.contains(KindSet::of(*base)),
            K::Arrow(..) => bound.contains(KindSet::ARROW),
            K::Var(_) => unreachable!("var/var handled by unify"),
        };
        if !allowed {
            return Err(Mismatch::Shapes { expected, actual });
        }
        if self.occurs(var, kind) {
            return Err(Mismatch::Infinite(var));
        }
        self.nodes[var.0 as usize] = Node::Bound(kind);
        Ok(())
    }

    fn occurs(&mut self, var: KindVar, kind: &'a K<'a>) -> bool {
        match self.head(kind) {
            K::Var(other) => *other == var,
            K::Base(_) => false,
            K::Arrow(from, to) => self.occurs(var, from) || self.occurs(var, to),
        }
    }

    /// Apply `kind` to one argument: returns the parameter and result kinds.
    /// A variable head becomes a fresh arrow (its bound must allow arrows).
    pub fn apply(&mut self, kind: &'a K<'a>) -> Result<(&'a K<'a>, &'a K<'a>), Mismatch<'a>> {
        match self.head(kind) {
            K::Arrow(param, result) => Ok((param, result)),
            head => {
                let param = self.fresh_k(KindSet::ALL);
                let result = self.fresh_k(KindSet::ALL);
                let arrow = self.bump.alloc(K::Arrow(param, result));
                self.unify(arrow, head)?;
                Ok((param, result))
            }
        }
    }

    /// Instantiate a scheme with fresh variables carrying the scheme's bounds.
    pub fn instantiate(&mut self, scheme: &KindScheme<'_>) -> &'a K<'a> {
        let vars: Vec<KindVar> = scheme.bounds.iter().map(|b| self.fresh(*b)).collect();
        self.instantiate_help(scheme.kind, &vars)
    }

    fn instantiate_help(&mut self, kind: &Kind<'_>, vars: &[KindVar]) -> &'a K<'a> {
        match kind {
            Kind::Base(base) => self.bump.alloc(K::Base(*base)),
            Kind::Var(index) => self.bump.alloc(K::Var(vars[*index as usize])),
            Kind::Arrow(from, to) => {
                let from = self.instantiate_help(from, vars);
                let to = self.instantiate_help(to, vars);
                self.bump.alloc(K::Arrow(from, to))
            }
        }
    }

    /// Zonk and generalize: every unbound variable becomes a scheme variable.
    pub fn generalize(&mut self, kind: &'a K<'a>) -> KindScheme<'a> {
        let mut vars: Vec<(KindVar, KindSet)> = Vec::new();
        let kind = self.generalize_help(kind, &mut vars);
        KindScheme {
            bounds: self
                .bump
                .alloc_slice_fill_iter(vars.iter().map(|(_, b)| *b)),
            kind,
        }
    }

    fn generalize_help(
        &mut self,
        kind: &'a K<'a>,
        vars: &mut Vec<(KindVar, KindSet)>,
    ) -> &'a Kind<'a> {
        match self.head(kind) {
            K::Base(base) => self.bump.alloc(Kind::Base(*base)),
            K::Var(var) => {
                let index = match vars.iter().position(|(v, _)| v == var) {
                    Some(i) => i,
                    None => {
                        let Node::Unbound(bound) = self.nodes[var.0 as usize] else {
                            unreachable!("head returns unbound roots")
                        };
                        vars.push((*var, bound));
                        vars.len() - 1
                    }
                };
                self.bump.alloc(Kind::Var(
                    index
                        .try_into()
                        .expect("kind scheme variable count exceeds u16"),
                ))
            }
            K::Arrow(from, to) => {
                let from = self.generalize_help(from, vars);
                let to = self.generalize_help(to, vars);
                self.bump.alloc(Kind::Arrow(from, to))
            }
        }
    }

    /// Zonk without generalizing, for error reporting. Unbound vars keep their id.
    pub fn zonk(&mut self, kind: &'a K<'a>) -> &'a K<'a> {
        match self.head(kind) {
            K::Arrow(from, to) => {
                let from = self.zonk(from);
                let to = self.zonk(to);
                self.bump.alloc(K::Arrow(from, to))
            }
            head => head,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_unification() {
        let bump = Bump::new();
        let mut infer = Infer::new(&bump);
        let big = bump.alloc(K::Base(BaseKind::Big));
        let term = bump.alloc(K::Base(BaseKind::Term));
        assert!(infer.unify(big, big).is_ok());
        assert!(matches!(
            infer.unify(big, term),
            Err(Mismatch::Shapes { .. })
        ));
    }

    #[test]
    fn storable_accepts_big_and_const_but_rejects_term() {
        let bump = Bump::new();
        let mut infer = Infer::new(&bump);
        for (base, allowed) in [
            (BaseKind::Big, true),
            (BaseKind::Const, true),
            (BaseKind::Term, false),
        ] {
            let var = infer.fresh_k(KindSet::STORABLE);
            assert_eq!(infer.unify(var, bump.alloc(K::Base(base))).is_ok(), allowed);
        }
    }

    #[test]
    fn linked_variables_share_intersected_bounds() {
        let bump = Bump::new();
        let mut infer = Infer::new(&bump);
        let a = infer.fresh_k(KindSet::STORABLE);
        let b = infer.fresh_k(KindSet::LITTLE);
        infer.unify(a, b).unwrap();
        assert_eq!(infer.generalize(a).bounds, &[KindSet::CONST]);
        assert!(infer.unify(a, bump.alloc(K::Base(BaseKind::Big))).is_err());
        infer
            .unify(b, bump.alloc(K::Base(BaseKind::Const)))
            .unwrap();
        assert!(matches!(
            infer.generalize(a).kind,
            Kind::Base(BaseKind::Const)
        ));
    }

    #[test]
    fn incompatible_bounds_do_not_merge() {
        let bump = Bump::new();
        let mut infer = Infer::new(&bump);
        let a = infer.fresh_k(KindSet::BIG);
        let b = infer.fresh_k(KindSet::LITTLE);
        assert!(infer.unify(a, b).is_err());
        assert_eq!(infer.generalize(a).bounds, &[KindSet::BIG]);
        assert_eq!(infer.generalize(b).bounds, &[KindSet::LITTLE]);
    }

    #[test]
    fn arrow_requires_an_arrow_capable_bound() {
        let bump = Bump::new();
        let mut infer = Infer::new(&bump);
        let any = infer.fresh_k(KindSet::ANY);
        let all = infer.fresh_k(KindSet::ALL);
        assert!(infer.apply(any).is_err());
        let (param, result) = infer.apply(all).unwrap();
        infer
            .unify(param, bump.alloc(K::Base(BaseKind::Big)))
            .unwrap();
        infer
            .unify(result, bump.alloc(K::Base(BaseKind::Term)))
            .unwrap();
        assert!(matches!(
            infer.generalize(all).kind,
            Kind::Arrow(Kind::Base(BaseKind::Big), Kind::Base(BaseKind::Term))
        ));
    }

    #[test]
    fn occurs_check_follows_links_and_arrow_children() {
        let bump = Bump::new();
        let mut infer = Infer::new(&bump);
        let a = infer.fresh_k(KindSet::ALL);
        let b = infer.fresh_k(KindSet::ALL);
        infer.unify(a, b).unwrap();
        let arrow = bump.alloc(K::Arrow(bump.alloc(K::Base(BaseKind::Big)), a));
        assert!(matches!(infer.unify(b, arrow), Err(Mismatch::Infinite(_))));
        assert_eq!(infer.generalize(a).bounds, &[KindSet::ALL]);
    }

    #[test]
    fn generalization_preserves_sharing_and_instantiation_is_fresh() {
        let bump = Bump::new();
        let mut infer = Infer::new(&bump);
        let a = infer.fresh_k(KindSet::STORABLE);
        let b = infer.fresh_k(KindSet::ANY);
        let kind = bump.alloc(K::Arrow(a, bump.alloc(K::Arrow(b, a))));
        let scheme = infer.generalize(kind);
        assert_eq!(scheme.bounds, &[KindSet::STORABLE, KindSet::ANY]);
        assert!(matches!(
            scheme.kind,
            Kind::Arrow(Kind::Var(0), Kind::Arrow(Kind::Var(1), Kind::Var(0)))
        ));
        let first = infer.instantiate(&scheme);
        let second = infer.instantiate(&scheme);
        let (param, rest) = infer.apply(first).unwrap();
        let (_, result) = infer.apply(rest).unwrap();
        infer
            .unify(param, bump.alloc(K::Base(BaseKind::Const)))
            .unwrap();
        assert!(matches!(
            infer.generalize(result).kind,
            Kind::Base(BaseKind::Const)
        ));
        assert_eq!(infer.generalize(second).bounds, scheme.bounds);
    }
}

#[cfg(test)]
mod environment_tests {
    use super::*;

    #[test]
    fn builtin_list_has_storable_elements() {
        let env = KindEnv::from_interfaces(None);
        let scheme = env.scheme(nash_ast::QualifiedName {
            home: nash_ast::primitives::builtin_home(),
            name: "list",
        });
        assert_eq!(scheme.bounds, &[KindSet::STORABLE]);
        assert!(matches!(
            scheme.kind,
            Kind::Arrow(Kind::Var(0), Kind::Base(BaseKind::Const))
        ));
    }
}

use crate::error::KindContext;
use crate::module::{PreAlias, PreUnion};
use crate::{Error, scc};
use nash_ast::{FieldType, ModuleName, Type as CanType};
use nash_ast::{QualifiedName, primitives};
use nash_region::{Located, Region};
use std::collections::{BTreeMap, BTreeSet};

/// Kind schemes of every type constructor visible to the module.
pub struct KindEnv<'a> {
    schemes: BTreeMap<QualifiedName<'a>, KindScheme<'a>>,
}

impl<'a> KindEnv<'a> {
    pub fn from_interfaces(interfaces: Option<&BTreeMap<&'a str, crate::Interface<'a>>>) -> Self {
        let mut schemes = BTreeMap::new();
        for p in primitives::PRIMITIVES {
            schemes.insert(
                QualifiedName {
                    home: primitives::builtin_home(),
                    name: p.name,
                },
                p.kind,
            );
        }
        // Match the legacy List entry still seeded by canonicalization.
        schemes.insert(
            QualifiedName {
                home: ModuleName {
                    package: None,
                    name: "List",
                },
                name: "List",
            },
            primitives::PRIMITIVES
                .iter()
                .find(|p| p.name == "List")
                .unwrap()
                .kind,
        );
        for interface in interfaces.into_iter().flat_map(|m| m.values()) {
            for union in interface.unions {
                schemes.insert(
                    QualifiedName {
                        home: interface.home,
                        name: union.name,
                    },
                    union.kind,
                );
            }
            for alias in interface.aliases {
                schemes.insert(
                    QualifiedName {
                        home: interface.home,
                        name: alias.name,
                    },
                    alias.kind,
                );
            }
        }
        KindEnv { schemes }
    }

    pub fn insert(&mut self, name: QualifiedName<'a>, scheme: KindScheme<'a>) {
        self.schemes.insert(name, scheme);
    }

    /// Every `Type::Named` reference was resolved by `types.rs`, so absence is a bug.
    pub fn scheme(&self, name: QualifiedName<'a>) -> KindScheme<'a> {
        *self
            .schemes
            .get(&name)
            .expect("kind env covers every resolved type")
    }
}

pub struct Schemes<'a> {
    unions: BTreeMap<&'a str, KindScheme<'a>>,
    aliases: BTreeMap<&'a str, KindScheme<'a>>,
}

impl<'a> Schemes<'a> {
    pub fn union(&self, name: &str) -> KindScheme<'a> {
        self.unions[name]
    }
    pub fn alias(&self, name: &str) -> KindScheme<'a> {
        self.aliases[name]
    }
}

enum Decl<'p, 'a> {
    Union(&'p PreUnion<'a>),
    Alias(&'p PreAlias<'a>),
}

impl<'p, 'a> Decl<'p, 'a> {
    fn name(&self) -> &'a Located<&'a str> {
        match self {
            Self::Union(u) => u.name,
            Self::Alias(a) => a.name,
        }
    }
    fn parameters(&self) -> &'a [&'a str] {
        match self {
            Self::Union(u) => u.parameters,
            Self::Alias(a) => a.parameters,
        }
    }
}

fn is_big_name(name: &str) -> bool {
    name.chars().next().is_some_and(char::is_uppercase)
}

/// Infer one module's type declarations, SCC by SCC, and record every
/// scheme in `env` under `home`.
pub(crate) fn infer_declarations<'a>(
    bump: &'a Bump,
    env: &mut KindEnv<'a>,
    home: ModuleName<'a>,
    unions: &[PreUnion<'a>],
    aliases: &[PreAlias<'a>],
) -> Result<Schemes<'a>, Vec<Error<'a>>> {
    let decls: Vec<Decl<'_, 'a>> = unions
        .iter()
        .map(Decl::Union)
        .chain(aliases.iter().map(Decl::Alias))
        .collect();
    let local: BTreeSet<&str> = decls.iter().map(|d| d.name().value).collect();

    let nodes = decls
        .iter()
        .map(|decl| {
            let mut deps = Vec::new();
            match decl {
                Decl::Union(u) => {
                    for ctor in u.ctors {
                        for arg in ctor.arguments {
                            local_type_edges(&arg.value, home, &local, &mut deps);
                        }
                    }
                }
                Decl::Alias(a) => local_type_edges(&a.typ.value, home, &local, &mut deps),
            }
            scc::Node {
                key: decl.name().value,
                value: decl,
                deps,
            }
        })
        .collect();

    let mut schemes = Schemes {
        unions: BTreeMap::new(),
        aliases: BTreeMap::new(),
    };
    let mut errors = Vec::new();
    let mut failed = BTreeSet::new();
    for component in scc::strongly_connected_components(nodes) {
        let group: Vec<&Decl<'_, 'a>> = match &component {
            scc::Scc::Acyclic(d) => vec![*d],
            scc::Scc::Cyclic(ds) => ds.to_vec(),
        };
        let mut dependencies = Vec::new();
        for decl in &group {
            match decl {
                Decl::Union(u) => {
                    for ctor in u.ctors {
                        for arg in ctor.arguments {
                            local_type_edges(&arg.value, home, &local, &mut dependencies);
                        }
                    }
                }
                Decl::Alias(a) => local_type_edges(&a.typ.value, home, &local, &mut dependencies),
            }
        }
        if dependencies.iter().any(|name| failed.contains(name)) {
            failed.extend(group.iter().map(|d| d.name().value));
            continue;
        }
        match infer_group(bump, env, home, &group) {
            Ok(results) => {
                for (decl, scheme) in group.iter().zip(results) {
                    let name = decl.name().value;
                    env.insert(QualifiedName { home, name }, scheme);
                    match decl {
                        Decl::Union(_) => schemes.unions.insert(name, scheme),
                        Decl::Alias(_) => schemes.aliases.insert(name, scheme),
                    };
                }
            }
            Err(errs) => {
                failed.extend(group.iter().map(|d| d.name().value));
                errors.extend(errs);
            }
        }
    }
    if errors.is_empty() {
        Ok(schemes)
    } else {
        Err(errors)
    }
}

/// Edges to local declarations, both `Named` (unions) and `Alias` references.
fn local_type_edges<'a>(
    typ: &CanType<'a>,
    home: ModuleName<'a>,
    local: &BTreeSet<&str>,
    edges: &mut Vec<&'a str>,
) {
    match typ {
        CanType::Named { reference, args } => {
            if reference.home == home && local.contains(reference.name) {
                edges.push(reference.name);
            }
            for arg in *args {
                local_type_edges(&arg.value, home, local, edges);
            }
        }
        CanType::Alias {
            reference,
            arguments,
            ..
        } => {
            if reference.home == home && local.contains(reference.name) {
                edges.push(reference.name);
            }
            for arg in *arguments {
                local_type_edges(&arg.typ.value, home, local, edges);
            }
        }
        CanType::Lambda { from, to } => {
            local_type_edges(&from.value, home, local, edges);
            local_type_edges(&to.value, home, local, edges);
        }
        CanType::Record { fields, .. } => {
            for field in *fields {
                local_type_edges(&field.typ.value, home, local, edges);
            }
        }
        CanType::Tuple {
            first,
            second,
            rest,
        } => {
            local_type_edges(&first.value, home, local, edges);
            local_type_edges(&second.value, home, local, edges);
            for r in *rest {
                local_type_edges(&r.value, home, local, edges);
            }
        }
        CanType::App { head, args } => {
            local_type_edges(&head.value, home, local, edges);
            for arg in *args {
                local_type_edges(&arg.value, home, local, edges);
            }
        }
        CanType::Var(_) | CanType::Unit => {}
    }
}

struct Scope<'a> {
    /// Type parameter name -> its kind, for the declaration being walked.
    params: BTreeMap<&'a str, &'a K<'a>>,
}

struct Walker<'e, 'a> {
    bump: &'a Bump,
    infer: Infer<'a>,
    env: &'e KindEnv<'a>,
    /// Monomorphic kinds of the SCC members, by name.
    group: BTreeMap<&'a str, &'a K<'a>>,
    home: ModuleName<'a>,
    errors: Vec<Error<'a>>,
}

fn infer_group<'a>(
    bump: &'a Bump,
    env: &KindEnv<'a>,
    home: ModuleName<'a>,
    group: &[&Decl<'_, 'a>],
) -> Result<Vec<KindScheme<'a>>, Vec<Error<'a>>> {
    let mut w = Walker {
        bump,
        infer: Infer::new(bump),
        env,
        group: BTreeMap::new(),
        home,
        errors: Vec::new(),
    };

    // Monomorphic kinds first, so recursive references resolve.
    let mut scopes = Vec::with_capacity(group.len());
    for decl in group {
        let params: Vec<(&'a str, &'a K<'a>)> = decl
            .parameters()
            .iter()
            .map(|p| (*p, w.infer.fresh_k(KindSet::ALL)))
            .collect();
        let result = match decl {
            Decl::Union(u) if is_big_name(u.name.value) => bump.alloc(K::Base(BaseKind::Big)),
            Decl::Union(_) => bump.alloc(K::Base(BaseKind::Term)),
            Decl::Alias(_) => w.infer.fresh_k(KindSet::ALL),
        };
        let kind = params
            .iter()
            .rev()
            .fold(result, |acc, (_, p)| &*bump.alloc(K::Arrow(p, acc)));
        w.group.insert(decl.name().value, kind);
        scopes.push((
            Scope {
                params: params.into_iter().collect(),
            },
            result,
        ));
    }

    for (decl, (scope, result)) in group.iter().zip(&scopes) {
        match decl {
            Decl::Union(u) => {
                let big = is_big_name(u.name.value);
                for ctor in u.ctors {
                    for (index, arg) in ctor.arguments.iter().enumerate() {
                        let k = w.infer_type(scope, arg);
                        let expected = if big {
                            K::Base(BaseKind::Big)
                        } else {
                            K::Var(w.infer.fresh(KindSet::ANY))
                        };
                        let expected = bump.alloc(expected);
                        let context = if big {
                            KindContext::BigField {
                                union: u.name.value,
                                ctor: ctor.name,
                                index: index as u16,
                            }
                        } else {
                            KindContext::LittleField {
                                union: u.name.value,
                                ctor: ctor.name,
                                index: index as u16,
                            }
                        };
                        w.expect(arg.region, context, expected, k);
                    }
                }
            }
            Decl::Alias(a) => {
                let big = is_big_name(a.name.value);
                let k = match &a.typ.value {
                    CanType::Record { fields, .. } => {
                        w.infer_record_body(scope, a.name.value, big, fields)
                    }
                    _ => w.infer_type(scope, a.typ),
                };
                w.expect(
                    a.typ.region,
                    KindContext::AliasCasing {
                        alias: a.name.value,
                        big,
                    },
                    result,
                    k,
                );
                let expected: &K = if big {
                    bump.alloc(K::Base(BaseKind::Big))
                } else {
                    w.infer.fresh_k(KindSet::LITTLE)
                };
                w.expect(
                    a.typ.region,
                    KindContext::AliasCasing {
                        alias: a.name.value,
                        big,
                    },
                    expected,
                    result,
                );
            }
        }
    }

    if !w.errors.is_empty() {
        return Err(w.errors);
    }
    Ok(group
        .iter()
        .map(|d| w.infer.generalize(w.group[d.name().value]))
        .collect())
}

impl<'e, 'a> Walker<'e, 'a> {
    fn infer_type(&mut self, scope: &Scope<'a>, typ: &'a Located<CanType<'a>>) -> &'a K<'a> {
        match &typ.value {
            CanType::Var(name) => scope.params[name],
            CanType::App { head, args } => {
                let kind = self.infer_type(scope, head);
                let head = match &head.value {
                    CanType::Var(name) => KindHead::Var(name),
                    CanType::Named { reference, .. } | CanType::Alias { reference, .. } => {
                        KindHead::Named(*reference)
                    }
                    _ => KindHead::Application,
                };
                self.apply_args(scope, typ.region, head, kind, args)
            }
            CanType::Named { reference, args } => {
                let head = self.head_kind(*reference);
                self.apply_args(scope, typ.region, KindHead::Named(*reference), head, args)
            }
            CanType::Alias {
                reference,
                arguments,
                ..
            } => {
                let head = self.head_kind(*reference);
                let args: Vec<_> = arguments.iter().map(|a| a.typ).collect();
                self.apply_args(scope, typ.region, KindHead::Named(*reference), head, &args)
            }
            CanType::Lambda { from, to } => {
                self.expect_any(scope, from);
                self.expect_any(scope, to);
                self.bump.alloc(K::Base(BaseKind::Term))
            }
            CanType::Tuple {
                first,
                second,
                rest,
            } => {
                self.expect_any(scope, first);
                self.expect_any(scope, second);
                for r in *rest {
                    self.expect_any(scope, r);
                }
                self.bump.alloc(K::Base(BaseKind::Term))
            }
            CanType::Unit => self.bump.alloc(K::Base(BaseKind::Const)),
            CanType::Record { .. } => {
                // Only legal as an alias body (plans/04 chunk A1); handled by infer_record_body.
                self.errors.push(Error::Unsupported {
                    feature: "anonymous record types outside alias bodies",
                    region: typ.region,
                });
                self.infer.fresh_k(KindSet::ANY)
            }
        }
    }

    fn head_kind(&mut self, reference: QualifiedName<'a>) -> &'a K<'a> {
        if reference.home == self.home
            && let Some(kind) = self.group.get(reference.name)
        {
            return kind;
        }
        let scheme = self.env.scheme(reference);
        self.infer.instantiate(&scheme)
    }

    fn apply_args(
        &mut self,
        scope: &Scope<'a>,
        region: Region,
        head: KindHead<'a>,
        mut kind: &'a K<'a>,
        args: &[&'a Located<CanType<'a>>],
    ) -> &'a K<'a> {
        for (index, arg) in args.iter().enumerate() {
            let (param, result) = match self.infer.apply(kind) {
                Ok(pair) => pair,
                Err(_) => {
                    self.errors.push(Error::KindTooManyArgs {
                        region,
                        head,
                        applied: args.len(),
                        accepted: index,
                    });
                    return self.infer.fresh_k(KindSet::ALL);
                }
            };
            let actual = self.infer_type(scope, arg);
            self.expect(
                arg.region,
                KindContext::TypeArg {
                    head,
                    index: index as u16,
                },
                param,
                actual,
            );
            kind = result;
        }
        kind
    }

    fn expect_any(&mut self, scope: &Scope<'a>, typ: &'a Located<CanType<'a>>) {
        let k = self.infer_type(scope, typ);
        let any = self.infer.fresh_k(KindSet::ANY);
        self.expect(typ.region, KindContext::ValuePosition, any, k);
    }

    fn infer_record_body(
        &mut self,
        scope: &Scope<'a>,
        alias: &'a str,
        big: bool,
        fields: &'a [FieldType<'a>],
    ) -> &'a K<'a> {
        for field in fields {
            let k = self.infer_type(scope, field.typ);
            let expected: &K = if big {
                self.bump.alloc(K::Base(BaseKind::Big))
            } else {
                self.infer.fresh_k(KindSet::ANY)
            };
            self.expect(
                field.typ.region,
                KindContext::RecordField {
                    alias,
                    field: field.field,
                    big,
                },
                expected,
                k,
            );
        }
        self.bump
            .alloc(K::Base(if big { BaseKind::Big } else { BaseKind::Term }))
    }

    fn expect(
        &mut self,
        region: Region,
        context: KindContext<'a>,
        expected: &'a K<'a>,
        actual: &'a K<'a>,
    ) {
        match self.infer.unify(expected, actual) {
            Ok(()) => {}
            Err(Mismatch::Shapes { .. }) => {
                let expected = self.render(expected);
                let actual = self.render(actual);
                self.errors.push(Error::KindMismatch {
                    region,
                    context: self.bump.alloc(context),
                    expected,
                    actual,
                });
            }
            Err(Mismatch::Infinite(_)) => self.errors.push(Error::KindInfinite {
                region,
                context: self.bump.alloc(context),
            }),
        }
    }

    /// Zonk to a `nash_ast::Kind` for error data; unbound vars are numbered in order of appearance.
    fn render(&mut self, kind: &'a K<'a>) -> KindScheme<'a> {
        self.infer.generalize(kind)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum KindHead<'a> {
    Application,
    Named(QualifiedName<'a>),
    Var(&'a str),
}

/// Explicit builtin interface for callers that have not installed the prelude.
pub fn builtin_interface<'a>(bump: &'a Bump) -> crate::Interface<'a> {
    crate::Interface {
        home: primitives::builtin_home(),
        values: &[],
        aliases: &[],
        binops: &[],
        unions: bump.alloc_slice_fill_iter(primitives::PRIMITIVES.iter().map(|p| {
            crate::interface::InterfaceUnion {
                name: p.name,
                parameters: bump.alloc_slice_fill_iter(
                    (0..p.arity).map(|i| &*bump.alloc_str(&format!("p{i}"))),
                ),
                ctors: &[],
                alternatives: 0,
                options: nash_ast::CtorOpts::Enum,
                visibility: crate::interface::UnionVisibility::Closed,
                kind: p.kind,
            }
        })),
    }
}

#[cfg(test)]
pub(crate) fn test_big_kind<'a>(bump: &'a Bump, arity: usize) -> KindScheme<'a> {
    let big: &Kind = bump.alloc(Kind::Base(BaseKind::Big));
    let kind = (0..arity).fold(big, |result, _| &*bump.alloc(Kind::Arrow(big, result)));
    KindScheme::mono(kind)
}
