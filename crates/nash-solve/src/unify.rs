//! Port of Elm's `Type.Unify`.
//!
//! Elm threads a CPS monad carrying the list of freshly created variables
//! plus ok/err continuations; here that is a `&mut Vec<Variable>` accumulator
//! and `Result<(), ()>`, with `Err(())` playing `mismatch`. The deliberate
//! places where Elm keeps unifying after a failure (argument lists, shared
//! record fields) are preserved explicitly.

use std::collections::BTreeMap;

use bumpalo::Bump;
use nash_constrain::error_type::ErrorType;
use nash_constrain::type_::{self, Content, Descriptor, FlatType, NO_MARK, NO_RANK};
use nash_constrain::{UnionFind, Variable};

use crate::annotation;

// UNIFY

#[derive(Debug)]
pub enum Answer<'a> {
    Ok(Vec<Variable>),
    Err(Vec<Variable>, &'a ErrorType<'a>, &'a ErrorType<'a>),
}

pub fn unify<'a>(bump: &'a Bump, uf: &mut UnionFind<'a>, v1: Variable, v2: Variable) -> Answer<'a> {
    // Capture before unions discard child edges. Only assignments made by the
    // failed comparison are untrustworthy; untouched siblings remain useful.
    let dependencies = crate::recovery::reachable(uf, [v1, v2]);
    let before: Vec<_> = dependencies
        .into_iter()
        .map(|variable| (variable, uf.get(variable).content.clone()))
        .collect();
    let mut vars = Vec::new();
    match guarded_unify(uf, &mut vars, v1, v2) {
        Ok(()) => Answer::Ok(vars),
        Err(()) => {
            let changed: Vec<_> = before
                .into_iter()
                .filter_map(|(variable, content)| {
                    let current = uf.get(variable).content.clone();
                    let linked_flex =
                        matches!(content, Content::FlexVar(_)) && uf.find(variable) != variable;
                    (linked_flex || !same_content(uf, &content, &current)).then_some(variable)
                })
                .collect();
            let t1 = annotation::to_error_type(bump, uf, v1);
            let t2 = annotation::to_error_type(bump, uf, v2);
            let preds = merged_predicates(uf, v1, v2);
            uf.union(
                v1,
                v2,
                Descriptor {
                    preds,
                    content: Content::Error,
                    rank: NO_RANK,
                    mark: NO_MARK,
                    copy: None,
                },
            );
            crate::recovery::poison_roots(uf, changed.into_iter().chain(vars.iter().copied()));
            Answer::Err(vars, t1, t2)
        }
    }
}

/// Compare type information, ignoring representative changes for equal types.
fn same_content(uf: &mut UnionFind<'_>, first: &Content<'_>, second: &Content<'_>) -> bool {
    let same_vars = |uf: &mut UnionFind<'_>, first: &[Variable], second: &[Variable]| {
        first.len() == second.len()
            && first
                .iter()
                .zip(second)
                .all(|(a, b)| uf.find(*a) == uf.find(*b))
    };
    match (first, second) {
        (Content::FlexVar(a), Content::FlexVar(b)) => a == b,
        (Content::RigidVar(a), Content::RigidVar(b)) => a == b,
        (Content::Error, Content::Error) => true,
        (Content::Structure(a), Content::Structure(b)) => match (a, b) {
            (FlatType::App1(ah, an, aa), FlatType::App1(bh, bn, ba)) => {
                ah == bh && an == bn && same_vars(uf, aa, ba)
            }
            (FlatType::AppV1(ah, aa), FlatType::AppV1(bh, ba)) => {
                uf.find(*ah) == uf.find(*bh) && same_vars(uf, aa, ba)
            }
            (FlatType::Fun1(af, at), FlatType::Fun1(bf, bt)) => {
                same_vars(uf, &[*af, *at], &[*bf, *bt])
            }
            (FlatType::Tuple1(af, as_, ar), FlatType::Tuple1(bf, bs, br)) => {
                same_vars(uf, &[*af, *as_], &[*bf, *bs]) && same_vars(uf, ar, br)
            }
            (FlatType::Record1(a), FlatType::Record1(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .zip(b)
                        .all(|((an, av), (bn, bv))| an == bn && uf.find(*av) == uf.find(*bv))
            }
            _ => false,
        },
        (
            Content::Alias {
                home: ah,
                name: an,
                args: aa,
                real: ar,
                body: ab,
            },
            Content::Alias {
                home: bh,
                name: bn,
                args: ba,
                real: br,
                body: bb,
            },
        ) => {
            ah == bh
                && an == bn
                && std::ptr::eq(*ab, *bb)
                && uf.find(*ar) == uf.find(*br)
                && same_alias_args(uf, aa, ba)
        }
        (
            Content::PartialAlias {
                home: ah,
                name: an,
                args: aa,
                remaining: ar,
                body: ab,
            },
            Content::PartialAlias {
                home: bh,
                name: bn,
                args: ba,
                remaining: br,
                body: bb,
            },
        ) => {
            ah == bh
                && an == bn
                && ar == br
                && std::ptr::eq(*ab, *bb)
                && same_alias_args(uf, aa, ba)
        }
        _ => false,
    }
}

