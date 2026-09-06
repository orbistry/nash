//! Evidence identity uses types, never the source locations carrying them.
use std::hash::{Hash, Hasher};

use nash_region::Located;

use crate::{Evidence, Type};

fn same_types(a: &[&Located<Type<'_>>], b: &[&Located<Type<'_>>]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_type(&a.value, &b.value))
}

fn same_type(a: &Type<'_>, b: &Type<'_>) -> bool {
    match (a, b) {
        (Type::Kinded { typ, .. }, b) => same_type(&typ.value, b),
        (a, Type::Kinded { typ, .. }) => same_type(a, &typ.value),
        (Type::Var(a), Type::Var(b)) => a == b,
        (Type::Unit, Type::Unit) => true,
        (Type::Lambda { from: af, to: at }, Type::Lambda { from: bf, to: bt }) => {
            same_type(&af.value, &bf.value) && same_type(&at.value, &bt.value)
        }
        (Type::App { head: ah, args: aa }, Type::App { head: bh, args: ba }) => {
            same_type(&ah.value, &bh.value) && same_types(aa, ba)
        }
        (
            Type::Named {
                reference: ar,
                args: aa,
            },
            Type::Named {
                reference: br,
                args: ba,
            },
        ) => ar == br && same_types(aa, ba),
        (
            Type::Record {
                fields: af,
                ext: ae,
            },
            Type::Record {
                fields: bf,
                ext: be,
            },
        ) => {
            ae == be
                && af.len() == bf.len()
                && af.iter().zip(*bf).all(|(a, b)| {
                    a.index == b.index
                        && a.field == b.field
                        && same_type(&a.typ.value, &b.typ.value)
                })
        }
        (
            Type::Tuple {
                first: af,
                second: as_,
                rest: ar,
            },
            Type::Tuple {
                first: bf,
                second: bs,
                rest: br,
            },
        ) => {
            same_type(&af.value, &bf.value)
                && same_type(&as_.value, &bs.value)
                && same_types(ar, br)
        }
        (
            Type::Alias {
                reference: ar,
                arguments: aa,
                remaining: ap,
                ..
            },
            Type::Alias {
                reference: br,
                arguments: ba,
                remaining: bp,
                ..
            },
        ) => {
            ar == br
                && ap == bp
                && aa.len() == ba.len()
                && aa
                    .iter()
                    .zip(*ba)
                    .all(|(a, b)| a.name == b.name && same_type(&a.typ.value, &b.typ.value))
        }
        _ => false,
    }
}

fn hash_types<H: Hasher>(types: &[&Located<Type<'_>>], state: &mut H) {
    types.len().hash(state);
    for typ in types {
        hash_type(&typ.value, state);
    }
}

fn hash_type<H: Hasher>(typ: &Type<'_>, state: &mut H) {
    if let Type::Kinded { typ, .. } = typ {
        return hash_type(&typ.value, state);
    }
    std::mem::discriminant(typ).hash(state);
    match typ {
        Type::Kinded { .. } => unreachable!("kind annotation stripped"),
        Type::Var(name) => name.hash(state),
        Type::Unit => {}
        Type::Lambda { from, to } => {
            hash_type(&from.value, state);
            hash_type(&to.value, state);
        }
        Type::App { head, args } => {
            hash_type(&head.value, state);
            hash_types(args, state);
        }
        Type::Named { reference, args } => {
            reference.hash(state);
            hash_types(args, state);
        }
        Type::Record { fields, ext } => {
            ext.hash(state);
            fields.len().hash(state);
            for f in *fields {
                f.index.hash(state);
                f.field.hash(state);
                hash_type(&f.typ.value, state);
            }
        }
        Type::Tuple {
            first,
            second,
            rest,
        } => {
            hash_type(&first.value, state);
            hash_type(&second.value, state);
            hash_types(rest, state);
        }
        Type::Alias {
            reference,
            arguments,
            remaining,
            ..
        } => {
            reference.hash(state);
            remaining.hash(state);
            arguments.len().hash(state);
            for a in *arguments {
                a.name.hash(state);
                hash_type(&a.typ.value, state);
            }
        }
    }
}

impl PartialEq for Evidence<'_> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::ReflexiveLift { typ: a }, Self::ReflexiveLift { typ: b })
            | (Self::StructuralEq { typ: a }, Self::StructuralEq { typ: b }) => {
                same_type(&a.value, &b.value)
            }
            (
                Self::Impl {
                    impl_: ai,
                    type_args: at,
                    args: aa,
                },
                Self::Impl {
                    impl_: bi,
                    type_args: bt,
                    args: ba,
                },
            ) => ai == bi && same_types(at, bt) && aa == ba,
            (
                Self::Given {
                    binder: ab,
                    index: ai,
                },
                Self::Given {
                    binder: bb,
                    index: bi,
                },
            ) => ab == bb && ai == bi,
            (Self::Super { of: ao, index: ai }, Self::Super { of: bo, index: bi }) => {
                ao == bo && ai == bi
            }
            _ => false,
        }
    }
}

impl Eq for Evidence<'_> {}

impl Hash for Evidence<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Self::ReflexiveLift { typ } | Self::StructuralEq { typ } => {
                hash_type(&typ.value, state)
            }
            Self::Impl {
                impl_,
                type_args,
                args,
            } => {
                impl_.hash(state);
                hash_types(type_args, state);
                args.hash(state);
            }
            Self::Given { binder, index } => {
                binder.hash(state);
                index.hash(state);
            }
            Self::Super { of, index } => {
                of.hash(state);
                index.hash(state);
            }
        }
    }
}
