use bumpalo::Bump;
use nash_ast::{Ctor, CtorOpts, Kind, Union};
use nash_region::Located;

use crate::matrix::{Row, is_exhaustive, is_useful};
use crate::pattern::{CONS_NAME, LIST, NIL_NAME, PAIR, PAIR_NAME};
use crate::render::{RenderContext, pattern_to_string};
use crate::{Literal, Pattern};

static BOOL: Union<'static> = Union {
    kind: &Kind::Type,
    context: &[],
    name: &Located::at_zero("bool"),
    parameters: &[],
    ctors: &[
        &Ctor {
            labels: None,
            name: "False",
            index: 0,
            arity: 0,
            arguments: &[],
        },
        &Ctor {
            labels: None,
            name: "True",
            index: 1,
            arity: 0,
            arguments: &[],
        },
    ],
    alternatives: 2,
    options: CtorOpts::Enum,
};
const F: Pattern<'static> = Pattern::Ctor {
    union: &BOOL,
    name: "False",
    args: &[],
};
const T: Pattern<'static> = Pattern::Ctor {
    union: &BOOL,
    name: "True",
    args: &[],
};
const ANY: Pattern<'static> = Pattern::Anything;
const NIL: Pattern<'static> = Pattern::Ctor {
    union: &LIST,
    name: NIL_NAME,
    args: &[],
};

