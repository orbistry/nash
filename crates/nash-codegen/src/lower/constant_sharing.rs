//! Share one literal leading argument of a builtin at two or more occurrences.
use super::{Error, Lower, Uplc};
use nash_plutus::{binder::Name, builtin::DefaultFunction, constant::Constant, term::Term};

struct Entry<'a> {
    func: DefaultFunction,
    literal: &'a Constant<'a>,
    value: Uplc<'a>,
    uses: usize,
    name: Option<&'a Name<'a>>,
}
fn prefix<'a>(lower: &Lower<'a>, term: Uplc<'a>) -> Option<(DefaultFunction, &'a Constant<'a>)> {
    let Term::Apply {
        function,
        argument: Term::Constant(literal),
    } = term
    else {
        return None;
    };
    let func = match function {
        Term::Builtin(func) if func.force_count() == 0 => **func,
        Term::Var(name) => {
            lower
                .shared
                .iter()
                .find(|(_, cached)| cached.unique() == name.unique())?
                .0
        }
        _ => return None,
    };
    (func.arity() > 1).then_some((func, literal))
}

pub(super) fn share<'a>(lower: &mut Lower<'a>, root: Uplc<'a>) -> Result<Uplc<'a>, Error> {
    let mut entries: Vec<Entry<'a>> = Vec::new();
    let mut pending = vec![root];
    while let Some(term) = pending.pop() {
        if let Some((func, literal)) = prefix(lower, term) {
            if let Some(entry) = entries
                .iter_mut()
                .find(|e| e.func == func && e.literal == literal)
            {
                entry.uses += 1;
            } else {
                entries.push(Entry {
                    func,
                    literal,
                    value: term,
                    uses: 1,
                    name: None,
                });
            }
        }
        match term {
            Term::Apply { function, argument } => pending.extend([*argument, *function]),
            Term::Lambda { body, .. } | Term::Delay(body) | Term::Force(body) => pending.push(body),
            Term::Case { constr, branches } => {
                pending.extend(branches.iter().rev().copied());
                pending.push(constr);
            }
            Term::Constr { fields, .. } => pending.extend(fields.iter().rev().copied()),
            Term::Var(_) | Term::Constant(_) | Term::Builtin(_) | Term::Error => {}
        }
    }
    entries.retain(|entry| entry.uses >= 2);
    if entries.is_empty() {
        return Ok(root);
    }
    for entry in &mut entries {
        entry.name = Some(Name::new(lower.arena, "partial", lower.fresh()?.unique()));
    }
    let mut result = rewrite(lower, root, &entries);
    // These bindings refer only to closed constants and the outer force cache,
    // not to one another. No source computation is moved or executed here.
    for entry in entries.iter().rev() {
        result = result
            .lambda(lower.arena, entry.name.unwrap())
            .apply(lower.arena, entry.value);
    }
    Ok(result)
}

fn rewrite<'a>(lower: &Lower<'a>, term: Uplc<'a>, entries: &[Entry<'a>]) -> Uplc<'a> {
    let mut pending = vec![(term, false, 0)];
    let mut results: Vec<Uplc<'a>> = Vec::new();
    let a = lower.arena;
    while let Some((term, finish, start)) = pending.pop() {
        if !finish {
            if let Some((func, literal)) = prefix(lower, term)
                && let Some(entry) = entries
                    .iter()
                    .find(|e| e.func == func && e.literal == literal)
            {
                results.push(Term::var(a, entry.name.unwrap()));
                continue;
            }
            pending.push((term, true, results.len()));
            match term {
                Term::Apply { function, argument } => {
                    pending.push((argument, false, 0));
                    pending.push((function, false, 0));
                }
                Term::Lambda { body, .. } | Term::Force(body) | Term::Delay(body) => {
                    pending.push((body, false, 0))
                }
                Term::Case { constr, branches } => {
                    pending.push((constr, false, 0));
                    pending.extend(branches.iter().rev().map(|t| (*t, false, 0)));
                }
                Term::Constr { fields, .. } => {
                    pending.extend(fields.iter().rev().map(|t| (*t, false, 0)))
                }
                _ => {}
            }
        } else {
            let mut children = results.drain(start..);
            let result = match term {
                Term::Apply { .. } => children.next().unwrap().apply(a, children.next().unwrap()),
                Term::Lambda { parameter, .. } => children.next().unwrap().lambda(a, parameter),
                Term::Force(_) => children.next().unwrap().force(a),
                Term::Delay(_) => children.next().unwrap().delay(a),
                Term::Case { branches, .. } => {
                    let branches: Vec<_> = children.by_ref().take(branches.len()).collect();
                    Term::case(a, children.next().unwrap(), a.alloc_slice_copy(&branches))
                }
                Term::Constr { tag, .. } => Term::constr(
                    a,
                    *tag,
                    a.alloc_slice_copy(&children.by_ref().collect::<Vec<_>>()),
                ),
                _ => term,
            };
            drop(children);
            results.push(result);
        }
    }
    results.pop().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nash_ir::{
        build::Builder,
        ty::{ConstTy, Ty},
    };
    use nash_plutus::arena::Arena;
    #[test]
    fn sharing_is_idempotent() {
        let arena = Arena::new();
        let b = Builder::new(&arena);
        let int = Ty::Const(&ConstTy::Int);
        let value = b.builtin(
            DefaultFunction::SubtractInteger,
            &[b.int(100), b.int(1)],
            int,
        );
        let root = b.builtin(DefaultFunction::AddInteger, &[value, value], int);
        let mut lower = Lower {
            arena: &arena,
            next_unique: 1,
            share: true,
            shared: Vec::new(),
        };
        let root = lower.term(root).unwrap();
        let once = share(&mut lower, root).unwrap();
        let twice = share(&mut lower, once).unwrap();
        assert!(std::ptr::eq(once, twice));
    }
}
