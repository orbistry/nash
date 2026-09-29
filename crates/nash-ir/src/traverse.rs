//! Borrowed Core traversal; shared subtrees are visited once per occurrence.
use crate::{
    build::Builder,
    core::{Branch, Core, CoreKind, RecBinder},
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
    let mut pending = vec![(core, false)];
    let mut results = Vec::new();
    let mut children = Vec::new();
    while let Some((node, finish)) = pending.pop() {
        children.clear();
        node.push_children_reversed(&mut children);
        if !finish {
            pending.push((node, true));
            pending.extend(children.iter().map(|child| (*child, false)));
        } else {
            let start = results.len() - children.len();
            let mapped = rebuild(build, node, &mut results.drain(start..), f);
            results.push(mapped);
        }
    }
    results.pop().unwrap()
}

pub(crate) fn rebuild<'a>(
    build: &Builder<'a>,
    core: &'a Core<'a>,
    children: &mut impl Iterator<Item = &'a Core<'a>>,
    f: &mut impl FnMut(&'a Core<'a>) -> Option<&'a Core<'a>>,
) -> &'a Core<'a> {
    let changed = match &core.kind {
        CoreKind::Var(_) | CoreKind::Lit(_) | CoreKind::Error => None,
        CoreKind::Lam { params, body } => {
            let mapped = children.next().unwrap();
            (!ptr::eq(*body, mapped)).then_some(CoreKind::Lam {
                params,
                body: mapped,
            })
        }
        CoreKind::App { func, args } => {
            let mapped = children.next().unwrap();
            let mapped_args = map_slice(build, args, |arg| {
                let value = children.next().unwrap();
                (value, !ptr::eq(arg, value))
            });
            (!ptr::eq(*func, mapped) || !ptr::eq(*args, mapped_args)).then_some(CoreKind::App {
                func: mapped,
                args: mapped_args,
            })
        }
        CoreKind::Let {
            binder,
            value,
            body,
        } => {
            let v = children.next().unwrap();
            let t = children.next().unwrap();
            (!ptr::eq(*value, v) || !ptr::eq(*body, t)).then_some(CoreKind::Let {
                binder: *binder,
                value: v,
                body: t,
            })
        }
        CoreKind::LetRec { binders, body } => {
            let mapped = map_slice(build, binders, |rb| {
                let body = children.next().unwrap();
                (RecBinder { body, ..rb }, !ptr::eq(rb.body, body))
            });
            let t = children.next().unwrap();
            (!ptr::eq(*binders, mapped) || !ptr::eq(*body, t)).then_some(CoreKind::LetRec {
                binders: mapped,
                body: t,
            })
        }
        CoreKind::Case {
            kind,
            scrutinee,
            branches,
            default,
        } => {
            let s = children.next().unwrap();
            let bs = map_slice(build, branches, |branch| {
                let body = children.next().unwrap();
                (Branch { body, ..branch }, !ptr::eq(branch.body, body))
            });
            let d = default.map(|_| children.next().unwrap());
            let same_default = match (default, d) {
                (Some(a), Some(b)) => ptr::eq(*a, b),
                (None, None) => true,
                _ => false,
            };
            (!ptr::eq(*scrutinee, s) || !ptr::eq(*branches, bs) || !same_default).then_some(
                CoreKind::Case {
                    kind: *kind,
                    scrutinee: s,
                    branches: bs,
                    default: d,
                },
            )
        }
        CoreKind::Constr { tag, fields } => {
            let mapped = map_slice(build, fields, |arg| {
                let value = children.next().unwrap();
                (value, !ptr::eq(arg, value))
            });
            (!ptr::eq(*fields, mapped)).then_some(CoreKind::Constr {
                tag: *tag,
                fields: mapped,
            })
        }
        CoreKind::Field {
            record,
            index,
            arity,
        } => {
            let mapped = children.next().unwrap();
            (!ptr::eq(*record, mapped)).then_some(CoreKind::Field {
                record: mapped,
                index: *index,
                arity: *arity,
            })
        }
        CoreKind::Builtin { func, args } => {
            let mapped = map_slice(build, args, |arg| {
                let value = children.next().unwrap();
                (value, !ptr::eq(arg, value))
            });
            (!ptr::eq(*args, mapped)).then_some(CoreKind::Builtin {
                func: *func,
                args: mapped,
            })
        }
        CoreKind::Trace { message, body } => {
            let m = children.next().unwrap();
            let t = children.next().unwrap();
            (!ptr::eq(*message, m) || !ptr::eq(*body, t)).then_some(CoreKind::Trace {
                message: m,
                body: t,
            })
        }
        CoreKind::Delay(body) => {
            let mapped = children.next().unwrap();
            (!ptr::eq(*body, mapped)).then_some(CoreKind::Delay(mapped))
        }
        CoreKind::Force(body) => {
            let mapped = children.next().unwrap();
            (!ptr::eq(*body, mapped)).then_some(CoreKind::Force(mapped))
        }
    };
    let node = changed.map_or(core, |kind| build.alloc(core.ty, kind));
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
