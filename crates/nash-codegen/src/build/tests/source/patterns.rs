case!(
    expect_empty_list_on_filled_list,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    main : bool
    main =
        let
            x : list int
            x = [1, 2]
        in
        case x of
            [] -> True
            _ -> fail
"#,
    Err(())
);

case!(
    expect_empty_list_on_new_list,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    main : bool
    main =
        let
            x : list int
            x = []
        in
        case x of
            [] -> True
            _ -> fail
"#,
    Ok("(con bool True)")
);

case!(
    when_bool_is_true,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    main =
        case True of
            True -> True
            False -> fail
"#,
    Ok("(con bool True)")
);

case!(
    when_bool_is_true_switched_cases,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    main =
        case True of
            False -> fail
            True -> True
"#,
    Ok("(con bool True)")
);

case!(
    when_bool_is_false,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    main =
        case False of
            False -> fail
            True -> True
"#,
    Err(())
);

case!(
    when_tuple_deconstruction,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    type thing = Thing { idx : int }
    type datum = A thing | B
    type redSpend = Spend int | Buy
    main =
        case (A (Thing { idx = 42 }), Buy) of
            (A a, Spend x) ->
                if equalsInteger a.idx x then True
                else trace "a.idx == x ? False" False
            (_, _) -> True
"#,
    Ok("(con bool True)")
);

case!(
    when_tuple_empty_lists,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    main =
        let
            bucket1 : list int
            bucket1 = [1, 2, 3]
            bucket2 : list int
            bucket2 = [1, 5, 6]
            bye =
                case (bucket1, bucket2) of
                    ([], _) -> False
                    (_, []) -> False
                    (a :: _, b :: _) -> equalsInteger a b
        in
        bye
"#,
    Ok("(con bool True)")
);

case!(
    pass_constr_as_function,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Eq exposing (..)
    type make = Make { a : int, b : subMake }
    type subMake = SubMake { c : int }
    impl Eq subMake where
        eq (SubMake a) (SubMake b) = equalsInteger a b
    impl Eq make where
        eq (Make a b) (Make c d) = if equalsInteger a c then eq b d else False
    hi sm toMake = toMake 3 sm
    main = eq (Make 3 (SubMake 1)) (hi (SubMake 1) Make)
"#,
    Ok("(con bool True)")
);

