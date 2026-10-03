//! Representation cancellation with runtime-shape evidence, independent of metadata.
use crate::{
    build::Builder,
    core::{Core, CoreKind},
};
use nash_plutus::{
    builtin::DefaultFunction as F, constant::Constant as C, data::PlutusData as D, typ::Type,
};
use std::collections::HashMap;

type Bindings<'a> = HashMap<u32, &'a Core<'a>>;

#[derive(Clone, Copy)]
enum Shape {
    Integer,
    Bytes,
    DataList,
    DataMap,
    String,
    Utf8,
    IData,
    BData,
    ListData,
    MapData,
}

// Evidence describes successful evaluation only. The expression itself is retained,
// including any failure, trace, or divergence. Type annotations are never evidence.
fn proves<'a>(mut core: &'a Core<'a>, shape: Shape, bindings: &Bindings<'a>) -> bool {
    loop {
        match core.kind {
            CoreKind::Trace { body, .. } => core = body,
            CoreKind::Var(name) => match bindings.get(&name.unique) {
                Some(value) => core = value,
                None => return false,
            },
            _ => break,
        }
    }
    match core.kind {
        CoreKind::Lit(c) => match (shape, c) {
            (Shape::Integer, C::Integer(_))
            | (Shape::Bytes, C::ByteString(_))
            | (Shape::String, C::String(_)) => true,
            (Shape::Utf8, C::ByteString(bytes)) => std::str::from_utf8(bytes).is_ok(),
            (Shape::DataList, C::ProtoList(Type::Data, items)) => {
                items.iter().all(|item| matches!(item, C::Data(_)))
            }
            (Shape::DataMap, C::ProtoList(Type::Pair(Type::Data, Type::Data), items)) => {
                items.iter().all(|item| {
                    matches!(
                        item,
                        C::ProtoPair(Type::Data, Type::Data, C::Data(_), C::Data(_))
                    )
                })
            }
            (Shape::IData, C::Data(D::Integer(_)))
            | (Shape::BData, C::Data(D::ByteString(_)))
            | (Shape::ListData, C::Data(D::List(_)))
            | (Shape::MapData, C::Data(D::Map(_))) => true,
            _ => false,
        },
        CoreKind::Builtin { func, args } if args.len() == func.arity() => matches!(
            (shape, func),
            (Shape::Integer, F::UnIData)
                | (Shape::Bytes, F::UnBData | F::EncodeUtf8)
                | (Shape::DataList, F::UnListData)
                | (Shape::DataMap, F::UnMapData)
                | (Shape::String, F::DecodeUtf8)
                | (Shape::Utf8, F::EncodeUtf8)
                | (Shape::IData, F::IData)
                | (Shape::BData, F::BData)
                | (Shape::ListData, F::ListData)
                | (Shape::MapData, F::MapData)
        ),
        _ => false,
    }
}

fn required(outer: F, inner: F) -> Option<Shape> {
    Some(match (outer, inner) {
        (F::UnIData, F::IData) => Shape::Integer,
        (F::UnBData, F::BData) => Shape::Bytes,
        (F::UnListData, F::ListData) => Shape::DataList,
        (F::UnMapData, F::MapData) => Shape::DataMap,
        (F::DecodeUtf8, F::EncodeUtf8) => Shape::String,
        (F::EncodeUtf8, F::DecodeUtf8) => Shape::Utf8,
        (F::IData, F::UnIData) => Shape::IData,
        (F::BData, F::UnBData) => Shape::BData,
        (F::ListData, F::UnListData) => Shape::ListData,
        (F::MapData, F::UnMapData) => Shape::MapData,
        _ => return None,
    })
}

fn resolve<'a>(mut core: &'a Core<'a>, bindings: &Bindings<'a>) -> &'a Core<'a> {
    while let CoreKind::Var(name) = core.kind {
        let Some(value) = bindings.get(&name.unique) else {
            break;
        };
        core = value;
    }
    core
}

fn projection<'a>(core: &'a Core<'a>, func: F) -> Option<&'a Core<'a>> {
    match core.kind {
        CoreKind::Builtin {
            func: actual,
            args: [pair],
        } if actual == func => Some(pair),
        _ => None,
    }
}