fn same_alias_args(
    uf: &mut UnionFind<'_>,
    first: &[(&str, Variable)],
    second: &[(&str, Variable)],
) -> bool {
    first.len() == second.len()
        && first
            .iter()
            .zip(second)
            .all(|((an, av), (bn, bv))| an == bn && uf.find(*av) == uf.find(*bv))
}

type UResult = Result<(), ()>;

// UNIFICATION HELPERS

struct Context<'a> {
    first: Variable,
    first_desc: Descriptor<'a>,
    second: Variable,
    second_desc: Descriptor<'a>,
}

// MERGE

fn merged_predicates(
    uf: &mut UnionFind<'_>,
    first: Variable,
    second: Variable,
) -> Vec<type_::PredId> {
    // Read the current representatives: recursive unification may have
    // changed them since the Context's descriptor snapshots were taken.
    let mut preds = uf.get(first).preds.clone();
    preds.extend_from_slice(&uf.get(second).preds);
    preds.sort_unstable();
    preds.dedup();
    preds
}

fn merge<'a>(uf: &mut UnionFind<'a>, context: &Context<'a>, content: Content<'a>) -> UResult {
    let preds = merged_predicates(uf, context.first, context.second);
    uf.union(
        context.first,
        context.second,
        Descriptor {
            preds,
            content,
            rank: context.first_desc.rank.min(context.second_desc.rank),
            mark: NO_MARK,
            copy: None,
        },
    );
    Ok(())
}

fn fresh<'a>(
    uf: &mut UnionFind<'a>,
    vars: &mut Vec<Variable>,
    context: &Context<'a>,
    content: Content<'a>,
) -> Variable {
    let var = uf.fresh(Descriptor {
        preds: Vec::new(),
        content,
        rank: context.first_desc.rank.min(context.second_desc.rank),
        mark: NO_MARK,
        copy: None,
    });
    vars.push(var);
    var
}

// ACTUALLY UNIFY THINGS

fn guarded_unify<'a>(
    uf: &mut UnionFind<'a>,
    vars: &mut Vec<Variable>,
    left: Variable,
    right: Variable,
) -> UResult {
    if uf.equivalent(left, right) {
        return Ok(());
    }
    nash_constrain::instantiate::normalize_variable(uf, left, vars);
    nash_constrain::instantiate::normalize_variable(uf, right, vars);
    let first_desc = uf.get(left).clone();
    let second_desc = uf.get(right).clone();
    actually_unify(
        uf,
        vars,
        Context {
            first: left,
            first_desc,
            second: right,
            second_desc,
        },
    )
}

fn sub_unify<'a>(
    uf: &mut UnionFind<'a>,
    vars: &mut Vec<Variable>,
    var1: Variable,
    var2: Variable,
) -> UResult {
    guarded_unify(uf, vars, var1, var2)
}

fn actually_unify<'a>(
    uf: &mut UnionFind<'a>,
    vars: &mut Vec<Variable>,
    context: Context<'a>,
) -> UResult {
    match context.first_desc.content.clone() {
        alias @ Content::PartialAlias { .. } => unify_partial_alias(uf, vars, &context, alias),
        Content::FlexVar(_) => unify_flex(uf, &context),

        Content::RigidVar(_) => unify_rigid(uf, &context),

        Content::Alias {
            home,
            name,
            args,
            real,
            body,
        } => unify_alias(uf, vars, &context, home, name, args, real, body),

        Content::Structure(flat_type) => unify_structure(uf, vars, &context, flat_type),

        // If there was an error, just pretend it is okay. This lets us
        // avoid "cascading" errors where one problem manifests as multiple
        // messages.
        Content::Error => merge(uf, &context, Content::Error),
    }
}

