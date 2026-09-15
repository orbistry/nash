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
        SourcePattern::Constant(value) => CanPattern::Constant(*value),
        SourcePattern::Pair { first, second } => {
            let (first, second) = crate::accumulate::accumulate2(
                canonicalize(bump, env, first, bindings),
                canonicalize(bump, env, second, bindings),
            )?;
            CanPattern::Pair { first, second }
        }
        SourcePattern::DataList { elements, tail } => {
            let elements = canonicalize_list(bump, env, elements, bindings)?;
            let tail = tail
                .map(|tail| canonicalize(bump, env, tail, bindings))
                .transpose()?;
            CanPattern::DataList { elements, tail }
        }
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

        SourcePattern::DataTuple {
            first,
            second,
            rest,
        }
        | SourcePattern::Tuple {
            first,
            second,
            rest,
        } => {
            let (first, second, rest) = crate::accumulate::accumulate3(
                canonicalize(bump, env, first, bindings),
                canonicalize(bump, env, second, bindings),
                canonicalize_list(bump, env, rest, bindings),
            )?;
            if matches!(&pattern.value, SourcePattern::DataTuple { .. }) {
                CanPattern::DataTuple {
                    first,
                    second,
                    rest,
                }
            } else {
                CanPattern::Tuple {
                    first,
                    second,
                    rest,
                }
            }
        }

        SourcePattern::Constructor {
            region,
            module,
            type_name,
            name,
            args,
            spread,
        } => {
            if let Some(primitive) =
                primitive_constructor(bump, env, *region, *module, *type_name, name)?
            {
                let arity = match primitive {
                    PrimitiveConstructor::Pair => 2,
                    PrimitiveConstructor::Unit => 0,
                };
                let args =
                    arrange_pattern_arguments(bump, *region, name, args, *spread, arity, None)?;
                match primitive {
                    PrimitiveConstructor::Unit => CanPattern::Unit,
                    PrimitiveConstructor::Pair => {
                        let (first, second) = crate::accumulate::accumulate2(
                            canonicalize(bump, env, args[0], bindings),
                            canonicalize(bump, env, args[1], bindings),
                        )?;
                        CanPattern::Pair { first, second }
                    }
                }
            } else {
                let ctor = resolve_constructor(bump, env, *region, *module, *type_name, name)?;
                let args =
                    arrange_constructor_arguments(bump, *region, name, args, *spread, &ctor)?;
                canonicalize_ctor_pattern(bump, env, pattern.region, name, args, &ctor, bindings)?
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

pub(crate) use nash_ast::primitives::StructuralConstructor as PrimitiveConstructor;

/// Structural runtime constructors are identified only after resolving their
/// declaring type. User types and imported aliases cannot impersonate them.
pub(crate) fn primitive_constructor<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    region: Region,
    module: Option<&'a str>,
    type_name: Option<&'a str>,
    name: &'a str,
) -> Result<Option<PrimitiveConstructor>, Vec<Error<'a>>> {
    let Some(type_name) = type_name else {
        return Ok(None);
    };
    let typ = match module {
        Some(module) => crate::types::find_type_qual(bump, env, region, module, type_name)?,
        None => crate::types::find_type(bump, env, region, type_name)?,
    };
    let environment::Type::Union { home, .. } = typ else {
        return Ok(None);
    };
    Ok(if home == nash_ast::primitives::builtin_home() {
        nash_ast::primitives::structural_constructor(type_name, name)
    } else {
        None
    })
}

/// Resolve an optional type namespace before selecting a constructor. The
/// owning module and declared union, not the spelling of the syntax, determine
/// membership; qualified imports can expose constructors without open imports.
pub(crate) fn resolve_constructor<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    region: Region,
    module: Option<&'a str>,
    type_name: Option<&'a str>,
    name: &'a str,
) -> Result<environment::Ctor<'a>, Vec<Error<'a>>> {
    let Some(type_name) = type_name else {
        return match module {
            Some(module) => env.find_ctor_qual(bump, region, module, name),
            None => env.find_ctor(bump, region, name),
        };
    };
    let typ = match module {
        Some(module) => crate::types::find_type_qual(bump, env, region, module, type_name)?,
        None => crate::types::find_type(bump, env, region, type_name)?,
    };
    let home = match typ {
        environment::Type::Union { home, .. } | environment::Type::Alias { home, .. } => home,
    };
    let matches_owner = |ctor: &environment::Ctor<'a>| match ctor {
        environment::Ctor::Union {
            home: owner,
            type_name: declared,
            ..
        } => *owner == home && *declared == type_name,
        environment::Ctor::Bool {
            home: owner, union, ..
        } => *owner == home && union.name.value == type_name,
        environment::Ctor::RecordCtor { .. } => false,
    };
    for info in env.ctors.get(name).into_iter().chain(
        env.q_ctors
            .values()
            .filter_map(|constructors| constructors.get(name)),
    ) {
        if let environment::Info::Specific(_, ctor) = info
            && matches_owner(ctor)
        {
            return Ok(*ctor);
        }
    }
    Err(vec![Error::NotFoundCtor {
        region,
        prefix: Some(type_name),
        name,
        suggestions: env.possible_ctor_names(bump),
    }])
}

