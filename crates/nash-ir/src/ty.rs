//! Runtime types with explicit representations. Erased types only annotate
//! representation-independent, unconstrained pass-through binders; constant
//! construction and casts require concrete metadata.

use nash_ast::{QualifiedName, primitives::Repr};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ty<'a> {
    /// A parametric value whose runtime representation is never inspected.
    Erased,
    /// A partially applied type constructor, retained only as type metadata.
    Constructor(AdtRef<'a>),
    Big(&'a BigTy<'a>),
    Const(&'a ConstTy<'a>),
    Term(&'a TermTy<'a>),
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum ConstTy<'a> {
    Int,
    Bytes,
    String,
    Bool,
    Unit,
    List(Ty<'a>),
    Pair(Ty<'a>, Ty<'a>),
    Array(Ty<'a>),
    BlsG1,
    BlsG2,
    BlsMlr,
    Value,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum BigTy<'a> {
    Int,
    Bytes,
    Data,
    List(Ty<'a>),
    Map(Ty<'a>, Ty<'a>),
    Adt(AdtRef<'a>),
    Record(&'a [Ty<'a>]),
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum TermTy<'a> {
    Adt(AdtRef<'a>),
    Tuple(&'a [Ty<'a>]),
    Record(&'a [Ty<'a>]),
    Fun(&'a [Ty<'a>], Ty<'a>),
}

/// A user ADT instantiated at ground type arguments; constructor field
/// types are looked up through `Adts` so recursive types stay finite.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AdtRef<'a> {
    pub name: QualifiedName<'a>,
    pub args: &'a [Ty<'a>],
}

impl<'a> Ty<'a> {
    pub fn repr(self) -> Option<Repr> {
        match self {
            Ty::Big(_) => Some(Repr::Big),
            Ty::Const(_) => Some(Repr::Const),
            Ty::Term(_) => Some(Repr::Term),
            Ty::Erased | Ty::Constructor(_) => None,
        }
    }

    pub fn plutus_type(
        self,
        arena: &'a nash_plutus::arena::Arena,
    ) -> Option<&'a nash_plutus::typ::Type<'a>> {
        use nash_plutus::typ::Type;
        Some(match self {
            Ty::Big(_) => Type::data(arena),
            Ty::Const(c) => match c {
                ConstTy::Int => Type::integer(arena),
                ConstTy::Bytes => Type::byte_string(arena),
                ConstTy::String => Type::string(arena),
                ConstTy::Bool => Type::bool(arena),
                ConstTy::Unit => Type::unit(arena),
                ConstTy::List(t) => Type::list(arena, t.plutus_type(arena)?),
                ConstTy::Pair(a, b) => {
                    Type::pair(arena, a.plutus_type(arena)?, b.plutus_type(arena)?)
                }
                ConstTy::Array(t) => Type::array(arena, t.plutus_type(arena)?),
                ConstTy::BlsG1 => Type::g1(arena),
                ConstTy::BlsG2 => Type::g2(arena),
                ConstTy::BlsMlr => Type::ml_result(arena),
                ConstTy::Value => Type::value(arena),
            },
            Ty::Term(_) | Ty::Erased | Ty::Constructor(_) => return None,
        })
    }
}

/// Constructor layouts of every ADT instance mentioned in a program.
#[derive(Debug, Default)]
pub struct Adts<'a> {
    pub layouts: std::collections::HashMap<AdtRef<'a>, &'a [&'a [Ty<'a>]]>,
}

impl std::fmt::Display for Ty<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fmt_ty(*self, f, 0)
    }
}