fn missing(matrix: &[Row<'_>], columns: usize) -> String {
    let bump = Bump::new();
    let rows = is_exhaustive(&bump, matrix, columns);
    rows.iter()
        .map(|row| {
            row.iter()
                .map(|p| pattern_to_string(RenderContext::Unambiguous, *p))
                .collect::<Vec<_>>()
                .join(" | ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn empty_matrix() {
    insta::assert_snapshot!(missing(&[], 2), @"_ | _");
}

#[test]
fn zero_columns() {
    let bump = Bump::new();
    assert_eq!(is_exhaustive(&bump, &[], 0).len(), 1);
    assert!(is_exhaustive(&bump, &[vec![]], 0).is_empty());
    assert!(is_useful(&[], &[]));
    assert!(!is_useful(&[vec![]], &[]));
}

#[test]
fn nil_and_cons() {
    let cons = Pattern::Ctor {
        union: &LIST,
        name: CONS_NAME,
        args: &[ANY, ANY],
    };
    insta::assert_snapshot!(missing(&[vec![NIL], vec![cons]], 1), @"");
}

#[test]
fn only_nil() {
    insta::assert_snapshot!(missing(&[vec![NIL]], 1), @"_ :: _");
}

#[test]
fn nested_cons() {
    let singleton = Pattern::Ctor {
        union: &LIST,
        name: CONS_NAME,
        args: &[ANY, NIL],
    };
    insta::assert_snapshot!(missing(&[vec![NIL], vec![singleton]], 1), @"_ :: _ :: _");
}

#[test]
fn literal_domain_needs_wildcard() {
    insta::assert_snapshot!(missing(&[vec![Pattern::Literal(Literal::Int(1))], vec![Pattern::Literal(Literal::Int(2))]], 1), @"_");
}

#[test]
fn pair_partial() {
    let pair = Pattern::Ctor {
        union: &PAIR,
        name: PAIR_NAME,
        args: &[T, ANY],
    };
    insta::assert_snapshot!(missing(&[vec![pair]], 1), @"( False, _ )");
}

#[test]
fn all_bool_constructors_make_wildcard_redundant() {
    assert!(!is_useful(&[vec![F], vec![T]], &[ANY]));
}

#[test]
fn one_bool_constructor_leaves_wildcard_useful() {
    assert!(is_useful(&[vec![T]], &[ANY]));
}

#[test]
fn wildcard_makes_constructor_redundant() {
    assert!(!is_useful(&[vec![ANY]], &[T]));
}

#[test]
fn distinct_literals_are_useful() {
    assert!(is_useful(
        &[vec![Pattern::Literal(Literal::Int(1))]],
        &[Pattern::Literal(Literal::Int(2))]
    ));
}

#[test]
fn repeated_bytes_are_redundant() {
    let bytes = Pattern::Literal(Literal::Bytes(&[0, 255]));
    assert!(!is_useful(&[vec![bytes]], &[bytes]));
}

#[test]
fn repeated_strings_are_redundant() {
    let string = Pattern::Literal(Literal::Str("hello"));
    assert!(!is_useful(&[vec![string]], &[string]));
}

#[test]
fn wildcard_makes_literal_redundant() {
    assert!(!is_useful(
        &[vec![ANY]],
        &[Pattern::Literal(Literal::Int(1))]
    ));
}

#[test]
fn overloaded_literal_can_overlap_a_constructor() {
    let literal = Pattern::Literal(Literal::Int(0));
    assert!(is_useful(&[vec![T]], &[literal]));
    assert!(is_useful(&[vec![literal]], &[T]));
    insta::assert_snapshot!(missing(&[vec![T], vec![literal]], 1), @"False");
}

#[test]
fn complete_constructors_make_overloaded_literal_redundant() {
    assert!(!is_useful(
        &[vec![T], vec![F]],
        &[Pattern::Literal(Literal::Int(0))]
    ));
}

#[test]
fn overloaded_literal_respects_other_columns() {
    let literal = Pattern::Literal(Literal::Int(0));
    assert!(is_useful(&[vec![T, ANY], vec![F, T]], &[literal, F]));
    assert!(!is_useful(&[vec![T, ANY], vec![F, T]], &[literal, T]));
    assert!(!is_useful(&[vec![literal, ANY]], &[literal, T]));
}

#[test]
fn complete_outer_constructors_can_have_holes() {
    let singleton = Pattern::Ctor {
        union: &LIST,
        name: CONS_NAME,
        args: &[T, NIL],
    };
    assert!(is_useful(&[vec![NIL], vec![singleton]], &[ANY]));
}

#[test]
fn recovered_constructor_preserves_remaining_columns() {
    let pair = Pattern::Ctor {
        union: &PAIR,
        name: PAIR_NAME,
        args: &[T, ANY],
    };
    insta::assert_snapshot!(missing(&[vec![pair, T], vec![ANY, F]], 2), @"( False, _ ) | True");
}

// Independent finite-domain oracle: direct boolean membership, with no
// specialization/matrix recursion. Check all 512 subsets of the nine rows
// (_, False, True)^2 and all nine candidate rows, flat and tuple-wrapped.
#[test]
fn finite_domain_matches_direct_enumeration() {
    fn matches(code: usize, value: bool) -> bool {
        code == 0 || (code == 2) == value
    }
    let patterns = [ANY, F, T];
    let codes: Vec<_> = (0..3).flat_map(|a| (0..3).map(move |b| [a, b])).collect();
    let values = [[false, false], [false, true], [true, false], [true, true]];
    for mask in 0..(1 << codes.len()) {
        let chosen: Vec<_> = codes
            .iter()
            .enumerate()
            .filter_map(|(i, row)| (mask & (1 << i) != 0).then_some(*row))
            .collect();
        let covered = |value: [bool; 2]| {
            chosen
                .iter()
                .any(|row| matches(row[0], value[0]) && matches(row[1], value[1]))
        };
        let expected_exhaustive = values.iter().all(|&v| covered(v));
        for tuple in [false, true] {
            let bump = Bump::new();
            let row = |code: &[usize; 2]| {
                let pair = [patterns[code[0]], patterns[code[1]]];
                if tuple {
                    vec![Pattern::Ctor {
                        union: &PAIR,
                        name: PAIR_NAME,
                        args: bump.alloc_slice_copy(&pair),
                    }]
                } else {
                    pair.to_vec()
                }
            };
            let matrix: Vec<_> = chosen.iter().map(row).collect();
            assert_eq!(
                is_exhaustive(&bump, &matrix, if tuple { 1 } else { 2 }).is_empty(),
                expected_exhaustive,
                "mask={mask}, tuple={tuple}"
            );
            for candidate in &codes {
                let useful = values.iter().any(|&v| {
                    matches(candidate[0], v[0]) && matches(candidate[1], v[1]) && !covered(v)
                });
                assert_eq!(
                    is_useful(&matrix, &row(candidate)),
                    useful,
                    "mask={mask}, candidate={candidate:?}, tuple={tuple}"
                );
            }
        }
    }
}
