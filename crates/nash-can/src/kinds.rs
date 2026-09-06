//! Kind inference: Haskell98-style, over type declaration SCCs.
//! See docs/kinds.md.

use bumpalo::Bump;
use nash_ast::{BaseKind, Kind, KindApplication, KindScheme, KindSet, ValueKinds};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KindVar(u32);

/// Inference-time kind. `Var` is a union-find index.
#[derive(Clone, Copy, Debug)]
pub enum K<'a> {
    Base(BaseKind),
    Var(KindVar),
    Arrow(&'a K<'a>, &'a K<'a>),
    Constructor {
        scheme: KindScheme<'a>,
        arguments: &'a [&'a K<'a>],
    },
}

#[derive(Clone, Copy, Debug)]
struct Application<'a> {
    head: &'a K<'a>,
    argument: &'a K<'a>,
    result: &'a K<'a>,
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

#[derive(Clone)]
pub struct Infer<'a> {
    bump: &'a Bump,
    nodes: Vec<Node<'a>>,
    applications: Vec<Application<'a>>,
    settling: bool,
}

impl<'a> Infer<'a> {
    pub fn new(bump: &'a Bump) -> Self {
        Infer {
            bump,
            nodes: Vec::new(),
            applications: Vec::new(),
            settling: false,
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
        if self.settling {
            return self.unify_inner(expected, actual);
        }
        self.settling = true;
        let result = self
            .unify_inner(expected, actual)
            .and_then(|()| self.settle());
        self.settling = false;
        result
    }

    fn unify_inner(&mut self, expected: &'a K<'a>, actual: &'a K<'a>) -> Result<(), Mismatch<'a>> {
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
            (K::Constructor { .. }, _) => {
                let expected = self.open_constructor(expected)?;
                self.unify(expected, actual)
            }
            (_, K::Constructor { .. }) => {
                let actual = self.open_constructor(actual)?;
                self.unify(expected, actual)
            }
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
            K::Arrow(..) | K::Constructor { .. } => bound.contains(KindSet::ARROW),
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
            K::Constructor { arguments, .. } => arguments.iter().any(|kind| self.occurs(var, kind)),
        }
    }

    /// Apply `kind` to one argument: returns the parameter and result kinds.
    /// A variable head becomes a fresh arrow (its bound must allow arrows).
    pub fn apply(&mut self, kind: &'a K<'a>) -> Result<(&'a K<'a>, &'a K<'a>), Mismatch<'a>> {
        match self.head(kind) {
            K::Arrow(param, result) => Ok((param, result)),
            K::Constructor { scheme, arguments } => {
                let opened = self.open_constructor(kind)?;
                let (parameter, result) = self.apply(opened)?;
                let result = if matches!(self.head(result), K::Arrow(..)) {
                    let mut captured = arguments.to_vec();
                    captured.push(parameter);
                    self.bump.alloc(K::Constructor {
                        scheme: *scheme,
                        arguments: self.bump.alloc_slice_fill_iter(captured),
                    })
                } else {
                    result
                };
                Ok((parameter, result))
            }
            head => {
                let param = self.fresh_k(KindSet::ALL);
                let result = self.fresh_k(KindSet::ALL);
                let arrow = self.fresh_k(KindSet::ARROW);
                self.unify(arrow, head)?;
                self.applications.push(Application {
                    head,
                    argument: param,
                    result,
                });
                Ok((param, result))
            }
        }
    }

    fn settle(&mut self) -> Result<(), Mismatch<'a>> {
        loop {
            let pending = std::mem::take(&mut self.applications);
            let mut progress = false;
            let mut retained: Vec<Application<'a>> = Vec::new();
            for application in pending {
                if matches!(self.head(application.head), K::Var(_)) {
                    if let Some(previous) = retained.iter().copied().find(|previous| {
                        self.same_kind(previous.head, application.head)
                            && self.same_kind(previous.argument, application.argument)
                    }) {
                        self.unify(previous.result, application.result)?;
                        progress = true;
                    } else {
                        retained.push(application);
                    }
                    continue;
                }
                progress = true;
                let (parameter, result) = self.apply(application.head)?;
                self.unify(parameter, application.argument)?;
                self.unify(result, application.result)?;
            }
            self.applications.extend(retained);
            if !progress {
                return Ok(());
            }
        }
    }

    fn same_kind(&mut self, left: &'a K<'a>, right: &'a K<'a>) -> bool {
        match (self.head(left), self.head(right)) {
            (K::Base(a), K::Base(b)) => a == b,
            (K::Var(a), K::Var(b)) => a == b,
            (K::Arrow(a, b), K::Arrow(c, d)) => self.same_kind(a, c) && self.same_kind(b, d),
            (
                K::Constructor {
                    scheme: a,
                    arguments: xs,
                },
                K::Constructor {
                    scheme: b,
                    arguments: ys,
                },
            ) => {
                a == b
                    && xs.len() == ys.len()
                    && xs.iter().zip(*ys).all(|(x, y)| self.same_kind(x, y))
            }
            _ => false,
        }
    }

    pub fn constructor(&mut self, scheme: KindScheme<'a>) -> &'a K<'a> {
        if matches!(scheme.kind, Kind::Arrow(..)) {
            self.bump.alloc(K::Constructor {
                scheme,
                arguments: &[],
            })
        } else {
            self.instantiate(&scheme)
        }
    }

    fn open_constructor(&mut self, kind: &'a K<'a>) -> Result<&'a K<'a>, Mismatch<'a>> {
        let K::Constructor { scheme, arguments } = self.head(kind) else {
            return Ok(kind);
        };
        let mut opened = self.instantiate(scheme);
        for argument in *arguments {
            let (parameter, result) = self.apply(opened)?;
            self.unify(parameter, argument)?;
            opened = result;
        }
        Ok(opened)
    }

    /// Instantiate a scheme with fresh variables carrying the scheme's bounds.
    pub fn instantiate(&mut self, scheme: &KindScheme<'_>) -> &'a K<'a> {
        let vars: Vec<KindVar> = scheme.bounds.iter().map(|b| self.fresh(*b)).collect();
        self.instantiate_applications(scheme.applications, &vars);
        self.instantiate_help(scheme.kind, &vars)
    }

    /// Prove a signature without changing the protected binder. Existential
    /// application witnesses may reuse existing facts, but may not narrow them.
    pub fn proves_values(
        &self,
        signature: ValueKinds<'a>,
        actual: &[&'a K<'a>],
        protected: &[&'a K<'a>],
    ) -> bool {
        assert_eq!(signature.kinds.len(), actual.len());
        let mut query = self.clone();
        let expected = query.instantiate_values(&signature);
        for (expected, actual) in expected.into_iter().zip(actual) {
            if query.unify(expected, actual).is_err() {
                return false;
            }
        }
        self.proves_extension(query, protected)
    }

    fn proves_extension(&self, query: Self, protected: &[&'a K<'a>]) -> bool {
        let mut original = self.clone();
        let before = original.generalize_values(protected);
        let facts = original.applications.clone();
        // Include obligations created while opening constructor schemes during
        // unification, as well as those directly retained in the signature.
        let obligations = query.applications.clone();
        let mut pending = vec![(query, 0)];
        let mut remaining = 16_384;
        while let Some((mut query, index)) = pending.pop() {
            if nash_ast::head::step(&mut remaining).is_err() {
                return false;
            }
            if query.generalize_values(protected) == before {
                return true;
            }
            let Some(wanted) = obligations.get(index) else {
                continue;
            };
            if !matches!(query.head(wanted.head), K::Var(_)) {
                pending.push((query, index + 1));
                continue;
            }
            for fact in &facts {
                if nash_ast::head::step(&mut remaining).is_err() {
                    return false;
                }
                let mut candidate = query.clone();
                if candidate.unify(wanted.head, fact.head).is_ok()
                    && candidate.unify(wanted.argument, fact.argument).is_ok()
                    && candidate.unify(wanted.result, fact.result).is_ok()
                {
                    pending.push((candidate, index + 1));
                }
            }
        }
        false
    }

    /// Instantiate all roots with the same fresh kind variables.
    pub fn instantiate_values(&mut self, kinds: &ValueKinds<'_>) -> Vec<&'a K<'a>> {
        let vars: Vec<KindVar> = kinds
            .bounds
            .iter()
            .map(|bound| self.fresh(*bound))
            .collect();
        self.instantiate_applications(kinds.applications, &vars);
        kinds
            .kinds
            .iter()
            .map(|kind| self.instantiate_help(kind, &vars))
            .collect()
    }

    fn instantiate_applications(&mut self, applications: &[KindApplication<'_>], vars: &[KindVar]) {
        for application in applications {
            let head = self.instantiate_help(application.head, vars);
            let argument = self.instantiate_help(application.argument, vars);
            let result = self.instantiate_help(application.result, vars);
            self.applications.push(Application {
                head,
                argument,
                result,
            });
        }
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
            Kind::Constructor { scheme, arguments } => {
                // The nested scheme has its own binder. Reallocate it through
                // an independent inference context rather than substituting
                // its variables with the enclosing value's variables.
                let mut nested = Infer::new(self.bump);
                let root = nested.instantiate(scheme);
                let scheme = nested.generalize(root);
                let arguments: Vec<_> = arguments
                    .iter()
                    .map(|kind| self.instantiate_help(kind, vars))
                    .collect();
                self.bump.alloc(K::Constructor {
                    scheme,
                    arguments: self.bump.alloc_slice_fill_iter(arguments),
                })
            }
        }
    }

    /// Zonk and generalize: every unbound variable becomes a scheme variable.
    pub fn generalize(&mut self, kind: &'a K<'a>) -> KindScheme<'a> {
        let mut vars: Vec<(KindVar, KindSet)> = Vec::new();
        let kind = self.generalize_help(kind, &mut vars);
        let applications = self.generalize_applications(&mut vars);
        KindScheme {
            applications,
            bounds: self
                .bump
                .alloc_slice_fill_iter(vars.iter().map(|(_, b)| *b)),
            kind,
        }
    }

    /// Generalize a value's kind roots together, preserving their correlations.
    pub fn generalize_values(&mut self, kinds: &[&'a K<'a>]) -> ValueKinds<'a> {
        let mut vars = Vec::new();
        let kinds: Vec<_> = kinds
            .iter()
            .map(|kind| self.generalize_help(kind, &mut vars))
            .collect();
        let applications = self.generalize_applications(&mut vars);
        ValueKinds {
            applications,
            bounds: self
                .bump
                .alloc_slice_fill_iter(vars.into_iter().map(|(_, bound)| bound)),
            kinds: self.bump.alloc_slice_fill_iter(kinds),
        }
    }

    fn generalize_applications(
        &mut self,
        vars: &mut Vec<(KindVar, KindSet)>,
    ) -> &'a [KindApplication<'a>] {
        // Retain the connected application graph, including intermediate
        // results of partial applications that are not free type variables.
        let mut pending = self.applications.clone();
        let mut applications = Vec::new();
        loop {
            let before = pending.len();
            let mut remaining = Vec::new();
            for application in pending {
                if self.application_reaches(application, vars) {
                    applications.push(KindApplication {
                        head: self.generalize_help(application.head, vars),
                        argument: self.generalize_help(application.argument, vars),
                        result: self.generalize_help(application.result, vars),
                    });
                } else {
                    remaining.push(application);
                }
            }
            pending = remaining;
            if pending.len() == before {
                break;
            }
        }
        self.bump.alloc_slice_fill_iter(applications)
    }

    fn application_reaches(
        &mut self,
        application: Application<'a>,
        vars: &[(KindVar, KindSet)],
    ) -> bool {
        fn reaches(infer: &mut Infer<'_>, kind: &K<'_>, vars: &[(KindVar, KindSet)]) -> bool {
            match kind {
                K::Base(_) => false,
                K::Var(var) => {
                    let var = infer.find(*var);
                    match infer.nodes[var.0 as usize] {
                        Node::Bound(kind) => reaches(infer, kind, vars),
                        _ => vars.iter().any(|(known, _)| *known == var),
                    }
                }
                K::Arrow(from, to) => reaches(infer, from, vars) || reaches(infer, to, vars),
                K::Constructor { arguments, .. } => {
                    arguments.iter().any(|kind| reaches(infer, kind, vars))
                }
            }
        }
        reaches(self, application.head, vars)
            || reaches(self, application.argument, vars)
            || reaches(self, application.result, vars)
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
            K::Constructor { scheme, arguments } => {
                let arguments: Vec<_> = arguments
                    .iter()
                    .map(|kind| self.generalize_help(kind, vars))
                    .collect();
                self.bump.alloc(Kind::Constructor {
                    scheme: *scheme,
                    arguments: self.bump.alloc_slice_fill_iter(arguments),
                })
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
    fn kind_proofs_reuse_existential_witnesses_without_restricting_them() {
        let bump = Bump::new();
        let signature = ValueKinds {
            bounds: &[KindSet::ARROW, KindSet::ANY],
            kinds: &[&Kind::Var(0)],
            applications: &[KindApplication {
                head: &Kind::Var(0),
                argument: &Kind::Var(1),
                result: &Kind::Base(BaseKind::Big),
            }],
        };
        let mut infer = Infer::new(&bump);
        let roots = infer.instantiate_values(&signature);
        let before = infer.generalize_values(&roots);
        assert!(infer.proves_values(signature, &roots, &roots));
        let specialized = ValueKinds {
            bounds: &[KindSet::ARROW],
            kinds: &[&Kind::Var(0)],
            applications: &[KindApplication {
                head: &Kind::Var(0),
                argument: &Kind::Base(BaseKind::Const),
                result: &Kind::Base(BaseKind::Big),
            }],
        };
        assert!(!infer.proves_values(specialized, &roots, &roots));
        assert_eq!(infer.generalize_values(&roots), before);
    }

    #[test]
    fn constructor_applications_preserve_bounds_and_captured_arguments() {
        let bump = Bump::new();
        for (bound, accepted) in [(KindSet::ANY, true), (KindSet::STORABLE, false)] {
            let mut infer = Infer::new(&bump);
            let head = infer.fresh_k(KindSet::ALL);
            for base in [BaseKind::Const, BaseKind::Term] {
                let (parameter, _) = infer.apply(head).unwrap();
                infer.unify(parameter, bump.alloc(K::Base(base))).unwrap();
            }
            let scheme = KindScheme {
                bounds: bump.alloc_slice_copy(&[bound]),
                kind: &Kind::Arrow(&Kind::Var(0), &Kind::Base(BaseKind::Term)),
                applications: &[],
            };
            let constructor = infer.constructor(scheme);
            assert_eq!(infer.unify(head, constructor).is_ok(), accepted);
        }

        let mut infer = Infer::new(&bump);
        let identity = infer.constructor(KindScheme {
            bounds: &[KindSet::LITTLE],
            kind: &Kind::Arrow(&Kind::Var(0), &Kind::Var(0)),
            applications: &[],
        });
        for base in [BaseKind::Const, BaseKind::Term] {
            let (parameter, result) = infer.apply(identity).unwrap();
            infer.unify(parameter, bump.alloc(K::Base(base))).unwrap();
            assert_eq!(infer.generalize(result).kind, &Kind::Base(base));
        }

        for tied in [false, true] {
            let mut infer = Infer::new(&bump);
            let second = bump.alloc(Kind::Var(if tied { 0 } else { 1 }));
            let scheme = KindScheme {
                bounds: &[KindSet::ANY, KindSet::ANY],
                kind: bump.alloc(Kind::Arrow(
                    &Kind::Var(0),
                    bump.alloc(Kind::Arrow(second, &Kind::Base(BaseKind::Term))),
                )),
                applications: &[],
            };
            let constructor = infer.constructor(scheme);
            let (first, partial) = infer.apply(constructor).unwrap();
            infer
                .unify(first, bump.alloc(K::Base(BaseKind::Const)))
                .unwrap();
            let (second, _) = infer.apply(partial).unwrap();
            assert_eq!(
                infer
                    .unify(second, bump.alloc(K::Base(BaseKind::Term)))
                    .is_ok(),
                !tied
            );
        }
    }

    #[test]
    fn value_kind_applications_retain_the_connected_binder() {
        let bump = Bump::new();
        // The intermediate partial application is not a free type variable.
        let signature = ValueKinds {
            bounds: &[
                KindSet::ARROW,
                KindSet::ANY,
                KindSet::ARROW,
                KindSet::ANY,
                KindSet::ANY,
            ],
            kinds: &[&Kind::Var(0), &Kind::Var(1), &Kind::Var(3), &Kind::Var(4)],
            applications: &[
                KindApplication {
                    head: &Kind::Var(0),
                    argument: &Kind::Var(1),
                    result: &Kind::Var(2),
                },
                KindApplication {
                    head: &Kind::Var(2),
                    argument: &Kind::Var(3),
                    result: &Kind::Var(4),
                },
            ],
        };
        let mut infer = Infer::new(&bump);
        let first = infer.instantiate_values(&signature);
        let second = infer.instantiate_values(&signature);
        infer
            .unify(second[1], bump.alloc(K::Base(BaseKind::Const)))
            .unwrap();
        let retained = infer.generalize_values(&first);
        assert_eq!(retained.applications.len(), 2);
        assert_eq!(retained.bounds.len(), 5);
        assert_eq!(retained.applications[0].head, retained.kinds[0]);
        assert_eq!(retained.applications[0].argument, retained.kinds[1]);
        assert_eq!(
            retained.applications[0].result,
            retained.applications[1].head
        );
        assert_eq!(retained.applications[1].argument, retained.kinds[2]);
        assert_eq!(retained.applications[1].result, retained.kinds[3]);
        let mut imported = Infer::new(&bump);
        let roots = imported.instantiate_values(&retained);
        assert_eq!(imported.generalize_values(&roots), retained);
    }

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
        let scheme = infer.generalize(all);
        assert_eq!(scheme.bounds, &[KindSet::ARROW]);
        assert_eq!(scheme.applications.len(), 1);
        assert_eq!(scheme.applications[0].head, scheme.kind);
        assert_eq!(scheme.applications[0].argument, &Kind::Base(BaseKind::Big));
        assert_eq!(scheme.applications[0].result, &Kind::Base(BaseKind::Term));
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
#[derive(Clone, Debug)]
pub struct KindEnv<'a> {
    trait_schemes: BTreeMap<QualifiedName<'a>, KindScheme<'a>>,
    schemes: BTreeMap<QualifiedName<'a>, KindScheme<'a>>,
}

impl Default for KindEnv<'_> {
    fn default() -> Self {
        Self::from_interfaces(None)
    }
}

