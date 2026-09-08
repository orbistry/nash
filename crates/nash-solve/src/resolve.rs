//! Coherent impl selection from known inference heads. Never binds a variable
//! to make an impl match; unresolved heads must wait for type inference.
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

pub(crate) fn select<'a>(
    tables: &Tables<'a>,
    uf: &mut UnionFind<'a>,
    trait_: QualifiedName<'a>,
    args: &[Variable],
) -> Selection<'a> {
    use nash_ast::head::{Match, matches};
    let mut types = InferenceTypes(uf);
    if args
        .iter()
        .any(|arg| matches!(types.view(*arg).0, View::Record(_) | View::Function))
    {
        return Selection::Missing;
    }
    let unknown_outer = args.iter().any(|arg| {
        matches!(
            types.view(*arg).0,
            View::Flexible | View::Rigid(_) | View::Application | View::Error
        )
    });
    let mut remaining = 16_384;
    let mut selected = None;
    let mut deferred = false;
    for (key, info) in tables.impls.iter().filter(|(key, _)| key.trait_ == trait_) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use nash_constrain::type_::make_descriptor;

    #[test]
    fn repeated_patterns_compare_closed_records_without_unifying_them() {
        use nash_ast::head::{Match, Types};
        let mut uf = UnionFind::new();
        let unit = uf.fresh(make_descriptor(Content::Structure(FlatType::App1(
            nash_ast::primitives::builtin_home(),
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
                nash_ast::primitives::builtin_home(),
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
