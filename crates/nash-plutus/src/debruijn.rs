//! Resolve unique names to one-based lexical De Bruijn indices.
use crate::{
    arena::Arena,
    binder::{DeBruijn, Name},
    term::Term,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("free variable {0:?}")]
pub struct FreeVariable<'a>(pub &'a Name<'a>);

pub fn to_debruijn<'a>(
    arena: &'a Arena,
    term: &'a Term<'a, Name<'a>>,
) -> Result<&'a Term<'a, DeBruijn>, FreeVariable<'a>> {
    enum Task<'a> {
        Visit(&'a Term<'a, Name<'a>>),
        Finish(&'a Term<'a, Name<'a>>, usize),
    }
    let mut pending = vec![Task::Visit(term)];
    let mut scope = Vec::new();
    let mut results: Vec<&'a Term<'a, DeBruijn>> = Vec::new();
    while let Some(task) = pending.pop() {
        match task {
            Task::Visit(term) => {
                pending.push(Task::Finish(term, results.len()));
                match term {
                    Term::Lambda { parameter, body } => {
                        scope.push(parameter.unique());
                        pending.push(Task::Visit(body));
                    }
                    Term::Apply { function, argument } => {
                        pending.push(Task::Visit(argument));
                        pending.push(Task::Visit(function));
                    }
                    Term::Delay(body) | Term::Force(body) => pending.push(Task::Visit(body)),
                    Term::Constr { fields, .. } => {
                        pending.extend(fields.iter().rev().map(|t| Task::Visit(t)))
                    }
                    Term::Case { constr, branches } => {
                        pending.extend(branches.iter().rev().map(|t| Task::Visit(t)));
                        pending.push(Task::Visit(constr));
                    }
                    _ => {}
                }
            }
            Task::Finish(term, start) => {
                let mut children = results.drain(start..);
                let result = match term {
                    Term::Var(name) => {
                        let position = scope
                            .iter()
                            .rposition(|id| *id == name.unique())
                            .ok_or(FreeVariable(name))?;
                        Term::var(arena, DeBruijn::new(arena, scope.len() - position))
                    }
                    Term::Lambda { .. } => {
                        scope.pop();
                        children
                            .next()
                            .unwrap()
                            .lambda(arena, DeBruijn::zero(arena))
                    }
                    Term::Apply { .. } => children
                        .next()
                        .unwrap()
                        .apply(arena, children.next().unwrap()),
                    Term::Delay(_) => children.next().unwrap().delay(arena),
                    Term::Force(_) => children.next().unwrap().force(arena),
                    Term::Constant(c) => Term::constant(arena, c),
                    Term::Builtin(f) => Term::builtin(arena, f),
                    Term::Error => Term::error(arena),
                    Term::Constr { tag, .. } => Term::constr(
                        arena,
                        *tag,
                        arena.alloc_slice_copy(&children.by_ref().collect::<Vec<_>>()),
                    ),
                    Term::Case { .. } => {
                        let constr = children.next().unwrap();
                        Term::case(
                            arena,
                            constr,
                            arena.alloc_slice_copy(&children.by_ref().collect::<Vec<_>>()),
                        )
                    }
                };
                drop(children);
                results.push(result);
            }
        }
    }
    Ok(results.pop().unwrap())
}
