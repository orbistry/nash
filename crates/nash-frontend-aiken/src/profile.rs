use aiken_lang::ast::BinOp;

pub(crate) fn module_name(path: &[String]) -> String {
    if is_builtin(path) {
        "Builtin".to_owned()
    } else {
        path.join(".")
    }
}

pub(crate) fn is_builtin(path: &[String]) -> bool {
    matches!(path, [aiken, builtin] if aiken == "aiken" && builtin == "builtin")
}

pub(crate) fn primitive(name: &str) -> Option<&'static str> {
    Some(match name {
        "Int" => "int",
        "ByteArray" => "bytes",
        "Bool" => "bool",
        "String" => "string",
        "Data" => "Data",
        "Void" => "unit",
        "List" => "list",
        "Pair" => "pair",
        "G1Element" => "bls_g1",
        "G2Element" => "bls_g2",
        "MillerLoopResult" => "bls_mlr",
        _ => return None,
    })
}

// Little Nash types admit constant/term fields; uppercase types require Big fields.
pub(crate) fn user_type(name: &str) -> String {
    let mut name = name.to_owned();
    if let Some(first) = name.get_mut(..1) {
        first.make_ascii_lowercase();
    }
    name
}

pub(crate) fn integer_operator(op: BinOp) -> Option<&'static str> {
    Some(match op {
        BinOp::AddInt => "addInteger",
        BinOp::SubInt => "subtractInteger",
        BinOp::MultInt => "multiplyInteger",
        BinOp::DivInt => "divideInteger",
        BinOp::ModInt => "modInteger",
        BinOp::LtInt | BinOp::GtEqInt => "lessThanInteger",
        BinOp::LtEqInt | BinOp::GtInt => "lessThanEqualsInteger",
        _ => return None,
    })
}

// Explicitly audited against uplc::DefaultFunction::aiken_name and Nash BUILTINS.
pub(crate) fn builtin_value(name: &str) -> Option<&'static str> {
    Some(match name {
        "add_integer" => "addInteger",
        "subtract_integer" => "subtractInteger",
        "multiply_integer" => "multiplyInteger",
        "divide_integer" => "divideInteger",
        "quotient_integer" => "quotientInteger",
        "remainder_integer" => "remainderInteger",
        "mod_integer" => "modInteger",
        "equals_integer" => "equalsInteger",
        "less_than_integer" => "lessThanInteger",
        "less_than_equals_integer" => "lessThanEqualsInteger",
        "append_bytearray" => "appendByteString",
        "cons_bytearray" => "consByteString",
        "slice_bytearray" => "sliceByteString",
        "length_of_bytearray" => "lengthOfByteString",
        "index_bytearray" => "indexByteString",
        "equals_bytearray" => "equalsByteString",
        "less_than_bytearray" => "lessThanByteString",
        "less_than_equals_bytearray" => "lessThanEqualsByteString",
        "sha2_256" => "sha2_256",
        "sha3_256" => "sha3_256",
        "blake2b_224" => "blake2b_224",
        "blake2b_256" => "blake2b_256",
        "keccak_256" => "keccak_256",
        "ripemd_160" => "ripemd_160",
        "verify_ed25519_signature" => "verifyEd25519Signature",
        "verify_ecdsa_secp256k1_signature" => "verifyEcdsaSecp256k1Signature",
        "verify_schnorr_secp256k1_signature" => "verifySchnorrSecp256k1Signature",
        "append_string" => "appendString",
        "equals_string" => "equalsString",
        "encode_utf8" => "encodeUtf8",
        "decode_utf8" => "decodeUtf8",
        "equals_data" => "equalsData",
        "serialise_data" => "serialiseData",
        _ => return None,
    })
}
