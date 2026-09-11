use nash_plutus::{
    arena::Arena,
    binder::{DeBruijn, Name},
    constant::Constant,
    debruijn, flat, pretty,
    program::{Program, Version},
    syn,
    term::Term,
};

#[test]
fn scoped_debruijn_roundtrip() {
    let arena = Arena::new();
    for source in [
        "(lam x (lam y [x y]))",
        "(lam x [(lam x x) x])",
        "(case (constr 0 (con integer 7)) (lam x x))",
        "(force (delay (error)))",
    ] {
        let parsed = syn::parse_term(&arena, source).into_result().unwrap();
        let printed = pretty::term(parsed);
        let reparsed = syn::parse_term(&arena, &printed).into_result().unwrap();
        assert_eq!(parsed, reparsed, "{printed}");
    }
}

#[test]
fn constants_roundtrip() {
    let arena = Arena::new();
    for source in [
        "(con integer -123)",
        "(con integer 340282366920938463463374607431768211456)",
        "(con bytestring #00ff)",
        "(con string \"a\\n\\t\\\"\\\\λ\")",
        "(con bool True)",
        "(con unit ())",
        "(con (list integer) [1, -2])",
        "(con (array (pair integer bool)) [(1, True)])",
        "(con data (Constr 2 [I 9, B #ff, List [], Map [(I 1, I 2)]]))",
        "(con value [(#aa, [(#bb, 2)])])",
    ] {
        let parsed = syn::parse_constant(&arena, source).into_result().unwrap();
        let printed = pretty::constant(parsed);
        let reparsed = syn::parse_constant(&arena, &printed).into_result().unwrap();
        assert_eq!(parsed, reparsed, "{printed}");
    }
    let value = Constant::string(&arena, "\0\u{1b}a\u{7f}9");
    let printed = pretty::constant(value);
    assert_eq!(
        value,
        syn::parse_constant(&arena, &printed).into_result().unwrap()
    );
}

#[test]
fn names_convert_with_shadowing_and_capture() {
    let arena = Arena::new();
    let outer = Name::new(&arena, "x", 1);
    let inner = Name::new(&arena, "x", 2);
    let named = Term::var(&arena, outer)
        .apply(&arena, Term::var(&arena, inner))
        .lambda(&arena, inner)
        .lambda(&arena, outer);
    let converted = debruijn::to_debruijn(&arena, named).unwrap();
    let printed = pretty::term(named);
    let parsed = syn::parse_term(&arena, &printed).into_result().unwrap();
    assert_eq!(converted, parsed);
    let p = Program::new(&arena, Version::plutus_v3(&arena), converted);
    let printed = pretty::program(p);
    let q = syn::parse_program(&arena, &printed).into_result().unwrap();
    assert_eq!(flat::encode(p).unwrap(), flat::encode(q).unwrap());
}

#[test]
fn free_variable_is_rejected() {
    let arena = Arena::new();
    let free = Name::new(&arena, "missing", 42);
    let err = debruijn::to_debruijn(&arena, Term::var(&arena, free)).unwrap_err();
    assert_eq!(err.0.unique(), 42);
}

#[test]
fn all_builtins_roundtrip() {
    let arena = Arena::new();
    use nash_plutus::builtin::DefaultFunction::*;
    for f in [
        AddInteger,
        SubtractInteger,
        MultiplyInteger,
        DivideInteger,
        QuotientInteger,
        RemainderInteger,
        ModInteger,
        EqualsInteger,
        LessThanInteger,
        LessThanEqualsInteger,
        AppendByteString,
        ConsByteString,
        SliceByteString,
        LengthOfByteString,
        IndexByteString,
        EqualsByteString,
        LessThanByteString,
        LessThanEqualsByteString,
        Sha2_256,
        Sha3_256,
        Blake2b_256,
        Keccak_256,
        Blake2b_224,
        VerifyEd25519Signature,
        VerifyEcdsaSecp256k1Signature,
        VerifySchnorrSecp256k1Signature,
        AppendString,
        EqualsString,
        EncodeUtf8,
        DecodeUtf8,
        IfThenElse,
        ChooseUnit,
        Trace,
        FstPair,
        SndPair,
        ChooseList,
        MkCons,
        HeadList,
        TailList,
        NullList,
        ChooseData,
        ConstrData,
        MapData,
        ListData,
        IData,
        BData,
        UnConstrData,
        UnMapData,
        UnListData,
        UnIData,
        UnBData,
        EqualsData,
        SerialiseData,
        MkPairData,
        MkNilData,
        MkNilPairData,
        Bls12_381_G1_Add,
        Bls12_381_G1_Neg,
        Bls12_381_G1_ScalarMul,
        Bls12_381_G1_Equal,
        Bls12_381_G1_Compress,
        Bls12_381_G1_Uncompress,
        Bls12_381_G1_HashToGroup,
        Bls12_381_G2_Add,
        Bls12_381_G2_Neg,
        Bls12_381_G2_ScalarMul,
        Bls12_381_G2_Equal,
        Bls12_381_G2_Compress,
        Bls12_381_G2_Uncompress,
        Bls12_381_G2_HashToGroup,
        Bls12_381_MillerLoop,
        Bls12_381_MulMlResult,
        Bls12_381_FinalVerify,
        IntegerToByteString,
        ByteStringToInteger,
        AndByteString,
        OrByteString,
        XorByteString,
        ComplementByteString,
        ReadBit,
        WriteBits,
        ReplicateByte,
        ShiftByteString,
        RotateByteString,
        CountSetBits,
        FindFirstSetBit,
        Ripemd_160,
        ExpModInteger,
        DropList,
        LengthOfArray,
        ListToArray,
        IndexArray,
        Bls12_381_G1_MultiScalarMul,
        Bls12_381_G2_MultiScalarMul,
        InsertCoin,
        LookupCoin,
        UnionValue,
        ValueContains,
        ValueData,
        UnValueData,
        ScaleValue,
    ] {
        let term = Term::<DeBruijn>::builtin(&arena, arena.alloc(f));
        let printed = pretty::term(term);
        assert_eq!(
            term,
            syn::parse_term(&arena, &printed).into_result().unwrap()
        );
    }
}

