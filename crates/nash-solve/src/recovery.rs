//! Dependency closure for failed inference computations.
//!
//! Failed comparisons invalidate changed shared variables. Containment carries
//! that failure to parents without invalidating independent siblings.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use nash_constrain::{Content, FlatType, UnionFind, Variable};

#[derive(Default)]
pub(crate) struct Dependencies {
    /// Recorded containment: parent handle to child handles.
    edges: BTreeMap<Variable, BTreeSet<Variable>>,
    /// Recorded parent handles by their current representative.
    parents: HashMap<Variable, Vec<Variable>>,
    computations: BTreeSet<(Variable, Variable)>,
    field_receivers: BTreeSet<(Variable, Variable)>,
    /// Representatives of variables whose only type information came from a
    /// structure that merged into an existing `Error`, such as the argument of
    /// a call to a failed definition. They are unknown because of that
    /// failure, not ambiguous.
    stranded: HashSet<Variable>,
    /// ORACLE input: stranded variables as recorded, before later merges.
    #[cfg(any(test, debug_assertions))]
    stranded_handles: BTreeSet<Variable>,
    /// Number of union-find merges already applied to `parents` and `stranded`.
    merged: usize,
}

impl Dependencies {
    /// Remember containment before normalization or union can remove an edge.
    /// Original variable handles remain valid after their representatives move.
    pub(crate) fn remember(
        &mut self,
        uf: &mut UnionFind<'_>,
        roots: impl IntoIterator<Item = Variable>,
    ) {
        self.sync(uf);
        let mut pending: Vec<_> = roots.into_iter().collect();
        let mut seen = BTreeSet::new();
        while let Some(variable) = pending.pop() {
            let variable = uf.find(variable);
            if !seen.insert(variable) {
                continue;
            }
            for child in children(&uf.get(variable).content) {
                let child = uf.find(child);
                let recorded = self.edges.entry(variable).or_default();
                if recorded.is_empty() {
                    self.parents.entry(variable).or_default().push(variable);
                }
                recorded.insert(child);
                pending.push(child);
            }
        }
    }

    /// Move the indexes keyed by representative along the merges made since
    /// the last call.
    pub(crate) fn sync(&mut self, uf: &UnionFind<'_>) {
        for &(loser, winner) in &uf.merges()[self.merged..] {
            if let Some(moved) = self.parents.remove(&loser) {
                self.parents.entry(winner).or_default().extend(moved);
            }
            if self.stranded.remove(&loser) {
                self.stranded.insert(winner);
            }
        }
        self.merged = uf.merges().len();
    }

    pub(crate) fn propagate(&self, uf: &mut UnionFind<'_>) -> bool {
        let mut changed = false;
        let edges = self
            .edges
            .iter()
            .flat_map(|(parent, children)| children.iter().map(move |child| (*parent, *child)));
        for (parent, child) in edges.chain(self.computations.iter().copied()) {
            // Retain an aggregate whose current shape already contains the
            // error: unification can still check its unaffected children.
            // A lost historical edge or a separate computation result needs
            // an explicit error marker because its visible tree has none.
            if is_poisoned(uf, [child]) && !is_poisoned(uf, [parent]) {
                changed |= poison_roots(uf, [parent]);
            }
        }
        for &(field, receiver) in &self.field_receivers {
            // A selected field does not depend on other fields in the record.
            // Its own type is linked when field resolution unifies the result.
            if matches!(uf.get(receiver).content, Content::Error) && !is_poisoned(uf, [field]) {
                changed |= poison_roots(uf, [field]);
            }
        }
        changed
    }

    pub(crate) fn computation(
        &mut self,
        output: Variable,
        inputs: impl IntoIterator<Item = Variable>,
    ) {
        self.computations
            .extend(inputs.into_iter().map(|input| (output, input)));
    }

    pub(crate) fn field(&mut self, output: Variable, receiver: Variable) {
        self.field_receivers.insert((output, receiver));
    }

    /// Remember variables the roots contained before they merged into `Error`.
    pub(crate) fn strand(
        &mut self,
        uf: &mut UnionFind<'_>,
        roots: impl IntoIterator<Item = Variable>,
    ) {
        let lost = self.detached(uf, roots);
        #[cfg(any(test, debug_assertions))]
        self.stranded_handles.extend(&lost);
        self.stranded.extend(lost);
    }