// UNIFY FLEXIBLE VARIABLES

fn unify_flex<'a>(uf: &mut UnionFind<'a>, context: &Context<'a>) -> UResult {
    let content = context.first_desc.content.clone();
    let other_content = context.second_desc.content.clone();
    match other_content {
        Content::Error => merge(uf, context, Content::Error),

        Content::FlexVar(maybe_name) => match maybe_name {
            None => merge(uf, context, content),
            Some(_) => merge(uf, context, Content::FlexVar(maybe_name)),
        },

        other @ (Content::RigidVar(_)
        | Content::Alias { .. }
        | Content::PartialAlias { .. }
        | Content::Structure(_)) => merge(uf, context, other),
    }
}

// UNIFY RIGID VARIABLES

fn unify_rigid<'a>(uf: &mut UnionFind<'a>, context: &Context<'a>) -> UResult {
    let content = context.first_desc.content.clone();
    match &context.second_desc.content {
        Content::FlexVar(_) => merge(uf, context, content),

        Content::RigidVar(_)
        | Content::Alias { .. }
        | Content::PartialAlias { .. }
        | Content::Structure(_) => Err(()),

        Content::Error => merge(uf, context, Content::Error),
    }
}

// UNIFY ALIASES

fn unify_partial_alias<'a>(
    uf: &mut UnionFind<'a>,
    vars: &mut Vec<Variable>,
    context: &Context<'a>,
    alias: Content<'a>,
) -> UResult {
    let Content::PartialAlias {
        home,
        name,
        args,
        remaining,
        ..
    } = &alias
    else {
        unreachable!()
    };
    match context.second_desc.content.clone() {
        Content::FlexVar(_) => merge(uf, context, alias),
        other @ Content::PartialAlias { .. } => {
            let Content::PartialAlias {
                home: other_home,
                name: other_name,
                args: other_args,
                remaining: other_remaining,
                ..
            } = &other
            else {
                unreachable!()
            };
            if home != other_home || name != other_name || remaining != other_remaining {
                return Err(());
            }
            unify_alias_args(uf, vars, args, other_args)?;
            merge(uf, context, other)
        }
        Content::Structure(flat) => unify_application_alias(uf, vars, context, flat, alias),
        Content::Error => merge(uf, context, Content::Error),
        _ => Err(()),
    }
}

/// Decompose the supplied suffix while keeping the original closed alias body.
fn unify_application_alias<'a>(
    uf: &mut UnionFind<'a>,
    vars: &mut Vec<Variable>,
    context: &Context<'a>,
    flat: FlatType<'a>,
    alias: Content<'a>,
) -> UResult {
    let FlatType::AppV1(head, applied) = type_::normalize_application(uf, flat) else {
        return Err(());
    };
    let (home, name, args, body, remaining) = match &alias {
        Content::Alias {
            home,
            name,
            args,
            body,
            ..
        } => (*home, *name, args, *body, &[][..]),
        Content::PartialAlias {
            home,
            name,
            args,
            body,
            remaining,
        } => (*home, *name, args, *body, remaining.as_slice()),
        _ => unreachable!(),
    };
    if applied.is_empty() || applied.len() > args.len() {
        return Err(());
    }
    let split = args.len() - applied.len();
    let partial = fresh(
        uf,
        vars,
        context,
        Content::PartialAlias {
            home,
            name,
            args: args[..split].to_vec(),
            body,
            remaining: args[split..]
                .iter()
                .map(|(name, _)| *name)
                .chain(remaining.iter().copied())
                .collect(),
        },
    );
    sub_unify(uf, vars, head, partial)?;
    let suffix: Vec<_> = args[split..].iter().map(|(_, var)| *var).collect();
    unify_args(uf, vars, &applied, &suffix)?;
    merge(uf, context, alias)
}