impl<'a> KindEnv<'a> {
    pub fn from_interfaces(interfaces: Option<&BTreeMap<&'a str, crate::Interface<'a>>>) -> Self {
        let mut schemes = BTreeMap::new();
        let mut trait_schemes = BTreeMap::new();
        for p in primitives::PRIMITIVES {
            schemes.insert(
                QualifiedName {
                    home: primitives::builtin_home(),
                    name: p.name,
                },
                p.kind,
            );
        }
        for interface in interfaces.into_iter().flat_map(|m| m.values()) {
            for trait_ in interface.traits {
                trait_schemes.insert(
                    QualifiedName {
                        home: interface.home,
                        name: trait_.name,
                    },
                    trait_.kind,
                );
            }
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
        KindEnv {
            schemes,
            trait_schemes,
        }
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
    /// Post-solve queries can see inferred record carriers. Their representation
    /// stays unknown until plan 04; none may acquire a narrower kind here.
    inferred_records: Option<Vec<&'a K<'a>>>,
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
        inferred_records: None,
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

    // Apply annotations after every body in the recursive group has constrained usage.
    for (decl, (scope, _)) in group.iter().zip(&scopes) {
        let parameters = match decl {
            Decl::Union(union) => union.source.value.arguments,
            Decl::Alias(alias) => alias.source.value.arguments,
        };
        for parameter in parameters {
            if let Some(annotation) = parameter.kind {
                let expected = w.annotation_kind(annotation);
                let actual = scope.params[parameter.name.value];
                w.expect(
                    annotation.region,
                    KindContext::ParamAnnotation {
                        type_name: decl.name().value,
                        param: parameter.name.value,
                    },
                    expected,
                    actual,
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
    fn annotation_kind(&mut self, kind: &Located<nash_source::Kind<'_>>) -> &'a K<'a> {
        use nash_source::Kind;
        match &kind.value {
            Kind::Big => self.bump.alloc(K::Base(BaseKind::Big)),
            Kind::Const => self.bump.alloc(K::Base(BaseKind::Const)),
            Kind::Term => self.bump.alloc(K::Base(BaseKind::Term)),
            Kind::Storable => self.infer.fresh_k(KindSet::STORABLE),
            Kind::Arrow { from, to } => {
                let from = self.annotation_kind(from);
                let to = self.annotation_kind(to);
                self.bump.alloc(K::Arrow(from, to))
            }
        }
    }

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
            CanType::Record { fields, .. } if self.inferred_records.is_some() => {
                for field in *fields {
                    self.expect_any(scope, field.typ);
                }
                let kind = self.infer.fresh_k(KindSet::ANY);
                self.inferred_records.as_mut().unwrap().push(kind);
                kind
            }
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
        self.infer.constructor(scheme)
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
            Err(Mismatch::Shapes { expected, actual }) => {
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
    let kinds = KindEnv::default();
    crate::Interface {
        impls: &[],
        traits: &[],
        home: primitives::builtin_home(),
        values: bump.alloc_slice_fill_iter(primitives::BUILTINS.iter().map(|builtin| {
            let annotation = nash_ast::Annotation {
                free_vars: builtin.free_vars,
                context: &[],
                kinds: ValueKinds::unconstrained(bump, builtin.free_vars.len()),
                typ: builtin.typ,
            };
            crate::interface::InterfaceValue {
                name: builtin.name,
                annotation: retain_annotation(
                    bump,
                    &kinds,
                    primitives::builtin_home(),
                    builtin.name,
                    &annotation,
                )
                .expect("builtin value schemes are well-kinded"),
            }
        })),
        aliases: &[],
        binops: &[],
        unions: bump.alloc_slice_fill_iter(primitives::PRIMITIVES.iter().map(|p| {
            crate::interface::InterfaceUnion {
                name: p.name,
                parameters: bump.alloc_slice_fill_iter(
                    (0..p.arity).map(|i| &*bump.alloc_str(&format!("p{i}"))),
                ),
                ctors: p.ctors,
                alternatives: p
                    .ctors
                    .len()
                    .try_into()
                    .expect("primitive constructor count"),
                options: if p.ctors.iter().all(|ctor| ctor.arguments.is_empty()) {
                    nash_ast::CtorOpts::Enum
                } else {
                    nash_ast::CtorOpts::Normal
                },
                visibility: if p.ctors.is_empty() {
                    crate::interface::UnionVisibility::Closed
                } else {
                    crate::interface::UnionVisibility::Open
                },
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

/// Check an annotation and return jointly generalized kinds in its free-variable order.
pub fn check_annotation<'a>(
    bump: &'a Bump,
    env: &KindEnv<'a>,
    home: ModuleName<'a>,
    name: &'a str,
    annotation: &nash_ast::Annotation<'a>,
) -> Result<ValueKinds<'a>, Vec<Error<'a>>> {
    check_annotation_specialization(bump, env, home, name, annotation, None)
}

pub(crate) fn check_annotation_specialization<'a>(
    bump: &'a Bump,
    env: &KindEnv<'a>,
    home: ModuleName<'a>,
    name: &'a str,
    annotation: &nash_ast::Annotation<'a>,
    specialization: Option<(ValueKinds<'a>, &[&'a Located<CanType<'a>>])>,
) -> Result<ValueKinds<'a>, Vec<Error<'a>>> {
    let mut walker = Walker {
        bump,
        infer: Infer::new(bump),
        env,
        group: BTreeMap::new(),
        home,
        errors: Vec::new(),
        inferred_records: None,
    };
    let scope = Scope {
        params: annotation
            .free_vars
            .iter()
            .map(|name| (*name, walker.infer.fresh_k(KindSet::ALL)))
            .collect(),
    };
    if let Some((kinds, types)) = specialization {
        assert_eq!(kinds.kinds.len(), types.len());
        let roots = walker.infer.instantiate_values(&kinds);
        for (expected, typ) in roots.into_iter().zip(types) {
            let actual = walker.infer_type(&scope, typ);
            walker.expect(
                typ.region,
                KindContext::Annotation { name },
                expected,
                actual,
            );
        }
    }
    for predicate in annotation.context {
        walker.infer_predicate(&scope, predicate, &BTreeMap::new());
    }
    let kind = walker.infer_type(&scope, annotation.typ);
    let any = walker.infer.fresh_k(KindSet::ANY);
    walker.expect(
        annotation.typ.region,
        KindContext::Annotation { name },
        any,
        kind,
    );
    if !walker.errors.is_empty() {
        return Err(walker.errors);
    }
    let roots: Vec<_> = annotation
        .free_vars
        .iter()
        .map(|name| scope.params[name])
        .collect();
    Ok(walker.infer.generalize_values(&roots))
}

pub(crate) fn retain_annotation<'a>(
    bump: &'a Bump,
    env: &KindEnv<'a>,
    home: ModuleName<'a>,
    name: &'a str,
    annotation: &nash_ast::Annotation<'a>,
) -> Result<&'a nash_ast::Annotation<'a>, Vec<Error<'a>>> {
    let kinds = check_annotation(bump, env, home, name, annotation)?;
    Ok(bump.alloc(nash_ast::Annotation {
        kinds,
        free_vars: annotation.free_vars,
        context: annotation.context,
        typ: annotation.typ,
    }))
}

pub(crate) fn check_decl_annotations<'a>(
    bump: &'a Bump,
    env: &KindEnv<'a>,
    home: ModuleName<'a>,
    decls: &nash_ast::Decls<'a>,
) -> Result<(), Vec<Error<'a>>> {
    let mut checker = AnnotationChecker {
        bump,
        env,
        home,
        errors: Vec::new(),
    };
    let mut next = decls;
    loop {
        match next {
            nash_ast::Decls::Empty => break,
            nash_ast::Decls::Declare {
                definition,
                next: rest,
            } => {
                checker.definition(definition);
                next = rest;
            }
            nash_ast::Decls::DeclareRec {
                definition,
                following,
                next: rest,
            } => {
                checker.definition(definition);
                for def in *following {
                    checker.definition(def);
                }
                next = rest;
            }
        }
    }
    if checker.errors.is_empty() {
        Ok(())
    } else {
        Err(checker.errors)
    }
}

struct AnnotationChecker<'e, 'a> {
    bump: &'a Bump,
    env: &'e KindEnv<'a>,
    home: ModuleName<'a>,
    errors: Vec<Error<'a>>,
}

impl<'a> AnnotationChecker<'_, 'a> {
    fn definition(&mut self, def: &nash_ast::Def<'a>) {
        match def {
            nash_ast::Def::Def { body, .. } => self.expression(&body.value),
            nash_ast::Def::TypedDef {
                kinds,
                context,
                name,
                free_vars,
                annotation,
                body,
                ..
            } => {
                let annotation = nash_ast::Annotation {
                    kinds: *kinds,
                    context,
                    free_vars,
                    typ: annotation,
                };
                if let Err(errors) =
                    check_annotation(self.bump, self.env, self.home, name.value, &annotation)
                {
                    self.errors.extend(errors);
                }
                self.expression(&body.value);
            }
        }
    }

    fn expression(&mut self, expr: &nash_ast::Expr<'a>) {
        use nash_ast::Expr;
        match expr {
            Expr::VarLocal(_)
            | Expr::VarTopLevel(_)
            | Expr::VarForeign { .. }
            | Expr::VarMethod { .. }
            | Expr::VarConstructor { .. }
            | Expr::VarOperator { .. }
            | Expr::Str(_)
            | Expr::Bytes(_)
            | Expr::Int(_)
            | Expr::Unit
            | Expr::Accessor(_) => {}
            Expr::Lambda { body: expr, .. } | Expr::Access { record: expr, .. } => {
                self.expression(&expr.value)
            }
            Expr::List(items) => {
                for item in *items {
                    self.expression(&item.value);
                }
            }
            Expr::Binop { left, right, .. } => {
                self.expression(&left.value);
                self.expression(&right.value);
            }
            Expr::Call {
                function,
                arguments,
            } => {
                self.expression(&function.value);
                for arg in *arguments {
                    self.expression(&arg.value);
                }
            }
            Expr::If {
                branches,
                final_else,
            } => {
                for branch in *branches {
                    self.expression(&branch.condition.value);
                    self.expression(&branch.then_branch.value);
                }
                self.expression(&final_else.value);
            }
            Expr::Let { definition, body } => {
                self.definition(definition);
                self.expression(&body.value);
            }
            Expr::LetRec { definitions, body } => {
                for def in *definitions {
                    self.definition(def);
                }
                self.expression(&body.value);
            }
            Expr::LetDestruct { value, body, .. } => {
                self.expression(&value.value);
                self.expression(&body.value);
            }
            Expr::Case {
                scrutinee,
                branches,
            } => {
                self.expression(&scrutinee.value);
                for branch in *branches {
                    self.expression(&branch.body.value);
                }
            }
            Expr::Update { base, fields, .. } => {
                self.expression(&base.value);
                for field in *fields {
                    self.expression(&field.value.value);
                }
            }
            Expr::Record(fields) => {
                for field in *fields {
                    self.expression(&field.value.value);
                }
            }
            Expr::Tuple {
                first,
                second,
                rest,
            } => {
                self.expression(&first.value);
                self.expression(&second.value);
                for item in *rest {
                    self.expression(&item.value);
                }
            }
        }
    }
}

/// Infer mutually dependent trait schemes without mixing the trait and type namespaces.
pub(crate) fn infer_traits<'a>(
    bump: &'a Bump,
    env: &mut KindEnv<'a>,
    home: ModuleName<'a>,
    traits: &[crate::traits::PreTrait<'a>],
) -> Result<BTreeMap<&'a str, KindScheme<'a>>, Vec<Error<'a>>> {
    let nodes = traits
        .iter()
        .map(|t| {
            let predicates = t.supers.iter().chain(
                t.methods
                    .iter()
                    .flat_map(|m| m.annotation.context.iter().skip(1)),
            );
            crate::scc::Node {
                key: t.source.value.name.value,
                value: t,
                deps: predicates
                    .filter(|p| p.trait_.home == home)
                    .map(|p| p.trait_.name)
                    .collect(),
            }
        })
        .collect();
    let mut schemes = BTreeMap::new();
    for component in crate::scc::strongly_connected_components(nodes) {
        let group = match component {
            crate::scc::Scc::Acyclic(t) => vec![t],
            crate::scc::Scc::Cyclic(group) => group,
        };
        let mut walker = Walker {
            bump,
            infer: Infer::new(bump),
            env,
            home,
            group: BTreeMap::new(),
            errors: Vec::new(),
            inferred_records: None,
        };
        let mut trait_kinds = BTreeMap::new();
        let mut scopes = Vec::new();
        for t in &group {
            let params: BTreeMap<_, _> = t
                .parameters
                .iter()
                .map(|p| (*p, walker.infer.fresh_k(KindSet::ALL)))
                .collect();
            let mut root: &K = bump.alloc(K::Base(BaseKind::Term));
            for p in t.parameters.iter().rev() {
                root = bump.alloc(K::Arrow(params[p], root));
            }
            trait_kinds.insert(
                QualifiedName {
                    home,
                    name: t.source.value.name.value,
                },
                root,
            );
            scopes.push(Scope { params });
        }
        for (t, scope) in group.iter().zip(&scopes) {
            for predicate in t.supers {
                walker.infer_predicate(scope, predicate, &trait_kinds);
            }
            for method in t.methods {
                let mut method_scope = Scope {
                    params: scope.params.clone(),
                };
                for variable in method.annotation.free_vars {
                    method_scope
                        .params
                        .entry(variable)
                        .or_insert_with(|| walker.infer.fresh_k(KindSet::ALL));
                }
                walker.expect_any(&method_scope, method.annotation.typ);
                for predicate in method.annotation.context.iter().skip(1) {
                    walker.infer_predicate(&method_scope, predicate, &trait_kinds);
                }
            }
            for param in t.source.value.params {
                if let Some(annotation) = param.kind {
                    let expected = walker.annotation_kind(annotation);
                    walker.expect(
                        annotation.region,
                        KindContext::ParamAnnotation {
                            type_name: t.source.value.name.value,
                            param: param.name.value,
                        },
                        expected,
                        scope.params[param.name.value],
                    );
                }
            }
        }
        if !walker.errors.is_empty() {
            return Err(walker.errors);
        }
        let finished: Vec<_> = trait_kinds
            .into_iter()
            .map(|(name, kind)| (name, walker.infer.generalize(kind)))
            .collect();
        for (name, kind) in finished {
            env.trait_schemes.insert(name, kind);
            schemes.insert(name.name, kind);
        }
    }
    Ok(schemes)
}

impl<'a> Walker<'_, 'a> {
    fn infer_predicate(
        &mut self,
        scope: &Scope<'a>,
        predicate: &nash_ast::Pred<'a>,
        group: &BTreeMap<QualifiedName<'a>, &'a K<'a>>,
    ) {
        let root = match group.get(&predicate.trait_) {
            Some(kind) => *kind,
            None => self.infer.instantiate(
                self.env
                    .trait_schemes
                    .get(&predicate.trait_)
                    .expect("trait kinds cover resolved predicates"),
            ),
        };
        self.apply_args(
            scope,
            predicate
                .args
                .first()
                .map_or(Region::zero(), |arg| arg.region),
            KindHead::Named(predicate.trait_),
            root,
            predicate.args,
        );
    }
}

/// Prove that a ground canonical type already has kind Big, without choosing
/// Big for a polymorphic kind. Used by post-solve reflexive Lift resolution.
/// The environment must cover every resolved type identity in `typ`.
pub fn proves_ground_big<'a>(
    bump: &'a Bump,
    env: &KindEnv<'a>,
    typ: &'a Located<CanType<'a>>,
) -> bool {
    let mut variables = BTreeSet::new();
    crate::types::collect_free_vars(&typ.value, &mut variables);
    if !variables.is_empty() {
        return false;
    }
    let mut walker = Walker {
        bump,
        env,
        home: nash_ast::primitives::builtin_home(),
        infer: Infer::new(bump),
        group: BTreeMap::new(),
        errors: Vec::new(),
        inferred_records: Some(Vec::new()),
    };
    let scope = Scope {
        params: BTreeMap::new(),
    };
    let kind = walker.infer_type(&scope, typ);
    if !walker.errors.is_empty() {
        return false;
    }
    let records = walker
        .infer
        .generalize_values(walker.inferred_records.as_ref().unwrap());
    let mut distinct = BTreeSet::new();
    for kind in records.kinds {
        if !matches!(kind, Kind::Var(index) if records.bounds[*index as usize] == KindSet::ANY && distinct.insert(*index))
        {
            return false;
        }
    }
    let scheme = walker.infer.generalize(kind);
    match scheme.kind {
        Kind::Base(BaseKind::Big) => true,
        Kind::Var(index) => scheme.bounds[*index as usize] == KindSet::BIG,
        _ => false,
    }
}

/// Coherence query with fresh kind binders for both recursive impl patterns.
/// Trait contexts contribute their inferred kinds, but do not otherwise prove
/// disjointness. This query never changes either retained impl scheme.
pub fn impls_overlap<'a>(
    bump: &'a Bump,
    env: &KindEnv<'a>,
    left: nash_ast::ImplKey<'a>,
    right: nash_ast::ImplKey<'a>,
    remaining: &mut usize,
) -> Result<bool, nash_ast::head::Limit> {
    if left.trait_ != right.trait_ {
        return Ok(false);
    }
    fn pattern_kind<'a>(
        bump: &'a Bump,
        env: &KindEnv<'a>,
        infer: &mut Infer<'a>,
        roots: &[&'a K<'a>],
        pattern: &nash_ast::Head<'a>,
        remaining: &mut usize,
        exhausted: &mut bool,
    ) -> Option<&'a K<'a>> {
        if nash_ast::head::step(remaining).is_err() {
            *exhausted = true;
            return None;
        }
        match pattern {
            nash_ast::Head::Var(index) => Some(roots[usize::from(*index)]),
            nash_ast::Head::Named { reference, args } => {
                let mut kind = infer.constructor(env.scheme(*reference));
                for arg in *args {
                    let actual = pattern_kind(bump, env, infer, roots, arg, remaining, exhausted)?;
                    let (expected, result) = infer.apply(kind).ok()?;
                    infer.unify(expected, actual).ok()?;
                    kind = result;
                }
                Some(kind)
            }
            nash_ast::Head::Unit => Some(bump.alloc(K::Base(BaseKind::Const))),
            nash_ast::Head::Tuple(_) | nash_ast::Head::Function(..) => {
                Some(bump.alloc(K::Base(BaseKind::Term)))
            }
        }
    }
    let mut infer = Infer::new(bump);
    let roots = [
        infer.instantiate_values(&left.kinds),
        infer.instantiate_values(&right.kinds),
    ];
    let initial = *remaining;
    let mut kind_work = initial;
    let mut exhausted = false;
    let overlap = nash_ast::head::overlaps(
        left.heads,
        right.heads,
        remaining,
        |a_side, a, b_side, b| {
            let Some(a) = pattern_kind(
                bump,
                env,
                &mut infer,
                &roots[usize::from(a_side)],
                a,
                &mut kind_work,
                &mut exhausted,
            ) else {
                return false;
            };
            let Some(b) = pattern_kind(
                bump,
                env,
                &mut infer,
                &roots[usize::from(b_side)],
                b,
                &mut kind_work,
                &mut exhausted,
            ) else {
                return false;
            };
            infer.unify(a, b).is_ok()
        },
    )?;
    *remaining = remaining
        .checked_sub(initial - kind_work)
        .ok_or(nash_ast::head::Limit)?;
    if exhausted {
        return Err(nash_ast::head::Limit);
    }
    Ok(overlap && infer.settle().is_ok())
}

/// Prove an impl's kind requirements at closed canonical type arguments.
pub fn proves_ground_signature<'a>(
    bump: &'a Bump,
    env: &KindEnv<'a>,
    signature: ValueKinds<'a>,
    types: &[&'a Located<CanType<'a>>],
) -> bool {
    let mut variables = BTreeSet::new();
    for typ in types {
        crate::types::collect_free_vars(&typ.value, &mut variables);
    }
    if !variables.is_empty() {
        return false;
    }
    ImplKinds {
        walker: Walker {
            bump,
            env,
            home: nash_ast::primitives::builtin_home(),
            infer: Infer::new(bump),
            group: BTreeMap::new(),
            errors: Vec::new(),
            inferred_records: Some(Vec::new()),
        },
        scope: Scope {
            params: BTreeMap::new(),
        },
    }
    .proves_signature(signature, types)
}

pub(crate) struct ImplKinds<'e, 'a> {
    walker: Walker<'e, 'a>,
    scope: Scope<'a>,
}

impl<'a> ImplKinds<'_, 'a> {
    pub(crate) fn proves_signature(
        &self,
        signature: ValueKinds<'a>,
        types: &[&'a Located<CanType<'a>>],
    ) -> bool {
        assert_eq!(signature.kinds.len(), types.len());
        let mut query = Walker {
            bump: self.walker.bump,
            env: self.walker.env,
            home: self.walker.home,
            infer: self.walker.infer.clone(),
            group: BTreeMap::new(),
            errors: Vec::new(),
            inferred_records: Some(Vec::new()),
        };
        let mut roots: Vec<_> = self.scope.params.values().copied().collect();
        let actual: Vec<_> = types
            .iter()
            .map(|typ| query.infer_type(&self.scope, typ))
            .collect();
        if !query.errors.is_empty()
            || !self
                .walker
                .infer
                .proves_extension(query.infer.clone(), &roots)
        {
            return false;
        }
        roots.extend_from_slice(&actual);
        roots.extend(query.inferred_records.as_ref().unwrap());
        query.infer.proves_values(signature, &actual, &roots)
    }

