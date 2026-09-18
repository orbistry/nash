use super::*;
use crate::{arena::Arena, binder::DeBruijn, program::Version};

#[test]
fn independent_aiken_v3_hash_and_cross_language_vectors() {
    // Aiken hello_world/plutus.json, compiler v1.1.0+9407b67, V3 blueprint.
    let cbor = hex::decode("59011d0101003232323232323225333002323232323253330073370e900118041baa0011323232533300a3370e900018059baa005132533300e0011613253333330120011616161613253330103012003132533300e3370e900018079baa005132533300f002100114a06644646600200200644a66602a00229404c94ccc04ccdc79bae301700200414a2266006006002602e0026eb0c048c04cc04cc04cc04cc04cc04cc04cc04cc040dd50059bae301230103754602460206ea801458cdc79bae3011300f375401091010d48656c6c6f2c20576f726c64210016375c002601e00260186ea801458c034c038008c030004c024dd50008b1805180580118048009804801180380098021baa00114984d9595cd2ab9d5573caae7d5d0aba25749").unwrap();
    assert_eq!(hex::encode(script_hash(PlutusVersion::V1, &cbor)), "9bd23c07636d2e8722eb9851215b02717edd36bdab66189591796848");
    assert_eq!(hex::encode(script_hash(PlutusVersion::V2, &cbor)), "86fed0146e9e88aafc14e3832792ead47c3c6f9b33e2c52c3b7d7f52");
    assert_eq!(hex::encode(script_hash(PlutusVersion::V3, &cbor)), "167f56e1b5de377df88962340a0461158e68d4b6caaea9d27c9d71e5");
}

fn check(term: &Term<'_, DeBruijn>, version: PlutusVersion) -> Result<(), TargetError> { validate_term(term, version) }

#[test]
fn complete_builtin_matrix_at_protocol_ten() {
    use PlutusVersion::*;
    let arena = Arena::new();
    for fun in DefaultFunction::ALL {
        let term = Term::<DeBruijn>::Builtin(fun);
        for version in [V1,V2,V3] {
            let expected = match version { V1 => *fun as u8 <= 50, V2 => *fun as u8 <= 53 || matches!(fun,DefaultFunction::IntegerToByteString | DefaultFunction::ByteStringToInteger), V3 => *fun as u8 <= 86 };
            assert_eq!(check(term.delay(&arena), version).is_ok(), expected, "{version:?} {fun:?}");
        }
    }
}
#[test]
fn nested_terms_and_constant_types_are_checked() {
    let arena = Arena::new();
    let unit = Term::<DeBruijn>::unit(&arena);
    let constr = arena.alloc(Term::Constr { tag:0, fields: &[unit] });
    let case = arena.alloc(Term::Case { constr, branches: &[unit] });
    for term in [constr,case] {
        let nested = unit.apply(&arena, term.delay(&arena));
        assert!(check(nested, PlutusVersion::V1).is_err());
        assert!(check(nested, PlutusVersion::V2).is_err());
        assert!(check(nested, PlutusVersion::V3).is_ok());
    }
    for typ in [Type::Array(&Type::Integer),Type::Value,Type::Bls12_381G1Element,Type::Bls12_381G2Element,Type::Bls12_381MlResult] {
        let constant = Constant::ProtoList(&Type::List(&typ), &[]);
        for version in [PlutusVersion::V1,PlutusVersion::V2,PlutusVersion::V3] {
            assert!(validate_constant(&constant,version).is_err());
        }
    }
    // Validate values as well as the declared element type.
    let malformed = Constant::ProtoList(&Type::Integer, &[&Constant::ProtoArray(&Type::Integer,&[])]);
    assert!(validate_constant(&malformed,PlutusVersion::V3).is_err());
}
#[test]
fn uplc_version_is_distinct_from_ledger_language() {
    let a = Arena::new();
    let term = Term::<DeBruijn>::unit(&a);
    for version in [PlutusVersion::V1,PlutusVersion::V2,PlutusVersion::V3] {
        assert!(validate_program(Program::new(&a,Version::plutus_v1(&a),term),version).is_ok());
    }
    let v3 = Program::new(&a,Version::plutus_v3(&a),term);
    assert!(validate_program(v3,PlutusVersion::V1).is_err());
    assert!(validate_program(v3,PlutusVersion::V2).is_err());
    assert!(validate_program(v3,PlutusVersion::V3).is_ok());
    assert!(validate_program(Program::new(&a,Version::new(&a,9,0,0),term),PlutusVersion::V3).is_err());
}