fn arrange_constructor_arguments<'a>(
    bump: &'a Bump,
    region: Region,
    name: &'a str,
    args: &'a [nash_source::PatternArgument<'a>],
    spread: Option<Region>,
    ctor: &environment::Ctor<'a>,
) -> Result<&'a [&'a Located<SourcePattern<'a>>], Vec<Error<'a>>> {
    let (arity, labels) = match ctor {
        environment::Ctor::Union {
            arity,
            union,
            index,
            ..
        } => (
            usize::from(*arity),
            union
                .ctors
                .iter()
                .find(|ctor| ctor.index == *index)
                .and_then(|ctor| ctor.labels),
        ),
        environment::Ctor::Bool { .. } => (0, None),
        environment::Ctor::RecordCtor { .. } => {
            return Err(vec![Error::PatternHasRecordCtor { region, name }]);
        }
    };
    arrange_pattern_arguments(bump, region, name, args, spread, arity, labels)
}

fn arrange_pattern_arguments<'a>(
    bump: &'a Bump,
    region: Region,
    name: &'a str,
    args: &'a [nash_source::PatternArgument<'a>],
    spread: Option<Region>,
    arity: usize,
    labels: Option<&'a [&'a str]>,
) -> Result<&'a [&'a Located<SourcePattern<'a>>], Vec<Error<'a>>> {
    // A nullary constructor has no function field map. A redundant spread on
    // it is accepted by the reference; on constructors with fields it is not.
    if let Some(region) = spread
        && arity != 0
        && args.len() == arity
    {
        return Err(vec![Error::InvalidConstructorPattern {
            region,
            name,
            reason: "The spread is unnecessary: every constructor field is already supplied.",
        }]);
    }
    if args.len() > arity || (spread.is_none() && args.len() != arity) {
        return Err(vec![Error::BadArity {
            region,
            context: BadArityContext::PatternArity,
            name,
            expected: arity,
            actual: args.len(),
        }]);
    }
    let mut ordered = Vec::with_capacity(arity);
    let first_labeled = args
        .iter()
        .position(|arg| arg.label.is_some())
        .unwrap_or(args.len());
    ordered.extend_from_slice(&args[..first_labeled]);
    if let Some(region) = spread {
        let discard = &*bump.alloc(Located::at(region, SourcePattern::Anything));
        ordered.extend((args.len()..arity).map(|_| nash_source::PatternArgument {
            label: None,
            pattern: discard,
        }));
    }
    ordered.extend_from_slice(&args[first_labeled..]);
    if let Some(labels) = labels {
        let mut last_label = None;
        let mut positional_after = 0;
        for arg in &ordered {
            if let Some(label) = arg.label {
                last_label = Some(label);
            } else if last_label.is_some() {
                positional_after += 1;
                if positional_after > 1 {
                    return Err(vec![Error::InvalidConstructorPattern {
                        region: arg.pattern.region,
                        name,
                        reason: "More than one positional argument follows a labeled argument.",
                    }]);
                }
            }
        }
        // Swapping is significant: a label may displace a positional argument.
        // Filling fixed slots instead would reject valid mixed patterns.
        let mut seen = BTreeMap::new();
        let mut index = 0;
        while index < ordered.len() {
            let Some(label) = ordered[index].label else {
                index += 1;
                continue;
            };
            let Some(position) = labels
                .iter()
                .position(|candidate| *candidate == label.value)
            else {
                return Err(vec![Error::LabeledCtorUnknownField {
                    region: label.region,
                    ctor: name,
                    field: label.value,
                }]);
            };
            if position == index {
                seen.insert(label.value, label.region);
                index += 1;
            } else {
                if let Some(first) = seen.insert(label.value, label.region) {
                    return Err(vec![Error::DuplicateField {
                        name: label.value,
                        first,
                        second: label.region,
                    }]);
                }
                ordered.swap(position, index);
            }
        }
    } else if let Some(label) = ordered.iter().find_map(|arg| arg.label) {
        return Err(vec![Error::LabeledCtorUnknownField {
            region: label.region,
            ctor: name,
            field: label.value,
        }]);
    }
    Ok(bump.alloc_slice_fill_iter(ordered.into_iter().map(|arg| arg.pattern)))
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
        environment::Ctor::Bool { union, index, .. } => {
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
                value: *index == 1,
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

#[cfg(test)]
mod tests {
    use super::*;
    use bumpalo::Bump;
    use nash_ast::{CtorOpts, ModuleName, Type as CanType, Union};
    use nash_can::DuplicatePatternContext;
    use nash_can::pattern::verify;

    use nash_can::environment::{Ctor, Env, Info};

    fn empty_env<'a>(_bump: &'a Bump) -> Env<'a> {
        Env {
            kinds: nash_can::kinds::KindEnv::from_interfaces(None),
            traits: Default::default(),
            q_traits: Default::default(),
            home: ModuleName {
                package: None,
                name: "Main",
            },
            vars: Default::default(),
            callables: Default::default(),
            types: Default::default(),
            ctors: Default::default(),
            binops: Default::default(),
            q_vars: Default::default(),
            q_types: Default::default(),
            q_ctors: Default::default(),
            generated_names: Default::default(),
        }
    }

    fn env_with_maybe<'a>(bump: &'a Bump) -> Env<'a> {
        let home = ModuleName {
            package: None,
            name: "Maybe",
        };
        let mut env = empty_env(bump);

        let nothing_ctor = bump.alloc(nash_ast::Ctor {
            labels: None,
            name: "Nothing",
            index: 1,
            arity: 0,
            arguments: &[],
        });
        let just_arg_typ = bump.alloc(Located::at(Region::zero(), CanType::Var("a")));
        let just_ctor = bump.alloc(nash_ast::Ctor {
            labels: None,
            name: "Just",
            index: 0,
            arity: 1,
            arguments: bump.alloc_slice_fill_iter([&*just_arg_typ]),
        });
        let maybe_union: &Union = bump.alloc(Union {
            data_layout: None,
            kind: bump.alloc(nash_ast::Kind::Arrow(
                &nash_ast::Kind::Type,
                &nash_ast::Kind::Type,
            )),
            context: &[],
            name: bump.alloc(Located::at(Region::zero(), "Maybe")),
            parameters: bump.alloc_slice_fill_iter(["a"]),
            ctors: bump.alloc_slice_fill_iter([&*nothing_ctor, &*just_ctor]),
            alternatives: 2,
            options: CtorOpts::Normal,
        });

        // Nothing: arity 0
        let nothing = Ctor::Union {
            home,
            type_name: "Maybe",
            type_vars: &["a"],
            union: maybe_union,
            index: 1,
            arity: 0,
            arguments: &[],
            options: CtorOpts::Normal,
            alternatives: 2,
        };
        env.ctors.insert("Nothing", Info::Specific(home, nothing));

        // Just: arity 1
        let just = Ctor::Union {
            home,
            type_name: "Maybe",
            type_vars: &["a"],
            union: maybe_union,
            index: 0,
            arity: 1,
            arguments: bump.alloc_slice_fill_iter([&*just_arg_typ]),
            options: CtorOpts::Normal,
            alternatives: 2,
        };
        env.ctors.insert("Just", Info::Specific(home, just));

        env
    }

    fn env_with_record_ctor<'a>(bump: &'a Bump) -> Env<'a> {
        let home = ModuleName {
            package: None,
            name: "Main",
        };
        let mut env = empty_env(bump);

        let field_typ: &Located<CanType> =
            bump.alloc(Located::at(Region::zero(), CanType::Var("a")));
        let record_type: &Located<CanType> = bump.alloc(Located::at(
            Region::zero(),
            CanType::Record {
                fields: bump.alloc_slice_fill_iter([
                    nash_ast::FieldType {
                        index: 0,
                        field: "x",
                        typ: field_typ,
                    },
                    nash_ast::FieldType {
                        index: 1,
                        field: "y",
                        typ: field_typ,
                    },
                ]),
            },
        ));
        let ctor = match &record_type.value {
            CanType::Record { fields, .. } => nash_can::environment::make_record_ctor(
                bump,
                home,
                "Point",
                &[],
                record_type,
                fields,
            ),
            _ => unreachable!(),
        };
        env.ctors.insert("Point", Info::Specific(home, ctor));

        env
    }

    fn env_with_bool<'a>(bump: &'a Bump) -> Env<'a> {
        let module = nash_parse::Parser::new(
            bump,
            "module Main exposing (..)\nimport Builtin exposing (type bool(..))\n",
        )
        .module()
        .unwrap();
        let interfaces = std::collections::BTreeMap::from([(
            "Builtin",
            nash_can::kinds::builtin_interface(bump),
        )]);
        nash_can::environment::foreign::create_initial_env(
            bump,
            empty_env(bump).home,
            Some(&interfaces),
            module.imports,
        )
        .unwrap()
    }

    fn parse_pattern<'a>(bump: &'a Bump, input: &str) -> &'a Located<SourcePattern<'a>> {
        let src = bump.alloc_str(input);
        let mut parser = nash_parse::Parser::new(bump, src);
        let (pat, _end) = parser.pattern_expr().expect("expected successful parse");
        pat
    }

    macro_rules! assert_pattern_snapshot {
        ($input:expr, $env_fn:ident) => {{
            let bump = Bump::new();
            let env = $env_fn(&bump);
            let pat = parse_pattern(&bump, $input);
            let result = verify(&bump, &env, DuplicatePatternContext::CaseBranch, pat);
            insta::with_settings!({
                description => $input,
                omit_expression => true,
            }, {
                insta::assert_debug_snapshot!(result.unwrap());
            });
        }};
    }

    macro_rules! assert_pattern_error_snapshot {
        ($input:expr, $env_fn:ident) => {{
            let bump = Bump::new();
            let env = $env_fn(&bump);
            let pat = parse_pattern(&bump, $input);
            let result = verify(&bump, &env, DuplicatePatternContext::CaseBranch, pat);
            insta::with_settings!({info => &"diagnostic",
                description => $input,
                omit_expression => true,
            }, {
                insta::assert_snapshot!(crate::snapshot_support::errors($input, &result.unwrap_err()));
            });
        }};
    }

    // === Success tests ===

    #[test]
    fn wildcard() {
        assert_pattern_snapshot!("_", empty_env);
    }

    #[test]
    fn variable() {
        assert_pattern_snapshot!("x", empty_env);
    }

    #[test]
    fn record_pattern() {
        assert_pattern_snapshot!("{ x, y }", empty_env);
    }

    #[test]
    fn unit() {
        assert_pattern_snapshot!("()", empty_env);
    }

    #[test]
    fn tuple_two() {
        assert_pattern_snapshot!("( a, b )", empty_env);
    }

    #[test]
    fn tuple_three() {
        assert_pattern_snapshot!("( a, b, c )", empty_env);
    }

    #[test]
    fn literal_int() {
        assert_pattern_snapshot!("42", empty_env);
    }

    #[test]
    fn literal_str() {
        assert_pattern_snapshot!(r#""hello""#, empty_env);
    }

    #[test]
    fn list_pattern() {
        assert_pattern_snapshot!("[ a, b ]", empty_env);
    }

    #[test]
    fn cons_pattern() {
        assert_pattern_snapshot!("x :: xs", empty_env);
    }

    #[test]
    fn ctor_no_args() {
        assert_pattern_snapshot!("Nothing", env_with_maybe);
    }

    #[test]
    fn ctor_with_args() {
        assert_pattern_snapshot!("Just x", env_with_maybe);
    }

    #[test]
    fn bool_true_pattern() {
        assert_pattern_snapshot!("True", env_with_bool);
    }

    #[test]
    fn bool_false_pattern() {
        assert_pattern_snapshot!("False", env_with_bool);
    }

    #[test]
    fn alias_pattern() {
        assert_pattern_snapshot!("(x, y) as pair", empty_env);
    }

    // === Error tests ===

    #[test]
    fn tuple_four() {
        assert_pattern_snapshot!("( a, b, c, d )", empty_env);
    }

    #[test]
    fn ctor_wrong_arity() {
        assert_pattern_error_snapshot!("Just x y", env_with_maybe);
    }

    #[test]
    fn ctor_not_found() {
        assert_pattern_error_snapshot!("Foo", empty_env);
    }

    #[test]
    fn record_ctor_in_pattern() {
        assert_pattern_error_snapshot!("Point", env_with_record_ctor);
    }

    #[test]
    fn duplicate_vars() {
        assert_pattern_error_snapshot!("( x, x )", empty_env);
    }

    #[test]
    fn bool_pattern_with_args_is_bad_arity() {
        assert_pattern_error_snapshot!("True x", env_with_bool);
    }

    #[test]
    fn duplicate_across_sibling_patterns() {
        let bump = Bump::new();
        let input = "module Main exposing (..)\nf = \\x x -> x\n";
        let module = nash_parse::Parser::new(&bump, input).module().unwrap();
        let errors =
            nash_can::canonicalize(&bump, nash_can::Context::default(), &module).unwrap_err();
        insta::with_settings!({info => &"diagnostic", description => input, omit_expression => true}, {
            insta::assert_snapshot!(crate::snapshot_support::errors(input, &errors));
        });
    }
}