    pub(crate) fn generalize(&mut self, variables: &[&str]) -> ValueKinds<'a> {
        let roots: Vec<_> = variables
            .iter()
            .map(|name| self.scope.params[name])
            .collect();
        self.walker.infer.generalize_values(&roots)
    }
    fn overlaps_reflexive_lift(
        &self,
        heads: &[&'a Located<CanType<'a>>],
    ) -> Result<bool, Vec<Error<'a>>> {
        fn named<'a>(
            typ: &CanType<'a>,
        ) -> Option<(QualifiedName<'a>, Vec<&'a Located<CanType<'a>>>)> {
            match typ {
                CanType::Named { reference, args } => Some((*reference, args.to_vec())),
                CanType::Alias {
                    reference,
                    arguments,
                    ..
                } => Some((*reference, arguments.iter().map(|a| a.typ).collect())),
                _ => None,
            }
        }
        let [first, second] = heads else {
            return Ok(false);
        };
        let mut variables = BTreeMap::new();
        let mut order = Vec::new();
        let first_pattern = crate::impls::canonicalize_pattern(
            self.walker.bump,
            first,
            &mut variables,
            &mut order,
        )?;
        let second_pattern = crate::impls::canonicalize_pattern(
            self.walker.bump,
            second,
            &mut variables,
            &mut order,
        )?;
        if !nash_ast::head::can_equal(&[first_pattern], &[second_pattern], &mut 16_384).map_err(
            |_| {
                vec![Error::ImplPatternLimit {
                    region: first.region,
                }]
            },
        )? {
            return Ok(false);
        }
        let (Some((a, aa)), Some((b, ba))) = (named(&first.value), named(&second.value)) else {
            return Ok(false);
        };
        if a != b || aa.len() != ba.len() {
            return Ok(false);
        }
        let mut query = Walker {
            bump: self.walker.bump,
            env: self.walker.env,
            home: self.walker.home,
            infer: self.walker.infer.clone(),
            group: BTreeMap::new(),
            errors: Vec::new(),
            inferred_records: None,
        };
        for (a, b) in aa.iter().zip(&ba) {
            let a = query.infer_type(&self.scope, a);
            let b = query.infer_type(&self.scope, b);
            if query.infer.unify(a, b).is_err() {
                return Ok(false);
            }
        }
        let kind = query.infer_type(&self.scope, first);
        let big = query.bump.alloc(K::Base(BaseKind::Big));
        Ok(query.errors.is_empty() && query.infer.unify(kind, big).is_ok())
    }
    /// Prove Big without narrowing the universally quantified impl variables.
    pub(crate) fn proves_big(&mut self, typ: &'a Located<CanType<'a>>) -> bool {
        let mut root: &K = self.walker.bump.alloc(K::Base(BaseKind::Term));
        for kind in self.scope.params.values().rev() {
            root = self.walker.bump.alloc(K::Arrow(kind, root));
        }
        let before = self.walker.infer.generalize(root);
        let mut query = Walker {
            bump: self.walker.bump,
            env: self.walker.env,
            home: self.walker.home,
            infer: self.walker.infer.clone(),
            group: BTreeMap::new(),
            errors: Vec::new(),
            inferred_records: None,
        };
        let kind = query.infer_type(&self.scope, typ);
        if !query.errors.is_empty() || query.infer.generalize(root) != before {
            return false;
        }
        let kind = query.infer.generalize(kind);
        match kind.kind {
            Kind::Base(BaseKind::Big) => true,
            Kind::Var(index) => kind.bounds[*index as usize] == KindSet::BIG,
            _ => false,
        }
    }
}

