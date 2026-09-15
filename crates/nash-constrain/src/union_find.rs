//! Port of Elm's `Type.UnionFind`.
//!
//! Elm's `Point` is an `IORef` cell graph; here the cells live in a single
//! `Vec` owned by [`UnionFind`] and a [`Variable`] is an index into it.
//! Index equality is exactly Elm's `IORef` identity equality, and `union`
//! keeps Elm's weight balancing so representative choices match.

use crate::type_::Descriptor;

/// A type variable: Elm's `Type.Variable = UF.Point Descriptor`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Variable(u32);

impl Variable {
    fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Debug)]
// Keep descriptors inline in the dense point vector; boxing adds an allocation
// and an indirection to every fresh inference variable.
#[allow(clippy::large_enum_variant)]
enum PointInfo<'a> {
    Info { weight: u32, desc: Descriptor<'a> },
    Link(Variable),
}

#[derive(Debug, Default)]
pub struct UnionFind<'a> {
    points: Vec<PointInfo<'a>>,
    declared_holes: std::collections::HashMap<
        nash_ast::DeclaredHoleId,
        (&'a nash_ast::DeclaredHole<'a>, Variable),
    >,
    declared_roots: std::collections::HashMap<Variable, &'a nash_ast::DeclaredHole<'a>>,
    captured_rigid_ranks: std::collections::HashMap<Variable, usize>,
    declared_store: nash_ast::declared::DeclaredStore,
    declared_arena: Option<&'a bumpalo::Bump>,
    declared_resolutions: std::collections::HashMap<
        nash_ast::DeclaredHoleId,
        Option<&'a nash_region::Located<nash_ast::Type<'a>>>,
    >,
    instantiations: Vec<std::collections::HashMap<nash_ast::DeclaredHoleId, Variable>>,
}

