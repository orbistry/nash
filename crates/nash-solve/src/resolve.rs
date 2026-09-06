//! Coherent impl selection from known inference heads. Never binds a variable
//! to make an impl match; unresolved heads must wait for type inference.
use bumpalo::Bump;
use nash_ast::{Head, HeadCon, ImplKey, QualifiedName};
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
            Content::Structure(FlatType::Record1(fields, ext)) => {
                pending.extend(fields.values().map(|var| (*var, outer)));
                pending.push((*ext, outer));
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
    Deferred,
    Missing,
    Impl {
        info: &'a ImplInfo<'a>,
        key: ImplKey<'a>,
        substitution: Vec<(&'a str, Variable)>,
    },
}

pub(crate) fn head_vars<'a>(head: &Head<'a>) -> &'a [&'a str] {
    match head {
        Head::Named { vars, .. } | Head::Tuple(vars) => vars,
        Head::Unit => &[],
    }
}

pub(crate) fn select<'a>(
    bump: &'a Bump,
    tables: &Tables<'a>,
    uf: &mut UnionFind<'a>,
    trait_: QualifiedName<'a>,
    args: &[Variable],
) -> Selection<'a> {
    let mut heads = Vec::new();
    let mut arguments = Vec::new();
    let mut unknown = false;
    for arg in args {
        if let Some(alias) = nash_constrain::instantiate::alias_application(uf, *arg) {
            heads.push(HeadCon::Named(QualifiedName {
                home: alias.home,
                name: alias.name,
            }));
            arguments.push(alias.args.into_iter().map(|(_, var)| var).collect());
            continue;
        }
        let content = uf.get(*arg).content.clone();
        let content = match content {
            Content::Structure(flat) => {
                Content::Structure(nash_constrain::type_::normalize_application(uf, flat))
            }
            content => content,
        };
        let (head, args) = match &content {
            Content::Structure(FlatType::App1(home, name, args)) => (
                HeadCon::Named(QualifiedName { home: *home, name }),
                args.clone(),
            ),
            Content::Alias {
                home, name, args, ..
            }
            | Content::PartialAlias {
                home, name, args, ..
            } => (
                HeadCon::Named(QualifiedName { home: *home, name }),
                args.iter().map(|(_, var)| *var).collect(),
            ),
            Content::Structure(FlatType::Unit1) => (HeadCon::Unit, Vec::new()),
            Content::Structure(FlatType::Tuple1(a, b, c)) => (
                HeadCon::Tuple(if c.is_some() { 3 } else { 2 }),
                [*a, *b].into_iter().chain(*c).collect(),
            ),
            Content::Structure(
                FlatType::Fun1(..) | FlatType::Record1(..) | FlatType::EmptyRecord1,
            ) => return Selection::Missing,
            _ => {
                unknown = true;
                continue;
            }
        };
        heads.push(head);
        arguments.push(args);
    }
    if unknown {
        return Selection::Deferred;
    }
    let key = ImplKey {
        trait_,
        heads: bump.alloc_slice_copy(&heads),
    };
    let Some(info) = tables.impls.get(&key).copied() else {
        return Selection::Missing;
    };
    let mut substitution = Vec::new();
    for (head, args) in info.heads.iter().zip(arguments) {
        let vars = head_vars(&head.value);
        // Higher-kinded heads may be partially applied. Matching the constructor
        // alone must not silently discard supplied or unsupplied arguments.
        if vars.len() != args.len() {
            return Selection::Missing;
        }
        substitution.extend(vars.iter().copied().zip(args));
    }
    Selection::Impl {
        info,
        key,
        substitution,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nash_constrain::type_::make_descriptor;

    #[test]
    fn outer_structure_carries_its_rank_to_unadjusted_children() {
        let mut uf = UnionFind::new();
        let mut child_desc = make_descriptor(Content::FlexVar(None));
        child_desc.rank = 3;
        let child = uf.fresh(child_desc);
        let mut outer_desc =
            make_descriptor(Content::Structure(FlatType::Tuple1(child, child, None)));
        outer_desc.rank = 2;
        let outer = uf.fresh(outer_desc);
        assert!(has_outer_flex(&mut uf, &[outer], 3));
        uf.modify(child, |desc| {
            desc.content = Content::Structure(FlatType::Unit1)
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
