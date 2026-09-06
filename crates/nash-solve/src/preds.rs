//! Solver-owned predicate bodies. Descriptor IDs refer into this store;
//! source provenance stays attached when a predicate is retained in a scheme.

use nash_ast::{NodeId, QualifiedName};
use nash_constrain::type_::PredId;
use nash_constrain::{Content, FlatType};
use nash_constrain::{UnionFind, Variable};
use nash_region::Region;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug)]
pub struct UseSite<'a> {
    pub node: NodeId,
    pub region: Region,
    pub name: &'a str,
}

#[derive(Clone, Debug)]
pub enum Origin<'a> {
    Use { site: UseSite<'a>, index: usize },
    Annotation { binder: NodeId, index: usize },
    Sub { parent: PredId, index: usize },
}

#[derive(Clone, Debug)]
pub struct Predicate<'a> {
    pub trait_: QualifiedName<'a>,
    pub args: Vec<Variable>,
    pub origin: Origin<'a>,
    pub solution: Option<Solution<'a>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Solution<'a> {
    ReflexiveLift {
        typ: Variable,
    },
    Impl {
        impl_: nash_ast::ImplRef<'a>,
        type_vars: Vec<Variable>,
        subs: Vec<PredId>,
    },
    Given {
        binder: NodeId,
        index: usize,
    },
    Super {
        binder: NodeId,
        index: usize,
        path: Vec<usize>,
    },
}

#[derive(Default)]
pub struct Store<'a> {
    predicates: Vec<Predicate<'a>>,
}

impl<'a> Store<'a> {
    pub fn use_site(&self, mut id: PredId) -> Option<UseSite<'a>> {
        loop {
            match self.get(id).origin {
                Origin::Use { site, .. } => return Some(site),
                Origin::Sub { parent, .. } => id = parent,
                Origin::Annotation { .. } => return None,
            }
        }
    }

    pub fn solve(&mut self, uf: &mut UnionFind<'a>, id: PredId, solution: Solution<'a>) {
        let predicate = &mut self.predicates[id.0 as usize];
        predicate.solution = Some(solution);
        self.detach(uf, id);
    }

    pub(crate) fn detach(&self, uf: &mut UnionFind<'a>, id: PredId) {
        for arg in &self.get(id).args {
            uf.modify(*arg, |desc| desc.preds.retain(|pending| *pending != id));
        }
    }
    pub fn iter(&self) -> impl Iterator<Item = &Predicate<'a>> {
        self.predicates.iter()
    }
    pub fn solve_given(
        &mut self,
        uf: &mut UnionFind<'a>,
        id: PredId,
        binder: NodeId,
        index: usize,
        path: Vec<usize>,
    ) {
        self.solve(
            uf,
            id,
            if path.is_empty() {
                Solution::Given { binder, index }
            } else {
                Solution::Super {
                    binder,
                    index,
                    path,
                }
            },
        );
    }
    pub fn get(&self, id: PredId) -> &Predicate<'a> {
        &self.predicates[id.0 as usize]
    }

    pub(crate) fn depth(&self, mut id: PredId) -> usize {
        let mut depth = 0;
        while let Origin::Sub { parent, .. } = self.get(id).origin {
            depth += 1;
            id = parent;
        }
        depth
    }

    pub fn push(&mut self, uf: &mut UnionFind<'a>, predicate: Predicate<'a>) -> PredId {
        let id = PredId(u32::try_from(self.predicates.len()).expect("predicate store exhausted"));
        for arg in &predicate.args {
            uf.modify(*arg, |desc| {
                if !desc.preds.contains(&id) {
                    desc.preds.push(id);
                }
            });
        }
        self.predicates.push(predicate);
        id
    }
}