impl<'a> UnionFind<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Each module observes committed owned refinements but keeps inference local.
    pub fn set_declared_context(
        &mut self,
        bump: &'a bumpalo::Bump,
        store: nash_ast::declared::DeclaredStore,
    ) {
        self.declared_arena = Some(bump);
        self.declared_store = store;
        self.declared_resolutions.clear();
        self.declared_holes.clear();
        self.declared_roots.clear();
        self.captured_rigid_ranks.clear();
        self.instantiations.clear();
    }

    pub fn declared_store(&self) -> &nash_ast::declared::DeclaredStore {
        &self.declared_store
    }

    pub(crate) fn resolve_declared(
        &mut self,
        hole: &nash_ast::DeclaredHole<'_>,
    ) -> Option<&'a nash_region::Located<nash_ast::Type<'a>>> {
        if let Some(solution) = self.declared_resolutions.get(&hole.id) {
            return *solution;
        }
        let solution = self
            .declared_arena
            .and_then(|bump| self.declared_store.resolve(bump, hole));
        self.declared_resolutions.insert(hole.id, solution);
        solution
    }

    /// Share generic identities across all pieces of one annotation.
    pub fn begin_instantiation(&mut self) -> bool {
        if self.instantiations.is_empty() {
            self.push_instantiation_scope();
            true
        } else {
            false
        }
    }

    pub fn end_instantiation(&mut self, owned: bool) {
        if owned {
            self.pop_instantiation_scope();
        }
    }

    pub(crate) fn push_instantiation_scope(&mut self) {
        self.instantiations.push(std::collections::HashMap::new());
    }

    pub(crate) fn pop_instantiation_scope(&mut self) {
        self.instantiations
            .pop()
            .expect("balanced annotation instantiation scope");
    }

    pub(crate) fn generic_variable(&self, id: nash_ast::DeclaredHoleId) -> Option<Variable> {
        self.instantiations
            .last()
            .and_then(|scope| scope.get(&id))
            .copied()
    }

    pub(crate) fn bind_generic_variable(
        &mut self,
        id: nash_ast::DeclaredHoleId,
        variable: Variable,
    ) {
        self.instantiations
            .last_mut()
            .expect("generic inside an annotation instantiation")
            .insert(id, variable);
    }

    pub fn declared_variable(&self, hole: &'a nash_ast::DeclaredHole<'a>) -> Option<Variable> {
        self.declared_holes
            .get(&hole.id)
            .map(|(_, variable)| *variable)
    }

    pub fn bind_declared_hole(&mut self, hole: &'a nash_ast::DeclaredHole<'a>, variable: Variable) {
        debug_assert_eq!(hole.kind, nash_ast::DeclaredHoleKind::Inference);
        let root = self.find(variable);
        let descriptor = self.get(root);
        let rigid_rank = matches!(descriptor.content, crate::type_::Content::RigidVar(_))
            .then_some(descriptor.rank);
        self.declared_holes.insert(hole.id, (hole, root));
        self.declared_roots.entry(root).or_insert(hole);
        if let Some(rank) = rigid_rank {
            self.captured_rigid_ranks.entry(root).or_insert(rank);
        } else {
            self.modify(root, |descriptor| {
                descriptor.rank = crate::type_::OUTERMOST_RANK
            });
        }
    }

    pub fn declared_hole(&mut self, variable: Variable) -> Option<&'a nash_ast::DeclaredHole<'a>> {
        let root = self.find(variable);
        self.declared_roots.get(&root).copied()
    }

    pub fn is_declared_capture(&mut self, variable: Variable) -> bool {
        let root = self.find(variable);
        self.captured_rigid_ranks.contains_key(&root)
    }

    pub fn declared_holes(
        &self,
    ) -> impl Iterator<Item = (&'a nash_ast::DeclaredHole<'a>, Variable)> + '_ {
        self.declared_holes.values().copied()
    }

    pub fn fresh(&mut self, desc: Descriptor<'a>) -> Variable {
        let var = Variable(self.points.len() as u32);
        self.points.push(PointInfo::Info { weight: 1, desc });
        var
    }

    /// Find the representative, compressing every point on the path to link
    /// directly to it (the net effect of Elm's recursive `repr`).
    fn repr(&mut self, point: Variable) -> Variable {
        let mut root = point;
        while let PointInfo::Link(next) = self.points[root.index()] {
            root = next;
        }
        let mut walk = point;
        while let PointInfo::Link(next) = self.points[walk.index()] {
            if next != root {
                self.points[walk.index()] = PointInfo::Link(root);
            }
            walk = next;
        }
        root
    }

    pub fn get(&mut self, point: Variable) -> &Descriptor<'a> {
        let root = self.repr(point);
        match &self.points[root.index()] {
            PointInfo::Info { desc, .. } => desc,
            PointInfo::Link(_) => unreachable!("repr returns a root"),
        }
    }

    pub fn set(&mut self, point: Variable, mut new_desc: Descriptor<'a>) {
        let root = self.repr(point);
        self.retain_declared_rank(root, &mut new_desc);
        match &mut self.points[root.index()] {
            PointInfo::Info { desc, .. } => *desc = new_desc,
            PointInfo::Link(_) => unreachable!("repr returns a root"),
        }
    }

    pub fn modify(&mut self, point: Variable, func: impl FnOnce(&mut Descriptor<'a>)) {
        let root = self.repr(point);
        let declared = self.declared_roots.contains_key(&root);
        let captured_rank = self.captured_rigid_ranks.get(&root).copied();
        match &mut self.points[root.index()] {
            PointInfo::Info { desc, .. } => {
                func(desc);
                Self::retain_rank(declared, captured_rank, desc);
            }
            PointInfo::Link(_) => unreachable!("repr returns a root"),
        }
    }

    pub fn union(&mut self, p1: Variable, p2: Variable, mut new_desc: Descriptor<'a>) {
        let point1 = self.repr(p1);
        let point2 = self.repr(p2);
        let declared =
            self.declared_roots.contains_key(&point1) || self.declared_roots.contains_key(&point2);
        let capture_rank =
            if declared && matches!(new_desc.content, crate::type_::Content::RigidVar(_)) {
                [point1, point2]
                    .into_iter()
                    .find_map(|root| {
                        let descriptor = self.get(root);
                        matches!(descriptor.content, crate::type_::Content::RigidVar(_))
                            .then_some((root, descriptor.rank))
                    })
                    .map(|(root, rank)| {
                        self.captured_rigid_ranks
                            .get(&root)
                            .copied()
                            .unwrap_or(rank)
                    })
            } else {
                self.captured_rigid_ranks
                    .get(&point1)
                    .or_else(|| self.captured_rigid_ranks.get(&point2))
                    .copied()
            };
        Self::retain_rank(declared, capture_rank, &mut new_desc);

        if point1 == point2 {
            if let Some(rank) = capture_rank {
                self.captured_rigid_ranks.insert(point1, rank);
            }
            match &mut self.points[point1.index()] {
                PointInfo::Info { desc, .. } => *desc = new_desc,
                PointInfo::Link(_) => unreachable!("repr returns a root"),
            }
            if declared {
                self.capture_declared_rigids(point1);
            }
            return;
        }

        let weight1 = match &self.points[point1.index()] {
            PointInfo::Info { weight, .. } => *weight,
            PointInfo::Link(_) => unreachable!("repr returns a root"),
        };
        let weight2 = match &self.points[point2.index()] {
            PointInfo::Info { weight, .. } => *weight,
            PointInfo::Link(_) => unreachable!("repr returns a root"),
        };

        let new_weight = weight1 + weight2;
        let (winner, loser) = if weight1 >= weight2 {
            (point1, point2)
        } else {
            (point2, point1)
        };
        let first_hole = self.declared_roots.remove(&point1);
        let second_hole = self.declared_roots.remove(&point2);
        let hole = match (first_hole, second_hole) {
            (Some(first), Some(second)) if second.id < first.id => Some(second),
            (Some(first), _) => Some(first),
            (None, second) => second,
        };
        if let Some(hole) = hole {
            self.declared_roots.insert(winner, hole);
        }
        self.captured_rigid_ranks.remove(&point1);
        self.captured_rigid_ranks.remove(&point2);
        if let Some(rank) = capture_rank {
            self.captured_rigid_ranks.insert(winner, rank);
        }

        self.points[loser.index()] = PointInfo::Link(winner);
        match &mut self.points[winner.index()] {
            PointInfo::Info { weight, desc } => {
                *weight = new_weight;
                *desc = new_desc;
            }
            PointInfo::Link(_) => unreachable!("repr returns a root"),
        }
        if declared {
            self.capture_declared_rigids(winner);
        }
    }

    /// A declaration can capture a rigid below a constructor, not only at its
    /// root. Preserve that binder's rank before rank adjustment visits the graph.
    fn capture_declared_rigids(&mut self, root: Variable) {
        use crate::type_::{Content, FlatType};
        let mut pending = vec![root];
        let mut seen = std::collections::HashSet::new();
        while let Some(variable) = pending.pop() {
            let variable = self.find(variable);
            if !seen.insert(variable) {
                continue;
            }
            let descriptor = self.get(variable);
            let rank = descriptor.rank;
            match descriptor.content.clone() {
                Content::RigidVar(_) => {
                    self.captured_rigid_ranks.entry(variable).or_insert(rank);
                }
                Content::Structure(FlatType::App1(_, _, args)) => pending.extend(args),
                Content::Structure(FlatType::AppV1(head, args)) => {
                    pending.push(head);
                    pending.extend(args);
                }
                Content::Structure(FlatType::Fun1(argument, result)) => {
                    pending.extend([argument, result])
                }
                Content::Structure(FlatType::Function1(arguments, result)) => {
                    pending.extend(arguments);
                    pending.push(result);
                }
                Content::Structure(FlatType::Tuple1(first, second, rest)) => {
                    pending.extend([first, second]);
                    pending.extend(rest);
                }
                Content::Structure(FlatType::Record1(fields)) => {
                    pending.extend(fields.into_values())
                }
                Content::Alias { args, real, .. } => {
                    pending.extend(args.into_iter().map(|(_, argument)| argument));
                    pending.push(real);
                }
                Content::PartialAlias { args, .. } => {
                    pending.extend(args.into_iter().map(|(_, argument)| argument))
                }
                Content::FlexVar(_) | Content::Error => {}
            }
        }
    }

    fn retain_declared_rank(&self, root: Variable, descriptor: &mut Descriptor<'a>) {
        Self::retain_rank(
            self.declared_roots.contains_key(&root),
            self.captured_rigid_ranks.get(&root).copied(),
            descriptor,
        );
    }

    fn retain_rank(declared: bool, captured_rank: Option<usize>, descriptor: &mut Descriptor<'a>) {
        if matches!(descriptor.content, crate::type_::Content::RigidVar(_)) {
            if let Some(rank) = captured_rank
                && descriptor.rank != crate::type_::NO_RANK
            {
                descriptor.rank = rank;
            }
        } else if declared {
            descriptor.rank = crate::type_::OUTERMOST_RANK;
        }
    }

    pub fn equivalent(&mut self, p1: Variable, p2: Variable) -> bool {
        self.repr(p1) == self.repr(p2)
    }

    /// The representative of this point's equivalence class.
    pub fn find(&mut self, point: Variable) -> Variable {
        self.repr(point)
    }

    /// True when this point has been unioned into another representative.
    pub fn redundant(&self, point: Variable) -> bool {
        matches!(self.points[point.index()], PointInfo::Link(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::type_::{Content, make_descriptor};

    fn flex<'a>(uf: &mut UnionFind<'a>, name: &'a str) -> Variable {
        uf.fresh(make_descriptor(Content::FlexVar(Some(name))))
    }

    fn name_of<'a>(uf: &mut UnionFind<'a>, var: Variable) -> Option<&'a str> {
        match uf.get(var).content {
            Content::FlexVar(name) => name,
            _ => panic!("expected flex var"),
        }
    }

    #[test]
    fn fresh_points_are_distinct() {
        let mut uf = UnionFind::new();
        let a = flex(&mut uf, "a");
        let b = flex(&mut uf, "b");
        assert!(!uf.equivalent(a, b));
        assert!(uf.equivalent(a, a));
        assert!(!uf.redundant(a));
    }

    #[test]
    fn union_makes_points_equivalent_and_sets_descriptor() {
        let mut uf = UnionFind::new();
        let a = flex(&mut uf, "a");
        let b = flex(&mut uf, "b");
        uf.union(a, b, make_descriptor(Content::FlexVar(Some("c"))));
        assert!(uf.equivalent(a, b));
        assert_eq!(name_of(&mut uf, a), Some("c"));
        assert_eq!(name_of(&mut uf, b), Some("c"));
        assert!(uf.redundant(a) != uf.redundant(b));
    }

    #[test]
    fn set_and_modify_write_through_links() {
        let mut uf = UnionFind::new();
        let a = flex(&mut uf, "a");
        let b = flex(&mut uf, "b");
        let c = flex(&mut uf, "c");
        uf.union(a, b, make_descriptor(Content::FlexVar(Some("ab"))));
        uf.union(b, c, make_descriptor(Content::FlexVar(Some("abc"))));

        uf.set(c, make_descriptor(Content::FlexVar(Some("via-c"))));
        assert_eq!(name_of(&mut uf, a), Some("via-c"));

        uf.modify(a, |desc| desc.content = Content::FlexVar(Some("via-a")));
        assert_eq!(name_of(&mut uf, b), Some("via-a"));
        assert_eq!(name_of(&mut uf, c), Some("via-a"));
    }

    #[test]
    fn union_by_weight_keeps_heavier_representative() {
        let mut uf = UnionFind::new();
        let a = flex(&mut uf, "a");
        let b = flex(&mut uf, "b");
        let c = flex(&mut uf, "c");
        // a-b makes a two-element class rooted somewhere; unioning the
        // singleton c in keeps the heavier class's representative.
        uf.union(a, b, make_descriptor(Content::FlexVar(Some("ab"))));
        uf.union(c, a, make_descriptor(Content::FlexVar(Some("abc"))));
        assert!(uf.equivalent(a, c));
        assert!(uf.redundant(c));
    }

    #[test]
    fn union_of_same_class_replaces_descriptor() {
        let mut uf = UnionFind::new();
        let a = flex(&mut uf, "a");
        let b = flex(&mut uf, "b");
        uf.union(a, b, make_descriptor(Content::FlexVar(Some("first"))));
        uf.union(b, a, make_descriptor(Content::FlexVar(Some("second"))));
        assert_eq!(name_of(&mut uf, a), Some("second"));
    }

    #[test]
    fn long_chains_compress_to_the_root() {
        let mut uf = UnionFind::new();
        let vars: Vec<Variable> = (0..64).map(|_| flex(&mut uf, "x")).collect();
        for pair in vars.windows(2) {
            uf.union(pair[0], pair[1], make_descriptor(Content::FlexVar(None)));
        }
        for var in &vars {
            assert!(uf.equivalent(*var, vars[0]));
        }
        assert_eq!(
            vars.iter().filter(|var| !uf.redundant(**var)).count(),
            1,
            "exactly one representative"
        );
    }
}
