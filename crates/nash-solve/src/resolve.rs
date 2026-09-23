//! Coherent selection from known heads and read-only probes for relational
//! equalities. The solver commits only a unique candidate's equalities.
use nash_ast::{HeadCon, ImplKey, QualifiedName};
use nash_can::environment::{ImplInfo, Tables};
use nash_constrain::{Content, FlatType, UnionFind, Variable};

/// Captured variables can still be fixed by an enclosing annotation after an
/// inner definition has been checked. Its givens must get first refusal then.
pub(crate) fn has_outer_flex(uf: &mut UnionFind<'_>, args: &[Variable], rank: usize) -> bool {
    let mut pending: Vec<_> = args.iter().map(|var| (*var, false)).collect();
    let mut seen = std::collections::BTreeSet::new();
    while let Some((var, inherited_outer)) = pending.pop() {
        if !seen.insert((uf.find(var), inherited_outer)) {
            continue;
        }
        let desc = uf.get(var);
        let outer =
            inherited_outer || (desc.rank != nash_constrain::type_::NO_RANK && desc.rank < rank);
        match &desc.content {
            Content::FlexVar(_) if outer => return true,
            Content::Structure(FlatType::AppV1(head, args)) => {
                pending.push((*head, outer));
                pending.extend(args.iter().map(|var| (*var, outer)));
            }
            Content::Structure(FlatType::App1(_, _, args)) => {
                pending.extend(args.iter().map(|var| (*var, outer)))
            }
            Content::Structure(FlatType::Fun1(a, b)) => pending.extend([(*a, outer), (*b, outer)]),
            Content::Structure(FlatType::Tuple1(a, b, c)) => {
                pending.extend([(*a, outer), (*b, outer)]);
                pending.extend(c.iter().map(|var| (*var, outer)));
            }
            Content::Structure(FlatType::Record1(fields)) => {
                pending.extend(fields.values().map(|var| (*var, outer)));
            }
            Content::Alias { args, .. } | Content::PartialAlias { args, .. } => {
                pending.extend(args.iter().map(|(_, var)| (*var, outer)))
            }
            _ => {}
        }
    }
    false
}

pub(crate) enum Selection<'a> {
    Limit,
    Deferred,
    Missing,
    Impl {
        info: &'a ImplInfo<'a>,
        key: ImplKey<'a>,
        substitution: Vec<(&'a str, Variable)>,
    },
}

#[derive(PartialEq, Eq)]
enum View<'a> {
    Named(QualifiedName<'a>),
    Tuple(usize),
    Function,
    Record(Vec<&'a str>),
    Application,
    Flexible,
    Rigid(Variable),
    Error,
}

struct InferenceTypes<'u, 'a>(&'u mut UnionFind<'a>);

impl<'a> InferenceTypes<'_, 'a> {
    fn view(&mut self, variable: Variable) -> (View<'a>, Vec<Variable>) {
        if let Some(alias) = nash_constrain::instantiate::alias_application(self.0, variable) {
            return (
                View::Named(QualifiedName {
                    home: alias.home,
                    name: alias.name,
                }),
                alias.args.into_iter().map(|(_, var)| var).collect(),
            );
        }
        let content = self.0.get(variable).content.clone();
        let content = match content {
            Content::Structure(flat) => {
                Content::Structure(nash_constrain::type_::normalize_application(self.0, flat))
            }
            content => content,
        };
        match content {
            Content::Structure(FlatType::App1(home, name, args)) => {
                (View::Named(QualifiedName { home, name }), args)
            }
            Content::Alias {
                home, name, args, ..
            }
            | Content::PartialAlias {
                home, name, args, ..
            } => (
                View::Named(QualifiedName { home, name }),
                args.into_iter().map(|(_, var)| var).collect(),
            ),

            Content::Structure(FlatType::Tuple1(a, b, rest)) => (
                View::Tuple(2 + rest.len()),
                [a, b].into_iter().chain(rest).collect(),
            ),
            Content::Structure(FlatType::Fun1(a, b)) => (View::Function, vec![a, b]),
            Content::Structure(FlatType::Record1(fields)) => (
                View::Record(fields.keys().copied().collect()),
                fields.into_values().collect(),
            ),
            Content::Structure(FlatType::AppV1(head, args)) => {
                (View::Application, [head].into_iter().chain(args).collect())
            }
            Content::FlexVar(_) => (View::Flexible, Vec::new()),
            Content::RigidVar(_) => (View::Rigid(self.0.find(variable)), Vec::new()),
            Content::Error => (View::Error, Vec::new()),
        }
    }
}