#[test]
fn bls_literals_and_unsupported_miller_results() {
    let arena = Arena::new();
    let g1 = Constant::g1(&arena, arena.alloc(blst::blst_p1::default()));
    let g2 = Constant::g2(&arena, arena.alloc(blst::blst_p2::default()));
    for value in [g1, g2] {
        let printed = pretty::try_constant(value).unwrap();
        let parsed = syn::parse_constant(&arena, &printed).into_result().unwrap();
        assert_eq!(pretty::constant(parsed), printed);
    }
    let ml = Constant::ml_result(&arena, arena.alloc(blst::blst_fp12::default()));
    assert!(pretty::try_constant(ml).is_err());
    let list = Constant::proto_list(&arena, nash_plutus::typ::Type::ml_result(&arena), &[]);
    assert!(pretty::try_constant(list).is_err());
}

#[test]
fn conversion_covers_case_fields_delays_and_unique_identity() {
    let arena = Arena::new();
    let x = Name::new(&arena, "not.a/parser name", 7);
    // Text is cosmetic: identity is the unique, even when the spelling differs.
    let occurrence = Name::new(&arena, "different", 7);
    let fields = arena.alloc_slice_copy(&[Term::var(&arena, occurrence)]);
    let constr = Term::constr(&arena, 0, fields);
    let branch = Term::var(&arena, x).lambda(&arena, x);
    let branches = arena.alloc_slice_copy(&[branch]);
    let named = Term::case(&arena, constr, branches)
        .delay(&arena)
        .force(&arena)
        .lambda(&arena, x);
    let converted = debruijn::to_debruijn(&arena, named).unwrap();
    let printed = pretty::term(named);
    assert_eq!(
        converted,
        syn::parse_term(&arena, &printed).into_result().unwrap()
    );
    let value = Program::new(&arena, Version::plutus_v3(&arena), converted)
        .apply(&arena, Term::integer_from(&arena, 9))
        .eval(&arena)
        .term
        .unwrap();
    assert_eq!(value, Term::integer_from(&arena, 9));
}

#[test]
fn named_debruijn_uses_indices_not_cosmetic_labels() {
    use nash_plutus::binder::NamedDeBruijn;
    let arena = Arena::new();
    let v = NamedDeBruijn::new(&arena, "wrong", 1);
    let binder = NamedDeBruijn::new(&arena, "x", 0);
    let term = Term::var(&arena, v).lambda(&arena, binder);
    let printed = pretty::term(term);
    assert_eq!(printed, "(lam i0 i0)");
    let parsed = syn::parse_term(&arena, &printed).into_result().unwrap();
    assert_eq!(
        parsed,
        Term::var(&arena, DeBruijn::new(&arena, 1)).lambda(&arena, DeBruijn::zero(&arena))
    );
}

#[test]
fn pretty_short_and_multiline_layout() {
    let arena = Arena::new();
    let source = "(lam x (lam y [x y]))";
    let term = syn::parse_term(&arena, source).into_result().unwrap();
    insta::assert_snapshot!(pretty::term(term), @"(lam i0 (lam i1 [i0 i1]))");
    let source =
        "(lam x [(builtin addInteger) (con integer 1234567890123456789012345678901234567890) x])";
    let term = syn::parse_term(&arena, source).into_result().unwrap();
    let printed = pretty::term(term);
    assert!(printed.contains("\n  "));
    assert_eq!(
        term,
        syn::parse_term(&arena, &printed).into_result().unwrap()
    );
}

#[test]
fn lambda_scope_does_not_escape_into_sibling() {
    let arena = Arena::new();
    let name = Name::new(&arena, "x", 1);
    let var = Term::var(&arena, name);
    let term = var.lambda(&arena, name).apply(&arena, var);
    assert!(debruijn::to_debruijn(&arena, term).is_err());
}
