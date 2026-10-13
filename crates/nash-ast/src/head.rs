//! Recursive impl-pattern operations shared by canonicalization and resolution.
use std::collections::BTreeMap;

use crate::Type;
use crate::primitives::{Repr, ReprSet};
use crate::{Head, HeadCon};
use nash_region::Located;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limit;

// Impl contexts need not decrease, so every trait search has a budget.
pub const WORK_LIMIT: usize = 16_384;
pub const DEPTH_LIMIT: usize = 256;

pub enum Match<T> {
    Yes(T),
    No,
    Deferred,
}

pub trait Types<'a> {
    type Node: Copy;
    fn in_class(&mut self, node: Self::Node, class: ReprSet) -> Match<()>;
    fn constructor(&mut self, node: Self::Node, expected: HeadCon<'a>) -> Match<Vec<Self::Node>>;
    fn equal(
        &mut self,
        first: Self::Node,
        second: Self::Node,
        remaining: &mut usize,
    ) -> Result<Match<()>, Limit>;
}

pub struct Canonical<'r, 'a>(pub &'r dyn Fn(&'a Located<Type<'a>>, ReprSet) -> Match<()>);

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

impl<'a> Types<'a> for Canonical<'_, 'a> {
    type Node = &'a Located<Type<'a>>;
    fn in_class(&mut self, node: Self::Node, class: ReprSet) -> Match<()> {
        (self.0)(node, class)
    }
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
        Head::Var { .. } => Vec::new(),
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
            Head::Var { index, repr } => {
                let binding = &mut bindings[usize::from(*index)];
                if let Some(previous) = *binding {
                    match types.equal(previous, argument, remaining)? {
                        Match::Yes(()) => {}
                        Match::No => return Ok(Match::No),
                        Match::Deferred => deferred = true,
                    }
                } else {
                    if !repr.is_all() {
                        match types.in_class(argument, *repr) {
                            Match::Yes(()) => {}
                            Match::No => return Ok(Match::No),
                            Match::Deferred => deferred = true,
                        }
                    }
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

/// How an undecided candidate affects impl selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectPolicy {
    /// Inference: any undecided candidate defers the whole selection.
    Defer,
    /// Rigid or ground arguments: an undecided candidate is not proven.
    Exact,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ImplSelection<I, N> {
    Selected { info: I, arguments: Vec<N> },
    Deferred,
    Missing,
}

/// Select the impl whose heads match `arguments`. Every candidate is matched,
/// and a second match stops the compiler. Two impls can match one argument
/// list only if they overlap on finite types, which canonicalization rejects;
/// if the representation givens contradict each other, which canonicalization
/// rejects in one context and the solver across nested contexts; or if an
/// argument is an infinite type, which inference does not select for (the
/// occurs check of its binding reports it). A second match is a compiler bug.
pub fn select_impl<'h, 'a: 'h, T: Types<'a>, I>(
    types: &mut T,
    candidates: impl IntoIterator<Item = (&'h [Head<'a>], usize, I)>,
    arguments: &[T::Node],
    remaining: &mut usize,
    policy: SelectPolicy,
) -> Result<ImplSelection<I, T::Node>, Limit> {
    let mut first: Option<&'h [Head<'a>]> = None;
    let mut selected = None;
    let mut deferred = false;
    for (heads, variable_count, info) in candidates {
        match matches(types, heads, arguments, variable_count, remaining)? {
            Match::No => {}
            Match::Deferred => deferred = true,
            Match::Yes(bound) => {
                if let Some(first) = first {
                    panic!(
                        "impl coherence violated: heads {first:?} and {heads:?} both match the same arguments"
                    );
                }
                first = Some(heads);
                selected = Some(ImplSelection::Selected {
                    info,
                    arguments: bound,
                });
            }
        }
    }
    Ok(match selected {
        _ if deferred && policy == SelectPolicy::Defer => ImplSelection::Deferred,
        Some(selected) => selected,
        None => ImplSelection::Missing,
    })
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
    while let Head::Var { index, .. } = term.pattern {
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
    repr_of: &dyn Fn(&Head<'_>) -> Option<Repr>,
    remaining: &mut usize,
) -> Result<bool, Limit> {
    unifiable(left, right, repr_of, remaining, true)
}

/// Equality within one impl preserves variables shared between its heads.
pub fn can_equal(
    left: &[Head<'_>],
    right: &[Head<'_>],
    repr_of: &dyn Fn(&Head<'_>) -> Option<Repr>,
    remaining: &mut usize,
) -> Result<bool, Limit> {
    unifiable(left, right, repr_of, remaining, false)
}

fn unifiable(
    left: &[Head<'_>],
    right: &[Head<'_>],
    repr_of: &dyn Fn(&Head<'_>) -> Option<Repr>,
    remaining: &mut usize,
    freshen: bool,
) -> Result<bool, Limit> {
    if left.len() != right.len() {
        return Ok(false);
    }
    let mut substitution = BTreeMap::new();
    let mut classes = BTreeMap::new();
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
        if let Head::Var { index, repr } = left.pattern {
            let mine = classes.get(&(left.side, *index)).copied().unwrap_or(*repr);
            match right.pattern {
                Head::Var { index: other, repr } => {
                    let theirs = classes.get(&(right.side, *other)).copied().unwrap_or(*repr);
                    let both = mine.intersect(theirs);
                    if both.is_empty() {
                        return Ok(false);
                    }
                    classes.insert((right.side, *other), both);
                }
                head => {
                    if repr_of(head).is_some_and(|actual| !mine.contains(actual)) {
                        return Ok(false);
                    }
                }
            }
            let variable = (left.side, *index);
            if matches!(right.pattern, Head::Var { index: other, .. } if variable == (right.side, *other))
            {
                continue;
            }
            let mut occurs = vec![right];
            while let Some(term) = occurs.pop() {
                let term = resolve(term, &substitution, remaining)?;
                step(remaining)?;
                if matches!(term.pattern, Head::Var { index: other, .. } if variable == (term.side, *other))
                {
                    return Ok(false);
                }
                occurs.extend(children(term.pattern).into_iter().map(|pattern| Term {
                    pattern,
                    side: term.side,
                }));
            }
            substitution.insert(variable, right);
        } else if matches!(right.pattern, Head::Var { .. }) {
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

#[cfg(test)]
mod select_tests {
    use super::*;
    use crate::primitives::Repr;

    /// Each candidate head is a variable restricted to one representation;
    /// the outcome of its class test is the scripted answer for that class.
    struct Scripted(Vec<(ReprSet, Outcome)>);

    #[derive(Clone, Copy)]
    enum Outcome {
        Yes,
        No,
        Deferred,
    }

    impl<'a> Types<'a> for Scripted {
        type Node = ();
        fn in_class(&mut self, _: (), class: ReprSet) -> Match<()> {
            match self.0.iter().find(|(candidate, _)| *candidate == class) {
                Some((_, Outcome::Yes)) => Match::Yes(()),
                Some((_, Outcome::Deferred)) => Match::Deferred,
                Some((_, Outcome::No)) | None => Match::No,
            }
        }
        fn constructor(&mut self, _: (), _: HeadCon<'a>) -> Match<Vec<()>> {
            unreachable!("candidates are variable heads")
        }
        fn equal(&mut self, _: (), _: (), _: &mut usize) -> Result<Match<()>, Limit> {
            unreachable!("each candidate has one variable")
        }
    }

    const CLASSES: [Repr; 3] = [Repr::Big, Repr::Const, Repr::Term];

    /// Candidate `i` answers `outcomes[i]`; the selection names the winning index.
    fn select(
        outcomes: &[Outcome],
        policy: SelectPolicy,
        budget: usize,
    ) -> Result<ImplSelection<usize, ()>, Limit> {
        let heads: Vec<_> = CLASSES
            .iter()
            .map(|repr| {
                [Head::Var {
                    index: 0,
                    repr: ReprSet::only(*repr),
                }]
            })
            .collect();
        let mut types = Scripted(
            CLASSES
                .iter()
                .zip(outcomes)
                .map(|(repr, outcome)| (ReprSet::only(*repr), *outcome))
                .collect(),
        );
        let candidates = heads
            .iter()
            .take(outcomes.len())
            .enumerate()
            .map(|(index, heads)| (heads.as_slice(), 1, index));
        select_impl(&mut types, candidates, &[()], &mut { budget }, policy)
    }

    use Outcome::{Deferred, No, Yes};
    use SelectPolicy::{Defer, Exact};

    fn render(result: Result<ImplSelection<usize, ()>, Limit>) -> String {
        match result {
            Ok(ImplSelection::Selected { info, .. }) => format!("selects impl {}", info + 1),
            Ok(ImplSelection::Deferred) => "waits".to_owned(),
            Ok(ImplSelection::Missing) => "no impl".to_owned(),
            Err(Limit) => "work limit".to_owned(),
        }
    }

    fn describe(outcomes: &[Outcome]) -> String {
        if outcomes.is_empty() {
            return "no impls".to_owned();
        }
        let words: Vec<_> = outcomes
            .iter()
            .map(|outcome| match outcome {
                Yes => "matches",
                No => "does not match",
                Deferred => "undecided",
            })
            .collect();
        words.join(", ")
    }

    #[test]
    fn selection_outcomes_by_policy() {
        let scenarios: &[(&[Outcome], usize)] = &[
            (&[], 100),
            (&[No, No], 100),
            (&[No, Yes, No], 100),
            (&[Yes, Deferred], 100),
            (&[Deferred, Yes], 100),
            (&[Deferred, No], 100),
            (&[No, Yes], 1),
        ];
        let mut output = String::new();
        for (outcomes, budget) in scenarios {
            let budget_note = if *budget == 1 {
                " (budget for one step)"
            } else {
                ""
            };
            output.push_str(&format!(
                "{}{budget_note}\n  inference: {}\n  closed types: {}\n",
                describe(outcomes),
                render(select(outcomes, Defer, *budget)),
                render(select(outcomes, Exact, *budget)),
            ));
        }
        insta::with_settings!({
            description => "Impls in declaration order and how each one matches the arguments. \
                Inference uses SelectPolicy::Defer; entailment and evidence use SelectPolicy::Exact.",
            omit_expression => true,
        }, {
            insta::assert_snapshot!(output);
        });
    }

    #[test]
    #[should_panic(expected = "impl coherence violated")]
    fn exact_stops_the_compiler_on_a_second_match() {
        let _ = select(&[Yes, No, Yes], Exact, 100);
    }

    #[test]
    #[should_panic(expected = "impl coherence violated")]
    fn defer_stops_the_compiler_on_a_second_match() {
        let _ = select(&[Yes, Deferred, Yes], Defer, 100);
    }
}