impl<'a> nash_ast::head::Types<'a> for InferenceTypes<'_, 'a> {
    type Node = Variable;
    fn in_class(
        &mut self,
        _: Variable,
        _: nash_ast::primitives::ReprSet,
    ) -> nash_ast::head::Match<()> {
        unreachable!("classed matching uses ClassedTypes")
    }
    fn constructor(
        &mut self,
        node: Variable,
        expected: HeadCon<'a>,
    ) -> nash_ast::head::Match<Vec<Variable>> {
        use nash_ast::head::Match;
        let (view, args) = self.view(node);
        let actual = match view {
            View::Named(name) => HeadCon::Named(name),

            View::Tuple(arity) => HeadCon::Tuple(arity),
            View::Function => HeadCon::Fun,
            View::Application | View::Flexible | View::Error => return Match::Deferred,
            View::Rigid(_) | View::Record(_) => return Match::No,
        };
        if actual == expected {
            Match::Yes(args)
        } else {
            Match::No
        }
    }
    fn equal(
        &mut self,
        first: Variable,
        second: Variable,
        remaining: &mut usize,
    ) -> Result<nash_ast::head::Match<()>, nash_ast::head::Limit> {
        use nash_ast::head::{Match, step};
        let mut pending = vec![(first, second)];
        let mut deferred = false;
        while let Some((first, second)) = pending.pop() {
            step(remaining)?;
            if self.0.find(first) == self.0.find(second) {
                continue;
            }
            let (a, aa) = self.view(first);
            let (b, ba) = self.view(second);
            if matches!(a, View::Flexible | View::Error)
                || matches!(b, View::Flexible | View::Error)
            {
                deferred = true;
            } else if a != b || aa.len() != ba.len() {
                if matches!(a, View::Application) || matches!(b, View::Application) {
                    deferred = true;
                } else {
                    return Ok(Match::No);
                }
            } else {
                pending.extend(aa.into_iter().zip(ba));
            }
        }
        Ok(if deferred {
            Match::Deferred
        } else {
            Match::Yes(())
        })
    }
}

struct ClassedTypes<'u, 'a> {
    types: InferenceTypes<'u, 'a>,
    tables: &'u Tables<'a>,
    givens: &'u [crate::preds::Body<'a>],
    allocated: &'u mut Vec<Variable>,
}
impl<'a> nash_ast::head::Types<'a> for ClassedTypes<'_, 'a> {
    type Node = Variable;
    fn constructor(
        &mut self,
        node: Variable,
        expected: HeadCon<'a>,
    ) -> nash_ast::head::Match<Vec<Variable>> {
        self.types.constructor(node, expected)
    }
    fn equal(
        &mut self,
        a: Variable,
        b: Variable,
        remaining: &mut usize,
    ) -> Result<nash_ast::head::Match<()>, nash_ast::head::Limit> {
        self.types.equal(a, b, remaining)
    }
    fn in_class(
        &mut self,
        node: Variable,
        class: nash_ast::primitives::ReprSet,
    ) -> nash_ast::head::Match<()> {
        use nash_ast::{
            head::Match,
            primitives::{ReprSet, ReprTrait},
        };
        let uf = &mut *self.types.0;
        let subject = crate::representation::subject(uf, &self.tables.kinds, node, self.allocated);
        if let Some(actual) =
            crate::representation::known(uf, &self.tables.kinds, subject, self.allocated)
        {
            return if class.contains(actual) {
                Match::Yes(())
            } else {
                Match::No
            };
        }
        let mut proven = ReprSet::ALL;
        for given in self.givens {
            if let crate::preds::Body::Trait { trait_, args, .. } = given
                && let Some(required) = ReprTrait::of(*trait_)
            {
                let arg =
                    crate::representation::subject(uf, &self.tables.kinds, args[0], self.allocated);
                if crate::preds::same_args(uf, &[arg], &[subject]) {
                    proven = proven.intersect(required.admits());
                }
            }
        }
        if !proven.is_all() && proven.intersect(class) == proven {
            return Match::Yes(());
        }
        match self.types.view(subject).0 {
            View::Flexible | View::Application | View::Error => Match::Deferred,
            _ => Match::No,
        }
    }
}

