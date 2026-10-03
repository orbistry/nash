//! Shared semantic and explicit performance inputs for representation cancellation.
use nash_ir::{
    build::Builder,
    core::*,
    ty::{ConstTy, Ty},
};
use nash_plutus::{
    builtin::DefaultFunction as F, constant::Constant, data::PlutusData as D, typ::Type,
};

pub fn cases<'a>(b: &Builder<'a>) -> Vec<(String, &'a Core<'a>, bool)> {
    let a = b.arena;
    let d = D::integer_from(a, 7);
    let data = Constant::data(a, d);
    let pair = Constant::proto_pair(a, &Type::Data, &Type::Data, data, data);
    let list = Constant::proto_list(a, &Type::Data, a.alloc([data]));
    let map = Constant::proto_list(
        a,
        a.alloc(Type::Pair(&Type::Data, &Type::Data)),
        a.alloc([pair]),
    );
    let mut cases = Vec::new();
    for (name, encode, decode, value, encoded) in [
        ("integer", F::IData, F::UnIData, b.int(7), b.lit(data)),
        (
            "bytes",
            F::BData,
            F::UnBData,
            b.lit(Constant::byte_string(a, b"hello")),
            b.lit(Constant::data(a, D::byte_string(a, b"hello"))),
        ),
        (
            "list",
            F::ListData,
            F::UnListData,
            b.lit(list),
            b.lit(Constant::data(a, D::list(a, a.alloc([d])))),
        ),
        (
            "map",
            F::MapData,
            F::UnMapData,
            b.lit(map),
            b.lit(Constant::data(a, D::map(a, a.alloc([(d, d)])))),
        ),
        (
            "utf8",
            F::EncodeUtf8,
            F::DecodeUtf8,
            b.lit(Constant::string(a, "héllo")),
            b.lit(Constant::byte_string(a, "héllo".as_bytes())),
        ),
    ] {
        let wrap = |x| b.builtin(decode, &[b.builtin(encode, &[x], encoded.ty)], value.ty);
        let trace = |s, x| b.trace(b.lit(Constant::string(a, s)), x);
        cases.push((format!("{name}_direct"), wrap(value), false));
        cases.push((
            format!("{name}_trace"),
            wrap(trace("operand", value)),
            false,
        ));
        cases.push((
            format!("{name}_decoded"),
            wrap(b.builtin(decode, &[encoded], value.ty)),
            false,
        ));
        cases.push((
            format!("{name}_decode_failure"),
            wrap(b.builtin(decode, &[b.int(0)], value.ty)),
            true,
        ));
        cases.push((
            format!("{name}_reverse"),
            b.builtin(
                encode,
                &[b.builtin(decode, &[encoded], value.ty)],
                encoded.ty,
            ),
            false,
        ));
        cases.push((
            format!("{name}_wrong_kind"),
            wrap(b.with_type(b.lit(Constant::bool(a, false)), value.ty)),
            true,
        ));
        cases.push((
            format!("{name}_cold"),
            b.if_(
                b.lit(Constant::bool(a, true)),
                value,
                wrap(trace("cold", value)),
            ),
            false,
        ));
        let x = Binder {
            name: b.fresh("x"),
            ty: value.ty,
        };
        let y = Binder {
            name: b.fresh("encoded"),
            ty: encoded.ty,
        };
        let z = Binder {
            name: b.fresh("alias"),
            ty: encoded.ty,
        };
        let decoded = b.builtin(decode, &[b.var(z.name, z.ty)], value.ty);
        let bindings = |body| {
            b.let_(
                x,
                trace("once", value),
                b.let_(
                    y,
                    b.builtin(encode, &[b.var(x.name, x.ty)], encoded.ty),
                    b.let_(z, b.var(y.name, y.ty), body),
                ),
            )
        };
        cases.push((
            format!("{name}_bound"),
            bindings(trace("after encoding", decoded)),
            false,
        ));
        cases.push((
            format!("{name}_captured"),
            bindings(b.force(b.delay(decoded), decoded.ty)),
            false,
        ));
        cases.push((
            format!("{name}_escaping"),
            bindings(b.constr(0, &[b.var(y.name, y.ty), decoded, decoded], Ty::Erased)),
            false,
        ));
        cases.push((
            format!("{name}_bound_invalid"),
            b.let_(
                x,
                trace(
                    "operand",
                    b.with_type(b.lit(Constant::bool(a, false)), value.ty),
                ),
                b.let_(
                    y,
                    b.builtin(encode, &[b.var(x.name, x.ty)], encoded.ty),
                    trace(
                        "must not run",
                        b.builtin(decode, &[b.var(y.name, y.ty)], value.ty),
                    ),
                ),
            ),
            true,
        ));
        cases.push((
            format!("{name}_reverse_bound_invalid"),
            b.let_(
                y,
                b.with_type(b.int(0), encoded.ty),
                b.let_(
                    x,
                    b.builtin(decode, &[b.var(y.name, y.ty)], value.ty),
                    trace(
                        "must not run",
                        b.builtin(encode, &[b.var(x.name, x.ty)], encoded.ty),
                    ),
                ),
            ),
            true,
        ));
        cases.push((
            format!("{name}_oversaturated"),
            b.app(wrap(value), &[b.int(1)], value.ty),
            true,
        ));
        let unknown = Binder {
            name: b.fresh("unknown"),
            ty: value.ty,
        };
        cases.push((
            format!("{name}_unknown_invalid"),
            b.app(
                b.lam(&[unknown], wrap(b.var(unknown.name, unknown.ty))),
                &[b.lit(Constant::bool(a, false))],
                value.ty,
            ),
            true,
        ));
        cases.push((
            format!("{name}_partial"),
            b.app(
                b.builtin(decode, &[], Ty::Erased),
                &[b.builtin(encode, &[value], encoded.ty)],
                value.ty,
            ),
            false,
        ));
    }
    let bad_utf8 = b.lit(Constant::byte_string(a, &[255]));
    cases.push((
        "utf8_reverse_invalid".into(),
        b.builtin(
            F::EncodeUtf8,
            &[b.builtin(F::DecodeUtf8, &[bad_utf8], Ty::Const(&ConstTy::String))],
            bad_utf8.ty,
        ),
        true,
    ));
    for (name, encode, decode, typ) in [
        ("list_wrong_tag", F::ListData, F::UnListData, &Type::Integer),
        ("map_wrong_tag", F::MapData, F::UnMapData, &Type::Data),
    ] {
        let value = b.lit(Constant::proto_list(a, typ, &[]));
        cases.push((
            name.into(),
            b.builtin(decode, &[b.builtin(encode, &[value], Ty::Erased)], value.ty),
            true,
        ));
    }
    let data_ty = b.lit(data).ty;
    let fields = Binder {
        name: b.fresh("fields"),
        ty: b.lit(list).ty,
    };
    let tag = Binder {
        name: b.fresh("tag"),
        ty: b.int(0).ty,
    };
    let encoded = Binder {
        name: b.fresh("constructor"),
        ty: data_ty,
    };
    let payload = Binder {
        name: b.fresh("payload"),
        ty: Ty::Erased,
    };
    let p = b.var(payload.name, payload.ty);
    let fst = b.builtin(F::FstPair, &[p], tag.ty);
    let snd = b.builtin(F::SndPair, &[p], fields.ty);
    let constructor = b.builtin(
        F::ConstrData,
        &[b.var(tag.name, tag.ty), b.var(fields.name, fields.ty)],
        data_ty,
    );
    let bind = |body| {
        b.let_(
            tag,
            b.int(0),
            b.let_(
                fields,
                b.lit(list),
                b.let_(
                    encoded,
                    constructor,
                    b.let_(
                        payload,
                        b.builtin(
                            F::UnConstrData,
                            &[b.var(encoded.name, encoded.ty)],
                            Ty::Erased,
                        ),
                        body,
                    ),
                ),
            ),
        )
    };
    cases.push(("constructor_tag".into(), bind(fst), false));
    cases.push(("constructor_fields".into(), bind(snd), false));
    cases.push((
        "constructor_rebuild".into(),
        bind(b.builtin(F::ConstrData, &[fst, snd], data_ty)),
        false,
    ));
    for (name, subject, fails) in [
        (
            "constructor_known_reverse",
            b.lit(Constant::data(a, D::constr(a, 3, a.alloc([d])))),
            false,
        ),
        ("constructor_wrong_variant", b.lit(data), true),
    ] {
        cases.push((
            name.into(),
            b.let_(
                encoded,
                subject,
                b.let_(
                    payload,
                    b.builtin(F::UnConstrData, &[b.var(encoded.name, data_ty)], Ty::Erased),
                    b.trace(
                        b.lit(Constant::string(a, "after decoding")),
                        b.builtin(F::ConstrData, &[fst, snd], data_ty),
                    ),
                ),
            ),
            fails,
        ));
    }
    let bad_tag = b.with_type(b.lit(Constant::bool(a, false)), tag.ty);
    cases.push((
        "constructor_bad_tag".into(),
        b.let_(
            tag,
            bad_tag,
            b.let_(
                fields,
                b.trace(b.lit(Constant::string(a, "fields evaluated")), b.lit(list)),
                b.let_(
                    encoded,
                    constructor,
                    b.builtin(
                        F::FstPair,
                        &[b.builtin(F::UnConstrData, &[b.var(encoded.name, data_ty)], Ty::Erased)],
                        tag.ty,
                    ),
                ),
            ),
        ),
        true,
    ));
    cases.push((
        "constructor_bad_fields".into(),
        b.let_(
            tag,
            b.int(0),
            b.let_(
                fields,
                b.with_type(b.lit(Constant::bool(a, false)), fields.ty),
                b.let_(
                    encoded,
                    constructor,
                    b.builtin(
                        F::SndPair,
                        &[b.builtin(F::UnConstrData, &[b.var(encoded.name, data_ty)], Ty::Erased)],
                        fields.ty,
                    ),
                ),
            ),
        ),
        true,
    ));
    cases
}
