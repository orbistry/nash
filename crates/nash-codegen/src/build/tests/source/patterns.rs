case!(
    expect_empty_list_on_filled_list,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
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
    import Primitive exposing (..)
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
    import Primitive exposing (..)
    import Builtin exposing (..)
    main : bool
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
    import Primitive exposing (..)
    import Builtin exposing (..)
    main : bool
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
    import Primitive exposing (..)
    import Builtin exposing (..)
    main : bool
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
    import Primitive exposing (..)
    import Builtin exposing (..)
    type thing = Thing { idx : int }
    type datum = A thing | B
    type redSpend = Spend int | Buy
    main : bool
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
    import Primitive exposing (..)
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
    import Primitive exposing (..)
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
    import Primitive exposing (..)
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
    import Primitive exposing (..)
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
    import Primitive exposing (..)
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
    import Primitive exposing (..)
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
    import Primitive exposing (..)
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
    import Primitive exposing (..)
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
    import Primitive exposing (..)
    import Builtin exposing (..)
    type option = Some int | None
    main : bool
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
    import Primitive exposing (..)
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
    import Primitive exposing (..)
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
    import Primitive exposing (..)
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
            empty : list Int
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
    import Primitive exposing (..)
    import Builtin exposing (..)
    import Eq exposing (..)
    import Lift exposing (lift)
    import Data exposing (toData)
    type MyDatum = MyDatum { a : Int }
    encodeMyDatum (MyDatum n) = Builtin.constrData 0 [toData n]
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
    import Primitive exposing (..)
    import Builtin exposing (..)
    import Eq exposing (..)
    import Lift exposing (lift)
    import Data exposing (toData)
    type MyDatum = MyDatum { a : Int }
    encodeMyDatum (MyDatum n) = Builtin.constrData 0 [toData n]
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
    import Primitive exposing (..)
    import Builtin exposing (..)
    import Eq exposing (..)
    import Lift exposing (lift)
    import Data exposing (toData)
    type MyDatum = MyDatum { a : Int }
    encodeMyDatum (MyDatum n) = Builtin.constrData 0 [toData n]
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
    import Primitive exposing (..)
    import Builtin exposing (..)
    import Data exposing (validate)
    import Lift exposing (lower)
    import Functor
    main =
        let
            a : Data
            a = List [I 1, I 2, I 3]
            checked : List Int
            checked = validate a
            values : list int
            values = Functor.map Builtin.unIData (Builtin.unListData checked)
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
    import Primitive exposing (..)
    import Builtin exposing (..)
    import Data exposing (validate)
    import Lift exposing (lower)
    import Functor
    main =
        let
            a : Data
            a = List [I 1, I 2, I 3]
            checked : List Int
            checked = validate a
            values : list int
            values = Functor.map Builtin.unIData (Builtin.unListData checked)
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
    import Primitive exposing (..)
    import Builtin exposing (..)
    import Data exposing (validate)
    import Lift exposing (lower)
    import Functor
    main =
        let
            a : Data
            a = List [I 1, I 2, I 3]
            checked : List Int
            checked = validate a
            values : list int
            values = Functor.map Builtin.unIData (Builtin.unListData checked)
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
    import Primitive exposing (..)
    import Builtin exposing (..)
    main : Data -> Bytes -> Data -> unit
    main redeemer policyId transaction = assert True
"#
);

