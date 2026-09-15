use aiken_lang::ast::BinOp;

pub(crate) fn module_name(path: &[String]) -> String {
    if is_builtin(path) || is_prelude(path) {
        "Builtin".to_owned()
    } else {
        path.join(".")
    }
}

pub(crate) fn is_builtin(path: &[String]) -> bool {
    matches!(path, [aiken, builtin] if aiken == "aiken" && builtin == "builtin")
}

pub(crate) fn is_prelude(path: &[String]) -> bool {
    matches!(path, [name] if name == "aiken")
}

pub(crate) fn primitive(name: &str) -> Option<&'static str> {
    Some(match name {
        "Int" => "int",
        "ByteArray" => "bytes",
        "Bool" => "bool",
        "String" => "string",
        "Data" => "Data",
        "Void" => "unit",
        "List" => "data_list",
        "Pair" => "data_pair",
        "Option" => "data_option",
        "G1Element" => "bls_g1",
        "G2Element" => "bls_g2",
        "MillerLoopResult" => "bls_mlr",
        "Ordering" => "data_ordering",
        "Never" => "data_never",
        "PRNG" => "data_prng",
        "__ScriptPurpose" => "data_script_purpose",
        "__ScriptContext" => "data_script_context",
        _ => return None,
    })
}

pub(crate) fn integer_operator(op: BinOp) -> Option<&'static str> {
    Some(match op {
        BinOp::AddInt => "addInteger",
        BinOp::SubInt => "subtractInteger",
        BinOp::MultInt => "multiplyInteger",
        BinOp::DivInt => "divideInteger",
        BinOp::ModInt => "modInteger",
        BinOp::LtInt | BinOp::GtInt => "lessThanInteger",
        BinOp::LtEqInt | BinOp::GtEqInt => "lessThanEqualsInteger",
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
        "if_then_else" => "ifThenElse",
        "choose_void" => "chooseValue",
        "debug" => "trace",
        "fst_pair" => "dataPairFirst",
        "snd_pair" => "dataPairSecond",
        "choose_list" => "dataListChoose",
        "cons_list" => "dataListCons",
        "head_list" => "dataListHead",
        "tail_list" => "dataListTail",
        "null_list" => "dataListNull",
        "choose_data" => "chooseData",
        "constr_data" => "dataConstr",
        "map_data" => "dataMap",
        "list_data" => "dataList",
        "i_data" => "iData",
        "b_data" => "bData",
        "un_constr_data" => "dataUnConstr",
        "un_map_data" => "dataUnMap",
        "un_list_data" => "dataUnList",
        "un_i_data" => "unIData",
        "un_b_data" => "unBData",
        "new_pair" => "dataPair",
        "new_list" => "dataNil",
        "new_pairs" => "dataNilPair",
        "unconstr_index" => "constrIndex",
        "unconstr_fields" => "constrFields",
        "bls12_381_g1_add" => "bls12_381_g1_add",
        "bls12_381_g1_neg" => "bls12_381_g1_neg",
        "bls12_381_g1_scalar_mul" => "bls12_381_g1_scalarMul",
        "bls12_381_g1_equal" => "bls12_381_g1_equal",
        "bls12_381_g1_compress" => "bls12_381_g1_compress",
        "bls12_381_g1_uncompress" => "bls12_381_g1_uncompress",
        "bls12_381_g1_hash_to_group" => "bls12_381_g1_hashToGroup",
        "bls12_381_g2_add" => "bls12_381_g2_add",
        "bls12_381_g2_neg" => "bls12_381_g2_neg",
        "bls12_381_g2_scalar_mul" => "bls12_381_g2_scalarMul",
        "bls12_381_g2_equal" => "bls12_381_g2_equal",
        "bls12_381_g2_compress" => "bls12_381_g2_compress",
        "bls12_381_g2_uncompress" => "bls12_381_g2_uncompress",
        "bls12_381_g2_hash_to_group" => "bls12_381_g2_hashToGroup",
        "bls12_381_miller_loop" => "bls12_381_millerLoop",
        "bls12_381_mul_miller_loop_result" => "bls12_381_mulMlResult",
        "bls12_381_final_verify" => "bls12_381_finalVerify",
        "integer_to_bytearray" => "integerToByteString",
        "bytearray_to_integer" => "byteStringToInteger",
        "and_bytearray" => "andByteString",
        "or_bytearray" => "orByteString",
        "xor_bytearray" => "xorByteString",
        "complement_bytearray" => "complementByteString",
        "read_bit" => "readBit",
        "write_bits" => "dataWriteBits",
        "replicate_byte" => "replicateByte",
        "shift_bytearray" => "shiftByteString",
        "rotate_bytearray" => "rotateByteString",
        "count_set_bits" => "countSetBits",
        "find_first_set_bit" => "findFirstSetBit",
        _ => return None,
    })
}

