use std::collections::BTreeMap;

use bumpalo::Bump;
use nash_ast::{ConstructorName, Pattern as CanPattern, PatternCtor, PatternCtorArg};
use nash_region::{Located, Region};
use nash_source::Pattern as SourcePattern;

use crate::Error;
use crate::environment::{self, Env, dups};
use crate::error::{BadArityContext, DuplicatePatternContext};

pub type Bindings<'a> = BTreeMap<&'a str, Region>;

/// Canonicalize a pattern, detect duplicate bindings, return (pattern, bindings).
/// Mirrors Elm's `Pattern.verify` wrapped around a single pattern.
pub fn verify<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    context: DuplicatePatternContext<'a>,
    pattern: &'a Located<SourcePattern<'a>>,
) -> Result<(&'a Located<CanPattern<'a>>, Bindings<'a>), Vec<Error<'a>>> {
    let (patterns, bindings) = verify_all(bump, env, context, std::slice::from_ref(&pattern))?;
    Ok((patterns[0], bindings))
}

/// Canonicalize several patterns inside ONE duplicate-detection scope,
/// like Elm's `Pattern.verify ctx (traverse (Pattern.canonicalize env) args)`.
/// This is what catches `\x x -> ...` and `f x x = ...` across arguments.
pub fn verify_all<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    context: DuplicatePatternContext<'a>,
    patterns: &[&'a Located<SourcePattern<'a>>],
) -> Result<(Vec<&'a Located<CanPattern<'a>>>, Bindings<'a>), Vec<Error<'a>>> {
    let mut bound: Vec<(&'a str, Region)> = Vec::new();
    let mut results = Vec::with_capacity(patterns.len());
    let mut errors = Vec::new();
    for pattern in patterns {
        match canonicalize(bump, env, pattern, &mut bound) {
            Ok(p) => results.push(p),
            Err(errs) => errors.extend(errs),
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let bindings = detect_duplicates(context, bound)?;
    Ok((results, bindings))
}

/// Run Elm's `Dups.detect (Error.DuplicatePattern context)` over collected
/// bindings. Exposed so typed definitions can share one scope between
/// `gather_typed_args` and the check.
pub fn detect_duplicates<'a>(
    context: DuplicatePatternContext<'a>,
    bound: Vec<(&'a str, Region)>,
) -> Result<Bindings<'a>, Vec<Error<'a>>> {
    dups::detect(bound, |name, first, second| Error::DuplicatePattern {
        context,
        name,
        first,
        second,
    })
}

pub fn canonicalize<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    pattern: &'a Located<SourcePattern<'a>>,
    bindings: &mut Vec<(&'a str, Region)>,
) -> Result<&'a Located<CanPattern<'a>>, Vec<Error<'a>>> {
    let can = match &pattern.value {
        SourcePattern::Anything => CanPattern::Anything,

        SourcePattern::Var(name) => {
            bindings.push((name, pattern.region));
            CanPattern::Var(name)
        }

        SourcePattern::Record(fields) => {
            for field in *fields {
                bindings.push((field.value, field.region));
            }
            let names = bump.alloc_slice_fill_iter(fields.iter().map(|f| f.value));
            CanPattern::Record(names)
        }

        SourcePattern::Alias {
            pattern: inner,
            name,
        } => {
            let can_inner = canonicalize(bump, env, inner, bindings)?;
            bindings.push((name.value, name.region));
            CanPattern::Alias {
                pattern: can_inner,
                name: name.value,
            }
        }

        SourcePattern::Unit => CanPattern::Unit,

        SourcePattern::Tuple {
            first,
            second,
            rest,
        } => {
            let (first, second, rest) = crate::accumulate::accumulate3(
                canonicalize(bump, env, first, bindings),
                canonicalize(bump, env, second, bindings),
                canonicalize_list(bump, env, rest, bindings),
            )?;
            CanPattern::Tuple {
                first,
                second,
                rest,
            }
        }

        SourcePattern::Ctor {
            region: name_region,
            name,
            args,
        } => {
            let ctor = env.find_ctor(bump, *name_region, name)?;
            canonicalize_ctor_pattern(bump, env, pattern.region, name, args, &ctor, bindings)?
        }

        SourcePattern::CtorQual {
            region: name_region,
            module,
            name,
            args,
        } => {
            let ctor = env.find_ctor_qual(bump, *name_region, module, name)?;
            canonicalize_ctor_pattern(bump, env, pattern.region, name, args, &ctor, bindings)?
        }

        SourcePattern::List(pats) => {
            let can_pats = canonicalize_list(bump, env, pats, bindings)?;
            CanPattern::List(can_pats)
        }

        SourcePattern::Cons { head, tail } => {
            let (head, tail) = crate::accumulate::accumulate2(
                canonicalize(bump, env, head, bindings),
                canonicalize(bump, env, tail, bindings),
            )?;
            CanPattern::Cons { head, tail }
        }

        SourcePattern::Str(s) => CanPattern::Str(s),
        SourcePattern::Bytes(bytes) => CanPattern::Bytes(bytes),
        SourcePattern::Int(n) => CanPattern::Int(*n),
    };

    Ok(bump.alloc(Located::at(pattern.region, can)))
}

fn canonicalize_ctor_pattern<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    region: Region,
    name: &'a str,
    args: &'a [&'a Located<SourcePattern<'a>>],
    ctor: &environment::Ctor<'a>,
    bindings: &mut Vec<(&'a str, Region)>,
) -> Result<CanPattern<'a>, Vec<Error<'a>>> {
    match ctor {
        environment::Ctor::Union {
            home,
            type_name,
            type_vars: _,
            union: union_def,
            index,
            arity,
            arguments: expected_types,
            options,
            alternatives,
        } => {
            let mut repeated = Vec::new();
            let args = if let [argument] = args
                && let SourcePattern::Record(names) = &argument.value
                && let Some(labels) = union_def
                    .ctors
                    .iter()
                    .find(|ctor| ctor.index == *index)
                    .and_then(|ctor| ctor.labels)
            {
                let unknown: Vec<_> = names
                    .iter()
                    .filter(|field| !labels.contains(&field.value))
                    .map(|field| Error::LabeledCtorUnknownField {
                        region: field.region,
                        ctor: name,
                        field: field.value,
                    })
                    .collect();
                if !unknown.is_empty() {
                    return Err(unknown);
                }
                // Keep repeated occurrences for the enclosing duplicate-binding check.
                // The expanded variable accounts for the first occurrence of each label.
                let mut seen = std::collections::BTreeSet::new();
                for field in *names {
                    if !seen.insert(field.value) {
                        repeated.push((field.value, field.region));
                    }
                }
                &*bump.alloc_slice_fill_iter(labels.iter().map(|label| {
                    &*bump.alloc(match names.iter().find(|name| name.value == *label) {
                        Some(name) => Located::at(name.region, SourcePattern::Var(name.value)),
                        None => Located::at(argument.region, SourcePattern::Anything),
                    })
                }))
            } else {
                args
            };
            if args.len() != *arity as usize {
                return Err(vec![Error::BadArity {
                    region,
                    context: BadArityContext::PatternArity,
                    name,
                    expected: *arity as usize,
                    actual: args.len(),
                }]);
            }

            let mut ctor_args = Vec::with_capacity(args.len());
            let mut errors = Vec::new();
            for (i, (pat, expected_typ)) in args
                .iter()
                .copied()
                .zip(expected_types.iter().copied())
                .enumerate()
            {
                match canonicalize(bump, env, pat, bindings) {
                    Ok(can_pat) => ctor_args.push(PatternCtorArg {
                        index: i as u16,
                        typ: expected_typ,
                        pattern: can_pat,
                    }),
                    Err(mut e) => errors.append(&mut e),
                }
            }
            bindings.extend(repeated);
            if !errors.is_empty() {
                return Err(errors);
            }

            Ok(CanPattern::Constructor(PatternCtor {
                reference: ConstructorName {
                    home: *home,
                    union: type_name,
                    name,
                },
                union: union_def,
                index: *index,
                arguments: bump.alloc_slice_fill_iter(ctor_args),
                options: *options,
                alternatives: *alternatives,
            }))
        }

        // `True`/`False` are nullary; like Elm, the arity check runs before
        // the Bool decision, so `True x` is a `BadArity` error.
        environment::Ctor::Bool { union, .. } => {
            if !args.is_empty() {
                return Err(vec![Error::BadArity {
                    region,
                    context: BadArityContext::PatternArity,
                    name,
                    expected: 0,
                    actual: args.len(),
                }]);
            }
            Ok(CanPattern::Bool {
                union,
                value: name == "True",
            })
        }

        environment::Ctor::RecordCtor { .. } => {
            Err(vec![Error::PatternHasRecordCtor { region, name }])
        }
    }
}

fn canonicalize_list<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    patterns: &'a [&'a Located<SourcePattern<'a>>],
    bindings: &mut Vec<(&'a str, Region)>,
) -> Result<&'a [&'a Located<CanPattern<'a>>], Vec<Error<'a>>> {
    let mut results = Vec::with_capacity(patterns.len());
    let mut errors = Vec::new();
    for pat in patterns {
        match canonicalize(bump, env, pat, bindings) {
            Ok(p) => results.push(p),
            Err(mut e) => errors.append(&mut e),
        }
    }
    if errors.is_empty() {
        Ok(bump.alloc_slice_fill_iter(results))
    } else {
        Err(errors)
    }
}