case!(
    list_fields_unwrap,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Lift exposing (lower)
    type Fields = Fields { a : Bytes, b : Int }
    dataFields : unit -> list Fields
    dataFields _ = [Fields { a = #"", b = 14 }, Fields { a = #"AA", b = 0 }]
    main =
        case dataFields () of
            (Fields { b }) :: _ -> lessThanInteger 0 (lower b)
            _ -> False
"#,
    Ok("(con bool True)")
);

case!(
    expect_head_discard_tail,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    main =
        let
            a : list int
            a = [1, 2, 3]
        in
        case a of
            h :: _ -> equalsInteger h h
            _ -> fail
"#,
    Ok("(con bool True)")
);

case!(
    expect_head_no_tail,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    main =
        let
            a : list int
            a = [1, 2, 3]
        in
        case a of
            [h] -> equalsInteger h h
            _ -> fail
"#,
    Err(())
);

case!(
    expect_head3_no_tail,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    main =
        let
            a : list int
            a = [1, 2, 3]
        in
        case a of
            [h, i, j] ->
                if equalsInteger h h then
                    if equalsInteger i i then equalsInteger j j else False
                else False
            _ -> fail
"#,
    Ok("(con bool True)")
);

case!(
    test_init_3,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Eq exposing (..)
    type option 'a = None | Some 'a
    impl Eq 'a => Eq (option 'a) where
        eq a b =
            case (a, b) of
                (None, None) -> True
                (Some x, Some y) -> eq x y
                _ -> False
    init self =
        case self of
            [] -> None
            _ -> Some (doInit self)
    doInit self =
        case self of
            [] -> trace "unreachable" fail
            [_] -> []
            x :: xs -> mkCons x (doInit xs)
    values : list int
    values = [1, 2, 3, 4]
    main = eq (init values) (Some [1, 2, 3])
"#,
    Ok("(con bool True)")
);

case!(
    list_clause_with_assign,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Eq exposing (..)
    type Option = Some Int | None
    doInit self =
        case self of
            [] -> trace "unreachable" fail
            [_] as a -> a
            [(Some _) as n, x] -> [n]
            [a, x] -> [x]
            a :: b :: c -> c
    main = eq (doInit [Some 1, None]) [Some 1]
"#,
    Ok("(con bool True)")
);

case!(
    expect_none,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    type option = Some int | None
    main =
        let
            x = None
        in
        case x of
            None -> True
            _ -> fail
"#,
    Ok("(con bool True)")
);

case!(
    head_list_on_map,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Data exposing (toData)
    main =
        let
            x = [mkPairData (I 1) (B #""), mkPairData (I 2) (B #"aa")]
            first = headList x
        in
        equalsData (toData (mapData [first])) (toData (mapData [mkPairData (I 1) (B #"")]))
"#,
    Ok("(con bool True)")
);

case!(
    tuple_2_match,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    type curveInt = ECI { ec : (int, int) } | Infinity
    equivalence ec1 ec2 =
        let
            input = (ec1, ec2)
        in
        case input of
            (ECI (x1, y1), ECI (x2, y2)) ->
                if equalsInteger (subtractInteger x2 x1) 0 then
                    equalsInteger (subtractInteger y2 y1) 0
                else False
            (Infinity, Infinity) -> True
            (Infinity, ECI _) -> False
            (ECI _, Infinity) -> False
    main = equivalence Infinity Infinity
"#,
    Ok("(con bool True)")
);

case!(
    foldl_type_mismatch,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Eq exposing (..)
    import Lift exposing (lower, lift)
    type Option 'a = None | Some 'a
    type Address = Address { paymentCredential : Bytes, stakeCredential : Option Bytes }
    type Output = Output { address : Address, value : List Int, datum : Option Int, referenceScript : Option Int }
    foldl self with zero =
        case self of
            [] -> zero
            x :: xs -> foldl xs with (with x zero)
    main =
        let
            addr1 = Address { paymentCredential = #"61646666", stakeCredential = None }
            out = Output { address = addr1, value = (lift empty), datum = None, referenceScript = None }
            empty : list int
            empty = []
            outputs : list Output
            outputs = [out, out, out]
            cry =
                foldl outputs
                    (\o mbB ->
                        case mbB of
                            None -> if eq o.address addr1 then Some o else None
                            otherwise -> otherwise
                    )
                    None
        in
        eq cry cry
"#,
    Ok("(con bool True)")
);

case!(
    record_update_output_2_vals,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Eq exposing (..)
    import Lift exposing (lift)
    import Data exposing (toData)
    type MyDatum = MyDatum { a : Int }
    encodeMyDatum (MyDatum n) = Constr 0 [toData n]
    type Address = Address { thing : Bytes }
    type Datum = NoDatum | InlineDatum Data
    type Option 'a = None | Some 'a
    type alias Output =
        { address : Address
        , value : Map Bytes (Map Bytes Int)
        , datum : Datum
        , scriptRef : Option Bytes
        }
    main =
        let
            emptyPairs : list (pair Bytes (Map Bytes Int))
            emptyPairs = []
            prevOutput : Output
            prevOutput =
                { address = (Address { thing = #"7363726970745f686173685f30" })
                , value = (lift emptyPairs)
                , datum = (InlineDatum (encodeMyDatum (MyDatum 3)))
                , scriptRef = None
                }
            nextOutput = { prevOutput | value = (lift emptyPairs), datum = prevOutput.datum }
        in
        eq prevOutput nextOutput
"#,
    Ok("(con bool True)")
);

case!(
    record_update_output_1_val,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Eq exposing (..)
    import Lift exposing (lift)
    import Data exposing (toData)
    type MyDatum = MyDatum { a : Int }
    encodeMyDatum (MyDatum n) = Constr 0 [toData n]
    type Address = Address { thing : Bytes }
    type Datum = NoDatum | InlineDatum Data
    type Option 'a = None | Some 'a
    type alias Output =
        { address : Address
        , value : Map Bytes (Map Bytes Int)
        , datum : Datum
        , scriptRef : Option Bytes
        }
    main =
        let
            emptyPairs : list (pair Bytes (Map Bytes Int))
            emptyPairs = []
            prevOutput : Output
            prevOutput =
                { address = (Address { thing = #"7363726970745f686173685f30" })
                , value = (lift emptyPairs)
                , datum = (InlineDatum (encodeMyDatum (MyDatum 3)))
                , scriptRef = None
                }
            nextOutput = { prevOutput | datum = prevOutput.datum }
        in
        eq prevOutput nextOutput
"#,
    Ok("(con bool True)")
);

case!(
    record_update_output_first_last_val,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Eq exposing (..)
    import Lift exposing (lift)
    import Data exposing (toData)
    type MyDatum = MyDatum { a : Int }
    encodeMyDatum (MyDatum n) = Constr 0 [toData n]
    type Address = Address { thing : Bytes }
    type Datum = NoDatum | InlineDatum Data
    type Option 'a = None | Some 'a
    type alias Output =
        { address : Address
        , value : Map Bytes (Map Bytes Int)
        , datum : Datum
        , scriptRef : Option Bytes
        }
    main =
        let
            emptyPairs : list (pair Bytes (Map Bytes Int))
            emptyPairs = []
            prevOutput : Output
            prevOutput =
                { address = (Address { thing = #"7363726970745f686173685f30" })
                , value = (lift emptyPairs)
                , datum = (InlineDatum (encodeMyDatum (MyDatum 3)))
                , scriptRef = None
                }
            nextOutput = { prevOutput | scriptRef = None, address = (Address { thing = #"7363726970745f686173685f30" }) }
        in
        eq prevOutput nextOutput
"#,
    Ok("(con bool True)")
);

case!(
    expect_head3_cast_data_no_tail,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Data exposing (validate)
    import Lift exposing (lower)
    main =
        let
            a : Data
            a = List [I 1, I 2, I 3]
            checked : List Int
            checked = validate a
            values : list int
            values = lower checked
        in
        case values of
            [h, i, j] -> if equalsInteger h h then
                    if equalsInteger i i then equalsInteger j j else False
                else False
            _ -> fail
"#,
    Ok("(con bool True)")
);

case!(
    expect_head_cast_data_no_tail,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Data exposing (validate)
    import Lift exposing (lower)
    main =
        let
            a : Data
            a = List [I 1, I 2, I 3]
            checked : List Int
            checked = validate a
            values : list int
            values = lower checked
        in
        case values of
            [h] -> equalsInteger h h
            _ -> fail
"#,
    Err(())
);

case!(
    expect_head_cast_data_with_tail,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Data exposing (validate)
    import Lift exposing (lower)
    main =
        let
            a : Data
            a = List [I 1, I 2, I 3]
            checked : List Int
            checked = validate a
            values : list int
            values = lower checked
        in
        case values of
            h :: j :: _ -> if equalsInteger h h then equalsInteger j j else False
            _ -> fail
"#,
    Ok("(con bool True)")
);

validator_case!(
    always_true_validator,
    r#"
    validator module Main exposing (main)
    import Builtin exposing (..)
    main : Data -> Bytes -> Data -> unit
    main redeemer policyId transaction = assert True
"#
);

validator_case!(
    generic_validator_type_test,
    r#"
    validator module Main exposing (main)
    import Builtin exposing (..)
    import Eq exposing (..)
    type Void = Void
    type A 'x = NoA | SomeA Void 'x
    type B = B { something : Void }
    type Option 'a = None | Some 'a
    main : Option Data -> A B -> Data -> Data -> unit
    main datum r outputRef transaction =
        assert
            (case r of
                NoA -> False
                SomeA _ (B something) -> eq something Void
            )
"#
);

case!(
    opaque_value_in_test,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Eq exposing (..)
    import Lift exposing (lift, lower)
    type Pair 'v = Pair Bytes 'v
    type Dict 'v = Dict { inner : List (Pair 'v) }
    type Value = Value { inner : Dict (Dict Int) }
    type Dat = Dat { c : Int, a : Value }
    dat : Dat
    dat =
        let
            innerEntries : list (Pair Int)
            innerEntries = [Pair #"aa" 4]
            innerDict : Dict Int
            innerDict = Dict { inner = (lift innerEntries) }
            v = Value { inner = (Dict { inner = (lift [Pair #"" innerDict]) }) }
        in
        Dat { c = 0, a = v }
    main =
        let
            val = dat.a
            entries : list (Pair (Dict Int))
            entries = lower val.inner.inner
            finalEntries : list (Pair Int)
            finalEntries = [Pair #"AA" 4]
            finalAmount : Dict Int
            finalAmount = Dict { inner = (lift finalEntries) }
        in
        case entries of
            [Pair _ amount] -> eq finalAmount amount
            _ -> fail
"#,
    Ok("(con bool True)")
);
