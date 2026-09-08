//! Port of the data half of Elm's `Type.Error`: the tree that type errors
//! describe types with, after unification variables have been resolved.
//!
//! The `toDoc`/`toComparison` rendering machinery is deferred along with the
//! rest of error reporting (nash stores error data only, like `nash-parse`
//! and `nash-can`).

use nash_ast::ModuleName;

/// Elm's `Type.Error.Type`. Bump-allocated; maps become name-sorted slices.
#[derive(Clone, Copy, Debug)]
pub enum ErrorType<'a> {
    VarApp(&'a ErrorType<'a>, &'a [&'a ErrorType<'a>]),
    Lambda(
        &'a ErrorType<'a>,
        &'a ErrorType<'a>,
        &'a [&'a ErrorType<'a>],
    ),
    Infinite,
    Error,
    FlexVar(&'a str),
    RigidVar(&'a str),
    Type {
        home: ModuleName<'a>,
        name: &'a str,
        args: &'a [&'a ErrorType<'a>],
    },
    Record {
        fields: &'a [(&'a str, &'a ErrorType<'a>)],
    },
    Tuple(
        &'a ErrorType<'a>,
        &'a ErrorType<'a>,
        &'a [&'a ErrorType<'a>],
    ),
    Alias {
        home: ModuleName<'a>,
        name: &'a str,
        args: &'a [(&'a str, &'a ErrorType<'a>)],
        real: &'a ErrorType<'a>,
    },
}

pub fn iterated_dealias<'a>(tipe: &'a ErrorType<'a>) -> &'a ErrorType<'a> {
    match tipe {
        ErrorType::Alias { real, .. } => iterated_dealias(real),
        _ => tipe,
    }
}
