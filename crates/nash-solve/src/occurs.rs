//! Port of Elm's `Type.Occurs`: does a variable occur in its own structure?

use std::collections::BTreeMap;

use nash_constrain::{Content, FlatType, UnionFind, Variable};

pub fn occurs(uf: &mut UnionFind<'_>, var: Variable) -> bool {
    occurs_help(uf, &mut Vec::new(), var, false)
}

fn occurs_help(
    uf: &mut UnionFind<'_>,
    seen: &mut Vec<Variable>,
    var: Variable,
    found_cycle: bool,
) -> bool {
    if seen.contains(&var) {
        return true;
    }

    let content = uf.get(var).content.clone();
    match content {
        Content::FlexVar(_) | Content::RigidVar(_) | Content::Error => found_cycle,

        Content::Structure(term) => {
            seen.push(var);
            let result = match term {
                FlatType::AppV1(head, args) => {
                    let acc = occurs_help(uf, seen, head, found_cycle);
                    args.iter()
                        .fold(acc, |acc, arg| occurs_help(uf, seen, *arg, acc))
                }
                FlatType::App1(_, _, args) => args
                    .iter()
                    .fold(found_cycle, |acc, arg| occurs_help(uf, seen, *arg, acc)),

                FlatType::Fun1(a, b) => {
                    let acc = occurs_help(uf, seen, b, found_cycle);
                    occurs_help(uf, seen, a, acc)
                }

                FlatType::Record1(fields) => fields
                    .values()
                    .fold(found_cycle, |acc, field| occurs_help(uf, seen, *field, acc)),

                FlatType::Tuple1(a, b, rest) => {
                    let acc = rest
                        .into_iter()
                        .rev()
                        .fold(found_cycle, |acc, c| occurs_help(uf, seen, c, acc));
                    let acc = occurs_help(uf, seen, b, acc);
                    occurs_help(uf, seen, a, acc)
                }
            };
            seen.pop();
            result
        }

        Content::Alias { args, .. } | Content::PartialAlias { args, .. } => {
            seen.push(var);
            let result = args.iter().fold(found_cycle, |acc, (_, arg)| {
                occurs_help(uf, seen, *arg, acc)
            });
            seen.pop();
            result
        }
    }
}

#[derive(Clone, Copy)]
enum Visit {
    Active,
    Done,
}

/// Does a cycle occur in a type reachable from `roots`? This follows the
/// edges of `occurs`, but visits each variable once, so a type that repeats a
/// shared part stays linear. `occurs` keeps Elm's path-only check for the
/// occurs check of a binding.
pub fn reaches_cycle(uf: &mut UnionFind<'_>, roots: &[Variable]) -> bool {
    let mut visits = BTreeMap::new();
    roots
        .iter()
        .any(|root| reaches_cycle_help(uf, &mut visits, *root))
}

fn reaches_cycle_help(
    uf: &mut UnionFind<'_>,
    visits: &mut BTreeMap<Variable, Visit>,
    var: Variable,
) -> bool {
    let var = uf.find(var);
    match visits.get(&var) {
        Some(Visit::Active) => return true,
        Some(Visit::Done) => return false,
        None => {}
    }
    visits.insert(var, Visit::Active);
    let children = match uf.get(var).content.clone() {
        Content::FlexVar(_) | Content::RigidVar(_) | Content::Error => Vec::new(),
        Content::Structure(FlatType::AppV1(head, args)) => [head].into_iter().chain(args).collect(),
        Content::Structure(FlatType::App1(_, _, args)) => args,
        Content::Structure(FlatType::Fun1(a, b)) => vec![a, b],
        Content::Structure(FlatType::Record1(fields)) => fields.into_values().collect(),
        Content::Structure(FlatType::Tuple1(a, b, rest)) => {
            [a, b].into_iter().chain(rest).collect()
        }
        Content::Alias { args, .. } | Content::PartialAlias { args, .. } => {
            args.into_iter().map(|(_, arg)| arg).collect()
        }
    };
    let found = children
        .into_iter()
        .any(|child| reaches_cycle_help(uf, visits, child));
    visits.insert(var, Visit::Done);
    found
}
