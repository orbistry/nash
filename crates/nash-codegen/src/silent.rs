//! O2 removes trace operations and their message computations.
use nash_ir::{
    build::Builder,
    core::*,
    hygiene,
    ty::{TermTy, Ty},
};
use nash_plutus::{arena::Arena, builtin::DefaultFunction};
use std::collections::HashMap;

#[derive(Debug, thiserror::Error)]
#[error("trace erasure requires a well-typed trace with at most two arguments")]
pub struct Error;

/// Source trace syntax has already obeyed its silent source policy. Prebuilt
/// Core traces and builtin message expressions are discarded even if they fail
/// or diverge. Returned values retain their ordinary evaluation.
pub fn erase<'a>(arena: &'a Arena, core: &'a Core<'a>) -> Result<&'a Core<'a>, Error> {
    let b = Builder::new(arena);
    let core = hygiene::freshen(&b, core);
    // Expose direct calls through builtin aliases before removing the builtin
    // itself. This also discards a failing message passed to a first-class alias.
    let mut bindings = HashMap::new();
    core.walk(&mut |node| {
        if let CoreKind::Let { binder, value, .. } = node.kind {
            bindings.insert(binder.name.unique, value);
        }
    });
    let core = core.map(&b, &mut |node| {
        let CoreKind::App { func, args } = node.kind else {
            return None;
        };
        let mut trace_args = trace_prefix(func, &bindings)?;
        trace_args.extend_from_slice(args);
        if trace_args.len() <= 2 {
            Some(b.builtin(DefaultFunction::Trace, &trace_args, node.ty))
        } else {
            Some(b.app(trace_args[1], &trace_args[2..], node.ty))
        }
    });
    let mut invalid = false;
    let result = core.map(&b, &mut |node| {
        let replacement = match node.kind {
            CoreKind::Trace { body, .. } => body,
            CoreKind::Builtin {
                func: DefaultFunction::Trace,
                args,
            } => {
                let value = match args {
                    [_, value] => *value,
                    [] | [_] => {
                        let value_ty = if args.is_empty() {
                            peel(&b, node.ty).and_then(|(_, rest)| peel(&b, rest))
                        } else {
                            peel(&b, node.ty)
                        };
                        let Some((param, result)) = value_ty else {
                            invalid = true;
                            return None;
                        };
                        let value = Binder {
                            name: b.fresh("trace_value"),
                            ty: param,
                        };
                        b.with_type(
                            b.lam(&[value], b.var(value.name, result)),
                            if args.is_empty() {
                                peel(&b, node.ty).unwrap().1
                            } else {
                                node.ty
                            },
                        )
                    }
                    _ => {
                        invalid = true;
                        return None;
                    }
                };
                if !args.is_empty() {
                    value
                } else {
                    let message = Binder {
                        name: b.fresh("trace_message"),
                        ty: peel(&b, node.ty).unwrap().0,
                    };
                    b.lam(&[message], value)
                }
            }
            _ => return None,
        };
        Some(b.with_type(replacement, node.ty))
    });
    if invalid { Err(Error) } else { Ok(result) }
}

// Only unsaturated trace references are aliases. A saturated trace returning a
// function may compute that function and must not be copied to its call sites.
fn trace_prefix<'a>(
    mut core: &'a Core<'a>,
    bindings: &HashMap<u32, &'a Core<'a>>,
) -> Option<Vec<&'a Core<'a>>> {
    let mut suffix: Vec<&'a Core<'a>> = Vec::new();
    loop {
        match core.kind {
            CoreKind::Var(name) => core = bindings.get(&name.unique)?,
            CoreKind::App { func, args } => {
                suffix.extend(args.iter().rev().copied());
                core = func;
            }
            CoreKind::Builtin {
                func: DefaultFunction::Trace,
                args,
            } if args.len() + suffix.len() < 2 => {
                let mut result = args.to_vec();
                result.extend(suffix.into_iter().rev());
                return Some(result);
            }
            _ => return None,
        }
    }
}

fn peel<'a>(b: &Builder<'a>, ty: Ty<'a>) -> Option<(Ty<'a>, Ty<'a>)> {
    let Ty::Term(TermTy::Fun(params, result)) = ty else {
        return None;
    };
    let (first, rest) = params.split_first()?;
    Some((
        *first,
        if rest.is_empty() {
            *result
        } else {
            Ty::Term(b.arena.alloc(TermTy::Fun(rest, *result)))
        },
    ))
}

#[cfg(test)]
mod tests;
