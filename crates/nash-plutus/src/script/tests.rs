use super::*;
use crate::{arena::Arena, binder::DeBruijn, program::Version};

#[test]
fn known_script_hash_vectors() {
    // Aiken hello_world/plutus.json, compiler v1.1.0+9407b67, V3 blueprint.
    // Aiken checkout bd0e4e30c6d8f51221bd9aabf22cfeb794d72884.
    // V1/V2 cross-language expected digests independently computed with
    // Python hashlib.blake2b(bytes([tag])+cbor, digest_size=28).
    let cbor = hex::decode("59011d0101003232323232323225333002323232323253330073370e900118041baa0011323232533300a3370e900018059baa005132533300e0011613253333330120011616161613253330103012003132533300e3370e900018079baa005132533300f002100114a06644646600200200644a66602a00229404c94ccc04ccdc79bae301700200414a2266006006002602e0026eb0c048c04cc04cc04cc04cc04cc04cc04cc04cc040dd50059bae301230103754602460206ea801458cdc79bae3011300f375401091010d48656c6c6f2c20576f726c64210016375c002601e00260186ea801458c034c038008c030004c024dd50008b1805180580118048009804801180380098021baa00114984d9595cd2ab9d5573caae7d5d0aba25749").unwrap();
    assert_eq!(
        hex::encode(script_hash(PlutusVersion::V1, &cbor)),
        "9bd23c07636d2e8722eb9851215b02717edd36bdab66189591796848"
    );
    assert_eq!(
        hex::encode(script_hash(PlutusVersion::V2, &cbor)),
        "86fed0146e9e88aafc14e3832792ead47c3c6f9b33e2c52c3b7d7f52"
    );
    assert_eq!(
        hex::encode(script_hash(PlutusVersion::V3, &cbor)),
        "167f56e1b5de377df88962340a0461158e68d4b6caaea9d27c9d71e5"
    );
}

#[test]
fn builtin_availability_at_protocol_eleven() {
    for fun in crate::builtin::DefaultFunction::ALL {
        let term = Term::<DeBruijn>::Builtin(fun);
        for version in [PlutusVersion::V1, PlutusVersion::V2, PlutusVersion::V3] {
            assert!(
                validate_term(&term, version, true).is_ok(),
                "{version:?} {fun:?}"
            );
        }
    }
    // Current runtime exposes exactly batches 1–6, not the future batch 7.
    assert_eq!(crate::builtin::DefaultFunction::ALL.len(), 101);
}

#[test]
fn nested_unserializable_constant_types_are_rejected() {
    for typ in [
        Type::Bls12_381G1Element,
        Type::Bls12_381G2Element,
        Type::Bls12_381MlResult,
    ] {
        for container in [
            Type::List(&typ),
            Type::Array(&typ),
            Type::Pair(&Type::Integer, &typ),
        ] {
            let constant = Constant::ProtoList(&container, &[]);
            for version in [PlutusVersion::V1, PlutusVersion::V2, PlutusVersion::V3] {
                assert!(validate_constant(&constant, version).is_err());
            }
        }
    }
    // Inspect contained values even when the declared element type differs.
    let malformed = Constant::ProtoList(
        &Type::Integer,
        &[&Constant::ProtoArray(&Type::Bls12_381G1Element, &[])],
    );
    assert!(validate_constant(&malformed, PlutusVersion::V3).is_err());
}

#[test]
fn arrays_and_values_are_serializable_at_protocol_eleven() {
    let a = Arena::new();
    let value = Constant::Value(crate::ledger_value::LedgerValue::empty(&a));
    let items = [&value];
    for constant in [
        Constant::ProtoArray(&Type::Value, &items),
        Constant::Value(crate::ledger_value::LedgerValue::empty(&a)),
    ] {
        let term = Term::<DeBruijn>::constant(&a, &constant);
        let program = Program::new(&a, Version::plutus_v3(&a), term);
        assert!(crate::flat::encode(program).is_ok());
        for version in [PlutusVersion::V1, PlutusVersion::V2, PlutusVersion::V3] {
            assert!(validate_program(program, version).is_ok());
        }
    }
}

#[test]
fn uplc_version_is_distinct_from_ledger_language() {
    let a = Arena::new();
    let term = Term::<DeBruijn>::unit(&a);
    for version in [PlutusVersion::V1, PlutusVersion::V2, PlutusVersion::V3] {
        for uplc in [Version::plutus_v1(&a), Version::plutus_v3(&a)] {
            assert!(validate_program(Program::new(&a, uplc, term), version).is_ok());
        }
        assert!(
            validate_program(Program::new(&a, Version::new(&a, 9, 0, 0), term), version).is_err()
        );
    }
}

#[test]
fn native_cases_and_constructors_require_uplc_110_for_every_ledger_language() {
    let a = Arena::new();
    let unit = Term::<DeBruijn>::unit(&a);
    let constr = Term::constr(&a, 0, &[]);
    let boolean = Term::bool(&a, true);
    let list = Term::constant(&a, a.alloc(Constant::ProtoList(&Type::Integer, &[])));
    let branches = [unit, unit];
    let constr_case = Term::case(&a, constr, &branches);
    let bool_case = Term::case(&a, boolean, &branches);
    let list_case = Term::case(&a, list, &branches);
    for term in [constr, constr_case, bool_case, list_case] {
        // Recurse through ordinary terms; do not only inspect the root.
        let nested = unit.apply(&a, term.delay(&a));
        for version in [PlutusVersion::V1, PlutusVersion::V2, PlutusVersion::V3] {
            assert!(
                validate_program(Program::new(&a, Version::plutus_v3(&a), nested), version).is_ok()
            );
            assert!(
                validate_program(Program::new(&a, Version::plutus_v1(&a), nested), version)
                    .is_err()
            );
        }
    }
}