/// Compare already-known types without solving a variable to make them fit.
pub(crate) fn same_args(uf: &mut UnionFind<'_>, left: &[Variable], right: &[Variable]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut pending: Vec<_> = left.iter().copied().zip(right.iter().copied()).collect();
    let mut seen = BTreeSet::new();
    while let Some((a, b)) = pending.pop() {
        let a = uf.find(a);
        let b = uf.find(b);
        if a == b || !seen.insert((a, b)) {
            continue;
        }
        if let (Some(a), Some(b)) = (
            nash_constrain::instantiate::alias_application(uf, a),
            nash_constrain::instantiate::alias_application(uf, b),
        ) {
            if a.home != b.home
                || a.name != b.name
                || a.remaining != b.remaining
                || a.args.len() != b.args.len()
            {
                return false;
            }
            pending.extend(
                a.args
                    .into_iter()
                    .zip(b.args)
                    .map(|((_, a), (_, b))| (a, b)),
            );
            continue;
        }
        fn normalize<'a>(uf: &mut UnionFind<'a>, var: Variable) -> Content<'a> {
            match uf.get(var).content.clone() {
                Content::Structure(flat) => {
                    Content::Structure(nash_constrain::type_::normalize_application(uf, flat))
                }
                content => content,
            }
        }
        let a_content = normalize(uf, a);
        let b_content = normalize(uf, b);
        match (a_content, b_content) {
            (
                Content::PartialAlias {
                    home: ha,
                    name: na,
                    args: aa,
                    remaining: ra,
                    ..
                },
                Content::PartialAlias {
                    home: hb,
                    name: nb,
                    args: ab,
                    remaining: rb,
                    ..
                },
            ) if ha == hb && na == nb && ra == rb && aa.len() == ab.len() => {
                pending.extend(aa.into_iter().zip(ab).map(|((_, a), (_, b))| (a, b)));
            }
            (
                Content::Structure(FlatType::AppV1(a, aa)),
                Content::Structure(FlatType::AppV1(b, ab)),
            ) if aa.len() == ab.len() => {
                pending.push((a, b));
                pending.extend(aa.into_iter().zip(ab));
            }
            (
                Content::Structure(FlatType::App1(ha, na, aa)),
                Content::Structure(FlatType::App1(hb, nb, ab)),
            ) if ha == hb && na == nb && aa.len() == ab.len() => {
                pending.extend(aa.into_iter().zip(ab))
            }
            (
                Content::Structure(FlatType::Fun1(a, b)),
                Content::Structure(FlatType::Fun1(c, d)),
            ) => pending.extend([(a, c), (b, d)]),
            (
                Content::Structure(FlatType::Tuple1(a, b, c)),
                Content::Structure(FlatType::Tuple1(d, e, f)),
            ) if c.is_some() == f.is_some() => {
                pending.extend([(a, d), (b, e)]);
                pending.extend(c.into_iter().zip(f));
            }
            (Content::Structure(FlatType::Unit1), Content::Structure(FlatType::Unit1))
            | (
                Content::Structure(FlatType::EmptyRecord1),
                Content::Structure(FlatType::EmptyRecord1),
            ) => {}
            (
                Content::Structure(FlatType::Record1(..)),
                Content::Structure(FlatType::Record1(..) | FlatType::EmptyRecord1),
            )
            | (
                Content::Structure(FlatType::EmptyRecord1),
                Content::Structure(FlatType::Record1(..)),
            ) => {
                let (Some((a, ae)), Some((b, be))) = (record_fields(uf, a), record_fields(uf, b))
                else {
                    return false;
                };
                if !a.keys().eq(b.keys()) {
                    return false;
                }
                pending.push((ae, be));
                pending.extend(a.into_values().zip(b.into_values()));
            }
            (
                Content::Alias {
                    home: ha,
                    name: na,
                    args: aa,
                    ..
                },
                Content::Alias {
                    home: hb,
                    name: nb,
                    args: ab,
                    ..
                },
            ) if ha == hb && na == nb && aa.len() == ab.len() => {
                pending.extend(aa.into_iter().zip(ab).map(|((_, a), (_, b))| (a, b)));
            }
            _ => return false,
        }
    }
    true
}

