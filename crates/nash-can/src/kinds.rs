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
