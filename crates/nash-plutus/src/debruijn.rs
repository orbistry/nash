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
    convert(arena, &mut Vec::new(), term)
}

fn convert<'a>(
    arena: &'a Arena,
    scope: &mut Vec<usize>,
    term: &'a Term<'a, Name<'a>>,
) -> Result<&'a Term<'a, DeBruijn>, FreeVariable<'a>> {
    Ok(match term {
        Term::Var(name) => {
            let position = scope
                .iter()
                .rposition(|unique| *unique == name.unique())
                .ok_or(FreeVariable(name))?;
            Term::var(arena, DeBruijn::new(arena, scope.len() - position))
        }
        Term::Lambda { parameter, body } => {
            scope.push(parameter.unique());
            let result = convert(arena, scope, body);
            scope.pop();
            result?.lambda(arena, DeBruijn::zero(arena))
        }
        Term::Apply { function, argument } => {
            convert(arena, scope, function)?.apply(arena, convert(arena, scope, argument)?)
        }
        Term::Delay(t) => convert(arena, scope, t)?.delay(arena),
        Term::Force(t) => convert(arena, scope, t)?.force(arena),
        Term::Constant(c) => Term::constant(arena, c),
        Term::Builtin(f) => Term::builtin(arena, f),
        Term::Error => Term::error(arena),
        Term::Constr { tag, fields } => {
            let fields = fields
                .iter()
                .map(|f| convert(arena, scope, f))
                .collect::<Result<Vec<_>, _>>()?;
            Term::constr(arena, *tag, arena.alloc_slice_copy(&fields))
        }
        Term::Case { constr, branches } => {
            let constr = convert(arena, scope, constr)?;
            let branches = branches
                .iter()
                .map(|b| convert(arena, scope, b))
                .collect::<Result<Vec<_>, _>>()?;
            Term::case(arena, constr, arena.alloc_slice_copy(&branches))
        }
    })
}
