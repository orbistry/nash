//! Bounded constant builtin evaluation for O1; explicit comptime keeps its own policy.
use nash_ir::{build::Builder, core::Core, ty::Ty};
use nash_plutus::{
    arena::Arena, builtin::DefaultFunction as F, constant::Constant as C, data::PlutusData as D,
    flat, machine::ExBudget, typ::Type,
};

#[derive(Clone, Copy)]
struct Limits {
    calls: usize,
    bytes: usize,
    budget: ExBudget,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            calls: 128,
            bytes: 4096,
            budget: ExBudget {
                cpu: 1_000_000,
                mem: 10_000,
            },
        }
    }
}

/// Fold constants and run existing ANF cleanup to stability.
/// One attempt allowance is shared by the whole invocation, including failures.
pub(crate) fn simplify<'a>(b: &Builder<'a>, core: &'a Core<'a>) -> &'a Core<'a> {
    simplify_with(b, core, Limits::default())
}
fn simplify_with<'a>(b: &Builder<'a>, mut core: &'a Core<'a>, limits: Limits) -> &'a Core<'a> {
    let arena = b.arena;
    let mut calls = limits.calls;
    loop {
        let folded = nash_ir::constant_fold::reduce(b, core, &mut |func, args| {
            if calls == 0 || !supported(func) {
                return None;
            }
            calls -= 1;
            let mut nodes = 1024;
            let mut bytes = limits.bytes;
            if !args
                .iter()
                .all(|c| valid(arena, c, 64, &mut nodes, &mut bytes))
            {
                return None;
            }
            let args: Vec<_> = args.iter().map(|c| b.lit(c)).collect();
            let call = b.builtin(func, &args, Ty::Erased);
            let size = |core| {
                let compiled = crate::program::assemble_core(arena, core).ok()?;
                Some(flat::encode(compiled.program).ok()?.len())
            };
            let before = size(call)?;
            let result =
                crate::comptime::eval_closed_budget(arena, &[], call, limits.budget).ok()?;
            let mut nodes = 1024;
            let mut bytes = limits.bytes;
            if !valid(arena, result, 64, &mut nodes, &mut bytes) {
                return None;
            }
            (size(b.lit(result))? <= before).then_some(result)
        });
        if std::ptr::eq(core, folded) {
            return core;
        }
        core = nash_ir::known_case::simplify_constr_data(b, folded);
        debug_assert!(nash_ir::anf::validate(core).is_ok());
        debug_assert!(nash_ir::hygiene::validate(core, &[]).is_ok());
    }
}

fn supported(f: F) -> bool {
    matches!(
        f,
        F::AddInteger
            | F::SubtractInteger
            | F::MultiplyInteger
            | F::DivideInteger
            | F::QuotientInteger
            | F::RemainderInteger
            | F::ModInteger
            | F::EqualsInteger
            | F::LessThanInteger
            | F::LessThanEqualsInteger
            | F::AppendByteString
            | F::SliceByteString
            | F::LengthOfByteString
            | F::EqualsByteString
            | F::LessThanByteString
            | F::LessThanEqualsByteString
            | F::AppendString
            | F::EqualsString
            | F::EncodeUtf8
            | F::DecodeUtf8
            | F::FstPair
            | F::SndPair
            | F::MkCons
            | F::HeadList
            | F::TailList
            | F::NullList
            | F::IData
            | F::BData
            | F::ListData
            | F::MapData
            | F::UnConstrData
            | F::UnMapData
            | F::UnListData
            | F::UnIData
            | F::UnBData
            | F::EqualsData
            | F::SerialiseData
            | F::MkPairData
            | F::MkNilData
            | F::MkNilPairData
    )
}

// Costing/Flat encoding recurse before the CEK budget is charged. Bound both
// metadata and payload depth/work first, and reject inconsistent raw constants.
fn spend(remaining: &mut usize, n: usize) -> bool {
    match remaining.checked_sub(n) {
        Some(left) => {
            *remaining = left;
            true
        }
        None => false,
    }
}
fn valid_type(t: &Type<'_>, depth: usize, nodes: &mut usize) -> bool {
    if depth == 0 || !spend(nodes, 1) {
        return false;
    }
    match t {
        Type::Bool | Type::Integer | Type::ByteString | Type::String | Type::Unit | Type::Data => {
            true
        }
        Type::List(t) => valid_type(t, depth - 1, nodes),
        Type::Pair(a, b) => valid_type(a, depth - 1, nodes) && valid_type(b, depth - 1, nodes),
        _ => false,
    }
}
fn valid_data(d: &D<'_>, depth: usize, nodes: &mut usize, bytes: &mut usize) -> bool {
    if depth == 0 || !spend(nodes, 1) {
        return false;
    }
    match d {
        // The existing CBOR encoder mishandles multi-limb negative Data integers.
        // Do not freeze that bug into a literal (or a serialiseData result).
        D::Integer(i) => {
            !(i.bits() > 64 && *i < &0.into()) && spend(bytes, i.bits().div_ceil(8) as usize)
        }
        D::ByteString(bs) => spend(bytes, bs.len()),
        D::List(xs) | D::Constr { fields: xs, .. } => {
            xs.iter().all(|x| valid_data(x, depth - 1, nodes, bytes))
        }
        D::Map(xs) => xs.iter().all(|(a, b)| {
            valid_data(a, depth - 1, nodes, bytes) && valid_data(b, depth - 1, nodes, bytes)
        }),
    }
}
fn valid<'a>(
    arena: &'a Arena,
    c: &'a C<'a>,
    depth: usize,
    nodes: &mut usize,
    bytes: &mut usize,
) -> bool {
    if depth == 0 || !spend(nodes, 1) {
        return false;
    }
    match c {
        C::Integer(i) => spend(bytes, i.bits().div_ceil(8) as usize),
        C::ByteString(bs) => spend(bytes, bs.len()),
        C::String(s) => spend(bytes, s.len()),
        C::Boolean(_) | C::Unit => true,
        C::Data(d) => valid_data(d, depth - 1, nodes, bytes),
        C::ProtoList(t, xs) => {
            valid_type(t, depth - 1, nodes)
                && xs
                    .iter()
                    .all(|x| valid(arena, x, depth - 1, nodes, bytes) && *t == x.type_of(arena))
        }
        C::ProtoPair(ta, tb, a, b) => {
            valid_type(ta, depth - 1, nodes)
                && valid_type(tb, depth - 1, nodes)
                && valid(arena, a, depth - 1, nodes, bytes)
                && valid(arena, b, depth - 1, nodes, bytes)
                && *ta == a.type_of(arena)
                && *tb == b.type_of(arena)
        }
        _ => false,
    }
}

#[cfg(test)]
#[path = "constant_fold/tests.rs"]
mod tests;