#[allow(clippy::too_many_arguments)]
fn unify_alias<'a>(
    uf: &mut UnionFind<'a>,
    vars: &mut Vec<Variable>,
    context: &Context<'a>,
    home: nash_ast::ModuleName<'a>,
    name: &'a str,
    args: Vec<(&'a str, Variable)>,
    real_var: Variable,
    body: &'a nash_region::Located<nash_ast::Type<'a>>,
) -> UResult {
    let nominal = matches!(body.value, nash_ast::Type::Record { .. });
    match context.second_desc.content.clone() {
        Content::PartialAlias { .. } => Err(()),
        Content::FlexVar(_) => merge(
            uf,
            context,
            Content::Alias {
                home,
                name,
                args,
                real: real_var,
                body,
            },
        ),

        Content::RigidVar(_) if nominal => Err(()),
        Content::RigidVar(_) => sub_unify(uf, vars, real_var, context.second),

        Content::Alias {
            home: other_home,
            name: other_name,
            args: other_args,
            real: other_real_var,
            body: other_body,
        } => {
            if name == other_name && home == other_home {
                unify_alias_args(uf, vars, &args, &other_args)?;
                merge(
                    uf,
                    context,
                    Content::Alias {
                        home: other_home,
                        name: other_name,
                        args: other_args,
                        real: other_real_var,
                        body: other_body,
                    },
                )
            } else if !nominal {
                // Unwrap only transparent sides so the record's identity survives.
                sub_unify(uf, vars, real_var, context.second)
            } else if !matches!(other_body.value, nash_ast::Type::Record { .. }) {
                sub_unify(uf, vars, context.first, other_real_var)
            } else {
                Err(())
            }
        }

        Content::Structure(flat @ FlatType::AppV1(..)) => unify_application_alias(
            uf,
            vars,
            context,
            flat,
            Content::Alias {
                home,
                name,
                args,
                body,
                real: real_var,
            },
        ),
        Content::Structure(_) if nominal => Err(()),
        Content::Structure(_) => sub_unify(uf, vars, real_var, context.second),

        Content::Error => merge(uf, context, Content::Error),
    }
}

