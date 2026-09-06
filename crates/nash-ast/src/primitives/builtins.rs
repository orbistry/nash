//! Typed value schemes for the synthetic Builtin module.

use super::builtin_home;
use crate::{QualifiedName, Type};
use nash_region::Located;

/// Backend operation; runtime arities and force counts remain in nash-plutus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltinLowering {
    /// Symbolic DefaultFunction variant, resolved by code generation.
    Plutus(&'static str),
    Identity,
    Error,
    /// Core-only representation intrinsics; lowered to typed casts in Plan 07.
    CastToData,
    CastFromDataShallow,
    CastValidateData,
    CastLift,
    CastLower,
}

impl BuiltinLowering {
    pub fn is_core_only(self) -> bool {
        matches!(
            self,
            Self::CastToData
                | Self::CastFromDataShallow
                | Self::CastValidateData
                | Self::CastLift
                | Self::CastLower
        )
    }
}

pub struct Builtin {
    pub name: &'static str,
    pub free_vars: &'static [&'static str],
    pub typ: &'static Located<Type<'static>>,
    pub lowering: BuiltinLowering,
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
    ($name:literal, $lowering:ident $(($variant:literal))?, [$($var:literal),*], $typ:expr) => {
        Builtin { name: $name, free_vars: &[$($var),*], typ: $typ, lowering: BuiltinLowering::$lowering $(($variant))? }
    };
}