pub(crate) fn check_impl_heads<'e, 'a>(
    bump: &'a Bump,
    env: &'e KindEnv<'a>,
    home: ModuleName<'a>,
    info: &crate::environment::TraitInfo<'a>,
    heads: &[&'a Located<CanType<'a>>],
    variables: &BTreeMap<&'a str, Region>,
    context: &[nash_ast::Pred<'a>],
) -> Result<ImplKinds<'e, 'a>, Vec<Error<'a>>> {
    let mut walker = Walker {
        bump,
        env,
        home,
        infer: Infer::new(bump),
        group: BTreeMap::new(),
        errors: Vec::new(),
        inferred_records: None,
    };
    let scope = Scope {
        params: variables
            .keys()
            .map(|name| (*name, walker.infer.fresh_k(KindSet::ALL)))
            .collect(),
    };
    let mut kind = walker.infer.instantiate(&info.kind);
    for (index, head) in heads.iter().enumerate() {
        let (expected, result) = match walker.infer.apply(kind) {
            Ok(parts) => parts,
            Err(_) => {
                walker.errors.push(Error::KindTooManyArgs {
                    region: head.region,
                    head: KindHead::Named(QualifiedName {
                        home: info.home,
                        name: info.name,
                    }),
                    applied: heads.len(),
                    accepted: index,
                });
                return Err(walker.errors);
            }
        };
        let actual = walker.infer_type(&scope, head);
        walker.expect(
            head.region,
            KindContext::ImplHead {
                trait_: QualifiedName {
                    home: info.home,
                    name: info.name,
                },
                index: index as u16,
            },
            expected,
            actual,
        );
        kind = result;
    }
    for predicate in context {
        walker.infer_predicate(&scope, predicate, &BTreeMap::new());
    }
    if walker.errors.is_empty() {
        let mut checked = ImplKinds { walker, scope };
        if (QualifiedName {
            home: info.home,
            name: info.name,
        }) == nash_ast::primitives::eq_trait()
            && let [head] = heads
            && checked.proves_big(head)
        {
            return Err(vec![Error::StructuralEqOverride { head }]);
        }
        if (QualifiedName {
            home: info.home,
            name: info.name,
        }) == nash_ast::primitives::lift_trait()
            && checked.overlaps_reflexive_lift(heads)?
        {
            return Err(vec![Error::ReflexiveLiftOverlap {
                heads: bump.alloc_slice_copy(heads),
            }]);
        }
        Ok(checked)
    } else {
        Err(walker.errors)
    }
}

pub(crate) fn check_trait_method_annotations<'a>(
    bump: &'a Bump,
    env: &KindEnv<'a>,
    home: ModuleName<'a>,
    traits: &[&'a Located<nash_ast::Trait<'a>>],
    impls: &[&'a Located<nash_ast::Impl<'a>>],
) -> Result<(), Vec<Error<'a>>> {
    let mut checker = AnnotationChecker {
        bump,
        env,
        home,
        errors: Vec::new(),
    };
    for trait_ in traits {
        for method in trait_.value.methods {
            if let Some(definition) = method.default {
                checker.definition(definition);
            }
        }
    }
    for impl_ in impls {
        for method in impl_.value.methods {
            checker.definition(method);
        }
    }
    if checker.errors.is_empty() {
        Ok(())
    } else {
        Err(checker.errors)
    }
}