fn fmt_ty(ty: Ty<'_>, f: &mut std::fmt::Formatter<'_>, context: u8) -> std::fmt::Result {
    use std::fmt::Write;
    fn application(
        f: &mut std::fmt::Formatter<'_>,
        name: &str,
        args: &[Ty<'_>],
        context: u8,
    ) -> std::fmt::Result {
        let parens = context > 1 && !args.is_empty();
        if parens {
            f.write_char('(')?;
        }
        f.write_str(name)?;
        for arg in args {
            f.write_char(' ')?;
            fmt_ty(*arg, f, 2)?;
        }
        if parens {
            f.write_char(')')?;
        }
        Ok(())
    }
    fn fields(
        f: &mut std::fmt::Formatter<'_>,
        fields: &[Ty<'_>],
        record: bool,
    ) -> std::fmt::Result {
        f.write_str(if record { "{ " } else { "(" })?;
        for (index, field) in fields.iter().enumerate() {
            if index > 0 {
                f.write_str(", ")?;
            }
            if record {
                write!(f, "_{index} : ")?;
            }
            fmt_ty(*field, f, 0)?;
        }
        f.write_str(if record { " }" } else { ")" })
    }
    match ty {
        Ty::Erased => f.write_str("'erased"),
        Ty::Constructor(adt) => application(f, &qualified_name(adt.name), adt.args, context),
        Ty::Big(big) => match big {
            BigTy::Int => f.write_str("Int"),
            BigTy::Bytes => f.write_str("Bytes"),
            BigTy::Data => f.write_str("Data"),
            BigTy::List(t) => application(f, "List", &[*t], context),
            BigTy::Map(k, v) => application(f, "Map", &[*k, *v], context),
            BigTy::Adt(adt) => application(f, &qualified_name(adt.name), adt.args, context),
            BigTy::Record(ts) => fields(f, ts, true),
        },
        Ty::Const(c) => match c {
            ConstTy::Int => f.write_str("int"),
            ConstTy::Bytes => f.write_str("bytes"),
            ConstTy::String => f.write_str("string"),
            ConstTy::Bool => f.write_str("bool"),
            ConstTy::Unit => f.write_str("unit"),
            ConstTy::BlsG1 => f.write_str("bls_g1"),
            ConstTy::BlsG2 => f.write_str("bls_g2"),
            ConstTy::BlsMlr => f.write_str("bls_mlr"),
            ConstTy::Value => f.write_str("value"),
            ConstTy::List(t) => application(f, "list", &[*t], context),
            ConstTy::Array(t) => application(f, "array", &[*t], context),
            ConstTy::Pair(a, b) => application(f, "pair", &[*a, *b], context),
        },
        Ty::Term(t) => match t {
            TermTy::Adt(adt) => application(f, &qualified_name(adt.name), adt.args, context),
            TermTy::Tuple(ts) => fields(f, ts, false),
            TermTy::Record(ts) => fields(f, ts, true),
            TermTy::Fun(args, result) => {
                let parens = context > 0 && !args.is_empty();
                if parens {
                    f.write_char('(')?;
                }
                for arg in *args {
                    fmt_ty(*arg, f, 1)?;
                    f.write_str(" -> ")?;
                }
                fmt_ty(*result, f, 0)?;
                if parens {
                    f.write_char(')')?;
                }
                Ok(())
            }
        },
    }
}

fn qualified_name(name: QualifiedName<'_>) -> String {
    let mut result = String::new();
    if let Some(package) = name.home.package {
        result.push_str(&format!("{}/{}:", package.author, package.project));
    }
    if !name.home.name.is_empty() {
        result.push_str(name.home.name);
        result.push('.');
    }
    result.push_str(name.name);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use nash_plutus::{arena::Arena, typ::Type};

    #[test]
    fn builtin_types_require_storable_components() {
        let arena = Arena::new();
        let big = Ty::Big(&BigTy::Int);
        let small = Ty::Const(&ConstTy::Int);
        assert_eq!(big.plutus_type(&arena), Some(Type::data(&arena)));
        let pair = Ty::Const(&ConstTy::Pair(big, small));
        assert_eq!(
            pair.plutus_type(&arena),
            Some(Type::pair(
                &arena,
                Type::data(&arena),
                Type::integer(&arena)
            ))
        );
        assert_eq!(Ty::Erased.repr(), None);
        assert_eq!(Ty::Erased.plutus_type(&arena), None);
        assert_eq!(
            Ty::Const(&ConstTy::List(Ty::Erased)).plutus_type(&arena),
            None
        );
        assert_eq!(Ty::Term(&TermTy::Tuple(&[])).plutus_type(&arena), None);
    }

    #[test]
    fn type_precedence_preserves_nested_functions_and_applications() {
        let int = Ty::Const(&ConstTy::Int);
        let function = Ty::Term(&TermTy::Fun(&[int], int));
        assert_eq!(
            Ty::Term(&TermTy::Fun(&[function], function)).to_string(),
            "(int -> int) -> int -> int"
        );
        assert_eq!(
            Ty::Const(&ConstTy::List(Ty::Const(&ConstTy::List(int)))).to_string(),
            "list (list int)"
        );
    }
}
