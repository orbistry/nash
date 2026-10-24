//! Recursive impl-pattern operations shared by canonicalization and resolution.
use std::collections::BTreeMap;

use crate::Type;
use crate::fuel::{OutOfFuel, tick};
use crate::primitives::{Repr, ReprSet};
use crate::{Head, HeadCon, QualifiedName};
use nash_region::Located;

pub enum Match<T> {
    Yes(T),
    No,
    Deferred,
}

pub trait Types<'a> {
    type Node: Copy;
    fn in_class(&mut self, node: Self::Node, class: ReprSet) -> Match<()>;
    fn constructor(&mut self, node: Self::Node, expected: HeadCon<'a>) -> Match<Vec<Self::Node>>;
    fn equal(&mut self, first: Self::Node, second: Self::Node) -> Result<Match<()>, OutOfFuel>;
}

/// Canonical types whose variables are rigid: a variable equals only itself
/// and matches no constructor.
pub struct Canonical<'r, 'a>(pub &'r dyn Fn(&'a Located<Type<'a>>, ReprSet) -> Match<()>);

/// Canonical types whose variables stay open: nothing will bind them, as in
/// code generation when no use fixes a phantom parameter. An open variable
/// equals only itself. A constructor test or a comparison that only an open
/// variable could decide is undecided, so under `SelectPolicy::Exact` a
/// selection that needs an open variable is `Deferred` unless another
/// candidate is selected.
pub struct Open<'r, 'a>(pub &'r dyn Fn(&'a Located<Type<'a>>, ReprSet) -> Match<()>);

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
        constructor_view(node, expected, false)
    }
    fn equal(&mut self, first: Self::Node, second: Self::Node) -> Result<Match<()>, OutOfFuel> {
        equal_views(first, second, false)
    }
}

impl<'a> Types<'a> for Open<'_, 'a> {
    type Node = &'a Located<Type<'a>>;
    fn in_class(&mut self, node: Self::Node, class: ReprSet) -> Match<()> {
        (self.0)(node, class)
    }
    fn constructor(&mut self, node: Self::Node, expected: HeadCon<'a>) -> Match<Vec<Self::Node>> {
        constructor_view(node, expected, true)
    }
    fn equal(&mut self, first: Self::Node, second: Self::Node) -> Result<Match<()>, OutOfFuel> {
        equal_views(first, second, true)
    }
}

// With `open`, a type variable is undecided instead of matching nothing.
fn constructor_view<'a>(
    node: &'a Located<Type<'a>>,
    expected: HeadCon<'a>,
    open: bool,
) -> Match<Vec<&'a Located<Type<'a>>>> {
    match canonical_view(node) {
        (CanonicalCon::Known(con), args) if con == expected => Match::Yes(args),
        (CanonicalCon::Var(_), _) if open => Match::Deferred,
        _ => Match::No,
    }
}

