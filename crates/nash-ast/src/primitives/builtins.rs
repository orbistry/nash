//! Typed value schemes for the synthetic Builtin module.

use super::builtin_home;
use crate::{QualifiedName, Type};
use nash_region::Located;

pub struct Builtin {
    pub name: &'static str,
    pub free_vars: &'static [&'static str],
    pub typ: &'static Located<Type<'static>>,
    pub variant: &'static str,
    pub context: &'static [crate::Pred<'static>],
}

pub(super) const fn named(
    name: &'static str,
    args: &'static [&'static Located<Type<'static>>],
) -> Located<Type<'static>> {
    Located::at_zero(Type::Named {
        reference: QualifiedName {
            home: builtin_home(),
            name,
        },
        args,
    })
}

macro_rules! function {
    ($result:expr) => { $result };
    ($first:expr, $($rest:expr),+) => {
        &Located::at_zero(Type::Lambda { from: $first, to: function!($($rest),+) })
    };
}

macro_rules! builtin {
    ($name:literal, $variant:literal, [$($var:literal),*], $typ:expr) => {
        Builtin { context: &[], name: $name, free_vars: &[$($var),*], typ: $typ, variant: $variant }
    };
}

const A: &Located<Type<'static>> = &Located::at_zero(Type::Var("a"));
const B: &Located<Type<'static>> = &Located::at_zero(Type::Var("b"));
const UNIT: &Located<Type<'static>> = &named("unit", &[]);
const INT: &Located<Type<'static>> = &named("int", &[]);
const BOOL: &Located<Type<'static>> = &named("bool", &[]);
const BYTES: &Located<Type<'static>> = &named("bytes", &[]);
const STRING: &Located<Type<'static>> = &named("string", &[]);
const DATA: &Located<Type<'static>> = &named("Data", &[]);
const BLS_G1: &Located<Type<'static>> = &named("bls_g1", &[]);
const BLS_G2: &Located<Type<'static>> = &named("bls_g2", &[]);
const BLS_MLR: &Located<Type<'static>> = &named("bls_mlr", &[]);
const VALUE: &Located<Type<'static>> = &named("value", &[]);

