//! Representation cancellation with runtime-shape evidence, independent of metadata.
use crate::{
    build::Builder,
    core::{Core, CoreKind},
};
use nash_plutus::{builtin::DefaultFunction as F, constant::Constant};

// If evaluation succeeds, these expressions yield a native integer. Their own
// evaluation (including errors and traces) is retained by cancellation.
fn integer(core: &Core<'_>) -> bool {
    match core.kind {
        CoreKind::Lit(Constant::Integer(_)) => true,
        CoreKind::Trace { body, .. } => integer(body),
        CoreKind::Builtin {
            func: F::UnIData,
            args,
        } => args.len() == 1,
        _ => false,
    }
}

/// Trial: cancel a direct `unIData (iData x)` only with integer-shape evidence.
/// No reverse conversion, alias reconstruction, or assumption from `Core::ty`.
pub fn reduce<'a>(b: &Builder<'a>, core: &'a Core<'a>) -> &'a Core<'a> {
    core.map(b, &mut |node| {
        let CoreKind::Builtin {
            func: F::UnIData,
            args: [wrapped],
        } = node.kind
        else {
            return None;
        };
        let CoreKind::Builtin {
            func: F::IData,
            args: [value],
        } = wrapped.kind
        else {
            return None;
        };
        integer(value).then(|| b.with_type(value, node.ty))
    })
}
