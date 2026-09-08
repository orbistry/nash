//! Recursive impl-pattern operations shared by canonicalization and resolution.
use std::collections::BTreeMap;

use crate::Type;
use crate::{Head, HeadCon};
use nash_region::Located;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limit;

pub enum Match<T> {
    Yes(T),
    No,
    Deferred,
}

pub trait Types<'a> {
    type Node: Copy;
    fn constructor(&mut self, node: Self::Node, expected: HeadCon<'a>) -> Match<Vec<Self::Node>>;
    fn equal(
        &mut self,
        first: Self::Node,
        second: Self::Node,
        remaining: &mut usize,
    ) -> Result<Match<()>, Limit>;
}

pub struct Canonical;

#[derive(PartialEq, Eq)]
enum CanonicalCon<'a> {
    Known(HeadCon<'a>),
    Var(&'a str),
    Record(Vec<&'a str>),
}

fn canonical_view<'a>(
    mut typ: &'a Located<Type<'a>>,
) -> (CanonicalCon<'a>, Vec<&'a Located<Type<'a>>>) {
    let mut suffixes = Vec::new();
    while let Type::App { head, args } = &typ.value {
        suffixes.push(*args);
        typ = head;
    }
    let (con, mut args) = match &typ.value {
        Type::Named { reference, args } => (
            CanonicalCon::Known(HeadCon::Named(*reference)),
            args.to_vec(),
        ),
        Type::Alias {
            reference,
            arguments,
            ..
        } => (
            CanonicalCon::Known(HeadCon::Named(*reference)),
            arguments.iter().map(|arg| arg.typ).collect(),
        ),
        Type::Var(name) => (CanonicalCon::Var(name), Vec::new()),

        Type::Tuple {
            first,
            second,
            rest,
        } => (
            CanonicalCon::Known(HeadCon::Tuple(2 + rest.len())),
            [*first, *second]
                .into_iter()
                .chain(rest.iter().copied())
                .collect(),
        ),
        Type::Lambda { from, to } => (CanonicalCon::Known(HeadCon::Fun), vec![*from, *to]),
        Type::Record { fields } => (
            CanonicalCon::Record(fields.iter().map(|field| field.field).collect()),
            fields.iter().map(|field| field.typ).collect(),
        ),
        Type::App { .. } => unreachable!("application head flattened"),
    };
    for suffix in suffixes.into_iter().rev() {
        args.extend_from_slice(suffix);
    }
    (con, args)
}

impl<'a> Types<'a> for Canonical {
    type Node = &'a Located<Type<'a>>;
    fn constructor(&mut self, node: Self::Node, expected: HeadCon<'a>) -> Match<Vec<Self::Node>> {
        let (con, args) = canonical_view(node);
        if con == CanonicalCon::Known(expected) {
            Match::Yes(args)
        } else {
            Match::No
        }
    }
    fn equal(
        &mut self,
        first: Self::Node,
        second: Self::Node,
        remaining: &mut usize,
    ) -> Result<Match<()>, Limit> {
        let mut pending = vec![(first, second)];
        while let Some((first, second)) = pending.pop() {
            step(remaining)?;
            let (a, aa) = canonical_view(first);
            let (b, ba) = canonical_view(second);
            if a != b || aa.len() != ba.len() {
                return Ok(Match::No);
            }
            pending.extend(aa.into_iter().zip(ba));
        }
        Ok(Match::Yes(()))
    }
}

pub fn step(remaining: &mut usize) -> Result<(), Limit> {
    *remaining = remaining.checked_sub(1).ok_or(Limit)?;
    Ok(())
}

pub fn children<'p, 'a>(head: &'p Head<'a>) -> Vec<&'p Head<'a>> {
    match head {
        Head::Named { args, .. } | Head::Tuple(args) => args.iter().collect(),
        Head::Function(from, to) => vec![from, to],
        Head::Var(_) => Vec::new(),
    }
}

pub fn matches<'a, T: Types<'a>>(
    types: &mut T,
    heads: &[Head<'a>],
    arguments: &[T::Node],
    variable_count: usize,
    remaining: &mut usize,
) -> Result<Match<Vec<T::Node>>, Limit> {
    if heads.len() != arguments.len() {
        return Ok(Match::No);
    }
    let mut bindings = vec![None; variable_count];
    let mut pending: Vec<_> = heads.iter().zip(arguments.iter().copied()).collect();
    let mut deferred = false;
    while let Some((head, argument)) = pending.pop() {
        step(remaining)?;
        match head {
            Head::Var(index) => {
                let binding = &mut bindings[usize::from(*index)];
                if let Some(previous) = *binding {
                    match types.equal(previous, argument, remaining)? {
                        Match::Yes(()) => {}
                        Match::No => return Ok(Match::No),
                        Match::Deferred => deferred = true,
                    }
                } else {
                    *binding = Some(argument);
                }
            }
            _ => match types.constructor(argument, head.con().expect("constructor pattern")) {
                Match::No => return Ok(Match::No),
                Match::Deferred => deferred = true,
                Match::Yes(arguments) => {
                    let patterns = children(head);
                    if patterns.len() != arguments.len() {
                        return Ok(Match::No);
                    }
                    pending.extend(patterns.into_iter().zip(arguments));
                }
            },
        }
    }
    if deferred {
        return Ok(Match::Deferred);
    }
    Ok(Match::Yes(
        bindings
            .into_iter()
            .map(|binding| binding.expect("each impl variable occurs in a head"))
            .collect(),
    ))
}

#[derive(Clone, Copy)]
struct Term<'p, 'a> {
    pattern: &'p Head<'a>,
    side: bool,
}

type Substitution<'p, 'a> = BTreeMap<(bool, u16), Term<'p, 'a>>;

fn resolve<'p, 'a>(
    mut term: Term<'p, 'a>,
    substitution: &Substitution<'p, 'a>,
    remaining: &mut usize,
) -> Result<Term<'p, 'a>, Limit> {
    while let Head::Var(index) = term.pattern {
        step(remaining)?;
        let Some(next) = substitution.get(&(term.side, *index)) else {
            break;
        };
        term = *next;
    }
    Ok(term)
}

/// Test structural overlap with independent variables for the two impl heads.
/// Heads have already been checked against their trait's closed kinds.
pub fn overlaps(
    left: &[Head<'_>],
    right: &[Head<'_>],
    remaining: &mut usize,
) -> Result<bool, Limit> {
    unifiable(left, right, remaining, true)
}

/// Equality within one impl preserves variables shared between its heads.
pub fn can_equal(
    left: &[Head<'_>],
    right: &[Head<'_>],
    remaining: &mut usize,
) -> Result<bool, Limit> {
    unifiable(left, right, remaining, false)
}

fn unifiable(
    left: &[Head<'_>],
    right: &[Head<'_>],
    remaining: &mut usize,
    freshen: bool,
) -> Result<bool, Limit> {
    if left.len() != right.len() {
        return Ok(false);
    }
    let mut substitution = BTreeMap::new();
    let mut pending: Vec<_> = left
        .iter()
        .zip(right)
        .map(|(a, b)| {
            (
                Term {
                    pattern: a,
                    side: false,
                },
                Term {
                    pattern: b,
                    side: freshen,
                },
            )
        })
        .collect();
    while let Some((left, right)) = pending.pop() {
        step(remaining)?;
        let left = resolve(left, &substitution, remaining)?;
        let right = resolve(right, &substitution, remaining)?;
        if let Head::Var(index) = left.pattern {
            let variable = (left.side, *index);
            if matches!(right.pattern, Head::Var(other) if variable == (right.side, *other)) {
                continue;
            }
            let mut occurs = vec![right];
            while let Some(term) = occurs.pop() {
                let term = resolve(term, &substitution, remaining)?;
                step(remaining)?;
                if matches!(term.pattern, Head::Var(other) if variable == (term.side, *other)) {
                    return Ok(false);
                }
                occurs.extend(children(term.pattern).into_iter().map(|pattern| Term {
                    pattern,
                    side: term.side,
                }));
            }
            substitution.insert(variable, right);
        } else if matches!(right.pattern, Head::Var(_)) {
            pending.push((right, left));
        } else {
            if left.pattern.con() != right.pattern.con() {
                return Ok(false);
            }
            let left_children = children(left.pattern);
            let right_children = children(right.pattern);
            if left_children.len() != right_children.len() {
                return Ok(false);
            }
            pending.extend(left_children.into_iter().zip(right_children).map(|(a, b)| {
                (
                    Term {
                        pattern: a,
                        side: left.side,
                    },
                    Term {
                        pattern: b,
                        side: right.side,
                    },
                )
            }));
        }
    }
    Ok(true)
}