pub(crate) fn in_class<'a>(
    tables: &Tables<'a>,
    uf: &mut UnionFind<'a>,
    node: Variable,
    class: nash_ast::primitives::ReprSet,
    givens: &[crate::preds::Body<'a>],
    allocated: &mut Vec<Variable>,
) -> nash_ast::head::Match<()> {
    use nash_ast::head::Types;
    ClassedTypes {
        types: InferenceTypes(uf),
        tables,
        givens,
        allocated,
    }
    .in_class(node, class)
}

pub(crate) fn select<'a>(
    tables: &Tables<'a>,
    uf: &mut UnionFind<'a>,
    trait_: QualifiedName<'a>,
    args: &[Variable],
    givens: &[crate::preds::Body<'a>],
    allocated: &mut Vec<Variable>,
) -> Selection<'a> {
    use nash_ast::head::{Match, matches};
    if args.len() >= 2 {
        match given_candidates(tables, uf, trait_, args, givens, allocated, &mut 16_384) {
            None => return Selection::Limit,
            Some(candidates) if !candidates.is_empty() => return Selection::Deferred,
            Some(_) => {}
        }
    }
    let mut types = ClassedTypes {
        types: InferenceTypes(uf),
        tables,
        givens,
        allocated,
    };
    let unknown_outer = args.iter().any(|arg| {
        matches!(
            types.types.view(*arg).0,
            View::Flexible | View::Rigid(_) | View::Application | View::Error
        )
    });
    let mut remaining = 16_384;
    let mut selected = None;
    let mut deferred = false;
    for (key, info) in tables.impls_for(trait_) {
        match matches(
            &mut types,
            key.heads,
            args,
            info.variables.len(),
            &mut remaining,
        ) {
            Err(_) => return Selection::Limit,
            Ok(Match::No) => {}
            Ok(Match::Deferred) => deferred = true,
            Ok(Match::Yes(arguments)) => {
                selected = Some(Selection::Impl {
                    info,
                    key: *key,
                    substitution: info.variables.iter().copied().zip(arguments).collect(),
                });
            }
        }
    }
    if deferred || (selected.is_none() && unknown_outer) {
        Selection::Deferred
    } else {
        selected.unwrap_or(Selection::Missing)
    }
}

/// Probe equalities without changing the inference graph. A candidate may relate
/// existing variables, but cannot manufacture a missing constructor application.
struct Probe<'u, 'a> {
    types: ClassedTypes<'u, 'a>,
    substitutions: std::collections::BTreeMap<Variable, Variable>,
    equations: Vec<(Variable, Variable)>,
    classes: Vec<(Variable, nash_ast::primitives::ReprSet)>,
}

impl<'a> Probe<'_, 'a> {
    fn root(&mut self, mut var: Variable) -> Variable {
        loop {
            var = self.types.types.0.find(var);
            match self.substitutions.get(&var) {
                Some(next) => var = *next,
                None => return var,
            }
        }
    }

    fn occurs(
        &mut self,
        needle: Variable,
        value: Variable,
        remaining: &mut usize,
    ) -> Result<bool, nash_ast::head::Limit> {
        let mut pending = vec![value];
        let mut seen = std::collections::BTreeSet::new();
        while let Some(value) = pending.pop() {
            nash_ast::head::step(remaining)?;
            let value = self.root(value);
            if value == needle {
                return Ok(true);
            }
            if seen.insert(value) {
                pending.extend(self.types.types.view(value).1);
            }
        }
        Ok(false)
    }

    fn classes_match(&mut self) -> nash_ast::head::Match<()> {
        use nash_ast::head::{Match, Types};
        let mut deferred = false;
        for (node, class) in self.classes.clone() {
            let node = self.root(node);
            match self.types.in_class(node, class) {
                Match::No => return Match::No,
                Match::Deferred => deferred = true,
                Match::Yes(()) => {}
            }
        }
        if deferred {
            Match::Deferred
        } else {
            Match::Yes(())
        }
    }
}