/// Like `unify_args`, but over `(name, variable)` pairs.
fn unify_alias_args<'a>(
    uf: &mut UnionFind<'a>,
    vars: &mut Vec<Variable>,
    args1: &[(&'a str, Variable)],
    args2: &[(&'a str, Variable)],
) -> UResult {
    let mut failed = false;
    for ((_, arg1), (_, arg2)) in args1.iter().zip(args2.iter()) {
        if sub_unify(uf, vars, *arg1, *arg2).is_err() {
            failed = true;
        }
    }
    if failed || args1.len() != args2.len() {
        Err(())
    } else {
        Ok(())
    }
}

// UNIFY STRUCTURES

fn unify_structure<'a>(
    uf: &mut UnionFind<'a>,
    vars: &mut Vec<Variable>,
    context: &Context<'a>,
    flat_type: FlatType<'a>,
) -> UResult {
    match context.second_desc.content.clone() {
        Content::FlexVar(_) => merge(uf, context, Content::Structure(flat_type)),

        Content::RigidVar(_) => Err(()),

        alias @ Content::PartialAlias { .. } => {
            unify_application_alias(uf, vars, context, flat_type, alias)
        }
        alias @ Content::Alias { .. } => {
            if matches!(flat_type, FlatType::AppV1(..)) {
                unify_application_alias(uf, vars, context, flat_type, alias)
            } else {
                let Content::Alias { real, body, .. } = alias else {
                    unreachable!()
                };
                if matches!(body.value, nash_ast::Type::Record { .. }) {
                    return Err(());
                }
                sub_unify(uf, vars, context.first, real)
            }
        }

        Content::Structure(other_flat_type) => match (
            nash_constrain::type_::normalize_application(uf, flat_type),
            nash_constrain::type_::normalize_application(uf, other_flat_type),
        ) {
            (FlatType::AppV1(f, xs), FlatType::AppV1(g, ys)) => {
                let (short, long, short_head, long_head) = if xs.len() <= ys.len() {
                    (xs, ys, f, g)
                } else {
                    (ys, xs, g, f)
                };
                let extra = long.len() - short.len();
                if extra == 0 {
                    sub_unify(uf, vars, short_head, long_head)?;
                } else {
                    let partial = fresh(
                        uf,
                        vars,
                        context,
                        Content::Structure(FlatType::AppV1(long_head, long[..extra].to_vec())),
                    );
                    sub_unify(uf, vars, short_head, partial)?;
                }
                unify_args(uf, vars, &short, &long[extra..])?;
                merge(
                    uf,
                    context,
                    Content::Structure(FlatType::AppV1(long_head, long)),
                )
            }
            (FlatType::AppV1(head, args), FlatType::App1(home, name, known))
            | (FlatType::App1(home, name, known), FlatType::AppV1(head, args)) => {
                if args.len() > known.len() {
                    return Err(());
                }
                let split = known.len() - args.len();
                let partial = fresh(
                    uf,
                    vars,
                    context,
                    Content::Structure(FlatType::App1(home, name, known[..split].to_vec())),
                );
                sub_unify(uf, vars, head, partial)?;
                unify_args(uf, vars, &args, &known[split..])?;
                merge(
                    uf,
                    context,
                    Content::Structure(FlatType::App1(home, name, known)),
                )
            }
            (
                FlatType::App1(home, name, args),
                FlatType::App1(other_home, other_name, other_args),
            ) if home == other_home && name == other_name => {
                unify_args(uf, vars, &args, &other_args)?;
                merge(
                    uf,
                    context,
                    Content::Structure(FlatType::App1(other_home, other_name, other_args)),
                )
            }

            (FlatType::Fun1(arg1, res1), FlatType::Fun1(arg2, res2)) => {
                sub_unify(uf, vars, arg1, arg2)?;
                sub_unify(uf, vars, res1, res2)?;
                merge(uf, context, Content::Structure(FlatType::Fun1(arg2, res2)))
            }

            (FlatType::Record1(fields1), FlatType::Record1(fields2)) => {
                unify_record(uf, vars, context, fields1, fields2)
            }

            (FlatType::Tuple1(a, b, rest), FlatType::Tuple1(x, y, other))
                if rest.len() == other.len() =>
            {
                sub_unify(uf, vars, a, x)?;
                sub_unify(uf, vars, b, y)?;
                unify_args(uf, vars, &rest, &other)?;
                merge(
                    uf,
                    context,
                    Content::Structure(FlatType::Tuple1(x, y, other)),
                )
            }

            _ => Err(()),
        },

        Content::Error => merge(uf, context, Content::Error),
    }
}

// UNIFY ARGS

/// Elm keeps unifying the remaining argument pairs after a mismatch (with
/// the error continuation as both continuations) and fails at the end.
fn unify_args<'a>(
    uf: &mut UnionFind<'a>,
    vars: &mut Vec<Variable>,
    args1: &[Variable],
    args2: &[Variable],
) -> UResult {
    let mut failed = false;
    for (arg1, arg2) in args1.iter().zip(args2.iter()) {
        if sub_unify(uf, vars, *arg1, *arg2).is_err() {
            failed = true;
        }
    }
    if failed || args1.len() != args2.len() {
        Err(())
    } else {
        Ok(())
    }
}

// UNIFY RECORDS

fn unify_record<'a>(
    uf: &mut UnionFind<'a>,
    vars: &mut Vec<Variable>,
    context: &Context<'a>,
    fields1: BTreeMap<&'a str, Variable>,
    fields2: BTreeMap<&'a str, Variable>,
) -> UResult {
    if !fields1.keys().eq(fields2.keys()) {
        return Err(());
    }
    let mut failed = false;
    for (a, b) in fields1.values().zip(fields2.values()) {
        failed |= sub_unify(uf, vars, *a, *b).is_err();
    }
    if failed {
        Err(())
    } else {
        merge(uf, context, Content::Structure(FlatType::Record1(fields1)))
    }
}

#[cfg(test)]
mod predicate_tests {
    use super::*;
    use type_::PredId;