validator_case!(
    generic_validator_type_test,
    r#"
    validator module Main exposing (main)
    import Primitive exposing (..)
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
    import Primitive exposing (..)
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

case!(
    builtin_pair_let,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    main : int
    main =
        let
            pair(tag, fields) = Builtin.unConstrData (Builtin.constrData 7 [I 42])
        in
        case fields of
            [I number] -> Builtin.addInteger tag number
            _ -> fail
    "#,
    Ok("(con integer 49)")
);

case!(
    builtin_pair_arguments_and_lambda,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    first pair(a, _) = a
    main : int
    main =
        let
            value = Builtin.unConstrData (Builtin.constrData 7 [])
            pair = first value
        in
        (\pair(a, _) -> Builtin.addInteger pair a) value
    "#,
    Ok("(con integer 14)")
);

case!(
    builtin_pair_nested_patterns,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    inspect value =
        case value of
            pair(I a, B bytes) -> Builtin.addInteger a (Builtin.lengthOfByteString bytes)
            _ -> 0
    main : int
    main = Builtin.addInteger (inspect (Builtin.mkPairData (I 40) (B #"aabb"))) (inspect (Builtin.mkPairData (B #"") (I 2)))
    "#,
    Ok("(con integer 42)")
);

case!(
    builtin_pair_do_binding,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Option exposing (type option(..))
    import Monad exposing (Monad)
    result =
        do
            pair(tag, _) <- Some (Builtin.unConstrData (Builtin.constrData 42 []))
            Some tag
    main : int
    main =
        case result of
            Some tag -> tag
            None -> fail
    "#,
    Ok("(con integer 42)")
);

case!(
    data_constr_pair_payload,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    rewrap value =
        case value of
            Constr payload -> Constr payload
            _ -> fail
    main : int
    main =
        case rewrap (Builtin.constrData 7 [I 42]) of
            Constr pair(index, [I number]) -> Builtin.addInteger index number
            _ -> fail
    "#,
    Ok("(con integer 49)")
);

case!(
    data_constr_pair_base_helpers,
    r#"
    module Main exposing (..)
    import Data
    import Primitive exposing (..)
    import Option exposing (type option(..))
    main : int
    main =
        let index = Data.tag (Builtin.constrData 7 [I 42]) in
        case Data.fields (Builtin.constrData 7 [I 42]) of
            [I number] -> Builtin.addInteger index number
            _ -> fail
    "#,
    Ok("(con integer 49)")
);

case!(
    big_constructor_field_skip,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    type Datum = Datum { a : Int, b : Int, c : Int, d : Int }
    main : int
    main = Builtin.unIData ((Datum { a = 10, b = 20, c = 30, d = 40 }).d)
    "#,
    Ok("(con integer 40)")
);

case!(
    big_field_skip_rejects_short_payload,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    type alias Record = { a : Int, b : Int, c : Int }
    malformed : Record
    malformed = coerce (List [I 1])
    main = malformed.c
    "#,
    Err(())
);

case!(
    mutual_dispatch_mixed_arities_and_partial_application,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    run captured =
        let
            first n =
                if equalsInteger n 0 then captured
                else
                    let next = second (subtractInteger n 1) in
                    next captured
            second n value =
                if equalsInteger n 0 then value
                else (\f -> f (subtractInteger n 1)) first
        in
        first 7
    main : int
    main = run 42
    "#,
    Ok("(con integer 42)")
);

case!(
    mutual_dispatch_selected_body_is_lazy,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    first n = if equalsInteger n 0 then 42 else second (subtractInteger n 1)
    second n = if equalsInteger n 0 then fail else first (subtractInteger n 1)
    main : int
    main = first 2
    "#,
    Ok("(con integer 42)")
);

case!(
    mixed_boolean_representations_short_circuit,
    r#"
    module Main exposing (..)
    import Primitive exposing (type bool(..))
    import Bool
    import Lift exposing (Lift)
    both : (Lift bool 'a, Lift bool 'b) => 'a -> 'b -> bool
    both a b = Bool.and a b
    bad : unit -> Bool.Bool
    bad _ = fail
    main : bool
    main =
        if Bool.or Bool.True (bad ()) then
            both Bool.True True
        else False
"#,
    Ok("(con bool True)")
);

case!(
    partial_big_boolean_application_is_strict,
    r#"
    module Main exposing (..)
    import Primitive exposing (type bool(..))
    import Bool
    bad : unit -> Bool.Bool
    bad _ = fail
    main : bool
    main =
        let
            partial : Bool.Bool -> bool
            partial = Bool.or Bool.True
        in
        partial (bad ())
"#,
    Err(())
);

case!(
    boolean_alias_uses_selected_lower,
    r#"
    module Main exposing (..)
    import Primitive exposing (type bool(..), coerce)
    import Bool
    import Lift exposing (Lift)
    type alias flag = bool
    impl Lift bool flag where
        lift value = coerce value
        lower value = if coerce value then False else True
    off : flag
    off = coerce True
    both : (Lift bool 'a, Lift bool 'b) => 'a -> 'b -> bool
    both a b = Bool.and a b
    main : bool
    main = if Bool.and off True then True else both off True
"#,
    Ok("(con bool False)")
);

case!(
    unselected_boolean_conversion_initializer_is_lazy,
    r#"
    module Main exposing (..)
    import Primitive exposing (type bool(..), coerce)
    import Bool
    import Lift exposing (Lift)
    type alias flag = bool
    impl Lift bool flag where
        lift item = coerce item
        lower = (fail "unselected conversion")
    value : flag
    value = coerce False
    main : bool
    main = Bool.or True value
"#,
    Ok("(con bool True)")
);

case!(
    big_boolean_lift_and_lower,
    r#"
    module Main exposing (..)
    import Bool
    import Lift exposing (Lift)
    make : bool -> Bool.Bool
    make = lift
    main : (bool, bool)
    main = (lower (make Primitive.False), lower (make Primitive.True))
    "#,
    Ok("(constr 0 (con bool False) (con bool True))")
);

case!(
    big_unit_pattern_keeps_scrutinee_effects,
    r#"
    module Main exposing (..)
    import Unit
    consume : Unit.Unit -> unit
    consume value = case value of
        Unit.Unit -> ()
    main : unit
    main = consume (trace "unit input" Unit.Unit)
    "#,
    Ok("(con unit ())")
);

case!(
    big_single_constructor_nested_fields,
    r#"
    module Main exposing (..)
    type Box = Box Int Int
    type Choice = First Box | Second Int | Third
    select : Choice -> int
    select value = case value of
        First (Box first second) -> Builtin.addInteger (Builtin.unIData first) (Builtin.unIData second)
        Second number -> Builtin.unIData number
        Third -> 0
    main : (int, int, int)
    main = (select (First (Box 20 22)), select (Second 7), select Third)
    "#,
    Ok("(constr 0 (con integer 42) (con integer 7) (con integer 0))")
);

case!(
    big_unit_pattern_keeps_scrutinee_failure,
    r#"
    module Main exposing (..)
    import Unit
    consume : Unit.Unit -> unit
    consume value = case value of
        Unit.Unit -> ()
    main : unit
    main = consume (fail "unit input")
    "#,
    Err(())
);

case!(
    little_single_constructor_keeps_scrutinee_effects,
    r#"
    module Main exposing (..)
    type token = Token
    consume : token -> int
    consume value = case value of
        Token -> 42
    main : int
    main = consume (trace "token input" Token)
    "#,
    Ok("(con integer 42)")
);

case!(
    general_data_keeps_variant_dispatch,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    classify : Data -> int
    classify value = case value of
        Constr _ -> 0
        Map _ -> 1
        List _ -> 2
        I _ -> 3
        B _ -> 4
    main : (int, int, int, int, int)
    main = (classify (Builtin.constrData 0 []), classify (Map []), classify (List []), classify (I 42), classify (B #"aa"))
    "#,
    Ok(
        "(constr 0\n  (con integer 0)\n  (con integer 1)\n  (con integer 2)\n  (con integer 3)\n  (con integer 4))"
    )
);

case!(
    big_constructor_ignored_payload,
    r#"
    module Main exposing (..)
    type Choice = Some Int | None
    tagOnly : Choice -> int
    tagOnly value = case value of
        Some _ -> 1
        None -> 0
    main : int
    main = tagOnly (Some 42)
    "#,
    Ok("(con integer 1)")
);

case!(
    big_single_constructor_ignored_fields_keep_effects,
    r#"
    module Main exposing (..)
    type Box = Box Int Int
    ignore : Box -> int
    ignore value = case value of
        Box _ _ -> 7
    main : int
    main = ignore (trace "box input" (Box 1 2))
    "#,
    Ok("(con integer 7)")
);

case!(
    big_record_pattern_skips_ignored_fields,
    r#"
    module Main exposing (..)
    type alias Record = { a : Int, b : Int, c : Int, d : Int, e : Int }
    read : Record -> int
    read value = case value of
        { b, d, e } -> Builtin.addInteger (Builtin.unIData b) (Builtin.addInteger (Builtin.unIData d) (Builtin.unIData e))
    main : int
    main = read { a = 1, b = 2, c = 3, d = 4, e = 5 }
    "#,
    Ok("(con integer 11)")
);

case!(
    labeled_big_update_preserves_layout_and_order,
    r#"
    module Main exposing (..)
    type Record = Record { a : Int, b : Int, c : Int }
    change : Record -> Record
    change value = { value | c = (trace "c" 30), a = (trace "a" 10) }
    main : Record
    main = change (trace "base" (Record 1 2 3))
    "#,
    Ok("(con data (Constr 0 [I 10, I 2, I 30]))")
);

case!(
    labeled_little_update_preserves_layout_and_order,
    r#"
    module Main exposing (..)
    type record = Record { a : int, b : int, c : int }
    change : record -> record
    change value = { value | c = (trace "c" 30), a = (trace "a" 10) }
    main : record
    main = change (trace "base" (Record 1 2 3))
    "#,
    Ok("(constr 0 (con integer 10) (con integer 2) (con integer 30))")
);

case!(
    validate_int_preserves_original_data,
    r#"
    module Main exposing (..)
    import Data exposing (ToData, Validate)
    main : Int
    main = validate (toData (Builtin.iData 42))
    "#,
    Ok("(con data (I 42))")
);

case!(
    validate_int_rejects_bytes,
    r#"
    module Main exposing (..)
    import Data exposing (ToData, Validate)
    main : Int
    main = validate (toData (Builtin.bData #"ff"))
    "#,
    Err(())
);

case!(
    validate_bytes_rejects_integer,
    r#"
    module Main exposing (..)
    import Data exposing (ToData, Validate)
    main : Bytes
    main = validate (toData (Builtin.iData 42))
    "#,
    Err(())
);

case!(
    data_ignored_payload_keeps_scrutinee_effects,
    r#"
    module Main exposing (..)
    import Data exposing (ToData)
    import Primitive exposing (Data(..))
    main : int
    main = case trace "data input" (toData (Builtin.iData 42)) of
        I _ -> 7
        _ -> fail
    "#,
    Ok("(con integer 7)")
);

case!(
    validate_bytes_preserves_original_data,
    r#"
    module Main exposing (..)
    import Data exposing (ToData, Validate)
    main : Bytes
    main = validate (toData (Builtin.bData #"ff"))
    "#,
    Ok("(con data (B #ff))")
);

case!(
    validate_list_rejects_wrong_element,
    r#"
    module Main exposing (..)
    import Data exposing (ToData, Validate)
    main : List Int
    main = validate (toData (Builtin.listData [Builtin.bData #"ff"]))
    "#,
    Err(())
);

case!(
    validate_map_rejects_wrong_key,
    r#"
    module Main exposing (..)
    import Data exposing (ToData, Validate)
    main : Map Int Bytes
    main = validate (toData (Builtin.mapData [Builtin.mkPairData (Builtin.bData #"aa") (Builtin.bData #"bb")]))
    "#,
    Err(())
);
