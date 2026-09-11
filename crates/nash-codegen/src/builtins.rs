//! Mapping from the compiler-owned value inventory to runtime operations.
use nash_ast::primitives::{BUILTINS, Builtin, BuiltinLowering};
use nash_plutus::builtin::DefaultFunction;

pub fn definition(name: &str) -> Option<&'static Builtin> {
    BUILTINS.iter().find(|builtin| builtin.name == name)
}

pub fn by_name(name: &str) -> Option<DefaultFunction> {
    let BuiltinLowering::Plutus(variant) = definition(name)?.lowering else {
        return None;
    };
    by_variant(variant)
}

fn by_variant(variant: &str) -> Option<DefaultFunction> {
    Some(match variant {
        "AddInteger" => DefaultFunction::AddInteger,
        "SubtractInteger" => DefaultFunction::SubtractInteger,
        "MultiplyInteger" => DefaultFunction::MultiplyInteger,
        "DivideInteger" => DefaultFunction::DivideInteger,
        "QuotientInteger" => DefaultFunction::QuotientInteger,
        "RemainderInteger" => DefaultFunction::RemainderInteger,
        "ModInteger" => DefaultFunction::ModInteger,
        "EqualsInteger" => DefaultFunction::EqualsInteger,
        "LessThanInteger" => DefaultFunction::LessThanInteger,
        "LessThanEqualsInteger" => DefaultFunction::LessThanEqualsInteger,
        "AppendByteString" => DefaultFunction::AppendByteString,
        "ConsByteString" => DefaultFunction::ConsByteString,
        "SliceByteString" => DefaultFunction::SliceByteString,
        "LengthOfByteString" => DefaultFunction::LengthOfByteString,
        "IndexByteString" => DefaultFunction::IndexByteString,
        "EqualsByteString" => DefaultFunction::EqualsByteString,
        "LessThanByteString" => DefaultFunction::LessThanByteString,
        "LessThanEqualsByteString" => DefaultFunction::LessThanEqualsByteString,
        "Sha2_256" => DefaultFunction::Sha2_256,
        "Sha3_256" => DefaultFunction::Sha3_256,
        "Blake2b_256" => DefaultFunction::Blake2b_256,
        "Keccak_256" => DefaultFunction::Keccak_256,
        "Blake2b_224" => DefaultFunction::Blake2b_224,
        "VerifyEd25519Signature" => DefaultFunction::VerifyEd25519Signature,
        "VerifyEcdsaSecp256k1Signature" => DefaultFunction::VerifyEcdsaSecp256k1Signature,
        "VerifySchnorrSecp256k1Signature" => DefaultFunction::VerifySchnorrSecp256k1Signature,
        "AppendString" => DefaultFunction::AppendString,
        "EqualsString" => DefaultFunction::EqualsString,
        "EncodeUtf8" => DefaultFunction::EncodeUtf8,
        "DecodeUtf8" => DefaultFunction::DecodeUtf8,
        "IfThenElse" => DefaultFunction::IfThenElse,
        "ChooseUnit" => DefaultFunction::ChooseUnit,
        "Trace" => DefaultFunction::Trace,
        "FstPair" => DefaultFunction::FstPair,
        "SndPair" => DefaultFunction::SndPair,
        "ChooseList" => DefaultFunction::ChooseList,
        "MkCons" => DefaultFunction::MkCons,
        "HeadList" => DefaultFunction::HeadList,
        "TailList" => DefaultFunction::TailList,
        "NullList" => DefaultFunction::NullList,
        "ChooseData" => DefaultFunction::ChooseData,
        "ConstrData" => DefaultFunction::ConstrData,
        "MapData" => DefaultFunction::MapData,
        "ListData" => DefaultFunction::ListData,
        "IData" => DefaultFunction::IData,
        "BData" => DefaultFunction::BData,
        "UnConstrData" => DefaultFunction::UnConstrData,
        "UnMapData" => DefaultFunction::UnMapData,
        "UnListData" => DefaultFunction::UnListData,
        "UnIData" => DefaultFunction::UnIData,
        "UnBData" => DefaultFunction::UnBData,
        "EqualsData" => DefaultFunction::EqualsData,
        "SerialiseData" => DefaultFunction::SerialiseData,
        "MkPairData" => DefaultFunction::MkPairData,
        "MkNilData" => DefaultFunction::MkNilData,
        "MkNilPairData" => DefaultFunction::MkNilPairData,
        "Bls12_381_G1_Add" => DefaultFunction::Bls12_381_G1_Add,
        "Bls12_381_G1_Neg" => DefaultFunction::Bls12_381_G1_Neg,
        "Bls12_381_G1_ScalarMul" => DefaultFunction::Bls12_381_G1_ScalarMul,
        "Bls12_381_G1_Equal" => DefaultFunction::Bls12_381_G1_Equal,
        "Bls12_381_G1_Compress" => DefaultFunction::Bls12_381_G1_Compress,
        "Bls12_381_G1_Uncompress" => DefaultFunction::Bls12_381_G1_Uncompress,
        "Bls12_381_G1_HashToGroup" => DefaultFunction::Bls12_381_G1_HashToGroup,
        "Bls12_381_G2_Add" => DefaultFunction::Bls12_381_G2_Add,
        "Bls12_381_G2_Neg" => DefaultFunction::Bls12_381_G2_Neg,
        "Bls12_381_G2_ScalarMul" => DefaultFunction::Bls12_381_G2_ScalarMul,
        "Bls12_381_G2_Equal" => DefaultFunction::Bls12_381_G2_Equal,
        "Bls12_381_G2_Compress" => DefaultFunction::Bls12_381_G2_Compress,
        "Bls12_381_G2_Uncompress" => DefaultFunction::Bls12_381_G2_Uncompress,
        "Bls12_381_G2_HashToGroup" => DefaultFunction::Bls12_381_G2_HashToGroup,
        "Bls12_381_MillerLoop" => DefaultFunction::Bls12_381_MillerLoop,
        "Bls12_381_MulMlResult" => DefaultFunction::Bls12_381_MulMlResult,
        "Bls12_381_FinalVerify" => DefaultFunction::Bls12_381_FinalVerify,
        "IntegerToByteString" => DefaultFunction::IntegerToByteString,
        "ByteStringToInteger" => DefaultFunction::ByteStringToInteger,
        "AndByteString" => DefaultFunction::AndByteString,
        "OrByteString" => DefaultFunction::OrByteString,
        "XorByteString" => DefaultFunction::XorByteString,
        "ComplementByteString" => DefaultFunction::ComplementByteString,
        "ReadBit" => DefaultFunction::ReadBit,
        "WriteBits" => DefaultFunction::WriteBits,
        "ReplicateByte" => DefaultFunction::ReplicateByte,
        "ShiftByteString" => DefaultFunction::ShiftByteString,
        "RotateByteString" => DefaultFunction::RotateByteString,
        "CountSetBits" => DefaultFunction::CountSetBits,
        "FindFirstSetBit" => DefaultFunction::FindFirstSetBit,
        "Ripemd_160" => DefaultFunction::Ripemd_160,
        "ExpModInteger" => DefaultFunction::ExpModInteger,
        "DropList" => DefaultFunction::DropList,
        "LengthOfArray" => DefaultFunction::LengthOfArray,
        "ListToArray" => DefaultFunction::ListToArray,
        "IndexArray" => DefaultFunction::IndexArray,
        "Bls12_381_G1_MultiScalarMul" => DefaultFunction::Bls12_381_G1_MultiScalarMul,
        "Bls12_381_G2_MultiScalarMul" => DefaultFunction::Bls12_381_G2_MultiScalarMul,
        "InsertCoin" => DefaultFunction::InsertCoin,
        "LookupCoin" => DefaultFunction::LookupCoin,
        "UnionValue" => DefaultFunction::UnionValue,
        "ValueContains" => DefaultFunction::ValueContains,
        "ValueData" => DefaultFunction::ValueData,
        "UnValueData" => DefaultFunction::UnValueData,
        "ScaleValue" => DefaultFunction::ScaleValue,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compiler_inventory_covers_every_runtime_builtin_and_arity() {
        for func in DefaultFunction::ALL {
            let declarations = BUILTINS
                .iter()
                .filter(|b| by_name(b.name) == Some(*func))
                .collect::<Vec<_>>();
            assert_eq!(declarations.len(), 1, "{func:?}");
            let mut typ = &declarations[0].typ.value;
            let mut arity = 0;
            while let nash_ast::Type::Lambda { to, .. } = typ {
                arity += 1;
                typ = &to.value;
            }
            assert_eq!(arity, func.arity(), "{func:?}");
        }
        for builtin in BUILTINS {
            if let BuiltinLowering::Plutus(variant) = builtin.lowering {
                assert!(by_variant(variant).is_some(), "{}", builtin.name);
            }
        }
        assert_eq!(by_name("identity"), None);
        assert_eq!(by_name("nonexistent"), None);
    }
}