const A: &Located<Type<'static>> = &Located::at_zero(Type::Var("a"));
const B: &Located<Type<'static>> = &Located::at_zero(Type::Var("b"));
const BIG_A: &Located<Type<'static>> = &Located::at_zero(Type::Kinded {
    typ: A,
    kind: &Located::at_zero(nash_source::Kind::Big),
});
const UNIT: &Located<Type<'static>> = &Located::at_zero(Type::Unit);
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
    builtin!(
        "addInteger",
        Plutus("AddInteger"),
        [],
        function!(INT, INT, INT)
    ),
    builtin!(
        "subtractInteger",
        Plutus("SubtractInteger"),
        [],
        function!(INT, INT, INT)
    ),
    builtin!(
        "multiplyInteger",
        Plutus("MultiplyInteger"),
        [],
        function!(INT, INT, INT)
    ),
    builtin!(
        "divideInteger",
        Plutus("DivideInteger"),
        [],
        function!(INT, INT, INT)
    ),
    builtin!(
        "quotientInteger",
        Plutus("QuotientInteger"),
        [],
        function!(INT, INT, INT)
    ),
    builtin!(
        "remainderInteger",
        Plutus("RemainderInteger"),
        [],
        function!(INT, INT, INT)
    ),
    builtin!(
        "modInteger",
        Plutus("ModInteger"),
        [],
        function!(INT, INT, INT)
    ),
    builtin!(
        "equalsInteger",
        Plutus("EqualsInteger"),
        [],
        function!(INT, INT, BOOL)
    ),
    builtin!(
        "lessThanInteger",
        Plutus("LessThanInteger"),
        [],
        function!(INT, INT, BOOL)
    ),
    builtin!(
        "lessThanEqualsInteger",
        Plutus("LessThanEqualsInteger"),
        [],
        function!(INT, INT, BOOL)
    ),
    builtin!(
        "appendByteString",
        Plutus("AppendByteString"),
        [],
        function!(BYTES, BYTES, BYTES)
    ),
    builtin!(
        "consByteString",
        Plutus("ConsByteString"),
        [],
        function!(INT, BYTES, BYTES)
    ),
    builtin!(
        "sliceByteString",
        Plutus("SliceByteString"),
        [],
        function!(INT, INT, BYTES, BYTES)
    ),
    builtin!(
        "lengthOfByteString",
        Plutus("LengthOfByteString"),
        [],
        function!(BYTES, INT)
    ),
    builtin!(
        "indexByteString",
        Plutus("IndexByteString"),
        [],
        function!(BYTES, INT, INT)
    ),
    builtin!(
        "equalsByteString",
        Plutus("EqualsByteString"),
        [],
        function!(BYTES, BYTES, BOOL)
    ),
    builtin!(
        "lessThanByteString",
        Plutus("LessThanByteString"),
        [],
        function!(BYTES, BYTES, BOOL)
    ),
    builtin!(
        "lessThanEqualsByteString",
        Plutus("LessThanEqualsByteString"),
        [],
        function!(BYTES, BYTES, BOOL)
    ),
    builtin!("sha2_256", Plutus("Sha2_256"), [], function!(BYTES, BYTES)),
    builtin!("sha3_256", Plutus("Sha3_256"), [], function!(BYTES, BYTES)),
    builtin!(
        "blake2b_256",
        Plutus("Blake2b_256"),
        [],
        function!(BYTES, BYTES)
    ),
    builtin!(
        "blake2b_224",
        Plutus("Blake2b_224"),
        [],
        function!(BYTES, BYTES)
    ),
    builtin!(
        "keccak_256",
        Plutus("Keccak_256"),
        [],
        function!(BYTES, BYTES)
    ),
    builtin!(
        "ripemd_160",
        Plutus("Ripemd_160"),
        [],
        function!(BYTES, BYTES)
    ),
    builtin!(
        "verifyEd25519Signature",
        Plutus("VerifyEd25519Signature"),
        [],
        function!(BYTES, BYTES, BYTES, BOOL)
    ),
    builtin!(
        "verifyEcdsaSecp256k1Signature",
        Plutus("VerifyEcdsaSecp256k1Signature"),
        [],
        function!(BYTES, BYTES, BYTES, BOOL)
    ),
    builtin!(
        "verifySchnorrSecp256k1Signature",
        Plutus("VerifySchnorrSecp256k1Signature"),
        [],
        function!(BYTES, BYTES, BYTES, BOOL)
    ),
    builtin!(
        "appendString",
        Plutus("AppendString"),
        [],
        function!(STRING, STRING, STRING)
    ),
    builtin!(
        "equalsString",
        Plutus("EqualsString"),
        [],
        function!(STRING, STRING, BOOL)
    ),
    builtin!(
        "encodeUtf8",
        Plutus("EncodeUtf8"),
        [],
        function!(STRING, BYTES)
    ),
    builtin!(
        "decodeUtf8",
        Plutus("DecodeUtf8"),
        [],
        function!(BYTES, STRING)
    ),
    builtin!(
        "ifThenElse",
        Plutus("IfThenElse"),
        ["a"],
        function!(BOOL, A, A, A)
    ),
    builtin!(
        "chooseUnit",
        Plutus("ChooseUnit"),
        ["a"],
        function!(UNIT, A, A)
    ),
    builtin!("trace", Plutus("Trace"), ["a"], function!(STRING, A, A)),
    builtin!(
        "fstPair",
        Plutus("FstPair"),
        ["a", "b"],
        function!(&named("pair", &[A, B]), A)
    ),
    builtin!(
        "sndPair",
        Plutus("SndPair"),
        ["a", "b"],
        function!(&named("pair", &[A, B]), B)
    ),
    builtin!(
        "chooseList",
        Plutus("ChooseList"),
        ["a", "b"],
        function!(&named("list", &[A]), B, B, B)
    ),
    builtin!(
        "mkCons",
        Plutus("MkCons"),
        ["a"],
        function!(A, &named("list", &[A]), &named("list", &[A]))
    ),
    builtin!(
        "headList",
        Plutus("HeadList"),
        ["a"],
        function!(&named("list", &[A]), A)
    ),
    builtin!(
        "tailList",
        Plutus("TailList"),
        ["a"],
        function!(&named("list", &[A]), &named("list", &[A]))
    ),
    builtin!(
        "nullList",
        Plutus("NullList"),
        ["a"],
        function!(&named("list", &[A]), BOOL)
    ),
    builtin!(
        "dropList",
        Plutus("DropList"),
        ["a"],
        function!(INT, &named("list", &[A]), &named("list", &[A]))
    ),
    builtin!(
        "chooseData",
        Plutus("ChooseData"),
        ["a"],
        function!(DATA, A, A, A, A, A, A)
    ),
    builtin!(
        "constrData",
        Plutus("ConstrData"),
        [],
        function!(INT, &named("list", &[DATA]), DATA)
    ),
    builtin!(
        "mapData",
        Plutus("MapData"),
        [],
        function!(&named("list", &[&named("pair", &[DATA, DATA])]), DATA)
    ),
    builtin!(
        "listData",
        Plutus("ListData"),
        ["a"],
        function!(&named("list", &[BIG_A]), DATA)
    ),
    builtin!("iData", Plutus("IData"), [], function!(INT, DATA)),
    builtin!("bData", Plutus("BData"), [], function!(BYTES, DATA)),
    builtin!(
        "unConstrData",
        Plutus("UnConstrData"),
        [],
        function!(DATA, &named("pair", &[INT, &named("list", &[DATA])]))
    ),
    builtin!(
        "unMapData",
        Plutus("UnMapData"),
        [],
        function!(DATA, &named("list", &[&named("pair", &[DATA, DATA])]))
    ),
    builtin!(
        "unListData",
        Plutus("UnListData"),
        [],
        function!(DATA, &named("list", &[DATA]))
    ),
    builtin!("unIData", Plutus("UnIData"), [], function!(DATA, INT)),
    builtin!("unBData", Plutus("UnBData"), [], function!(DATA, BYTES)),
    builtin!(
        "equalsData",
        Plutus("EqualsData"),
        [],
        function!(DATA, DATA, BOOL)
    ),
    builtin!(
        "serialiseData",
        Plutus("SerialiseData"),
        [],
        function!(DATA, BYTES)
    ),
    builtin!(
        "mkPairData",
        Plutus("MkPairData"),
        [],
        function!(DATA, DATA, &named("pair", &[DATA, DATA]))
    ),
    builtin!(
        "mkNilData",
        Plutus("MkNilData"),
        [],
        function!(UNIT, &named("list", &[DATA]))
    ),
    builtin!(
        "mkNilPairData",
        Plutus("MkNilPairData"),
        [],
        function!(UNIT, &named("list", &[&named("pair", &[DATA, DATA])]))
    ),
    builtin!(
        "bls12_381_g1_add",
        Plutus("Bls12_381_G1_Add"),
        [],
        function!(BLS_G1, BLS_G1, BLS_G1)
    ),
    builtin!(
        "bls12_381_g1_neg",
        Plutus("Bls12_381_G1_Neg"),
        [],
        function!(BLS_G1, BLS_G1)
    ),
    builtin!(
        "bls12_381_g1_scalarMul",
        Plutus("Bls12_381_G1_ScalarMul"),
        [],
        function!(INT, BLS_G1, BLS_G1)
    ),
    builtin!(
        "bls12_381_g1_equal",
        Plutus("Bls12_381_G1_Equal"),
        [],
        function!(BLS_G1, BLS_G1, BOOL)
    ),
    builtin!(
        "bls12_381_g1_compress",
        Plutus("Bls12_381_G1_Compress"),
        [],
        function!(BLS_G1, BYTES)
    ),
    builtin!(
        "bls12_381_g1_uncompress",
        Plutus("Bls12_381_G1_Uncompress"),
        [],
        function!(BYTES, BLS_G1)
    ),
    builtin!(
        "bls12_381_g1_hashToGroup",
        Plutus("Bls12_381_G1_HashToGroup"),
        [],
        function!(BYTES, BYTES, BLS_G1)
    ),
    builtin!(
        "bls12_381_g1_multiScalarMul",
        Plutus("Bls12_381_G1_MultiScalarMul"),
        [],
        function!(&named("list", &[INT]), &named("list", &[BLS_G1]), BLS_G1)
    ),
    builtin!(
        "bls12_381_g2_add",
        Plutus("Bls12_381_G2_Add"),
        [],
        function!(BLS_G2, BLS_G2, BLS_G2)
    ),
    builtin!(
        "bls12_381_g2_neg",
        Plutus("Bls12_381_G2_Neg"),
        [],
        function!(BLS_G2, BLS_G2)
    ),
    builtin!(
        "bls12_381_g2_scalarMul",
        Plutus("Bls12_381_G2_ScalarMul"),
        [],
        function!(INT, BLS_G2, BLS_G2)
    ),
    builtin!(
        "bls12_381_g2_equal",
        Plutus("Bls12_381_G2_Equal"),
        [],
        function!(BLS_G2, BLS_G2, BOOL)
    ),
    builtin!(
        "bls12_381_g2_compress",
        Plutus("Bls12_381_G2_Compress"),
        [],
        function!(BLS_G2, BYTES)
    ),
    builtin!(
        "bls12_381_g2_uncompress",
        Plutus("Bls12_381_G2_Uncompress"),
        [],
        function!(BYTES, BLS_G2)
    ),
    builtin!(
        "bls12_381_g2_hashToGroup",
        Plutus("Bls12_381_G2_HashToGroup"),
        [],
        function!(BYTES, BYTES, BLS_G2)
    ),
    builtin!(
        "bls12_381_g2_multiScalarMul",
        Plutus("Bls12_381_G2_MultiScalarMul"),
        [],
        function!(&named("list", &[INT]), &named("list", &[BLS_G2]), BLS_G2)
    ),
    builtin!(
        "bls12_381_millerLoop",
        Plutus("Bls12_381_MillerLoop"),
        [],
        function!(BLS_G1, BLS_G2, BLS_MLR)
    ),
    builtin!(
        "bls12_381_mulMlResult",
        Plutus("Bls12_381_MulMlResult"),
        [],
        function!(BLS_MLR, BLS_MLR, BLS_MLR)
    ),
    builtin!(
        "bls12_381_finalVerify",
        Plutus("Bls12_381_FinalVerify"),
        [],
        function!(BLS_MLR, BLS_MLR, BOOL)
    ),
    builtin!(
        "integerToByteString",
        Plutus("IntegerToByteString"),
        [],
        function!(BOOL, INT, INT, BYTES)
    ),
    builtin!(
        "byteStringToInteger",
        Plutus("ByteStringToInteger"),
        [],
        function!(BOOL, BYTES, INT)
    ),
    builtin!(
        "andByteString",
        Plutus("AndByteString"),
        [],
        function!(BOOL, BYTES, BYTES, BYTES)
    ),
    builtin!(
        "orByteString",
        Plutus("OrByteString"),
        [],
        function!(BOOL, BYTES, BYTES, BYTES)
    ),
    builtin!(
        "xorByteString",
        Plutus("XorByteString"),
        [],
        function!(BOOL, BYTES, BYTES, BYTES)
    ),
    builtin!(
        "complementByteString",
        Plutus("ComplementByteString"),
        [],
        function!(BYTES, BYTES)
    ),
    builtin!(
        "readBit",
        Plutus("ReadBit"),
        [],
        function!(BYTES, INT, BOOL)
    ),
    builtin!(
        "writeBits",
        Plutus("WriteBits"),
        [],
        function!(BYTES, &named("list", &[INT]), BOOL, BYTES)
    ),
    builtin!(
        "replicateByte",
        Plutus("ReplicateByte"),
        [],
        function!(INT, INT, BYTES)
    ),
    builtin!(
        "shiftByteString",
        Plutus("ShiftByteString"),
        [],
        function!(BYTES, INT, BYTES)
    ),
    builtin!(
        "rotateByteString",
        Plutus("RotateByteString"),
        [],
        function!(BYTES, INT, BYTES)
    ),
    builtin!(
        "countSetBits",
        Plutus("CountSetBits"),
        [],
        function!(BYTES, INT)
    ),
    builtin!(
        "findFirstSetBit",
        Plutus("FindFirstSetBit"),
        [],
        function!(BYTES, INT)
    ),
    builtin!(
        "expModInteger",
        Plutus("ExpModInteger"),
        [],
        function!(INT, INT, INT, INT)
    ),
    builtin!(
        "lengthOfArray",
        Plutus("LengthOfArray"),
        ["a"],
        function!(&named("array", &[A]), INT)
    ),
    builtin!(
        "listToArray",
        Plutus("ListToArray"),
        ["a"],
        function!(&named("list", &[A]), &named("array", &[A]))
    ),
    builtin!(
        "indexArray",
        Plutus("IndexArray"),
        ["a"],
        function!(&named("array", &[A]), INT, A)
    ),
    builtin!(
        "insertCoin",
        Plutus("InsertCoin"),
        [],
        function!(BYTES, BYTES, INT, VALUE, VALUE)
    ),
    builtin!(
        "lookupCoin",
        Plutus("LookupCoin"),
        [],
        function!(BYTES, BYTES, VALUE, INT)
    ),
    builtin!(
        "unionValue",
        Plutus("UnionValue"),
        [],
        function!(VALUE, VALUE, VALUE)
    ),
    builtin!(
        "valueContains",
        Plutus("ValueContains"),
        [],
        function!(VALUE, VALUE, BOOL)
    ),
    builtin!("valueData", Plutus("ValueData"), [], function!(VALUE, DATA)),
    builtin!(
        "unValueData",
        Plutus("UnValueData"),
        [],
        function!(DATA, VALUE)
    ),
    builtin!(
        "scaleValue",
        Plutus("ScaleValue"),
        [],
        function!(INT, VALUE, VALUE)
    ),
    builtin!("identity", Identity, ["a"], function!(A, A)),
    builtin!("error", Error, ["a"], function!(UNIT, A)),
    builtin!("castToData", CastToData, ["a", "b"], function!(A, B)),
    builtin!(
        "castFromDataShallow",
        CastFromDataShallow,
        ["a", "b"],
        function!(A, B)
    ),
    builtin!(
        "castValidateData",
        CastValidateData,
        ["a", "b"],
        function!(A, B)
    ),
    builtin!("castLift", CastLift, ["a", "b"], function!(A, B)),
    builtin!("castLower", CastLower, ["a", "b"], function!(A, B)),
];
