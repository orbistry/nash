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
use nash_constrain::type_::{
    self, Content, Descriptor, FlatType, NO_MARK, NO_RANK, unnamed_flex_var,
};
use nash_constrain::{UnionFind, Variable};

use crate::annotation;

// UNIFY

#[derive(Debug)]
pub enum Answer<'a> {
    Ok(Vec<Variable>),
    Err(Vec<Variable>, &'a ErrorType<'a>, &'a ErrorType<'a>),
}

pub fn unify<'a>(bump: &'a Bump, uf: &mut UnionFind<'a>, v1: Variable, v2: Variable) -> Answer<'a> {
    let mut vars = Vec::new();
    match guarded_unify(uf, &mut vars, v1, v2) {
        Ok(()) => Answer::Ok(vars),
        Err(()) => {
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
            Answer::Err(vars, t1, t2)
        }
    }
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
        Content::FlexVar(_) => unify_flex(uf, &context),

        Content::RigidVar(_) => unify_rigid(uf, &context),

        Content::Alias {
            home,
            name,
            args,
            real,
        } => unify_alias(uf, vars, &context, home, name, args, real),

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

        other @ (Content::RigidVar(_) | Content::Alias { .. } | Content::Structure(_)) => {
            merge(uf, context, other)
        }
    }
}

// UNIFY RIGID VARIABLES

fn unify_rigid<'a>(uf: &mut UnionFind<'a>, context: &Context<'a>) -> UResult {
    let content = context.first_desc.content.clone();
    match &context.second_desc.content {
        Content::FlexVar(_) => merge(uf, context, content),

        Content::RigidVar(_) | Content::Alias { .. } | Content::Structure(_) => Err(()),

        Content::Error => merge(uf, context, Content::Error),
    }
}

// UNIFY ALIASES

