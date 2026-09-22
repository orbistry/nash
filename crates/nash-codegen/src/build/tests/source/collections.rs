case!(
    acceptance_test_19_map_none_wrap_int,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin exposing (..)
    import Literal exposing (..)
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
    none : option int
    none = None
    main = eq (map none (\_ -> 14)) None
    "#,
    Ok("(con bool True)")
);

case!(
    acceptance_test_19_map_wrap_void,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin exposing (..)
    import Literal exposing (..)
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
    none : option int
    none = None
    main = eq (map none (\_ -> ())) None
    "#,
    Ok("(con bool True)")
);

case!(
    acceptance_test_20_map_some,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin exposing (..)
    import Literal exposing (..)
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
    main = eq (map (Some 14) (\n -> addInteger n 1)) (Some 15)
    "#,
    Ok("(con bool True)")
);

case!(
    acceptance_test_22_filter_map,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin exposing (..)
    import Literal exposing (..)
    import Eq exposing (..)
    type option 'a = None | Some 'a
    impl Eq 'a => Eq (option 'a) where
        eq a b =
            case (a, b) of
                (None, None) -> True
                (Some x, Some y) -> eq x y
                _ -> False
    foldr xs f zero =
        case xs of
            [] -> zero
            x :: rest -> f x (foldr rest f zero)
    filterMap xs f =
        foldr xs (\x ys ->
            case f x of
                None -> ys
                Some y -> mkCons y ys
        ) []
    empty : list int
    empty = []
    main = eq (filterMap empty (\_ -> Some 42)) []
    "#,
    Ok("(con bool True)")
);

case!(
    acceptance_test_24_map_pair,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin exposing (..)
    import Data exposing (toData)
    import Literal exposing (..)
    import Eq exposing (..)
    type option 'a = None | Some 'a
    optionEq same a b =
        case (a, b) of
            (None, None) -> True
            (Some x, Some y) -> same x y
            _ -> False
    map2 optA optB f =
        case optA of
            None -> None
            Some a ->
                case optB of
                    None -> None
                    Some b -> Some (f a b)
    main = optionEq pairEq (map2 (Some 14) (Some 42) (\a b -> mkPairData (iData a) (iData b))) (Some (mkPairData (iData 14) (iData 42)))
    pairEq a b = if equalsData (toData (fstPair a)) (toData (fstPair b)) then equalsData (toData (sndPair a)) (toData (sndPair b)) else False
    "#,
    Ok("(con bool True)")
);

case!(
    acceptance_test_24_map2,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin exposing (..)
    import Literal exposing (..)
    import Eq exposing (..)
    type option 'a = None | Some 'a
    optionEq same a b =
        case (a, b) of
            (None, None) -> True
            (Some x, Some y) -> same x y
            _ -> False
    map2 optA optB f =
        case optA of
            None -> None
            Some a ->
                case optB of
                    None -> None
                    Some b -> Some (f a b)
    pairEq (a, b) (c, d) = if eq a c then eq b d else False
    main = optionEq pairEq (map2 (Some 14) (Some 42) (\a b -> (a, b))) (Some (14, 42))
    "#,
    Ok("(con bool True)")
);

case!(
    acceptance_test_25_void_equal,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin exposing (..)
    import Literal exposing (..)
    import Eq exposing (..)
    main = eq () ()
    "#,
    Ok("(con bool True)")
);

case!(
    acceptance_test_27_flat_map,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin exposing (..)
    import Literal exposing (..)
    import Eq exposing (..)
    foldr xs f zero =
        case xs of
            [] -> zero
            x :: rest -> f x (foldr rest f zero)
    concat left right = foldr left (\x xs -> mkCons x xs) right
    flatMap xs f =
        case xs of
            [] -> []
            x :: rest -> concat (f x) (flatMap rest f)
    main = eq (flatMap [1, 2, 3] (\a -> [a, a])) [1, 1, 2, 2, 3, 3]
    "#,
    Ok("(con bool True)")
);

case!(
    acceptance_test_28_unique_empty_list,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin exposing (..)
    import Literal exposing (..)
    import Eq exposing (..)
    filter xs f =
        case xs of
            [] -> []
            x :: rest ->
                if f x then mkCons x (filter rest f)
                else filter rest f
    unique xs =
        case xs of
            [] -> []
            x :: rest -> mkCons x (unique (filter rest (\y -> neq y x)))
    empty : list int
    empty = []
    main = eq (unique empty) []
    "#,
    Ok("(con bool True)")
);

case!(
    acceptance_test_28_unique_list,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin exposing (..)
    import Literal exposing (..)
    import Eq exposing (..)
    filter xs f =
        case xs of
            [] -> []
            x :: rest ->
                if f x then mkCons x (filter rest f)
                else filter rest f
    unique xs =
        case xs of
            [] -> []
            x :: rest -> mkCons x (unique (filter rest (\y -> neq y x)))
    main = eq (unique [1, 2, 3, 1]) [1, 2, 3]
    "#,
    Ok("(con bool True)")
);

