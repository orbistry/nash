//! Dependency closure for failed inference computations.
//!
//! Failed comparisons invalidate changed shared variables. Containment carries
//! that failure to parents without invalidating independent siblings.

use std::collections::BTreeSet;

use nash_constrain::{Content, FlatType, UnionFind, Variable};

#[derive(Default)]
pub(crate) struct Dependencies {
    edges: BTreeSet<(Variable, Variable)>,
    computations: BTreeSet<(Variable, Variable)>,
    field_receivers: BTreeSet<(Variable, Variable)>,
}

impl Dependencies {
    /// Remember containment before normalization or union can remove an edge.
    /// Original variable handles remain valid after their representatives move.
    pub(crate) fn remember(
        &mut self,
        uf: &mut UnionFind<'_>,
        roots: impl IntoIterator<Item = Variable>,
    ) {
        let mut pending: Vec<_> = roots.into_iter().collect();
        let mut seen = BTreeSet::new();
        while let Some(variable) = pending.pop() {
            let variable = uf.find(variable);
            if !seen.insert(variable) {
                continue;
            }
            for child in children(&uf.get(variable).content) {
                let child = uf.find(child);
                self.edges.insert((variable, child));
                pending.push(child);
            }
        }
    }

    pub(crate) fn propagate(&self, uf: &mut UnionFind<'_>) -> bool {
        let mut changed = false;
        for &(parent, child) in self.edges.iter().chain(&self.computations) {
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

    pub(crate) fn invalidate(
        &self,
        uf: &mut UnionFind<'_>,
        roots: impl IntoIterator<Item = Variable>,
    ) {
        let variables = self.historical(uf, roots);
        poison_roots(uf, variables);
    }

    /// Nodes removed by normalization still belong to the original computation.
    pub(crate) fn detached(
        &self,
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

    fn historical(
        &self,
        uf: &mut UnionFind<'_>,
        roots: impl IntoIterator<Item = Variable>,
    ) -> BTreeSet<Variable> {
        let mut variables = reachable(uf, roots);
        loop {
            let before = variables.len();
            for &(parent, child) in &self.edges {
                if variables.contains(&uf.find(parent)) {
                    variables.extend(reachable(uf, [child]));
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
