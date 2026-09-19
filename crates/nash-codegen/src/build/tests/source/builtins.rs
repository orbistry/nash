case!(
    bls12_381_elements_to_data_conversion,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    type alias proof = { piA : bls_g1, piB : bls_g2 }
    encode : proof -> Data
    encode p = Constr 0 [B (bls12_381_g1_compress p.piA), B (bls12_381_g2_compress p.piB)]
    pk : proof
    pk =
        { piA = (bls12_381_g1_uncompress #"b28cb29bc282be68df977b35eb9d8e98b3a0a3fc7c372990bddc50419ca86693e491755338fed4fb42231a7c081252ce")
        , piB = (bls12_381_g2_uncompress #"b9215e5bc481ba6552384c89c23d45bd650b69462868248bfbb83aee7060579404dba41c781dec7c2bec5fccec06842e0e66ad6d86c7c76c468a32c9c0080eea0219d0953b44b1c4f5605afb1e5a3193264ff730222e94f55207628235f3b423")
        }
    main : bool
    main = equalsData (encode pk) (encode pk)
"#,
    Ok("(con bool True)")
);

case!(
    bls12_381_elements_from_data_conversion,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    type alias proof = { piA : bls_g1, piB : bls_g2 }
    encode : proof -> Data
    encode p = Constr 0 [B (bls12_381_g1_compress p.piA), B (bls12_381_g2_compress p.piB)]
    pk : proof
    pk =
        { piA = (bls12_381_g1_uncompress #"b28cb29bc282be68df977b35eb9d8e98b3a0a3fc7c372990bddc50419ca86693e491755338fed4fb42231a7c081252ce")
        , piB = (bls12_381_g2_uncompress #"b9215e5bc481ba6552384c89c23d45bd650b69462868248bfbb83aee7060579404dba41c781dec7c2bec5fccec06842e0e66ad6d86c7c76c468a32c9c0080eea0219d0953b44b1c4f5605afb1e5a3193264ff730222e94f55207628235f3b423")
        }
    main : bool
    main =
        case encode pk of
            Constr 0 [B point, _] -> bls12_381_g1_equal (bls12_381_g1_uncompress point) pk.piA
            _ -> fail
"#,
    Ok("(con bool True)")
);

case!(
    cast_never,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Data exposing (ToData)
    type Option = Some Int | None
    type never = Never
    decodeNever : Data -> never
    decodeNever value =
        case value of
            Constr 1 [] -> Never
            _ -> fail
    main : unit
    main =
        case decodeNever (toData None) of
            Never -> ()
"#,
    Ok("(con unit ())")
);

traced_case!(
    hard_soft_cast,
    hard_soft_cast_silent,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Option exposing (type option(..))
    type foo = Bar | Bax
    toData : foo -> Data
    toData value =
        case value of
            Bar -> Constr 0 []
            Bax -> Constr 1 []
    decode : Data -> option foo
    decode value =
        case value of
            Constr 0 [] -> Some Bar
            Constr 1 [] -> Some Bax
            _ -> None
    hardCast value =
        case decode value of
            Some result -> result
            None -> fail
    same a b =
        case (a, b) of
            (Bar, Bar) -> True
            (Bax, Bax) -> True
            _ -> False
    main : bool
    main =
        let
            x = toData Bar
        in
        case decode x of
            Some _ -> True
            None ->
                let
                    y = hardCast x
                in
                same y y
"#,
    Ok("(con bool True)")
);

case!(
    source_identity,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    identity value = value
    main : bool
    main = if identity True then identity True else False
"#,
    Ok("(con bool True)")
);

case!(
    mk_cons_direct_invoke_1,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Eq exposing (..)
    expected : list int
    expected = [1]
    main : bool
    main = eq (Builtin.mkCons 1 []) expected
"#,
    Ok("(con bool True)")
);

case!(
    mk_cons_direct_invoke_2,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Data exposing (ToData)
    type Option = Some Int | None
    main : bool
    main = equalsData
        (toData (Builtin.listData (Builtin.mkCons (Some 42) [None])))
        (toData (Builtin.listData [Some 42, None]))
"#,
    Ok("(con bool True)")
);

case!(
    mk_cons_direct_invoke_3,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Data exposing (toData)
    main : bool
    main = equalsData
        (toData (Builtin.mapData (Builtin.mkCons (Builtin.mkPairData (I 1) (I 1)) (Builtin.mkNilPairData ()))))
        (toData (Builtin.mapData [Builtin.mkPairData (I 1) (I 1)]))
"#,
    Ok("(con bool True)")
);

case!(
    mk_nil_pair_data,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Data exposing (toData)
    main : bool
    main = equalsData
        (toData (Builtin.mapData (Builtin.mkNilPairData ())))
        (toData (Builtin.mapData (Builtin.mkNilPairData ())))
"#,
    Ok("(con bool True)")
);

case!(
    mk_nil_list_data,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Data exposing (toData)
    main : bool
    main = equalsData
        (toData (Builtin.listData (Builtin.mkNilData ())))
        (toData (Builtin.listData (Builtin.mkNilData ())))
"#,
    Ok("(con bool True)")
);

case!(
    mk_pair_data,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Data exposing (toData)
    main : bool
    main = equalsData
        (toData (Builtin.fstPair (Builtin.mkPairData (Builtin.iData 1) (Builtin.iData 2))))
        (toData (Builtin.iData 1))
"#,
    Ok("(con bool True)")
);

case!(
    pattern_bytearray,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    main : bool
    main =
        let
            bytes : bytes
            bytes = "foo"
        in
        case bytes of
            "bar" -> False
            #"666f6f" -> True
            _ -> False
"#,
    Ok("(con bool True)")
);

case!(
    dangling_trace_expect_standalone,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    main : unit
    main = trace "foo" (assert True)
"#,
    Ok("(con unit ())")
);

case!(
    dangling_trace_expect_in_sequence,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    main : unit
    main =
        let
            predicate = True
        in
        trace "foo" (assert predicate)
"#,
    Ok("(con unit ())")
);

case!(
    dangling_trace_expect_in_trace,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    main : unit
    main = trace "foo" (trace "bar" (assert True))
"#,
    Ok("(con unit ())")
);

case!(
    as_data,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Data exposing (ToData)
    type Foo = Foo Int
    type Bar = Bar Int
    main : bool
    main =
        if Builtin.nullList [toData (Foo 14), toData (Bar 42)] then False
        else True
"#,
    Ok("(con bool True)")
);

traced_case!(
    expect_non_empty_list_with_as_binding_fails_in_silent_and_verbose,
    expect_non_empty_list_with_as_binding_fails_silent,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Eq exposing (..)
    fromAssetList : list int -> list int
    fromAssetList inner =
        case inner of
            (_ :: _) as x -> x
            _ -> fail
    main : bool
    main =
        let
            x = fromAssetList []
        in
        eq x x
"#,
    Err(())
);

case!(
    data_scalar_codecs,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Data exposing (toData, fromData)
    decodedInt : Int
    decodedInt = fromData (I 42)
    decodedBytes : Bytes
    decodedBytes = fromData (B #"abcd")
    main =
        if equalsInteger (unIData decodedInt) 42 then
            if equalsByteString (unBData decodedBytes) #"abcd" then
                equalsData (toData decodedInt) (I 42)
            else False
        else False
    "#,
    Ok("(con bool True)")
);

case!(
    data_nested_list_codec,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Data exposing (toData, fromData)
    input = List [List [I 1, I 2], List []]
    decoded : List (List Int)
    decoded = fromData input
    main = equalsData (toData decoded) input
    "#,
    Ok("(con bool True)")
);

case!(
    data_map_codec,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Data exposing (toData, fromData)
    input = Map [mkPairData (B #"aa") (List [I 42])]
    decoded : Map Bytes (List Int)
    decoded = fromData input
    main = equalsData (toData decoded) input
    "#,
    Ok("(con bool True)")
);

case!(
    data_nested_list_rejects_malformed_element,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Data exposing (validate)
    decoded : List (List Int)
    decoded = validate (List [List [I 1, B #"aa"]])
    main = nullList (unListData decoded)
    "#,
    Err(())
);

case!(
    data_map_rejects_malformed_value,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Data exposing (validate)
    decoded : Map Bytes (List Int)
    decoded = validate (Map [mkPairData (B #"aa") (List [B #"bb"])])
    main = nullList (unMapData decoded)
    "#,
    Err(())
);

case!(
    coerce_preserves_runtime_value,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    claimed : bytes
    claimed = coerce original
    original : int
    original = 42
    main : int
    main = coerce claimed
    "#,
    Ok("(con integer 42)")
);

case!(
    coerce_first_class_function,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    apply f x = f x
    identity x = x
    hidden : Data
    hidden = apply coerce identity
    recovered : int -> int
    recovered = coerce hidden
    main : int
    main = recovered 42
    "#,
    Ok("(con integer 42)")
);

case!(
    coerce_evaluates_argument,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    bad : int
    bad = fail
    main : bytes
    main = coerce bad
    "#,
    Err(())
);

case!(
    from_data_leaves_nested_data_unchecked,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Data exposing (fromData)
    decoded : List (List Int)
    decoded = fromData (List [List [B #"aa"]])
    main = nullList (unListData decoded)
    "#,
    Ok("(con bool False)")
);

case!(
    from_data_leaves_outer_shape_unchecked,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Data exposing (fromData)
    decoded : Int
    decoded = fromData (B #"aa")
    main = equalsData (coerce decoded) (B #"aa")
    "#,
    Ok("(con bool True)")
);

case!(
    validate_data_rejects_wrong_scalar_shape,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Data exposing (validate)
    decoded : Int
    decoded = validate (B #"aa")
    main = unIData decoded
    "#,
    Err(())
);

case!(
    validate_data_accepts_nested_map,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Data exposing (validate, toData)
    input = Map [mkPairData (B #"aa") (List [I 42])]
    decoded : Map Bytes (List Int)
    decoded = validate input
    main = equalsData (toData decoded) input
    "#,
    Ok("(con bool True)")
);

case!(
    from_data_default_for_user_type,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Data exposing (FromData)
    type Datum = Datum Int
    decoded : Datum
    decoded = fromData (Constr 0 [I 42])
    main =
        case decoded of
            Datum number -> unIData number
    "#,
    Ok("(con integer 42)")
);

case!(
    from_data_malformed_value_fails_on_use,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Data exposing (fromData)
    decoded : Int
    decoded = fromData (B #"aa")
    main = unIData decoded
    "#,
    Err(())
);

case!(
    to_data_blanket_user_types_and_collections,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Data exposing (toData)
    type Token = Token Int
    type alias Wrapped = { item : Token }
    wrapped : Wrapped
    wrapped = { item = (Token 42) }
    identityData : Big 'a => 'a -> Data
    identityData = toData
    main =
        if equalsData (toData wrapped) (List [Constr 0 [I 42]]) then
            check
        else False
    check =
        if equalsData (identityData (Token 42)) (Constr 0 [I 42]) then
            if equalsData (toData (listData [Token 42])) (List [Constr 0 [I 42]]) then
                equalsData (toData (mapData [mkPairData (Token 1) (Token 2)]))
                    (Map [mkPairData (Constr 0 [I 1]) (Constr 0 [I 2])])
            else False
        else False
    "#,
    Ok("(con bool True)")
);

case!(
    from_data_blanket_without_validation_instances,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Data exposing (fromData, toData)
    type Token = Token Int
    type alias Wrapped = { item : Token }
    unchecked : Big 'a => Data -> 'a
    unchecked = fromData
    wrapped : Wrapped
    wrapped = unchecked (List [Constr 0 [I 42]])
    tokens : List Token
    tokens = unchecked (List [B #"aa"])
    mapping : Map Token Token
    mapping = fromData (Map [mkPairData (B #"aa") (I 7)])
    main =
        if equalsData (toData wrapped) (List [Constr 0 [I 42]]) then
            if equalsData (toData tokens) (List [B #"aa"]) then
                equalsData (toData mapping) (Map [mkPairData (B #"aa") (I 7)])
            else False
        else False
    "#,
    Ok("(con bool True)")
);

case!(
    custom_validate_rejects_wrong_shape,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Data exposing (Validate)
    type Datum = Datum Int
    impl Validate Datum where
        validate value =
            case value of
                Constr 0 [I _] -> coerce value
                _ -> fail
    decoded : Datum
    decoded = validate (Constr 0 [B #"aa"])
    main =
        case decoded of
            Datum number -> unIData number
    "#,
    Err(())
);