    #[test]
    fn failed_composite_unification_poison_reaches_shared_children() {
        let bump = Bump::new();
        let mut uf = UnionFind::new();
        let shared = type_::mk_flex_var(&mut uf);
        let unit = |uf: &mut UnionFind<'_>| {
            uf.fresh(type_::make_descriptor(Content::Structure(FlatType::App1(
                nash_ast::primitives::builtin_home(),
                "unit",
                vec![],
            ))))
        };
        let first = unit(&mut uf);
        let second = unit(&mut uf);
        let record = uf.fresh(type_::make_descriptor(Content::Structure(
            FlatType::Record1(BTreeMap::new()),
        )));
        let left = uf.fresh(type_::make_descriptor(Content::Structure(
            FlatType::Tuple1(shared, first, vec![]),
        )));
        let right = uf.fresh(type_::make_descriptor(Content::Structure(
            FlatType::Tuple1(second, record, vec![]),
        )));
        assert!(matches!(
            unify(&bump, &mut uf, left, right),
            Answer::Err(..)
        ));
        let later = uf.fresh(type_::make_descriptor(Content::Structure(
            FlatType::Record1(BTreeMap::new()),
        )));
        assert!(
            matches!(unify(&bump, &mut uf, shared, later), Answer::Ok(_)),
            "partial child assignments must not cause a second mismatch"
        );
        assert!(matches!(uf.get(shared).content, Content::Error));
    }

    #[test]
    fn application_head_and_argument_cycles_are_detected_and_rendered() {
        for head_cycle in [true, false] {
            let bump = Bump::new();
            let mut uf = UnionFind::new();
            let f = type_::mk_flex_var(&mut uf);
            let a = type_::mk_flex_var(&mut uf);
            let b = type_::mk_flex_var(&mut uf);
            let left = uf.fresh(type_::make_descriptor(Content::Structure(FlatType::AppV1(
                f,
                vec![a],
            ))));
            let right_args = if head_cycle {
                vec![b, a]
            } else {
                let nested = uf.fresh(type_::make_descriptor(Content::Structure(FlatType::App1(
                    nash_ast::ModuleName {
                        package: None,
                        name: "Main",
                    },
                    "Box",
                    vec![a],
                ))));
                vec![nested]
            };
            let right = uf.fresh(type_::make_descriptor(Content::Structure(FlatType::AppV1(
                f, right_args,
            ))));
            assert!(matches!(unify(&bump, &mut uf, left, right), Answer::Ok(_)));
            assert!(crate::occurs::occurs(&mut uf, left));
            let error = annotation::to_error_type(&bump, &mut uf, left);
            assert!(format!("{error:?}").contains("Infinite"));
        }
    }

    #[test]
    fn linked_application_heads_terminate_during_normalization() {
        let bump = Bump::new();
        let mut uf = UnionFind::new();
        let f = type_::mk_flex_var(&mut uf);
        let g = type_::mk_flex_var(&mut uf);
        let a = type_::mk_flex_var(&mut uf);
        uf.modify(f, |desc| {
            desc.content = Content::Structure(FlatType::AppV1(g, vec![a]))
        });
        uf.modify(g, |desc| {
            desc.content = Content::Structure(FlatType::AppV1(f, vec![a]))
        });
        assert!(crate::occurs::occurs(&mut uf, f));
        let error = annotation::to_error_type(&bump, &mut uf, f);
        assert!(format!("{error:?}").contains("Infinite"));
    }

    #[test]
    fn merges_preserve_obligations_added_after_context_capture() {
        let mut uf = UnionFind::new();
        let a = type_::mk_flex_var(&mut uf);
        let b = type_::mk_flex_var(&mut uf);
        let c = type_::mk_flex_var(&mut uf);
        uf.modify(a, |desc| desc.preds = vec![PredId(0), PredId(1)]);
        uf.modify(b, |desc| desc.preds = vec![PredId(1)]);
        uf.modify(c, |desc| desc.preds = vec![PredId(2)]);
        let context = Context {
            first: a,
            first_desc: uf.get(a).clone(),
            second: b,
            second_desc: uf.get(b).clone(),
        };
        let bump = Bump::new();
        assert!(matches!(unify(&bump, &mut uf, a, c), Answer::Ok(_)));
        merge(
            &mut uf,
            &context,
            Content::Structure(FlatType::App1(
                nash_ast::primitives::builtin_home(),
                "unit",
                Vec::new(),
            )),
        )
        .unwrap();
        for var in [a, b, c] {
            assert_eq!(uf.get(var).preds, [PredId(0), PredId(1), PredId(2)]);
        }

        let record = uf.fresh(type_::make_descriptor(Content::Structure(
            FlatType::Record1(BTreeMap::new()),
        )));
        uf.modify(record, |desc| desc.preds = vec![PredId(3)]);
        assert!(matches!(unify(&bump, &mut uf, b, record), Answer::Err(..)));
        assert_eq!(
            uf.get(a).preds,
            [PredId(0), PredId(1), PredId(2), PredId(3)]
        );
    }
}
