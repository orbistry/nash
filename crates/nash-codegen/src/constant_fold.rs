//! Constant builtin evaluation and cleanup to a fixed point.
use nash_ir::{build::Builder, core::Core};
use nash_plutus::{
    arena::Arena, constant::Constant as C, data::PlutusData as D, machine::eval_constant_builtin,
    typ::Type,
};

/// Fold every representable pure constant call, with no size or execution budget.
pub(crate) fn simplify<'a>(b: &Builder<'a>, mut core: &'a Core<'a>) -> &'a Core<'a> {
    loop {
        let folded = nash_ir::constant_fold::reduce(b, core, &mut |func, args| {
            if !args.iter().all(|c| valid(b.arena, c)) {
                return None;
            }
            let result = eval_constant_builtin(b.arena, func, args)?;
            valid(b.arena, result).then_some(result)
        });
        let next = nash_ir::known_case::simplify_constr_data(b, folded);
        debug_assert!(nash_ir::anf::validate(next).is_ok());
        debug_assert!(nash_ir::hygiene::validate(next, &[]).is_ok());
        if std::ptr::eq(core, next) {
            return next;
        }
        core = next;
    }
}

// Validate metadata, payloads and literal representation with heap worklists.
// These checks impose no depth, node, byte or output-growth limit.
fn valid<'a>(arena: &'a Arena, c: &'a C<'a>) -> bool {
    let mut constants = vec![c];
    let mut types = Vec::new();
    let mut pairs = Vec::new();
    let mut data = Vec::new();
    while let Some(c) = constants.pop() {
        match c {
            C::ProtoList(t, xs) | C::ProtoArray(t, xs) => {
                types.push(*t);
                for x in *xs {
                    constants.push(x);
                    pairs.push((*t, x.type_of(arena)));
                }
            }
            C::ProtoPair(ta, tb, a, b) => {
                types.extend([*ta, *tb]);
                constants.extend([*a, *b]);
                pairs.extend([(*ta, a.type_of(arena)), (*tb, b.type_of(arena))]);
            }
            C::Data(d) => data.push(*d),
            // UPLC's Flat format does not permit BLS constants in scripts.
            C::Bls12_381G1Element(_) | C::Bls12_381G2Element(_) | C::Bls12_381MlResult(_) => {
                return false;
            }
            _ => {}
        }
    }
    while let Some(t) = types.pop() {
        match t {
            Type::List(t) | Type::Array(t) => types.push(t),
            Type::Pair(a, b) => types.extend([*a, *b]),
            Type::Bls12_381G1Element | Type::Bls12_381G2Element | Type::Bls12_381MlResult => {
                return false;
            }
            _ => {}
        }
    }
    while let Some((a, b)) = pairs.pop() {
        match (a, b) {
            (Type::List(a), Type::List(b)) | (Type::Array(a), Type::Array(b)) => pairs.push((a, b)),
            (Type::Pair(a, b), Type::Pair(c, d)) => pairs.extend([(*a, *c), (*b, *d)]),
            _ if std::mem::discriminant(a) == std::mem::discriminant(b) => {}
            _ => return false,
        }
    }
    while let Some(d) = data.pop() {
        match d {
            // Existing CBOR encoding mishandles negative multi-limb Data integers.
            // Keep evaluation at runtime until that serialization bug is fixed.
            D::Integer(i) if i.bits() > 64 && *i < &0.into() => return false,
            D::List(xs) | D::Constr { fields: xs, .. } => data.extend(*xs),
            D::Map(xs) => {
                for (a, b) in *xs {
                    data.extend([*a, *b]);
                }
            }
            _ => {}
        }
    }
    true
}

#[cfg(test)]
#[path = "constant_fold/tests.rs"]
mod tests;
