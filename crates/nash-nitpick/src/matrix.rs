//! Maranget's usefulness and exhaustiveness over simplified patterns, from
//! Elm's `Nitpick/PatternMatches.hs`.

use std::collections::BTreeMap;

use bumpalo::Bump;
use nash_ast::{Ctor, Union};

use crate::pattern::{Literal, Pattern};

pub(crate) type Row<'a> = Vec<Pattern<'a>>;

// EXHAUSTIVE PATTERNS

/// Elm's `isExhaustive`: rows of missing patterns, empty when exhaustive.
///
/// Invariant: every row of `matrix` has length `n`; every result row has
/// length `n`.
pub(crate) fn is_exhaustive<'a>(bump: &'a Bump, matrix: &[Row<'a>], n: usize) -> Vec<Row<'a>> {
    if matrix.is_empty() {
        return vec![vec![Pattern::Anything; n]];
    }
    if n == 0 {
        return vec![];
    }

    let ctors = collect_ctors(matrix);
    let Some((_, &alts)) = ctors.first_key_value() else {
        return is_exhaustive(
            bump,
            &specialize_all(matrix, specialize_row_by_anything),
            n - 1,
        )
        .into_iter()
        .map(|rest| prepend(Pattern::Anything, rest))
        .collect();
    };

    if ctors.len() < usize::from(alts.alternatives) {
        let rest = is_exhaustive(
            bump,
            &specialize_all(matrix, specialize_row_by_anything),
            n - 1,
        );
        alts.ctors
            .iter()
            .filter(|ctor| !ctors.contains_key(ctor.name))
            .flat_map(|ctor| {
                let missing = Pattern::Ctor {
                    union: alts,
                    name: ctor.name,
                    args: bump.alloc_slice_fill_copy(usize::from(ctor.arity), Pattern::Anything),
                };
                rest.iter().map(move |tail| prepend(missing, tail.clone()))
            })
            .collect()
    } else {
        alts.ctors
            .iter()
            .flat_map(|ctor| {
                let arity = usize::from(ctor.arity);
                let specialized =
                    specialize_all(matrix, |row| specialize_row_by_ctor(ctor.name, arity, row));
                is_exhaustive(bump, &specialized, arity + n - 1)
                    .into_iter()
                    .map(move |row| recover_ctor(bump, alts, ctor.name, arity, row))
            })
            .collect()
    }
}

/// Elm's `recoverCtor`.
fn recover_ctor<'a>(
    bump: &'a Bump,
    union: &'a Union<'a>,
    name: &'a str,
    arity: usize,
    patterns: Row<'a>,
) -> Row<'a> {
    let (args, rest) = patterns.split_at(arity);
    prepend(
        Pattern::Ctor {
            union,
            name,
            args: bump.alloc_slice_copy(args),
        },
        rest.to_vec(),
    )
}

fn prepend<'a>(head: Pattern<'a>, mut rest: Row<'a>) -> Row<'a> {
    rest.insert(0, head);
    rest
}