    /// Call `sync` after the last merge before a lookup.
    pub(crate) fn is_stranded(&self, uf: &mut UnionFind<'_>, variable: Variable) -> bool {
        debug_assert_eq!(
            self.merged,
            uf.merges().len(),
            "stranded lookup after an unsynced merge"
        );
        let found = self.stranded.contains(&uf.find(variable));
        #[cfg(debug_assertions)]
        assert_eq!(found, self.stranded_scan(uf, variable));
        found
    }

    /// ORACLE: the scan of every stranded variable that the lookup replaces.
    #[cfg(any(test, debug_assertions))]
    fn stranded_scan(&self, uf: &mut UnionFind<'_>, variable: Variable) -> bool {
        let variable = uf.find(variable);
        self.stranded_handles
            .iter()
            .any(|stranded| uf.find(*stranded) == variable)
    }

    pub(crate) fn invalidate(
        &mut self,
        uf: &mut UnionFind<'_>,
        roots: impl IntoIterator<Item = Variable>,
    ) {
        let variables = self.historical(uf, roots);
        poison_roots(uf, variables);
    }

    /// Nodes removed by normalization still belong to the original computation.
    pub(crate) fn detached(
        &mut self,
        uf: &mut UnionFind<'_>,
        roots: impl IntoIterator<Item = Variable>,
    ) -> BTreeSet<Variable> {
        let roots: Vec<_> = roots.into_iter().collect();
        let current = reachable(uf, roots.iter().copied());
        self.historical(uf, roots)
            .difference(&current)
            .copied()
            .collect()
    }

    /// Everything reachable through current structure or recorded containment.
    fn historical(
        &mut self,
        uf: &mut UnionFind<'_>,
        roots: impl IntoIterator<Item = Variable>,
    ) -> BTreeSet<Variable> {
        self.sync(uf);
        let roots: Vec<_> = roots.into_iter().collect();
        let mut pending = roots.clone();
        let mut variables = BTreeSet::new();
        while let Some(variable) = pending.pop() {
            let variable = uf.find(variable);
            if !variables.insert(variable) {
                continue;
            }
            pending.extend(children(&uf.get(variable).content));
            for parent in self.parents.get(&variable).into_iter().flatten() {
                pending.extend(self.edges[parent].iter().copied());
            }
        }
        #[cfg(debug_assertions)]
        assert_eq!(variables, self.historical_scan(uf, roots));
        variables
    }

    /// ORACLE: the fixed-point scan that `historical` replaces.
    #[cfg(any(test, debug_assertions))]
    fn historical_scan(&self, uf: &mut UnionFind<'_>, roots: Vec<Variable>) -> BTreeSet<Variable> {
        let mut variables = reachable(uf, roots);
        loop {
            let before = variables.len();
            for (parent, recorded) in &self.edges {
                if variables.contains(&uf.find(*parent)) {
                    for child in recorded {
                        variables.extend(reachable(uf, [*child]));
                    }
                }
            }
            if variables.len() == before {
                break;
            }
        }
        variables
    }
}

fn children(content: &Content<'_>) -> Vec<Variable> {
    match content {
        Content::Alias { args, real, .. } => {
            args.iter().map(|(_, var)| *var).chain([*real]).collect()
        }
        Content::PartialAlias { args, .. } => args.iter().map(|(_, var)| *var).collect(),
        Content::Structure(FlatType::App1(_, _, args)) => args.clone(),
        Content::Structure(FlatType::AppV1(head, args)) => {
            [*head].into_iter().chain(args.iter().copied()).collect()
        }
        Content::Structure(FlatType::Fun1(from, to)) => vec![*from, *to],
        Content::Structure(FlatType::Tuple1(first, second, rest)) => [*first, *second]
            .into_iter()
            .chain(rest.iter().copied())
            .collect(),
        Content::Structure(FlatType::Record1(fields)) => fields.values().copied().collect(),
        Content::FlexVar(_) | Content::RigidVar(_) | Content::Error => Vec::new(),
    }
}