case!(
    acceptance_test_23_to_list,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin exposing (..)
    import Data exposing (toData)
    import Literal exposing (..)
    import Eq exposing (..)
    type assocList = AssocList { inner : list (pair Bytes Int) }
    impl Eq assocList where
        eq left right = equalsData (toData (mapData left.inner)) (toData (mapData right.inner))
    new = AssocList { inner = [] }
    toList : assocList -> list (pair Bytes Int)
    toList m = m.inner
    insert : assocList -> bytes -> int -> assocList
    insert m k v = AssocList { inner = (doInsert m.inner k v) }
    doInsert elems k v =
        case elems of
            [] -> [mkPairData (bData k) (iData v)]
            entry :: rest ->
                let
                    k2 = unBData (fstPair entry)
                    v2 = unIData (sndPair entry)
                in
                if eq k k2 then mkCons (mkPairData (bData k) (iData v)) rest
                else mkCons (mkPairData (bData k2) (iData v2)) (doInsert rest k v)
    fixture1 = insert (insert new "foo" 42) "bar" 14
    main = equalsData (toData (mapData (toList fixture1))) (toData (mapData [mkPairData (bData "foo") (iData 42), mkPairData (bData "bar") (iData 14)]))
    "#,
    Ok("(con bool True)")
);

case!(
    acceptance_test_29_union_pair,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin exposing (..)
    import Data exposing (toData)
    import Literal exposing (..)
    import Eq exposing (..)
    type assocList = AssocList { inner : list (pair Bytes Int) }
    impl Eq assocList where
        eq left right = equalsData (toData (mapData left.inner)) (toData (mapData right.inner))
    new = AssocList { inner = [] }
    toList : assocList -> list (pair Bytes Int)
    toList m = m.inner
    insert : assocList -> bytes -> int -> assocList
    insert m k v = AssocList { inner = (doInsert m.inner k v) }
    doInsert elems k v =
        case elems of
            [] -> [mkPairData (bData k) (iData v)]
            entry :: rest ->
                let
                    k2 = unBData (fstPair entry)
                    v2 = unIData (sndPair entry)
                in
                if eq k k2 then mkCons (mkPairData (bData k) (iData v)) rest
                else mkCons (mkPairData (bData k2) (iData v2)) (doInsert rest k v)
    fixture1 = insert (insert new "foo" 42) "bar" 14
    fromList xs = AssocList { inner = (doFromList xs) }
    doFromList : list (pair Bytes Int) -> list (pair Bytes Int)
    doFromList xs =
        case xs of
            [] -> []
            entry :: rest -> doInsert (doFromList rest) (unBData (fstPair entry)) (unIData (sndPair entry))
    union : assocList -> assocList -> assocList
    union left right = AssocList { inner = (doUnion left.inner right.inner) }
    doUnion left right =
        case left of
            [] -> right
            entry :: rest -> doUnion rest (doInsert right (unBData (fstPair entry)) (unIData (sndPair entry)))
    main = eq (union fixture1 new) fixture1
    "#,
    Ok("(con bool True)")
);

case!(
    acceptance_test_29_union_tuple,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin exposing (..)
    import Literal exposing (..)
    import Eq exposing (..)
    -- Native lists require Storable elements; tuples use a recursive source list.
    type sequence 'a = Nil | Cons 'a (sequence 'a)
    seqEq same left right =
        case (left, right) of
            (Nil, Nil) -> True
            (Cons x xs, Cons y ys) -> if same x y then seqEq same xs ys else False
            _ -> False
    tupleEq (a, b) (c, d) = if eq a c then eq b d else False
    type assocList 'key 'value = AssocList { inner : sequence ('key, 'value) }
    impl (Eq 'key, Eq 'value) => Eq (assocList 'key 'value) where
        eq left right = seqEq tupleEq left.inner right.inner
    emptyList = AssocList { inner = Nil }
    fromList xs = AssocList { inner = (doFromList xs) }
    doFromList xs =
        case xs of
            Nil -> Nil
            Cons (k, v) rest -> doInsert (doFromList rest) k v
    insert : Eq 'key => assocList 'key 'value -> 'key -> 'value -> assocList 'key 'value
    insert m k v = AssocList { inner = (doInsert m.inner k v) }
    doInsert elems k v =
        case elems of
            Nil -> Cons (k, v) Nil
            Cons (k2, v2) rest ->
                if eq k k2 then Cons (k, v) rest
                else Cons (k2, v2) (doInsert rest k v)
    union : Eq 'key => assocList 'key 'value -> assocList 'key 'value -> assocList 'key 'value
    union left right = AssocList { inner = (doUnion left.inner right.inner) }
    doUnion left right =
        case left of
            Nil -> right
            Cons (k, v) rest -> doUnion rest (doInsert right k v)
    fixture1 : assocList bytes int
    fixture1 = insert (insert emptyList "foo" 42) "bar" 14
    main = eq (union fixture1 emptyList) fixture1
    "#,
    Ok("(con bool True)")
);

case!(
    acceptance_test_30_abs,
    r#"
    module Main exposing (..)
    import Primitive exposing (..)
    import Builtin exposing (..)
    import Literal exposing (..)
    import Eq exposing (..)
    abs a = if lessThanInteger a 0 then subtractInteger 0 a else a
    main = eq (abs (subtractInteger 0 14)) 14
    "#,
    Ok("(con bool True)")
);