fn specialize_all<'a>(
    matrix: &[Row<'a>],
    specialize: impl Fn(&[Pattern<'a>]) -> Option<Row<'a>>,
) -> Vec<Row<'a>> {
    matrix.iter().filter_map(|row| specialize(row)).collect()
}

// REDUNDANT PATTERNS

/// Elm's `isUseful`: does `vector` match something no row of `matrix` does?
pub(crate) fn is_useful<'a>(matrix: &[Row<'a>], vector: &[Pattern<'a>]) -> bool {
    if matrix.is_empty() {
        return true;
    }
    let Some((first, patterns)) = vector.split_first() else {
        return false;
    };
    match *first {
        Pattern::Ctor { name, args, .. } => {
            let specialized =
                specialize_all(matrix, |row| specialize_row_by_ctor(name, args.len(), row));
            is_useful(&specialized, &[args, patterns].concat())
        }
        Pattern::Anything => match is_complete(matrix) {
            Complete::No => is_useful(
                &specialize_all(matrix, specialize_row_by_anything),
                patterns,
            ),
            Complete::Yes(alts) => alts.iter().any(|ctor| {
                let arity = usize::from(ctor.arity);
                let specialized =
                    specialize_all(matrix, |row| specialize_row_by_ctor(ctor.name, arity, row));
                let mut vector = vec![Pattern::Anything; arity];
                vector.extend_from_slice(patterns);
                is_useful(&specialized, &vector)
            }),
        },
        Pattern::Literal(literal) => {
            // Nash literals are overloaded through FromInt/FromString/FromBytes.
            // A solved column may mix constructors and opaque literal values.
            // Complete structural coverage also covers any such value; otherwise
            // only equal literals and wildcards can prove this row redundant.
            if matrix
                .iter()
                .any(|row| matches!(row.first(), Some(Pattern::Ctor { .. })))
            {
                let wildcard = prepend(Pattern::Anything, patterns.to_vec());
                if !is_useful(matrix, &wildcard) {
                    return false;
                }
            }
            is_useful(
                &specialize_all(matrix, |row| specialize_row_by_literal(literal, row)),
                patterns,
            )
        }
    }
}

/// Invariant: `row.len() == N` implies `result.len() == arity + N - 1`.
fn specialize_row_by_ctor<'a>(
    ctor_name: &str,
    arity: usize,
    row: &[Pattern<'a>],
) -> Option<Row<'a>> {
    match row {
        [Pattern::Ctor { name, args, .. }, patterns @ ..] => {
            (*name == ctor_name).then(|| [*args, patterns].concat())
        }
        [Pattern::Anything, patterns @ ..] => {
            let mut out = vec![Pattern::Anything; arity];
            out.extend_from_slice(patterns);
            Some(out)
        }
        // The constructor of an overloaded literal is unknown. It cannot
        // establish coverage of this structural alternative.
        [Pattern::Literal(_), ..] => None,
        [] => unreachable!("empty rows are never specialized"),
    }
}

/// Invariant: `row.len() == N` implies `result.len() == N - 1`.
fn specialize_row_by_literal<'a>(literal: Literal<'a>, row: &[Pattern<'a>]) -> Option<Row<'a>> {
    match row {
        [Pattern::Literal(lit), patterns @ ..] => (*lit == literal).then(|| patterns.to_vec()),
        [Pattern::Anything, patterns @ ..] => Some(patterns.to_vec()),
        [Pattern::Ctor { .. }, ..] => None,
        [] => unreachable!("empty rows are never specialized"),
    }
}

/// Invariant: `row.len() == N` implies `result.len() == N - 1`.
fn specialize_row_by_anything<'a>(row: &[Pattern<'a>]) -> Option<Row<'a>> {
    match row {
        [Pattern::Anything, patterns @ ..] => Some(patterns.to_vec()),
        _ => None,
    }
}

// ALL CONSTRUCTORS ARE PRESENT?

pub(crate) enum Complete<'a> {
    Yes(&'a [&'a Ctor<'a>]),
    No,
}

/// Elm's `isComplete`.
fn is_complete<'a>(matrix: &[Row<'a>]) -> Complete<'a> {
    let ctors = collect_ctors(matrix);
    match ctors.first_key_value() {
        Some((_, union)) if ctors.len() == usize::from(union.alternatives) => {
            Complete::Yes(union.ctors)
        }
        _ => Complete::No,
    }
}

// COLLECT CTORS

/// Elm's `collectCtors`: constructor names seen in the first column.
fn collect_ctors<'a>(matrix: &[Row<'a>]) -> BTreeMap<&'a str, &'a Union<'a>> {
    matrix
        .iter()
        .filter_map(|row| match row.first() {
            Some(Pattern::Ctor { union, name, .. }) => Some((*name, *union)),
            _ => None,
        })
        .collect()
}