impl<'a> nash_ast::head::Types<'a> for Probe<'_, 'a> {
    type Node = Variable;
    fn constructor(
        &mut self,
        node: Variable,
        expected: HeadCon<'a>,
    ) -> nash_ast::head::Match<Vec<Variable>> {
        let node = self.root(node);
        self.types.constructor(node, expected)
    }
    fn in_class(
        &mut self,
        node: Variable,
        class: nash_ast::primitives::ReprSet,
    ) -> nash_ast::head::Match<()> {
        self.classes.push((node, class));
        nash_ast::head::Match::Yes(())
    }
    fn equal(
        &mut self,
        a: Variable,
        b: Variable,
        remaining: &mut usize,
    ) -> Result<nash_ast::head::Match<()>, nash_ast::head::Limit> {
        use nash_ast::head::Match;
        let mut pending = vec![(a, b)];
        let mut deferred = false;
        while let Some((a, b)) = pending.pop() {
            nash_ast::head::step(remaining)?;
            let a = self.root(a);
            let b = self.root(b);
            if a == b {
                continue;
            }
            let (av, aa) = self.types.types.view(a);
            let (bv, ba) = self.types.types.view(b);
            if av == View::Flexible || bv == View::Flexible {
                let (var, value) = if av == View::Flexible { (a, b) } else { (b, a) };
                if self.occurs(var, value, remaining)? {
                    return Ok(Match::No);
                }
                self.substitutions.insert(var, value);
                self.equations.push((var, value));
            } else if matches!(av, View::Application | View::Error)
                || matches!(bv, View::Application | View::Error)
            {
                deferred = true;
            } else if av != bv || aa.len() != ba.len() {
                // Transparent aliases can unify with their bodies. Do not rule
                // out a competing dictionary just because nominal views differ.
                let transparent = |uf: &mut UnionFind<'a>, node| {
                    nash_constrain::instantiate::alias_application(uf, node).is_some_and(|alias| {
                        alias.remaining.is_empty()
                            && !matches!(alias.body.value, nash_ast::Type::Record { .. })
                    })
                };
                if transparent(self.types.types.0, a) || transparent(self.types.types.0, b) {
                    deferred = true;
                } else {
                    return Ok(Match::No);
                }
            } else {
                pending.extend(aa.into_iter().zip(ba));
            }
        }
        Ok(if deferred {
            Match::Deferred
        } else {
            Match::Yes(())
        })
    }
}

type Equations = Vec<(Variable, Variable)>;
type Candidates = Vec<Option<Equations>>;

fn given_candidates<'a>(
    tables: &Tables<'a>,
    uf: &mut UnionFind<'a>,
    trait_: QualifiedName<'a>,
    args: &[Variable],
    givens: &[crate::preds::Body<'a>],
    allocated: &mut Vec<Variable>,
    remaining: &mut usize,
) -> Option<Candidates> {
    use nash_ast::head::{Match, Types};
    let mut candidates = Vec::new();
    let mut seen: Vec<&crate::preds::Body<'a>> = Vec::new();
    // Local dictionaries take priority even when their hidden parameters still
    // need equalities before ordinary given lookup can recognize them.
    for given in givens {
        let crate::preds::Body::Trait {
            trait_: name,
            args: supplied,
            ..
        } = given
        else {
            continue;
        };
        if *name != trait_ || supplied.len() != args.len() {
            continue;
        }
        if seen.iter().any(|previous| previous.same(uf, given)) {
            continue;
        }
        seen.push(given);
        let mut probe = Probe {
            types: ClassedTypes {
                types: InferenceTypes(uf),
                tables,
                givens,
                allocated,
            },
            substitutions: Default::default(),
            equations: Vec::new(),
            classes: Vec::new(),
        };
        let mut matched = Match::Yes(());
        for (a, b) in args.iter().zip(supplied) {
            match probe.equal(*a, *b, remaining).ok()? {
                Match::No => {
                    matched = Match::No;
                    break;
                }
                Match::Deferred => matched = Match::Deferred,
                Match::Yes(()) => {}
            }
        }
        match matched {
            Match::No => {}
            Match::Deferred => candidates.push(None),
            Match::Yes(()) => candidates.push(Some(probe.equations)),
        }
    }
    Some(candidates)
}