// With `open`, a mismatch at a type variable is undecided instead of false.
// A definite mismatch elsewhere still decides.
fn equal_views<'a>(
    first: &'a Located<Type<'a>>,
    second: &'a Located<Type<'a>>,
    open: bool,
) -> Result<Match<()>, OutOfFuel> {
    let mut pending = vec![(first, second)];
    let mut deferred = false;
    while let Some((first, second)) = pending.pop() {
        tick()?;
        let (a, aa) = canonical_view(first);
        let (b, ba) = canonical_view(second);
        if a == b && aa.len() == ba.len() {
            pending.extend(aa.into_iter().zip(ba));
        } else if open && (matches!(a, CanonicalCon::Var(_)) || matches!(b, CanonicalCon::Var(_))) {
            deferred = true;
        } else {
            return Ok(Match::No);
        }
    }
    Ok(if deferred {
        Match::Deferred
    } else {
        Match::Yes(())
    })
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
) -> Result<Match<Vec<T::Node>>, OutOfFuel> {
    if heads.len() != arguments.len() {
        return Ok(Match::No);
    }
    let mut bindings = vec![None; variable_count];
    let mut pending: Vec<_> = heads.iter().zip(arguments.iter().copied()).collect();
    let mut deferred = false;
    while let Some((head, argument)) = pending.pop() {
        tick()?;
        match head {
            Head::Var { index, repr } => {
                let binding = &mut bindings[usize::from(*index)];
                if let Some(previous) = *binding {
                    match types.equal(previous, argument)? {
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
    /// Entailment and evidence: an undecided candidate is not proven, so a
    /// selected candidate wins over it. With no selection, an undecided
    /// candidate makes the result `Deferred`.
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
    policy: SelectPolicy,
) -> Result<ImplSelection<I, T::Node>, OutOfFuel> {
    let mut first: Option<&'h [Head<'a>]> = None;
    let mut selected = None;
    let mut deferred = false;
    for (heads, variable_count, info) in candidates {
        match matches(types, heads, arguments, variable_count)? {
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
        None if deferred => ImplSelection::Deferred,
        None => ImplSelection::Missing,
    })
}

/// What a representation-tested head position learns about its argument.
pub enum CoverageSubject<S> {
    /// The class test is decided; ordinary selection handles it.
    Decided,
    /// The argument can still change; coverage cannot apply yet.
    Deferred,
    /// A rigid argument whose class is open within `admitted`.
    Rigid { subject: S, admitted: ReprSet },
}

/// Type and given view for the finite search over representation cells.
pub trait CoverageTypes<'a>: Types<'a> {
    type Subject: Copy + Ord;
    fn coverage_subject(
        &mut self,
        node: Self::Node,
        tested: ReprSet,
    ) -> CoverageSubject<Self::Subject>;
    /// True when a given holds `trait_` with this subject as a direct argument.
    fn given_at(&mut self, trait_: QualifiedName<'a>, subject: Self::Subject) -> bool;
    /// Class tests of later selections see these representations.
    fn set_coverage_assumptions(&mut self, assumptions: &[(Self::Subject, Repr)]);
}

pub enum Coverage<S, I, N> {
    /// No rigid subject decides the selection, or an argument can still change.
    NotApplicable,
    /// No given holds the wanted trait at this subject.
    Unstated(S),
    /// This cell selects no impl.
    Missing(Vec<(S, Repr)>),
    Complete(Vec<CoveredImpl<S, I, N>>),
}

pub struct CoveredImpl<S, I, N> {
    pub assumptions: Vec<(S, Repr)>,
    pub info: I,
    pub arguments: Vec<N>,
}

/// Select an impl for every admitted representation cell of the rigid
/// variables at representation-tested head positions. Only such positions
/// create dimensions. The limit comes first: every subject needs a given of
/// `trait_` at it, so a signature never hides the trait that selects. Every
/// step of the discovery, the cells and every selection draws from the fuel.
pub fn cover_impls<'h, 'a: 'h, T: CoverageTypes<'a>, I: Copy>(
    types: &mut T,
    trait_: QualifiedName<'a>,
    candidates: &[(&'h [Head<'a>], usize, I)],
    arguments: &[T::Node],
) -> Result<Coverage<T::Subject, I, T::Node>, OutOfFuel> {
    let mut subjects = BTreeMap::new();
    let mut deferred = false;
    for (heads, _, _) in candidates {
        let mut pending: Vec<_> = heads.iter().zip(arguments.iter().copied()).collect();
        while let Some((head, argument)) = pending.pop() {
            tick()?;
            match head {
                Head::Var { repr, .. } if repr.is_all() => {}
                Head::Var { repr, .. } => match types.coverage_subject(argument, *repr) {
                    CoverageSubject::Decided => {}
                    CoverageSubject::Deferred => deferred = true,
                    CoverageSubject::Rigid { subject, admitted } => {
                        subjects
                            .entry(subject)
                            .and_modify(|old: &mut ReprSet| *old = old.intersect(admitted))
                            .or_insert(admitted);
                    }
                },
                _ => match types.constructor(argument, head.con().expect("constructor pattern")) {
                    Match::Yes(nodes) => {
                        let patterns = children(head);
                        if patterns.len() == nodes.len() {
                            pending.extend(patterns.into_iter().zip(nodes));
                        }
                    }
                    Match::Deferred => deferred = true,
                    Match::No => {}
                },
            }
        }
    }
    if deferred || subjects.is_empty() {
        return Ok(Coverage::NotApplicable);
    }
    if let Some(subject) = subjects
        .keys()
        .copied()
        .find(|subject| !types.given_at(trait_, *subject))
    {
        return Ok(Coverage::Unstated(subject));
    }
    let mut cells = vec![Vec::new()];
    for (subject, admitted) in subjects {
        let mut next = Vec::new();
        for cell in cells {
            for repr in [Repr::Big, Repr::Const, Repr::Term] {
                if admitted.contains(repr) {
                    tick()?;
                    let mut cell: Vec<_> = cell.clone();
                    cell.push((subject, repr));
                    next.push(cell);
                }
            }
        }
        cells = next;
    }
    let mut covered = Vec::new();
    for assumptions in cells {
        types.set_coverage_assumptions(&assumptions);
        let selected = select_impl(
            types,
            candidates.iter().copied(),
            arguments,
            SelectPolicy::Defer,
        );
        types.set_coverage_assumptions(&[]);
        match selected? {
            ImplSelection::Selected { info, arguments } => covered.push(CoveredImpl {
                assumptions,
                info,
                arguments,
            }),
            ImplSelection::Missing | ImplSelection::Deferred => {
                return Ok(Coverage::Missing(assumptions));
            }
        }
    }
    Ok(Coverage::Complete(covered))
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
) -> Result<Term<'p, 'a>, OutOfFuel> {
    while let Head::Var { index, .. } = term.pattern {
        tick()?;
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
) -> Result<bool, OutOfFuel> {
    unifiable(left, right, repr_of, true)
}

/// Equality within one impl preserves variables shared between its heads.
pub fn can_equal(
    left: &[Head<'_>],
    right: &[Head<'_>],
    repr_of: &dyn Fn(&Head<'_>) -> Option<Repr>,
) -> Result<bool, OutOfFuel> {
    unifiable(left, right, repr_of, false)
}

fn unifiable(
    left: &[Head<'_>],
    right: &[Head<'_>],
    repr_of: &dyn Fn(&Head<'_>) -> Option<Repr>,
    freshen: bool,
) -> Result<bool, OutOfFuel> {
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
        tick()?;
        let left = resolve(left, &substitution)?;
        let right = resolve(right, &substitution)?;
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
                let term = resolve(term, &substitution)?;
                tick()?;
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
        fn equal(&mut self, _: (), _: ()) -> Result<Match<()>, OutOfFuel> {
            unreachable!("each candidate has one variable")
        }
    }

    const CLASSES: [Repr; 3] = [Repr::Big, Repr::Const, Repr::Term];

    /// Candidate `i` answers `outcomes[i]`; the selection names the winning index.
    fn select(
        outcomes: &[Outcome],
        policy: SelectPolicy,
        fuel: u64,
    ) -> Result<ImplSelection<usize, ()>, OutOfFuel> {
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
        crate::fuel::run(crate::fuel::Fuel::Limited(fuel), || {
            select_impl(&mut types, candidates, &[()], policy)
        })
    }

    use Outcome::{Deferred, No, Yes};
    use SelectPolicy::{Defer, Exact};

    fn render(policy: SelectPolicy, result: Result<ImplSelection<usize, ()>, OutOfFuel>) -> String {
        match result {
            Ok(ImplSelection::Selected { info, .. }) => format!("selects impl {}", info + 1),
            // Inference waits for its variables; nothing binds an open type.
            Ok(ImplSelection::Deferred) if policy == Defer => "waits".to_owned(),
            Ok(ImplSelection::Deferred) => "undecided".to_owned(),
            Ok(ImplSelection::Missing) => "no impl".to_owned(),
            Err(OutOfFuel) => "out of fuel".to_owned(),
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
        let scenarios: &[(&[Outcome], u64)] = &[
            (&[], 100),
            (&[No, No], 100),
            (&[No, Yes, No], 100),
            (&[Yes, Deferred], 100),
            (&[Deferred, Yes], 100),
            (&[Deferred, No], 100),
            (&[No, Yes], 1),
        ];
        let mut output = String::new();
        for (outcomes, fuel) in scenarios {
            let fuel_note = if *fuel == 1 {
                " (fuel for one step)"
            } else {
                ""
            };
            output.push_str(&format!(
                "{}{fuel_note}\n  inference: {}\n  entailment and evidence: {}\n",
                describe(outcomes),
                render(Defer, select(outcomes, Defer, *fuel)),
                render(Exact, select(outcomes, Exact, *fuel)),
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

#[cfg(test)]
mod coverage_tests {
    use super::*;

    /// Arguments are rigid subjects, or a pair of them for tuple heads. A
    /// subject belongs to a class only through the current cell.
    #[derive(Clone, Copy)]
    enum Arg {
        Subject(u8),
        Pair(u8, u8),
    }

    struct Rigid {
        given: Vec<u8>,
        deferred: bool,
        assumptions: Vec<(u8, Repr)>,
    }

    impl<'a> Types<'a> for Rigid {
        type Node = Arg;
        fn in_class(&mut self, node: Arg, class: ReprSet) -> Match<()> {
            let Arg::Subject(subject) = node else {
                return Match::No;
            };
            match self.assumptions.iter().find(|(other, _)| *other == subject) {
                Some((_, repr)) if class.contains(*repr) => Match::Yes(()),
                _ => Match::No,
            }
        }
        fn constructor(&mut self, node: Arg, expected: HeadCon<'a>) -> Match<Vec<Arg>> {
            match (node, expected) {
                (Arg::Pair(a, b), HeadCon::Tuple(2)) => {
                    Match::Yes(vec![Arg::Subject(a), Arg::Subject(b)])
                }
                _ => Match::No,
            }
        }
        fn equal(&mut self, _: Arg, _: Arg) -> Result<Match<()>, OutOfFuel> {
            unreachable!("no head repeats a variable")
        }
    }

    impl<'a> CoverageTypes<'a> for Rigid {
        type Subject = u8;
        fn coverage_subject(&mut self, node: Arg, _: ReprSet) -> CoverageSubject<u8> {
            match node {
                _ if self.deferred => CoverageSubject::Deferred,
                Arg::Subject(subject) => CoverageSubject::Rigid {
                    subject,
                    admitted: ReprSet::ALL,
                },
                Arg::Pair(..) => unreachable!("tuple heads are constructors"),
            }
        }
        fn given_at(&mut self, _: QualifiedName<'a>, subject: u8) -> bool {
            self.given.contains(&subject)
        }
        fn set_coverage_assumptions(&mut self, assumptions: &[(u8, Repr)]) {
            self.assumptions = assumptions.to_vec();
        }
    }

    const BIG: ReprSet = ReprSet::only(Repr::Big);
    const LITTLE: ReprSet = crate::primitives::ReprTrait::Little.admits();
    const fn var(index: u16, repr: ReprSet) -> Head<'static> {
        Head::Var { index, repr }
    }
    const BIG_HEAD: [Head<'static>; 1] = [var(0, BIG)];
    const LITTLE_HEAD: [Head<'static>; 1] = [var(0, LITTLE)];
    const PAIR_BIG: [Head<'static>; 1] = [Head::Tuple(&[var(0, BIG), var(1, ReprSet::ALL)])];
    const PAIR_LITTLE_BIG: [Head<'static>; 1] = [Head::Tuple(&[var(0, LITTLE), var(1, BIG)])];
    const PAIR_LITTLE_LITTLE: [Head<'static>; 1] = [Head::Tuple(&[var(0, LITTLE), var(1, LITTLE)])];

    /// Enough fuel that no scenario but the deliberately short one runs out.
    const PLENTY: u64 = 1 << 20;

    fn search(
        impls: &[(&'static str, &[Head<'static>])],
        argument: Arg,
        given: &[u8],
        deferred: bool,
        fuel: u64,
    ) -> (Result<Coverage<u8, &'static str, Arg>, OutOfFuel>, u64) {
        let trait_ = QualifiedName {
            home: crate::ModuleName {
                package: None,
                name: "Main",
            },
            name: "Probe",
        };
        let candidates: Vec<_> = impls
            .iter()
            .map(|(name, heads)| {
                let variables = match heads {
                    [Head::Tuple(_)] => 2,
                    _ => 1,
                };
                (*heads, variables, *name)
            })
            .collect();
        let mut types = Rigid {
            given: given.to_vec(),
            deferred,
            assumptions: Vec::new(),
        };
        crate::fuel::run(crate::fuel::Fuel::Limited(fuel), || {
            let result = cover_impls(&mut types, trait_, &candidates, &[argument]);
            let crate::fuel::Fuel::Limited(left) = crate::fuel::remaining() else {
                unreachable!("limited fuel stays limited")
            };
            (result, fuel - left)
        })
    }

    fn cell(assumptions: &[(u8, Repr)]) -> String {
        assumptions
            .iter()
            .map(|(subject, repr)| format!("'{} {repr:?}", char::from(b'a' + subject)))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn render(result: Result<Coverage<u8, &'static str, Arg>, OutOfFuel>) -> String {
        match result {
            Ok(Coverage::NotApplicable) => "not applicable".to_owned(),
            Ok(Coverage::Unstated(subject)) => {
                format!("needs the trait at '{}", char::from(b'a' + subject))
            }
            Ok(Coverage::Missing(assumptions)) => {
                format!("no impl for the cell {}", cell(&assumptions))
            }
            Ok(Coverage::Complete(covered)) => covered
                .iter()
                .map(|covered| format!("{} selects {}", cell(&covered.assumptions), covered.info))
                .collect::<Vec<_>>()
                .join("; "),
            Err(OutOfFuel) => "out of fuel".to_owned(),
        }
    }

    #[test]
    fn cover_impls_outcomes() {
        let split: &[(&str, &[Head])] = &[("big", &BIG_HEAD), ("little", &LITTLE_HEAD)];
        let only_big: &[(&str, &[Head])] = &[("big", &BIG_HEAD)];
        let pairs: &[(&str, &[Head])] = &[
            ("first big", &PAIR_BIG),
            ("first little, second big", &PAIR_LITTLE_BIG),
            ("both little", &PAIR_LITTLE_LITTLE),
        ];
        let element = Arg::Subject(0);
        let both = Arg::Pair(0, 1);
        let (_, pairs_cost) = search(pairs, both, &[0, 1], false, PLENTY);
        let scenarios = [
            (
                "Big and Little impls, given at 'a",
                split,
                element,
                &[0][..],
                false,
                PLENTY,
            ),
            (
                "only a Big impl, given at 'a",
                only_big,
                element,
                &[0],
                false,
                PLENTY,
            ),
            (
                "Big and Little impls, no given",
                split,
                element,
                &[],
                false,
                PLENTY,
            ),
            (
                "only a Big impl, no given (the limit comes first)",
                only_big,
                element,
                &[],
                false,
                PLENTY,
            ),
            (
                "Big and Little impls, the argument can still change",
                split,
                element,
                &[0],
                true,
                PLENTY,
            ),
            (
                "pair impls, given at 'a and 'b",
                pairs,
                both,
                &[0, 1],
                false,
                PLENTY,
            ),
            (
                "pair impls, given at 'a only",
                pairs,
                both,
                &[0],
                false,
                PLENTY,
            ),
            (
                "pair impls, one step less than the whole search",
                pairs,
                both,
                &[0, 1],
                false,
                pairs_cost - 1,
            ),
        ];
        let mut output = String::new();
        for (name, impls, argument, given, deferred, fuel) in scenarios {
            let (result, _) = search(impls, argument, given, deferred, fuel);
            output.push_str(&format!("{name}\n  {}\n", render(result)));
        }
        insta::with_settings!({
            description => "Coverage of a wanted predicate whose arguments are rigid variables. \
                Big covers Big; Little covers Const and Term. One fuel counter pays for subject \
                discovery, the cells, and every selection.",
            omit_expression => true,
        }, {
            insta::assert_snapshot!(output);
        });
    }
}