fn record_fields<'a>(
    uf: &mut UnionFind<'a>,
    mut variable: Variable,
) -> Option<(BTreeMap<&'a str, Variable>, Variable)> {
    let mut fields = BTreeMap::new();
    let mut seen = BTreeSet::new();
    loop {
        variable = uf.find(variable);
        if !seen.insert(variable) {
            return None;
        }
        match uf.get(variable).content.clone() {
            Content::Structure(FlatType::Record1(more, ext)) => {
                for (name, typ) in more {
                    fields.entry(name).or_insert(typ);
                }
                variable = ext;
            }
            Content::Alias { real, .. } => variable = real,
            _ => return Some((fields, variable)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nash_constrain::type_::make_descriptor;

    #[test]
    fn matching_normalizes_record_extensions_without_unifying_them() {
        let mut uf = UnionFind::new();
        let a = uf.fresh(make_descriptor(Content::RigidVar("a")));
        let empty = uf.fresh(make_descriptor(Content::Structure(FlatType::EmptyRecord1)));
        let y = uf.fresh(make_descriptor(Content::Structure(FlatType::Record1(
            BTreeMap::from([("y", a)]),
            empty,
        ))));
        let nested = uf.fresh(make_descriptor(Content::Structure(FlatType::Record1(
            BTreeMap::from([("x", a)]),
            y,
        ))));
        let flat = uf.fresh(make_descriptor(Content::Structure(FlatType::Record1(
            BTreeMap::from([("x", a), ("y", a)]),
            empty,
        ))));
        assert!(same_args(&mut uf, &[nested], &[flat]));
        let wrapped_empty = uf.fresh(make_descriptor(Content::Structure(FlatType::Record1(
            BTreeMap::new(),
            empty,
        ))));
        assert!(same_args(&mut uf, &[wrapped_empty], &[empty]));
        assert!(!same_args(&mut uf, &[nested], &[y]));
        assert!(!uf.equivalent(nested, flat));
    }

    #[test]
    fn application_matching_waits_for_known_heads() {
        let mut uf = UnionFind::new();
        let head = uf.fresh(make_descriptor(Content::FlexVar(None)));
        let a = uf.fresh(make_descriptor(Content::RigidVar("a")));
        let home = nash_ast::ModuleName {
            package: None,
            name: "Main",
        };
        let application = uf.fresh(make_descriptor(Content::Structure(FlatType::AppV1(
            head,
            vec![a],
        ))));
        let known = uf.fresh(make_descriptor(Content::Structure(FlatType::App1(
            home,
            "Box",
            vec![a],
        ))));
        assert!(!same_args(&mut uf, &[application], &[known]));
        assert!(matches!(uf.get(head).content, Content::FlexVar(None)));
        uf.modify(head, |desc| {
            desc.content = Content::Structure(FlatType::App1(home, "Box", vec![]))
        });
        assert!(same_args(&mut uf, &[application], &[known]));
        let other_home = nash_ast::ModuleName {
            package: None,
            name: "Other",
        };
        let other = uf.fresh(make_descriptor(Content::Structure(FlatType::App1(
            other_home,
            "Box",
            vec![a],
        ))));
        assert!(!same_args(&mut uf, &[application], &[other]));
    }

    #[test]
    fn matching_preserves_unknown_variables_and_nominal_alias_identity() {
        let bump = bumpalo::Bump::new();
        let body = bump.alloc(nash_region::Located::at_zero(nash_ast::Type::Unit));
        let mut uf = UnionFind::new();
        let a = uf.fresh(make_descriptor(Content::RigidVar("a")));
        let another_a = uf.fresh(make_descriptor(Content::RigidVar("a")));
        let unknown = uf.fresh(make_descriptor(Content::FlexVar(None)));
        assert!(!same_args(&mut uf, &[a], &[another_a]));
        assert!(!same_args(&mut uf, &[a], &[unknown]));
        assert!(!uf.equivalent(a, unknown));
        assert!(matches!(uf.get(unknown).content, Content::FlexVar(None)));
        let home = nash_ast::ModuleName {
            package: None,
            name: "Main",
        };
        let first = uf.fresh(make_descriptor(Content::Structure(FlatType::App1(
            home,
            "List",
            vec![a],
        ))));
        let second = uf.fresh(make_descriptor(Content::Structure(FlatType::App1(
            home,
            "List",
            vec![a],
        ))));
        assert!(same_args(&mut uf, &[first], &[second]));
        let alias_a = uf.fresh(make_descriptor(Content::Alias {
            body,
            home,
            name: "A",
            args: vec![("x", a)],
            real: first,
        }));
        let alias_b = uf.fresh(make_descriptor(Content::Alias {
            body,
            home,
            name: "B",
            args: vec![("x", a)],
            real: first,
        }));
        assert!(!same_args(&mut uf, &[alias_a], &[alias_b]));
        assert!(!same_args(&mut uf, &[alias_a], &[first]));
    }
}