#[allow(clippy::too_many_arguments)]
fn unify_alias<'a>(
    uf: &mut UnionFind<'a>,
    vars: &mut Vec<Variable>,
    context: &Context<'a>,
    home: nash_ast::ModuleName<'a>,
    name: &'a str,
    args: Vec<(&'a str, Variable)>,
    real_var: Variable,
) -> UResult {
    match context.second_desc.content.clone() {
        Content::FlexVar(_) => merge(
            uf,
            context,
            Content::Alias {
                home,
                name,
                args,
                real: real_var,
            },
        ),

        Content::RigidVar(_) => sub_unify(uf, vars, real_var, context.second),

        Content::Alias {
            home: other_home,
            name: other_name,
            args: other_args,
            real: other_real_var,
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
                    },
                )
            } else {
                sub_unify(uf, vars, real_var, other_real_var)
            }
        }

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

        Content::Alias { real, .. } => sub_unify(uf, vars, context.first, real),

        Content::Structure(other_flat_type) => match (flat_type, other_flat_type) {
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

            (FlatType::EmptyRecord1, FlatType::EmptyRecord1) => {
                merge(uf, context, Content::Structure(FlatType::EmptyRecord1))
            }

            (FlatType::Record1(fields, ext), FlatType::EmptyRecord1) if fields.is_empty() => {
                sub_unify(uf, vars, ext, context.second)
            }

            (FlatType::EmptyRecord1, FlatType::Record1(fields, ext)) if fields.is_empty() => {
                sub_unify(uf, vars, context.first, ext)
            }

            (FlatType::Record1(fields1, ext1), FlatType::Record1(fields2, ext2)) => {
                let structure1 = gather_fields(uf, fields1, ext1);
                let structure2 = gather_fields(uf, fields2, ext2);
                unify_record(uf, vars, context, structure1, structure2)
            }

            (FlatType::Tuple1(a, b, None), FlatType::Tuple1(x, y, None)) => {
                sub_unify(uf, vars, a, x)?;
                sub_unify(uf, vars, b, y)?;
                merge(
                    uf,
                    context,
                    Content::Structure(FlatType::Tuple1(x, y, None)),
                )
            }

            (FlatType::Tuple1(a, b, Some(c)), FlatType::Tuple1(x, y, Some(z))) => {
                sub_unify(uf, vars, a, x)?;
                sub_unify(uf, vars, b, y)?;
                sub_unify(uf, vars, c, z)?;
                merge(
                    uf,
                    context,
                    Content::Structure(FlatType::Tuple1(x, y, Some(z))),
                )
            }

            (FlatType::Unit1, FlatType::Unit1) => {
                merge(uf, context, Content::Structure(FlatType::Unit1))
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
    structure1: RecordStructure<'a>,
    structure2: RecordStructure<'a>,
) -> UResult {
    let RecordStructure {
        fields: fields1,
        ext: ext1,
    } = structure1;
    let RecordStructure {
        fields: fields2,
        ext: ext2,
    } = structure2;

    let shared_fields: BTreeMap<&'a str, (Variable, Variable)> = fields1
        .iter()
        .filter_map(|(name, var1)| fields2.get(name).map(|var2| (*name, (*var1, *var2))))
        .collect();
    let unique_fields1: BTreeMap<&'a str, Variable> = fields1
        .iter()
        .filter(|(name, _)| !fields2.contains_key(*name))
        .map(|(name, var)| (*name, *var))
        .collect();
    let unique_fields2: BTreeMap<&'a str, Variable> = fields2
        .iter()
        .filter(|(name, _)| !fields1.contains_key(*name))
        .map(|(name, var)| (*name, *var))
        .collect();

    if unique_fields1.is_empty() {
        if unique_fields2.is_empty() {
            sub_unify(uf, vars, ext1, ext2)?;
            unify_shared_fields(uf, vars, context, shared_fields, BTreeMap::new(), ext1)
        } else {
            let sub_record = fresh(
                uf,
                vars,
                context,
                Content::Structure(FlatType::Record1(unique_fields2, ext2)),
            );
            sub_unify(uf, vars, ext1, sub_record)?;
            unify_shared_fields(
                uf,
                vars,
                context,
                shared_fields,
                BTreeMap::new(),
                sub_record,
            )
        }
    } else if unique_fields2.is_empty() {
        let sub_record = fresh(
            uf,
            vars,
            context,
            Content::Structure(FlatType::Record1(unique_fields1, ext1)),
        );
        sub_unify(uf, vars, sub_record, ext2)?;
        unify_shared_fields(
            uf,
            vars,
            context,
            shared_fields,
            BTreeMap::new(),
            sub_record,
        )
    } else {
        let mut other_fields = unique_fields1.clone();
        other_fields.extend(unique_fields2.iter().map(|(name, var)| (*name, *var)));

        let ext = fresh(uf, vars, context, unnamed_flex_var());
        let sub1 = fresh(
            uf,
            vars,
            context,
            Content::Structure(FlatType::Record1(unique_fields1, ext)),
        );
        let sub2 = fresh(
            uf,
            vars,
            context,
            Content::Structure(FlatType::Record1(unique_fields2, ext)),
        );
        sub_unify(uf, vars, ext1, sub2)?;
        sub_unify(uf, vars, sub1, ext2)?;
        unify_shared_fields(uf, vars, context, shared_fields, other_fields, ext)
    }
}

fn unify_shared_fields<'a>(
    uf: &mut UnionFind<'a>,
    vars: &mut Vec<Variable>,
    context: &Context<'a>,
    shared_fields: BTreeMap<&'a str, (Variable, Variable)>,
    other_fields: BTreeMap<&'a str, Variable>,
    ext: Variable,
) -> UResult {
    let shared_count = shared_fields.len();
    let mut matching_fields: BTreeMap<&'a str, Variable> = BTreeMap::new();
    for (name, (actual, expected)) in shared_fields {
        // A field that fails to unify is dropped, not fatal here; the size
        // comparison below turns any dropped field into a mismatch.
        if sub_unify(uf, vars, actual, expected).is_ok() {
            matching_fields.insert(name, actual);
        }
    }

    if shared_count == matching_fields.len() {
        let mut all_fields = matching_fields;
        for (name, var) in other_fields {
            all_fields.entry(name).or_insert(var);
        }
        merge(
            uf,
            context,
            Content::Structure(FlatType::Record1(all_fields, ext)),
        )
    } else {
        Err(())
    }
}

// GATHER RECORD STRUCTURE

struct RecordStructure<'a> {
    fields: BTreeMap<&'a str, Variable>,
    ext: Variable,
}

fn gather_fields<'a>(
    uf: &mut UnionFind<'a>,
    mut fields: BTreeMap<&'a str, Variable>,
    variable: Variable,
) -> RecordStructure<'a> {
    let mut variable = variable;
    loop {
        match uf.get(variable).content.clone() {
            Content::Structure(FlatType::Record1(sub_fields, sub_ext)) => {
                for (name, var) in sub_fields {
                    fields.entry(name).or_insert(var);
                }
                variable = sub_ext;
            }

            // TODO may be dropping useful alias info here
            Content::Alias { real, .. } => {
                variable = real;
            }

            _ => {
                return RecordStructure {
                    fields,
                    ext: variable,
                };
            }
        }
    }
}

#[cfg(test)]
mod predicate_tests {
    use super::*;
    use type_::PredId;

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
        merge(&mut uf, &context, Content::Structure(FlatType::Unit1)).unwrap();
        for var in [a, b, c] {
            assert_eq!(uf.get(var).preds, [PredId(0), PredId(1), PredId(2)]);
        }

        let record = uf.fresh(type_::make_descriptor(Content::Structure(
            FlatType::EmptyRecord1,
        )));
        uf.modify(record, |desc| desc.preds = vec![PredId(3)]);
        assert!(matches!(unify(&bump, &mut uf, b, record), Answer::Err(..)));
        assert_eq!(
            uf.get(a).preds,
            [PredId(0), PredId(1), PredId(2), PredId(3)]
        );
    }
}
