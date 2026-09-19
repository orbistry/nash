case!(
    acceptance_test_1_length,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Eq exposing (..)
    length xs =
        case xs of
            [] -> 0
            _ :: rest -> addInteger 1 (length rest)
    main = equalsInteger (length [1, 2, 3]) 3
    "#,
    Ok("(con bool True)")
);

case!(
    acceptance_test_2_repeat,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Eq exposing (..)
    repeat x n =
        if lessThanEqualsInteger n 0 then []
        else mkCons x (repeat x (subtractInteger n 1))
    main = eq (repeat #"61696b656e" 2) [#"61696b656e", #"61696b656e"]
    "#,
    Ok("(con bool True)")
);

case!(
    acceptance_test_3_concat,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Eq exposing (..)
    foldr xs f zero =
        case xs of
            [] -> zero
            x :: rest -> f x (foldr rest f zero)
    concat left right = foldr left (\x xs -> mkCons x xs) right
    main = eq (concat [1, 2, 3] [4, 5, 6]) [1, 2, 3, 4, 5, 6]
    "#,
    Ok("(con bool True)")
);

case!(
    acceptance_test_4_concat_no_anon_func,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Eq exposing (..)
    foldr xs f zero =
        case xs of
            [] -> zero
            x :: rest -> f x (foldr rest f zero)
    prepend x xs = mkCons x xs
    concat left right = foldr left prepend right
    main = eq (concat [1, 2, 3] [4, 5, 6]) [1, 2, 3, 4, 5, 6]
    "#,
    Ok("(con bool True)")
);

case!(
    acceptance_test_5_direct_head,
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
    main =
        let
            head = \xs ->
                case xs of
                    [] -> None
                    _ -> Some (headList xs)
        in
        eq (head [1, 2, 3]) (Some 1)
    "#,
    Ok("(con bool True)")
);

case!(
    acceptance_test_5_direct_2_heads,
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
    main =
        let
            head = \xs ->
                case xs of
                    [] -> None
                    [a] -> Some xs
                    a :: b :: rest -> Some [a, b]
        in
        eq (head [1, 2, 3]) (Some [1, 2])
    "#,
    Ok("(con bool True)")
);

case!(
    acceptance_test_5_head_not_empty,
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
    head : list int -> option int
    head xs =
        case xs of
            [] -> None
            _ -> Some (headList xs)
    main = eq (head [1, 2, 3]) (Some 1)
    "#,
    Ok("(con bool True)")
);

case!(
    acceptance_test_5_head_empty,
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
    head : list int -> option int
    head xs =
        case xs of
            [] -> None
            _ -> Some (headList xs)
    main = eq (head []) None
    "#,
    Ok("(con bool True)")
);

case!(
    acceptance_test_6_if_else,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Eq exposing (..)
    main =
        let
            x = 1
        in
        if equalsInteger x 1 then True else False
    "#,
    Ok("(con bool True)")
);

case!(
    acceptance_test_6_equals_pair,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Data exposing (toData)
    empty : list Data
    empty = []
    same a b = if equalsData (toData (fstPair a)) (toData (fstPair b)) then equalsData (toData (sndPair a)) (toData (sndPair b)) else False
    main = same (mkPairData (iData 1) (listData empty)) (mkPairData (iData 1) (listData empty))
    "#,
    Ok("(con bool True)")
);

case!(
    acceptance_test_6_equals_tuple,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Eq exposing (..)
    same : (int, list int) -> (int, list int) -> bool
    same (a, b) (c, d) = if eq a c then eq b d else False
    main = same (1, []) (1, [])
    "#,
    Ok("(con bool True)")
);

case!(
    acceptance_test_7_unzip_tuple,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Eq exposing (..)
    type sequence 'a = Nil | Cons 'a (sequence 'a)
    unzip xs =
        case xs of
            Nil -> ([], [])
            Cons (a, b) rest ->
                let
                    (aTail, bTail) = unzip rest
                in
                (mkCons a aTail, mkCons b bTail)
    same (a, b) (c, d) = if eq a c then eq b d else False
    main =
        let
            x = Cons (3, #"55") (Cons (4, #"7799") Nil)
        in
        same (unzip x) ([3, 4], [#"55", #"7799"])
    "#,
    Ok("(con bool True)")
);

case!(
    acceptance_test_7_unzip_pair,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Data exposing (toData)
    unzip : list (pair Int Bytes) -> pair (List Int) (List Bytes)
    unzip xs =
        case xs of
            [] -> mkPairData (listData []) (listData [])
            entry :: rest ->
                let
                    tails = unzip rest
                in
                mkPairData
                    (listData (mkCons (fstPair entry) (unListData (fstPair tails))))
                    (listData (mkCons (sndPair entry) (unListData (sndPair tails))))
    same : pair (List Int) (List Bytes) -> pair (List Int) (List Bytes) -> bool
    same a b =
        if equalsData (toData (fstPair a)) (toData (fstPair b)) then
            equalsData (toData (sndPair a)) (toData (sndPair b))
        else False
    main =
        let
            x = [mkPairData (iData 3) (bData #"55"), mkPairData (iData 4) (bData #"7799")]
        in
        same (unzip x)
            (mkPairData (listData [iData 3, iData 4]) (listData [bData #"55", bData #"7799"]))
    "#,
    Ok("(con bool True)")
);

case!(
    acceptance_test_8_is_empty,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Eq exposing (..)
    isEmpty bytes = equalsInteger (Builtin.lengthOfByteString bytes) 0
    main = eq (isEmpty #"") True
    "#,
    Ok("(con bool True)")
);

case!(
    acceptance_test_8_is_not_empty,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Eq exposing (..)
    isEmpty bytes = equalsInteger (Builtin.lengthOfByteString bytes) 0
    main = eq (isEmpty #"01") False
    "#,
    Ok("(con bool True)")
);

case!(
    acceptance_test_9_is_empty,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Eq exposing (..)
    isEmpty bytes = equalsInteger (lengthOfByteString bytes) 0
    main = eq (isEmpty #"") True
    "#,
    Ok("(con bool True)")
);

case!(
    acceptance_test_10_map_none,
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
    map opt f =
        case opt of
            None -> None
            Some a -> Some (f a)
    addOne n = addInteger n 1
    main = eq (map None addOne) None
    "#,
    Ok("(con bool True)")
);

case!(
    acceptance_test_10_map_some,
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
    map opt f =
        case opt of
            None -> None
            Some a -> Some (f a)
    addOne n = addInteger n 1
    main = eq (map (Some 1) addOne) (Some 2)
    "#,
    Ok("(con bool True)")
);

case!(
    acceptance_test_11_map_empty,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Eq exposing (..)
    map xs f =
        case xs of
            [] -> []
            x :: rest -> mkCons (f x) (map rest f)
    main = eq (map [] (\n -> addInteger n 1)) []
    "#,
    Ok("(con bool True)")
);

case!(
    acceptance_test_11_map_filled,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Eq exposing (..)
    map xs f =
        case xs of
            [] -> []
            x :: rest -> mkCons (f x) (map rest f)
    main = eq (map [6, 7, 8] (\n -> addInteger n 1)) [7, 8, 9]
    "#,
    Ok("(con bool True)")
);

case!(
    acceptance_test_12_filter_even,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Eq exposing (..)
    filter xs f =
        case xs of
            [] -> []
            x :: rest ->
                if f x then mkCons x (filter rest f)
                else filter rest f
    main = eq (filter [1, 2, 3, 4, 5, 6] (\x -> equalsInteger (modInteger x 2) 0)) [2, 4, 6]
    "#,
    Ok("(con bool True)")
);

case!(
    acceptance_test_14_list_creation,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Eq exposing (..)
    main = [subtractInteger 0 2, subtractInteger 0 1, 0]
    "#,
    Ok("(con (list integer) [-2, -1, 0])")
);

case!(
    acceptance_test_15_zero_arg,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Data exposing (toData)
    type pairs = Pairs { inner : list (pair Data Data) }
    new () = Pairs { inner = [] }
    same (Pairs a) (Pairs b) = equalsData (toData (mapData a)) (toData (mapData b))
    main = same (new ()) (Pairs { inner = [] })
    "#,
    Ok("(con bool True)")
);

case!(
    acceptance_test_16_drop,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Eq exposing (..)
    slice bytes start end = sliceByteString start end bytes
    length bytes = lengthOfByteString bytes
    drop bytes n = slice bytes n (subtractInteger (length bytes) n)
    main =
        let
            x = #"01020304050607"
        in
        eq (drop x 2) #"0304050607"
    "#,
    Ok("(con bool True)")
);

case!(
    acceptance_test_17_take,
    r#"
    module Main exposing (..)
    import Builtin exposing (..)
    import Eq exposing (..)
    slice bytes start end = sliceByteString start end bytes
    take bytes n = slice bytes 0 n
    main = eq (take #"010203" 2) #"0102"
    "#,
    Ok("(con bool True)")
);

case!(
    acceptance_test_18_or_else,
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
    orElse opt default =
        case opt of
            None -> default
            Some a -> a
    main = equalsInteger (orElse (Some 42) 14) 42
    "#,
    Ok("(con bool True)")
);