pub(crate) fn reachable(
    uf: &mut UnionFind<'_>,
    roots: impl IntoIterator<Item = Variable>,
) -> BTreeSet<Variable> {
    let mut pending: Vec<_> = roots.into_iter().collect();
    let mut seen = BTreeSet::new();
    while let Some(variable) = pending.pop() {
        let variable = uf.find(variable);
        if !seen.insert(variable) {
            continue;
        }
        pending.extend(children(&uf.get(variable).content));
    }
    seen
}

pub(crate) fn is_poisoned(
    uf: &mut UnionFind<'_>,
    roots: impl IntoIterator<Item = Variable>,
) -> bool {
    reachable(uf, roots)
        .into_iter()
        .any(|variable| matches!(uf.get(variable).content, Content::Error))
}

pub(crate) fn poison(uf: &mut UnionFind<'_>, roots: impl IntoIterator<Item = Variable>) -> bool {
    let variables = reachable(uf, roots);
    poison_roots(uf, variables)
}

/// A dependent expression is unavailable, but its siblings remain trustworthy.
pub(crate) fn poison_roots(
    uf: &mut UnionFind<'_>,
    roots: impl IntoIterator<Item = Variable>,
) -> bool {
    let mut changed = false;
    for variable in roots {
        if !matches!(uf.get(variable).content, Content::Error) {
            uf.modify(variable, |desc| desc.content = Content::Error);
            changed = true;
        }
    }
    changed
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use nash_constrain::type_::{make_descriptor, mk_flex_var};

    /// Fixed-seed generator, so the randomized tests need no dependency.
    pub(crate) struct Lcg(pub u64);

    impl Lcg {
        pub(crate) fn below(&mut self, bound: usize) -> usize {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (self.0 >> 33) as usize % bound
        }

        pub(crate) fn pick(&mut self, variables: &[Variable]) -> Variable {
            variables[self.below(variables.len())]
        }
    }

    #[test]
    fn historical_closure_and_stranded_lookup_agree_with_the_scans() {
        let (mut closures, mut stranded, mut unstranded) = (0, 0, 0);
        for seed in 0..300 {
            let mut random = Lcg(seed);
            let mut uf = UnionFind::new();
            let mut dependencies = Dependencies::default();
            let mut variables: Vec<_> = (0..4).map(|_| mk_flex_var(&mut uf)).collect();
            for _ in 0..200 {
                match random.below(9) {
                    0 => variables.push(mk_flex_var(&mut uf)),
                    1 | 2 => {
                        let children = (random.pick(&variables), random.pick(&variables));
                        variables.push(uf.fresh(make_descriptor(Content::Structure(
                            FlatType::Fun1(children.0, children.1),
                        ))));
                    }
                    3 => dependencies.remember(&mut uf, [random.pick(&variables)]),
                    4 => {
                        // Unification keeps the content of one side.
                        let (first, second) = (random.pick(&variables), random.pick(&variables));
                        let kept = if random.below(2) == 0 { first } else { second };
                        let content = uf.get(kept).content.clone();
                        uf.union(first, second, make_descriptor(content));
                    }
                    5 => {
                        // Normalization removes the visible children of a node.
                        let variable = random.pick(&variables);
                        uf.modify(variable, |desc| desc.content = Content::FlexVar(None));
                    }
                    6 => dependencies.strand(&mut uf, [random.pick(&variables)]),
                    7 => {
                        dependencies.sync(&uf);
                        let variable = random.pick(&variables);
                        let scan = dependencies.stranded_scan(&mut uf, variable);
                        assert_eq!(dependencies.is_stranded(&mut uf, variable), scan);
                        if scan {
                            stranded += 1;
                        } else {
                            unstranded += 1;
                        }
                    }
                    _ => {
                        let roots = vec![random.pick(&variables), random.pick(&variables)];
                        let scan = dependencies.historical_scan(&mut uf, roots.clone());
                        assert_eq!(dependencies.historical(&mut uf, roots), scan);
                        closures += 1;
                    }
                }
            }
        }
        assert!(closures > 5_000);
        assert!(stranded > 500 && unstranded > 500);
    }
}