pub(crate) fn builtin_arity(name: &str) -> Option<usize> {
    builtin_value(name)?;
    Some(match name {
        "new_list" | "new_pairs" => 0,
        "choose_data" => 6,
        "slice_bytearray"
        | "verify_ed25519_signature"
        | "verify_ecdsa_secp256k1_signature"
        | "verify_schnorr_secp256k1_signature"
        | "if_then_else"
        | "choose_list"
        | "integer_to_bytearray"
        | "and_bytearray"
        | "or_bytearray"
        | "xor_bytearray"
        | "write_bits" => 3,
        "add_integer"
        | "subtract_integer"
        | "multiply_integer"
        | "divide_integer"
        | "quotient_integer"
        | "remainder_integer"
        | "mod_integer"
        | "equals_integer"
        | "less_than_integer"
        | "less_than_equals_integer"
        | "append_bytearray"
        | "cons_bytearray"
        | "index_bytearray"
        | "equals_bytearray"
        | "less_than_bytearray"
        | "less_than_equals_bytearray"
        | "append_string"
        | "equals_string"
        | "choose_void"
        | "debug"
        | "cons_list"
        | "constr_data"
        | "equals_data"
        | "new_pair"
        | "bls12_381_g1_add"
        | "bls12_381_g1_scalar_mul"
        | "bls12_381_g1_equal"
        | "bls12_381_g1_hash_to_group"
        | "bls12_381_g2_add"
        | "bls12_381_g2_scalar_mul"
        | "bls12_381_g2_equal"
        | "bls12_381_g2_hash_to_group"
        | "bls12_381_miller_loop"
        | "bls12_381_mul_miller_loop_result"
        | "bls12_381_final_verify"
        | "bytearray_to_integer"
        | "read_bit"
        | "replicate_byte"
        | "shift_bytearray"
        | "rotate_bytearray" => 2,
        _ => 1,
    })
}

pub(crate) fn prelude_value(name: &str) -> Option<&'static str> {
    Some(match name {
        "not" => "boolNot",
        "identity" => "identity",
        "as_data" => "asData",
        "always" => "always",
        "flip" => "flip",
        "tautology" => "tautology",
        "enumerate" => "enumerate",
        "encode_base16" => "encodeBase16",
        "from_int" => "integerDecimal",
        "do_from_int" => "doFromInt",
        "diagnostic" => "diagnostic",
        _ => return None,
    })
}

pub(crate) fn prelude_arity(name: &str) -> Option<usize> {
    Some(match name {
        "not" | "identity" | "as_data" | "flip" | "tautology" => 1,
        "always" | "from_int" | "do_from_int" | "diagnostic" => 2,
        "encode_base16" => 3,
        "enumerate" => 4,
        _ => return None,
    })
}

