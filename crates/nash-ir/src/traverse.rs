//! Borrowed Core traversal; shared subtrees are visited once per occurrence.
use crate::{
    build::Builder,
    core::{Branch, Core, RecBinder},
};
use std::ptr;

/// Bottom-up map. The callback sees mapped children and can replace the node;
/// replacement subtrees are not traversed again. No-op nodes and child slices
/// retain their original pointers. Binder/type metadata is never rewritten.
pub fn map<'a>(
    build: &Builder<'a>,
    core: &'a Core<'a>,
    f: &mut impl FnMut(&'a Core<'a>) -> Option<&'a Core<'a>>,
) -> &'a Core<'a> {
    let changed = match core {
        Core::Var(_) | Core::Lit(_) | Core::Error => None,
        Core::Lam { params, body } => {
            let mapped = map(build, body, f);
            (!ptr::eq(*body, mapped)).then_some(Core::Lam {
                params,
                body: mapped,
            })
        }
        Core::App { func, args } => {
            let mapped = map(build, func, f);
            let mapped_args = map_slice(build, args, |arg| {
                let value = map(build, arg, f);
                (value, !ptr::eq(arg, value))
            });
            (!ptr::eq(*func, mapped) || !ptr::eq(*args, mapped_args)).then_some(Core::App {
                func: mapped,
                args: mapped_args,
            })
        }
        Core::Let {
            binder,
            value,
            body,
        } => {
            let v = map(build, value, f);
            let t = map(build, body, f);
            (!ptr::eq(*value, v) || !ptr::eq(*body, t)).then_some(Core::Let {
                binder: *binder,
                value: v,
                body: t,
            })
        }
        Core::LetRec { binders, body } => {
            let mapped = map_slice(build, binders, |rb| {
                let body = map(build, rb.body, f);
                (RecBinder { body, ..rb }, !ptr::eq(rb.body, body))
            });
            let t = map(build, body, f);
            (!ptr::eq(*binders, mapped) || !ptr::eq(*body, t)).then_some(Core::LetRec {
                binders: mapped,
                body: t,
            })
        }
        Core::Case {
            kind,
            scrutinee,
            branches,
            default,
        } => {
            let s = map(build, scrutinee, f);
            let bs = map_slice(build, branches, |branch| {
                let body = map(build, branch.body, f);
                (Branch { body, ..branch }, !ptr::eq(branch.body, body))
            });
            let d = default.map(|body| map(build, body, f));
            let same_default = match (default, d) {
                (Some(a), Some(b)) => ptr::eq(*a, b),
                (None, None) => true,
                _ => false,
            };
            (!ptr::eq(*scrutinee, s) || !ptr::eq(*branches, bs) || !same_default).then_some(
                Core::Case {
                    kind: *kind,
                    scrutinee: s,
                    branches: bs,
                    default: d,
                },
            )
        }
        Core::Constr { tag, fields } => {
            let mapped = map_slice(build, fields, |arg| {
                let value = map(build, arg, f);
                (value, !ptr::eq(arg, value))
            });
            (!ptr::eq(*fields, mapped)).then_some(Core::Constr {
                tag: *tag,
                fields: mapped,
            })
        }
        Core::Field {
            record,
            index,
            arity,
        } => {
            let mapped = map(build, record, f);
            (!ptr::eq(*record, mapped)).then_some(Core::Field {
                record: mapped,
                index: *index,
                arity: *arity,
            })
        }
        Core::Builtin { func, args } => {
            let mapped = map_slice(build, args, |arg| {
                let value = map(build, arg, f);
                (value, !ptr::eq(arg, value))
            });
            (!ptr::eq(*args, mapped)).then_some(Core::Builtin {
                func: *func,
                args: mapped,
            })
        }
        Core::Cast {
            kind,
            from,
            to,
            arg,
        } => {
            let mapped = map(build, arg, f);
            (!ptr::eq(*arg, mapped)).then_some(Core::Cast {
                kind: *kind,
                from: *from,
                to: *to,
                arg: mapped,
            })
        }
        Core::Trace { message, body } => {
            let m = map(build, message, f);
            let t = map(build, body, f);
            (!ptr::eq(*message, m) || !ptr::eq(*body, t)).then_some(Core::Trace {
                message: m,
                body: t,
            })
        }
        Core::Delay(body) => {
            let mapped = map(build, body, f);
            (!ptr::eq(*body, mapped)).then_some(Core::Delay(mapped))
        }
        Core::Force(body) => {
            let mapped = map(build, body, f);
            (!ptr::eq(*body, mapped)).then_some(Core::Force(mapped))
        }
    };
    let node = changed.map_or(core, |value| &*build.arena.alloc(value));
    f(node).unwrap_or(node)
}

fn map_slice<'a, T: Copy>(
    build: &Builder<'a>,
    slice: &'a [T],
    mut f: impl FnMut(T) -> (T, bool),
) -> &'a [T] {
    let mut changed: Option<Vec<T>> = None;
    for (index, item) in slice.iter().copied().enumerate() {
        let (item, different) = f(item);
        if different && changed.is_none() {
            changed = Some(slice[..index].to_vec());
        }
        if let Some(items) = &mut changed {
            items.push(item);
        }
    }
    match changed {
        Some(items) => build.arena.alloc_slice_copy(&items),
        None => slice,
    }
}

#[cfg(test)]
mod tests;