pub const BUILTINS: &[Builtin] = &[
    builtin!("addInteger", "AddInteger", [], function!(INT, INT, INT)),
    builtin!(
        "subtractInteger",
        "SubtractInteger",
        [],
        function!(INT, INT, INT)
    ),
    builtin!(
        "multiplyInteger",
        "MultiplyInteger",
        [],
        function!(INT, INT, INT)
    ),
    builtin!(
        "divideInteger",
        "DivideInteger",
        [],
        function!(INT, INT, INT)
    ),
    builtin!(
        "quotientInteger",
        "QuotientInteger",
        [],
        function!(INT, INT, INT)
    ),
    builtin!(
        "remainderInteger",
        "RemainderInteger",
        [],
        function!(INT, INT, INT)
    ),
    builtin!("modInteger", "ModInteger", [], function!(INT, INT, INT)),
    builtin!(
        "equalsInteger",
        "EqualsInteger",
        [],
        function!(INT, INT, BOOL)
    ),
    builtin!(
        "lessThanInteger",
        "LessThanInteger",
        [],
        function!(INT, INT, BOOL)
    ),
    builtin!(
        "lessThanEqualsInteger",
        "LessThanEqualsInteger",
        [],
        function!(INT, INT, BOOL)
    ),
    builtin!(
        "appendByteString",
        "AppendByteString",
        [],
        function!(BYTES, BYTES, BYTES)
    ),
    builtin!(
        "consByteString",
        "ConsByteString",
        [],
        function!(INT, BYTES, BYTES)
    ),
    builtin!(
        "sliceByteString",
        "SliceByteString",
        [],
        function!(INT, INT, BYTES, BYTES)
    ),
    builtin!(
        "lengthOfByteString",
        "LengthOfByteString",
        [],
        function!(BYTES, INT)
    ),
    builtin!(
        "indexByteString",
        "IndexByteString",
        [],
        function!(BYTES, INT, INT)
    ),
    builtin!(
        "equalsByteString",
        "EqualsByteString",
        [],
        function!(BYTES, BYTES, BOOL)
    ),
    builtin!(
        "lessThanByteString",
        "LessThanByteString",
        [],
        function!(BYTES, BYTES, BOOL)
    ),
    builtin!(
        "lessThanEqualsByteString",
        "LessThanEqualsByteString",
        [],
        function!(BYTES, BYTES, BOOL)
    ),
    builtin!("sha2_256", "Sha2_256", [], function!(BYTES, BYTES)),
    builtin!("sha3_256", "Sha3_256", [], function!(BYTES, BYTES)),
    builtin!("blake2b_256", "Blake2b_256", [], function!(BYTES, BYTES)),
    builtin!("blake2b_224", "Blake2b_224", [], function!(BYTES, BYTES)),
    builtin!("keccak_256", "Keccak_256", [], function!(BYTES, BYTES)),
    builtin!("ripemd_160", "Ripemd_160", [], function!(BYTES, BYTES)),
    builtin!(
        "verifyEd25519Signature",
        "VerifyEd25519Signature",
        [],
        function!(BYTES, BYTES, BYTES, BOOL)
    ),
    builtin!(
        "verifyEcdsaSecp256k1Signature",
        "VerifyEcdsaSecp256k1Signature",
        [],
        function!(BYTES, BYTES, BYTES, BOOL)
    ),
    builtin!(
        "verifySchnorrSecp256k1Signature",
        "VerifySchnorrSecp256k1Signature",
        [],
        function!(BYTES, BYTES, BYTES, BOOL)
    ),
    builtin!(
        "appendString",
        "AppendString",
        [],
        function!(STRING, STRING, STRING)
    ),
    builtin!(
        "equalsString",
        "EqualsString",
        [],
        function!(STRING, STRING, BOOL)
    ),
    builtin!("encodeUtf8", "EncodeUtf8", [], function!(STRING, BYTES)),
    builtin!("decodeUtf8", "DecodeUtf8", [], function!(BYTES, STRING)),
    builtin!("ifThenElse", "IfThenElse", ["a"], function!(BOOL, A, A, A)),
    builtin!("chooseUnit", "ChooseUnit", ["a"], function!(UNIT, A, A)),
    builtin!("trace", "Trace", ["a"], function!(STRING, A, A)),
    builtin!(
        "fstPair",
        "FstPair",
        ["a", "b"],
        function!(&named("pair", &[A, B]), A)
    ),
    builtin!(
        "sndPair",
        "SndPair",
        ["a", "b"],
        function!(&named("pair", &[A, B]), B)
    ),
    builtin!(
        "chooseList",
        "ChooseList",
        ["a", "b"],
        function!(&named("list", &[A]), B, B, B)
    ),
    builtin!(
        "mkCons",
        "MkCons",
        ["a"],
        function!(A, &named("list", &[A]), &named("list", &[A]))
    ),
    builtin!(
        "headList",
        "HeadList",
        ["a"],
        function!(&named("list", &[A]), A)
    ),
    builtin!(
        "tailList",
        "TailList",
        ["a"],
        function!(&named("list", &[A]), &named("list", &[A]))
    ),
    builtin!(
        "nullList",
        "NullList",
        ["a"],
        function!(&named("list", &[A]), BOOL)
    ),
    builtin!(
        "dropList",
        "DropList",
        ["a"],
        function!(INT, &named("list", &[A]), &named("list", &[A]))
    ),
    builtin!(
        "chooseData",
        "ChooseData",
        ["a"],
        function!(DATA, A, A, A, A, A, A)
    ),
    builtin!(
        "constrData",
        "ConstrData",
        [],
        function!(INT, &named("list", &[DATA]), DATA)
    ),
    Builtin {
        name: "mapData",
        free_vars: &["a", "b"],
        typ: function!(
            &named("list", &[&named("pair", &[A, B])]),
            &named("Map", &[A, B])
        ),
        variant: "MapData",
        context: &[
            crate::Pred::Trait {
                trait_: super::ReprTrait::Big.qualified(),
                args: &[A],
            },
            crate::Pred::Trait {
                trait_: super::ReprTrait::Big.qualified(),
                args: &[B],
            },
        ],
    },
    Builtin {
        name: "listData",
        free_vars: &["a"],
        typ: function!(&named("list", &[A]), &named("List", &[A])),
        variant: "ListData",
        context: &[crate::Pred::Trait {
            trait_: super::ReprTrait::Big.qualified(),
            args: &[A],
        }],
    },
    builtin!("iData", "IData", [], function!(INT, &named("Int", &[]))),
    builtin!("bData", "BData", [], function!(BYTES, &named("Bytes", &[]))),
    builtin!(
        "unConstrData",
        "UnConstrData",
        [],
        function!(DATA, &named("pair", &[INT, &named("list", &[DATA])]))
    ),
    Builtin {
        name: "unMapData",
        free_vars: &["a", "b"],
        typ: function!(
            &named("Map", &[A, B]),
            &named("list", &[&named("pair", &[A, B])])
        ),
        variant: "UnMapData",
        context: &[
            crate::Pred::Trait {
                trait_: super::ReprTrait::Big.qualified(),
                args: &[A],
            },
            crate::Pred::Trait {
                trait_: super::ReprTrait::Big.qualified(),
                args: &[B],
            },
        ],
    },
    Builtin {
        name: "unListData",
        free_vars: &["a"],
        typ: function!(&named("List", &[A]), &named("list", &[A])),
        variant: "UnListData",
        context: &[crate::Pred::Trait {
            trait_: super::ReprTrait::Big.qualified(),
            args: &[A],
        }],
    },
    builtin!("unIData", "UnIData", [], function!(&named("Int", &[]), INT)),
    builtin!(
        "unBData",
        "UnBData",
        [],
        function!(&named("Bytes", &[]), BYTES)
    ),
    builtin!("equalsData", "EqualsData", [], function!(DATA, DATA, BOOL)),
    builtin!("serialiseData", "SerialiseData", [], function!(DATA, BYTES)),
    Builtin {
        name: "mkPairData",
        free_vars: &["a", "b"],
        typ: function!(A, B, &named("pair", &[A, B])),
        variant: "MkPairData",
        context: &[
            crate::Pred::Trait {
                trait_: super::ReprTrait::Big.qualified(),
                args: &[A],
            },
            crate::Pred::Trait {
                trait_: super::ReprTrait::Big.qualified(),
                args: &[B],
            },
        ],
    },
    builtin!(
        "mkNilData",
        "MkNilData",
        [],
        function!(UNIT, &named("list", &[DATA]))
    ),
    builtin!(
        "mkNilPairData",
        "MkNilPairData",
        [],
        function!(UNIT, &named("list", &[&named("pair", &[DATA, DATA])]))
    ),
    builtin!(
        "bls12_381_g1_add",
        "Bls12_381_G1_Add",
        [],
        function!(BLS_G1, BLS_G1, BLS_G1)
    ),
    builtin!(
        "bls12_381_g1_neg",
        "Bls12_381_G1_Neg",
        [],
        function!(BLS_G1, BLS_G1)
    ),
    builtin!(
        "bls12_381_g1_scalarMul",
        "Bls12_381_G1_ScalarMul",
        [],
        function!(INT, BLS_G1, BLS_G1)
    ),
    builtin!(
        "bls12_381_g1_equal",
        "Bls12_381_G1_Equal",
        [],
        function!(BLS_G1, BLS_G1, BOOL)
    ),
    builtin!(
        "bls12_381_g1_compress",
        "Bls12_381_G1_Compress",
        [],
        function!(BLS_G1, BYTES)
    ),
    builtin!(
        "bls12_381_g1_uncompress",
        "Bls12_381_G1_Uncompress",
        [],
        function!(BYTES, BLS_G1)
    ),
    builtin!(
        "bls12_381_g1_hashToGroup",
        "Bls12_381_G1_HashToGroup",
        [],
        function!(BYTES, BYTES, BLS_G1)
    ),
    builtin!(
        "bls12_381_g1_multiScalarMul",
        "Bls12_381_G1_MultiScalarMul",
        [],
        function!(&named("list", &[INT]), &named("list", &[BLS_G1]), BLS_G1)
    ),
    builtin!(
        "bls12_381_g2_add",
        "Bls12_381_G2_Add",
        [],
        function!(BLS_G2, BLS_G2, BLS_G2)
    ),
    builtin!(
        "bls12_381_g2_neg",
        "Bls12_381_G2_Neg",
        [],
        function!(BLS_G2, BLS_G2)
    ),
    builtin!(
        "bls12_381_g2_scalarMul",
        "Bls12_381_G2_ScalarMul",
        [],
        function!(INT, BLS_G2, BLS_G2)
    ),
    builtin!(
        "bls12_381_g2_equal",
        "Bls12_381_G2_Equal",
        [],
        function!(BLS_G2, BLS_G2, BOOL)
    ),
    builtin!(
        "bls12_381_g2_compress",
        "Bls12_381_G2_Compress",
        [],
        function!(BLS_G2, BYTES)
    ),
    builtin!(
        "bls12_381_g2_uncompress",
        "Bls12_381_G2_Uncompress",
        [],
        function!(BYTES, BLS_G2)
    ),
    builtin!(
        "bls12_381_g2_hashToGroup",
        "Bls12_381_G2_HashToGroup",
        [],
        function!(BYTES, BYTES, BLS_G2)
    ),
    builtin!(
        "bls12_381_g2_multiScalarMul",
        "Bls12_381_G2_MultiScalarMul",
        [],
        function!(&named("list", &[INT]), &named("list", &[BLS_G2]), BLS_G2)
    ),
    builtin!(
        "bls12_381_millerLoop",
        "Bls12_381_MillerLoop",
        [],
        function!(BLS_G1, BLS_G2, BLS_MLR)
    ),
    builtin!(
        "bls12_381_mulMlResult",
        "Bls12_381_MulMlResult",
        [],
        function!(BLS_MLR, BLS_MLR, BLS_MLR)
    ),
    builtin!(
        "bls12_381_finalVerify",
        "Bls12_381_FinalVerify",
        [],
        function!(BLS_MLR, BLS_MLR, BOOL)
    ),
    builtin!(
        "integerToByteString",
        "IntegerToByteString",
        [],
        function!(BOOL, INT, INT, BYTES)
    ),
    builtin!(
        "byteStringToInteger",
        "ByteStringToInteger",
        [],
        function!(BOOL, BYTES, INT)
    ),
    builtin!(
        "andByteString",
        "AndByteString",
        [],
        function!(BOOL, BYTES, BYTES, BYTES)
    ),
    builtin!(
        "orByteString",
        "OrByteString",
        [],
        function!(BOOL, BYTES, BYTES, BYTES)
    ),
    builtin!(
        "xorByteString",
        "XorByteString",
        [],
        function!(BOOL, BYTES, BYTES, BYTES)
    ),
    builtin!(
        "complementByteString",
        "ComplementByteString",
        [],
        function!(BYTES, BYTES)
    ),
    builtin!("readBit", "ReadBit", [], function!(BYTES, INT, BOOL)),
    builtin!(
        "writeBits",
        "WriteBits",
        [],
        function!(BYTES, &named("list", &[INT]), BOOL, BYTES)
    ),
    builtin!(
        "replicateByte",
        "ReplicateByte",
        [],
        function!(INT, INT, BYTES)
    ),
    builtin!(
        "shiftByteString",
        "ShiftByteString",
        [],
        function!(BYTES, INT, BYTES)
    ),
    builtin!(
        "rotateByteString",
        "RotateByteString",
        [],
        function!(BYTES, INT, BYTES)
    ),
    builtin!("countSetBits", "CountSetBits", [], function!(BYTES, INT)),
    builtin!(
        "findFirstSetBit",
        "FindFirstSetBit",
        [],
        function!(BYTES, INT)
    ),
    builtin!(
        "expModInteger",
        "ExpModInteger",
        [],
        function!(INT, INT, INT, INT)
    ),
    builtin!(
        "lengthOfArray",
        "LengthOfArray",
        ["a"],
        function!(&named("array", &[A]), INT)
    ),
    builtin!(
        "listToArray",
        "ListToArray",
        ["a"],
        function!(&named("list", &[A]), &named("array", &[A]))
    ),
    builtin!(
        "indexArray",
        "IndexArray",
        ["a"],
        function!(&named("array", &[A]), INT, A)
    ),
    builtin!(
        "insertCoin",
        "InsertCoin",
        [],
        function!(BYTES, BYTES, INT, VALUE, VALUE)
    ),
    builtin!(
        "lookupCoin",
        "LookupCoin",
        [],
        function!(BYTES, BYTES, VALUE, INT)
    ),
    builtin!(
        "unionValue",
        "UnionValue",
        [],
        function!(VALUE, VALUE, VALUE)
    ),
    builtin!(
        "valueContains",
        "ValueContains",
        [],
        function!(VALUE, VALUE, BOOL)
    ),
    builtin!("valueData", "ValueData", [], function!(VALUE, DATA)),
    builtin!("unValueData", "UnValueData", [], function!(DATA, VALUE)),
    builtin!("scaleValue", "ScaleValue", [], function!(INT, VALUE, VALUE)),
];