// ConstrData has two operands, not a unary inverse. Only reuse already evaluated
// variables through retained producer bindings. Those bindings preserve tag-range,
// list-shape, and Data-variant checks, including when the selected field is unused.
fn constructor<'a>(core: &'a Core<'a>, bindings: &Bindings<'a>) -> Option<&'a Core<'a>> {
    if let CoreKind::Builtin {
        func: F::ConstrData,
        args: [tag, fields],
    } = core.kind
    {
        let left = projection(resolve(tag, bindings), F::FstPair)?;
        let right = projection(resolve(fields, bindings), F::SndPair)?;
        // Keep the decoder's strict binding; never discard two direct calls.
        if !matches!(left.kind, CoreKind::Var(_))
            || !matches!(right.kind, CoreKind::Var(_))
            || !std::ptr::eq(resolve(left, bindings), resolve(right, bindings))
        {
            return None;
        }
        let CoreKind::Builtin {
            func: F::UnConstrData,
            args: [data],
        } = resolve(left, bindings).kind
        else {
            return None;
        };
        return matches!(data.kind, CoreKind::Var(_)).then_some(data);
    }
    let CoreKind::Builtin { func, args: [pair] } = core.kind else {
        return None;
    };
    let index = match func {
        F::FstPair => 0,
        F::SndPair => 1,
        _ => return None,
    };
    let CoreKind::Builtin {
        func: F::UnConstrData,
        args: [data],
    } = resolve(pair, bindings).kind
    else {
        return None;
    };
    if !matches!(data.kind, CoreKind::Var(_)) {
        return None;
    }
    let CoreKind::Builtin {
        func: F::ConstrData,
        args: [tag, fields],
    } = resolve(data, bindings).kind
    else {
        return None;
    };
    let value = [*tag, *fields][index];
    matches!(value.kind, CoreKind::Var(_)).then_some(value)
}

/// Cancel representation round trips. Requires globally unique, scoped names.
/// Bound producers are reused only when their operand is a variable: no duplication,
/// new bindings, or movement of computations/literals across evaluation points.
/// Unary operands also require shape evidence. Keeping unknown operands unchanged
/// avoids extending their lifetimes and increasing Flat size on failing programs.
pub fn reduce<'a>(b: &Builder<'a>, core: &'a Core<'a>) -> &'a Core<'a> {
    let mut bindings = Bindings::new();
    core.walk(&mut |node| {
        if let CoreKind::Let { binder, value, .. } = node.kind {
            bindings.insert(binder.name.unique, value);
        }
    });
    core.map(b, &mut |node| {
        if let Some(value) = constructor(node, &bindings) {
            return Some(b.with_type(value, node.ty));
        }
        let CoreKind::Builtin {
            func: outer,
            args: [wrapped],
        } = node.kind
        else {
            return None;
        };
        let producer = *wrapped;
        let bound = matches!(producer.kind, CoreKind::Var(_));
        let producer = resolve(producer, &bindings);
        let CoreKind::Builtin {
            func: inner,
            args: [value],
        } = producer.kind
        else {
            return None;
        };
        if bound && !matches!(value.kind, CoreKind::Var(_)) {
            return None;
        }
        let shape = required(outer, inner)?;
        proves(value, shape, &bindings).then(|| b.with_type(value, node.ty))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ty::Ty;
    use nash_plutus::arena::Arena;

    #[test]
    fn inconsistent_container_payloads_are_not_shape_evidence() {
        let arena = Arena::new();
        let b = Builder::new(&arena);
        let boolean = C::bool(&arena, false);
        let wrong_pair = C::proto_pair(&arena, &Type::Data, &Type::Data, boolean, boolean);
        for (encode, decode, constant) in [
            (
                F::ListData,
                F::UnListData,
                C::proto_list(&arena, &Type::Data, arena.alloc([boolean])),
            ),
            (
                F::MapData,
                F::UnMapData,
                C::proto_list(
                    &arena,
                    arena.alloc(Type::Pair(&Type::Data, &Type::Data)),
                    arena.alloc([wrong_pair]),
                ),
            ),
        ] {
            // These deliberately inconsistent constants can panic in the runtime.
            // Check pass noninterference without executing malformed payloads.
            let value = b.lit(constant);
            let core = b.builtin(decode, &[b.builtin(encode, &[value], Ty::Erased)], value.ty);
            assert!(std::ptr::eq(core, reduce(&b, core)));
        }
    }
}