/// Only a unique viable candidate may improve a multi-parameter constraint.
/// Deferred competitors count; ordinary prerequisites never exclude a candidate.
pub(crate) fn improvement<'a>(
    tables: &Tables<'a>,
    uf: &mut UnionFind<'a>,
    trait_: QualifiedName<'a>,
    args: &[Variable],
    givens: &[crate::preds::Body<'a>],
    allocated: &mut Vec<Variable>,
) -> Option<Vec<(Variable, Variable)>> {
    use nash_ast::head::{Match, Types};
    if args.len() < 2 {
        return None;
    }
    let mut remaining = 16_384;
    let mut candidates =
        given_candidates(tables, uf, trait_, args, givens, allocated, &mut remaining)?;
    if !candidates.is_empty() {
        return if candidates.len() == 1 {
            candidates.pop().flatten().filter(|e| !e.is_empty())
        } else {
            None
        };
    }
    for (key, info) in tables.impls_for(trait_) {
        let mut probe = Probe {
            types: ClassedTypes {
                types: InferenceTypes(uf),
                tables,
                givens,
                allocated,
            },
            substitutions: Default::default(),
            equations: Vec::new(),
            classes: Vec::new(),
        };
        let matched = nash_ast::head::matches(
            &mut probe,
            key.heads,
            args,
            info.variables.len(),
            &mut remaining,
        )
        .ok()?;
        if matches!(matched, Match::No) {
            continue;
        }
        let classes = probe.classes_match();
        if matches!(classes, Match::No) {
            continue;
        }
        let ready = matches!(matched, Match::Yes(_)) && matches!(classes, Match::Yes(()));
        candidates.push(ready.then_some(probe.equations));
    }
    if trait_ == nash_ast::primitives::lift_trait()
        && tables.has_reflexive_lift()
        && args.len() == 2
    {
        let mut probe = Probe {
            types: ClassedTypes {
                types: InferenceTypes(uf),
                tables,
                givens,
                allocated,
            },
            substitutions: Default::default(),
            equations: Vec::new(),
            classes: Vec::new(),
        };
        match probe.equal(args[0], args[1], &mut remaining).ok()? {
            Match::No => {}
            Match::Deferred => candidates.push(None),
            Match::Yes(()) => candidates.push(Some(probe.equations)),
        }
    }
    if candidates.len() != 1 {
        return None;
    }
    candidates
        .pop()
        .flatten()
        .filter(|equations| !equations.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nash_constrain::type_::make_descriptor;

    #[test]
    fn repeated_patterns_compare_closed_records_without_unifying_them() {
        use nash_ast::head::{Match, Types};
        let mut uf = UnionFind::new();
        let unit = uf.fresh(make_descriptor(Content::Structure(FlatType::App1(
            nash_ast::primitives::primitive_home(),
            "unit",
            Vec::new(),
        ))));
        let unknown = uf.fresh(make_descriptor(Content::FlexVar(None)));
        let first = uf.fresh(make_descriptor(Content::Structure(FlatType::Record1(
            [("a", unit), ("b", unit)].into(),
        ))));
        let second = uf.fresh(make_descriptor(Content::Structure(FlatType::Record1(
            [("b", unit), ("a", unit)].into(),
        ))));
        let fewer = uf.fresh(make_descriptor(Content::Structure(FlatType::Record1(
            [("a", unit)].into(),
        ))));
        let flexible = uf.fresh(make_descriptor(Content::Structure(FlatType::Record1(
            [("a", unknown), ("b", unit)].into(),
        ))));
        let mut types = InferenceTypes(&mut uf);
        assert!(matches!(
            types.equal(first, second, &mut 100),
            Ok(Match::Yes(()))
        ));
        assert!(matches!(types.equal(first, fewer, &mut 100), Ok(Match::No)));
        assert!(matches!(
            types.equal(first, flexible, &mut 100),
            Ok(Match::Deferred)
        ));
        assert_ne!(types.0.find(first), types.0.find(second));
        assert!(matches!(types.0.get(unknown).content, Content::FlexVar(_)));
    }
    #[test]
    fn outer_structure_carries_its_rank_to_unadjusted_children() {
        let mut uf = UnionFind::new();
        let mut child_desc = make_descriptor(Content::FlexVar(None));
        child_desc.rank = 3;
        let child = uf.fresh(child_desc);
        let mut outer_desc =
            make_descriptor(Content::Structure(FlatType::Tuple1(child, child, vec![])));
        outer_desc.rank = 2;
        let outer = uf.fresh(outer_desc);
        assert!(has_outer_flex(&mut uf, &[outer], 3));
        uf.modify(child, |desc| {
            desc.content = Content::Structure(FlatType::App1(
                nash_ast::primitives::primitive_home(),
                "unit",
                Vec::new(),
            ))
        });
        assert!(
            !has_outer_flex(&mut uf, &[outer], 3),
            "known ground arguments do not wait"
        );
        let generic = uf.fresh(make_descriptor(Content::FlexVar(None)));
        assert!(
            !has_outer_flex(&mut uf, &[generic], 3),
            "NO_RANK denotes a generalized variable"
        );
    }
}