pub(crate) fn prelude_constructor(name: &str) -> Option<(&'static str, &'static str)> {
    Some(match name {
        "Pair" => ("data_pair", "Pair"),
        "Void" => ("unit", "Void"),
        "False" => ("bool", "False"),
        "True" => ("bool", "True"),
        "Some" => ("data_option", "Some"),
        "None" => ("data_option", "None"),
        "Less" => ("data_ordering", "Less"),
        "Equal" => ("data_ordering", "Equal"),
        "Greater" => ("data_ordering", "Greater"),
        "Never" => ("data_never", "Never"),
        "Seeded" => ("data_prng", "Seeded"),
        "Replayed" => ("data_prng", "Replayed"),
        "__Mint" => ("data_script_purpose", "__Mint"),
        "__Spend" => ("data_script_purpose", "__Spend"),
        "__Withdraw" => ("data_script_purpose", "__Withdraw"),
        "__Publish" => ("data_script_purpose", "__Publish"),
        "__Vote" => ("data_script_purpose", "__Vote"),
        "__Propose" => ("data_script_purpose", "__Propose"),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_pinned_builtin_environment_has_compiler_owned_lowerings() {
        let upstream = aiken_lang::builtins::plutus(&aiken_lang::IdGenerator::new());
        for (name, value) in &upstream.values {
            let lowered = builtin_value(name).unwrap_or_else(|| panic!("missing builtin {name}"));
            let declaration =
                nash_codegen::builtins::definition(lowered).expect("compiler-owned builtin");
            let aiken_lang::tipo::Type::Fn { args, .. } = value.tipo.as_ref() else {
                panic!("{name}")
            };
            assert_eq!(builtin_arity(name), Some(args.len()), "{name}");
            assert_eq!(
                source_type(&value.tipo, &mut Default::default()),
                compiler_type(&declaration.typ.value, &mut Default::default()),
                "{name}"
            );
        }
        assert_eq!(upstream.values.len(), 89);
        for newer in [
            "exp_mod_integer",
            "drop_list",
            "length_of_array",
            "bls12_381_g1_multi_scalar_mul",
        ] {
            assert_eq!(builtin_value(newer), None);
        }
    }

    #[test]
    fn exact_pinned_prelude_values_and_types_are_available() {
        use aiken_lang::tipo::ValueConstructorVariant;
        let upstream = aiken_lang::builtins::prelude(&aiken_lang::IdGenerator::new());
        for name in upstream.types.keys() {
            assert!(
                primitive(name).is_some()
                    || matches!(name.as_str(), "Pairs" | "Fuzzer" | "Sampler"),
                "{name}"
            );
        }
        for (name, value) in &upstream.values {
            match &value.variant {
                ValueConstructorVariant::ModuleFn { arity, .. } => {
                    assert_eq!(prelude_arity(name), Some(*arity), "{name}");
                    let lowered = prelude_value(name)
                        .unwrap_or_else(|| panic!("missing prelude function {name}"));
                    let declaration = nash_codegen::builtins::definition(lowered)
                        .expect("compiler-owned prelude");
                    assert_eq!(
                        source_type(&value.tipo, &mut Default::default()),
                        compiler_type(&declaration.typ.value, &mut Default::default()),
                        "{name}"
                    );
                }
                ValueConstructorVariant::Record { .. } => {
                    let (typ, ctor) = prelude_constructor(name)
                        .unwrap_or_else(|| panic!("missing prelude constructor {name}"));
                    assert!(
                        nash_ast::primitives::structural_constructor(typ, ctor).is_some()
                            || nash_ast::primitives::PRIMITIVES
                                .iter()
                                .any(|p| p.name == typ && p.ctors.iter().any(|c| c.name == ctor)),
                        "{name}"
                    );
                }
                _ => panic!("unexpected prelude value {name}: {:?}", value.variant),
            }
        }
    }

    fn source_type(
        typ: &aiken_lang::tipo::Type,
        variables: &mut std::collections::BTreeMap<u64, usize>,
    ) -> String {
        use aiken_lang::tipo::{Type, TypeVar};
        match typ {
            Type::App { name, args, .. } => {
                let name = primitive(name).expect("builtin prelude type");
                format!(
                    "{name}<{}>",
                    args.iter()
                        .map(|t| source_type(t, variables))
                        .collect::<Vec<_>>()
                        .join(",")
                )
            }
            Type::Pair { fst, snd, .. } => format!(
                "data_pair<{},{}>",
                source_type(fst, variables),
                source_type(snd, variables)
            ),
            Type::Fn { args, ret, .. } => {
                let result = if args.is_empty() {
                    vec!["unit<>".to_owned()]
                } else {
                    args.iter().map(|t| source_type(t, variables)).collect()
                };
                result
                    .into_iter()
                    .rev()
                    .fold(source_type(ret, variables), |result, arg| {
                        format!("({arg}->{result})")
                    })
            }
            Type::Var { tipo, .. } => match &*tipo.borrow() {
                TypeVar::Link { tipo } => source_type(tipo, variables),
                TypeVar::Generic { id } | TypeVar::Unbound { id } => {
                    let next = variables.len();
                    format!("v{}", variables.entry(*id).or_insert(next))
                }
            },
            Type::Tuple { .. } => panic!("tuple is not a builtin signature"),
        }
    }

    fn compiler_type(
        typ: &nash_ast::Type<'_>,
        variables: &mut std::collections::BTreeMap<String, usize>,
    ) -> String {
        match typ {
            nash_ast::Type::Named { reference, args } => format!(
                "{}<{}>",
                reference.name,
                args.iter()
                    .map(|t| compiler_type(&t.value, variables))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            nash_ast::Type::Lambda { from, to } => format!(
                "({}->{})",
                compiler_type(&from.value, variables),
                compiler_type(&to.value, variables)
            ),
            nash_ast::Type::Function { arguments, result } => {
                let args: Vec<_> = arguments
                    .iter()
                    .map(|t| compiler_type(&t.value, variables))
                    .collect();
                args.into_iter()
                    .rev()
                    .fold(compiler_type(&result.value, variables), |result, arg| {
                        format!("({arg}->{result})")
                    })
            }
            nash_ast::Type::Var(name) => {
                let next = variables.len();
                format!("v{}", variables.entry((*name).to_owned()).or_insert(next))
            }
            _ => panic!("unsupported compiler-owned signature {typ:?}"),
        }
    }
}
