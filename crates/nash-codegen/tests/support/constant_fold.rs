//! Shared inputs for the constant-evaluation trial and its explicit measurements.
use nash_ir::{
    build::Builder,
    core::{Binder, Core},
    ty::Ty,
};
use nash_plutus::{
    builtin::DefaultFunction as F, constant::Constant as C, data::PlutusData as D, typ::Type,
};

pub fn cases<'a>(b: &Builder<'a>) -> Vec<(&'static str, &'a Core<'a>, bool)> {
    let a = b.arena;
    let bytes = |s| b.lit(C::byte_string(a, s));
    let string = |s| b.lit(C::string(a, s));
    let int = b.int(7);
    let data = b.lit(C::data(a, D::integer_from(a, 7)));
    let list = b.lit(C::proto_list(
        a,
        &Type::Integer,
        a.alloc([C::integer_from(a, 7), C::integer_from(a, 8)]),
    ));
    let empty = b.lit(C::proto_list(a, &Type::Integer, &[]));
    let pair = b.lit(C::proto_pair(
        a,
        &Type::Integer,
        &Type::ByteString,
        C::integer_from(a, 7),
        C::byte_string(a, b"a"),
    ));
    let call = |f, args: &[&'a Core<'a>]| b.builtin(f, args, Ty::Erased);
    let add = call(F::AddInteger, &[b.int(20), b.int(22)]);
    let x = Binder {
        name: b.fresh("x"),
        ty: add.ty,
    };
    let dlist = b.lit(C::proto_list(
        a,
        &Type::Data,
        a.alloc([C::data(a, D::integer_from(a, 7))]),
    ));
    let dmap = b.lit(C::proto_list(
        a,
        a.alloc(Type::Pair(&Type::Data, &Type::Data)),
        &[],
    ));
    let expanding = b.lit(C::data(
        a,
        D::list(a, a.alloc_slice_copy(&[D::integer_from(a, 7); 30])),
    ));
    let prefix = string(a.as_bump().alloc_str(&"prefix".repeat(6)));
    let shared = Binder {
        name: b.fresh("prefix"),
        ty: prefix.ty,
    };
    let shared_prefix = b.let_(
        shared,
        prefix,
        b.constr(
            0,
            &[
                call(
                    F::AppendString,
                    &[b.var(shared.name, shared.ty), string("x")],
                ),
                call(
                    F::AppendString,
                    &[b.var(shared.name, shared.ty), string("y")],
                ),
            ],
            Ty::Erased,
        ),
    );
    let distinct_results = b.constr(
        0,
        &[
            call(F::AppendString, &[prefix, string("x")]),
            call(F::AppendString, &[prefix, string("y")]),
        ],
        Ty::Erased,
    );
    vec![
        ("shared_prefix_distinct_results", shared_prefix, false),
        ("repeated_literal_distinct_results", distinct_results, false),
        (
            "expanding_data_list",
            call(F::UnListData, &[expanding]),
            false,
        ),
        ("addition", add, false),
        (
            "nested_arithmetic",
            call(F::MultiplyInteger, &[add, b.int(2)]),
            false,
        ),
        (
            "bound_arithmetic",
            b.let_(
                x,
                add,
                call(F::MultiplyInteger, &[b.var(x.name, x.ty), b.int(2)]),
            ),
            false,
        ),
        (
            "division",
            call(F::DivideInteger, &[b.int(-7), b.int(3)]),
            false,
        ),
        (
            "division_zero",
            call(F::DivideInteger, &[int, b.int(0)]),
            true,
        ),
        (
            "wrong_integer_shape",
            call(F::AddInteger, &[bytes(b"x"), int]),
            true,
        ),
        (
            "byte_append",
            call(F::AppendByteString, &[bytes(b"ab"), bytes(b"cd")]),
            false,
        ),
        (
            "byte_slice",
            call(F::SliceByteString, &[b.int(1), b.int(2), bytes(b"abcd")]),
            false,
        ),
        (
            "byte_length",
            call(F::LengthOfByteString, &[bytes(b"abcd")]),
            false,
        ),
        (
            "byte_compare",
            call(F::LessThanByteString, &[bytes(b"abc"), bytes(b"abd")]),
            false,
        ),
        (
            "string_append",
            call(F::AppendString, &[string("hello "), string("world")]),
            false,
        ),
        ("encode_utf8", call(F::EncodeUtf8, &[string("สวัสดี")]), false),
        (
            "decode_utf8",
            call(F::DecodeUtf8, &[bytes("héllo".as_bytes())]),
            false,
        ),
        ("invalid_utf8", call(F::DecodeUtf8, &[bytes(&[255])]), true),
        ("integer_data", call(F::IData, &[int]), false),
        ("decode_integer_data", call(F::UnIData, &[data]), false),
        ("wrong_data_variant", call(F::UnBData, &[data]), true),
        ("list_data", call(F::ListData, &[dlist]), false),
        ("map_data", call(F::MapData, &[dmap]), false),
        ("data_equal", call(F::EqualsData, &[data, data]), false),
        ("serialise_data", call(F::SerialiseData, &[data]), false),
        ("list_head", call(F::HeadList, &[list]), false),
        ("list_tail", call(F::TailList, &[list]), false),
        ("empty_head", call(F::HeadList, &[empty]), true),
        ("empty_tail", call(F::TailList, &[empty]), true),
        ("list_null", call(F::NullList, &[empty]), false),
        ("pair_first", call(F::FstPair, &[pair]), false),
        ("pair_second", call(F::SndPair, &[pair]), false),
        ("trace_before", b.trace(string("before"), add), false),
        (
            "trace_builtin",
            call(F::Trace, &[string("keep me"), int]),
            false,
        ),
        (
            "cold_failure",
            b.if_(
                b.lit(C::bool(a, true)),
                int,
                call(F::DivideInteger, &[int, b.int(0)]),
            ),
            false,
        ),
        (
            "selected_branch",
            b.if_(
                call(F::EqualsInteger, &[int, b.int(7)]),
                b.trace(string("yes"), add),
                b.trace(string("no"), b.error(Ty::Erased)),
            ),
            false,
        ),
        (
            "strict_failure_order",
            call(
                F::AddInteger,
                &[
                    b.trace(string("first"), b.error(int.ty)),
                    call(F::DivideInteger, &[int, b.int(0)]),
                ],
            ),
            true,
        ),
        ("unsupported_hash", call(F::Sha2_256, &[bytes(b"x")]), false),
        (
            "partial",
            b.app(
                b.builtin(F::AddInteger, &[int], Ty::Erased),
                &[b.int(1)],
                Ty::Erased,
            ),
            false,
        ),
        (
            "large_input",
            call(
                F::AppendByteString,
                &[bytes(a.alloc_slice_copy(&[42; 4096])), bytes(b"z")],
            ),
            false,
        ),
        (
            "repeated_call",
            b.constr(0, &[add, add, add], Ty::Erased),
            false,
        ),
    ]
}
