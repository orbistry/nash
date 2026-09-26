mod semantic;
use indoc::indoc;
macro_rules! assert_format_snapshot {
    ($source:expr) => {{
        let source=indoc!($source);
        let formatted=super::format(source).unwrap_or_else(|e| panic!("input: {e:?}"));
        let second=super::format(&formatted).unwrap_or_else(|e| panic!("output:\n{formatted}\n{e:?}"));
        assert_eq!(formatted,second,"formatting must be idempotent");
        assert_eq!(semantic::tree(source),semantic::tree(&formatted),"formatting changed the AST");
        insta::with_settings!({description=>source,omit_expression=>true},{insta::assert_snapshot!(formatted);});
        formatted
    }};
}
#[test]
fn values() {
    assert_format_snapshot!("x=42\ny a b=a+b\n");
}
#[test]
fn expressions() {
    assert_format_snapshot!(
        r#"
    module Main exposing (..)
    f x = if x then [1,2] else [3]
    g x = let a = x in case a of
        Some n -> n
        _ -> 0
    record = { a=1,b=2 }
    tuple = (1, "hello", 0xff)
    apply = List.map (\x -> x + 1) [1,2]
"#
    );
}
#[test]
fn declarations() {
    assert_format_snapshot!(
        r#"
    module Main exposing (Thing(..), Value, run)
    import Foo as F exposing (item)
    type Thing 'a = First 'a | Second
    type alias Value = { first : int, second : string }
    trait Eq 'a => Test 'a where
        test : 'a -> bool
        test _ = True
    impl Test int where
        test n = n == 0
    run : int -> int
    run n = n
"#
    );
}
#[test]
fn comments() {
    assert_format_snapshot!(
        r#"
    -- before
    module Main exposing (..)
    {-| Main docs -}
    import Foo
    -- value comment
    {-| Value docs -}
    x = [ 1, -- first
        2 -- second
        ]
    -- end
"#
    );
}
#[test]
fn patterns_and_types() {
    assert_format_snapshot!(
        r#"
    type alias Func ('a : Little) = ('a -> int) -> (list 'a, pair int int)
    f pair(a,b) (x :: xs) { field } = (a,b,x,xs,field)
    g value = case value of
        (Some [x, y] as both) -> both
        Other.Value _ -> fail
        _ -> todo "later"
    qualified : (Eq 'a, Other.Show 'a) => 'a -> string
    qualified x = Other.show x
"#
    );
}
#[test]
fn operators() {
    assert_format_snapshot!(
        r#"
    infix left 6 (+) = add
    a = (x + y) * z
    b = f (-x) (g y) (x |> h)
    c = (f x).field
    d = (+)
    e = (1 +)
    f = (+ 1)
    g = x |> f |> h
"#
    );
}
#[test]
fn keywords_and_macros() {
    assert_format_snapshot!(
        r#"
    @inline
    f x = trace "read" (assert (x > 0))
    g = comptime (1 + 2)
    h = Array.fromList!([1,2])
    i = do
        x <- read
        let y = x + 1
        write y
        pure y
"#
    );
}
#[test]
fn tests_block() {
    assert_format_snapshot!(
        r#"
    module Main exposing (..)
    tests
        import Prop
        test "truth" = do
            assert True
        test "failure" fail within (cpu 1000, mem 50) = do
            fail
        prop "identity" fail once =
            let
                x via Prop.int
                y via Prop.int
            in
            do
                assert (x == y)
"#
    );
}
#[test]
fn base_modules() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../nash-driver/base/src");
    fn visit(path: &std::path::Path) {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(&path);
            } else if path.extension().is_some_and(|e| e == "nash") {
                let source = std::fs::read_to_string(&path).unwrap();
                let formatted = super::format(&source)
                    .unwrap_or_else(|e| panic!("{} input {e:?}", path.display()));
                let second = super::format(&formatted)
                    .unwrap_or_else(|e| panic!("{} output:\n{formatted}\n{e:?}", path.display()));
                assert_eq!(formatted, second, "{}", path.display());
                assert_eq!(
                    semantic::tree(&source),
                    semantic::tree(&formatted),
                    "{} semantic change",
                    path.display()
                );
            }
        }
    }
    visit(&root);
}
#[test]
fn formatting_diff() {
    let source = "module Main exposing (..)\n\nx= 1  \ny =2";
    let formatted = super::format(source).unwrap();
    let report = nash_report::format::difference("src/Main.nash", source, &formatted).unwrap();
    insta::with_settings!({description=>source,omit_expression=>true},{insta::assert_snapshot!(nash_report::render_plain(&report,&nash_report::Source::new(source),"src/Main.nash"));});
    assert!(nash_report::format::difference("src/Main.nash", &formatted, &formatted).is_none());
}
#[test]
fn semantics_sensitive_forms() {
    assert_format_snapshot!(
        r#"
    module Main exposing (type thing(..), Thing(..))
    type thing = Small
    type Thing = Positional ({ field : int }) | Labeled { field : int }
    a = (assert x) + y
    b = (comptime f) x
    c = (fail "reason") + x
    d = f ({ field = 1 })
"#
    );
}
#[test]
fn literals() {
    assert_format_snapshot!(
        r##"
    a = "héllo\n\"world\""
    b = #"deadbeef"
    c = """first
      second
    third"""
    d = -42
    e = ()
    f = []
    g = {}
"##
    );
}
#[test]
fn record_updates() {
    assert_format_snapshot!(
        r#"
    read = .field
    change record = { record | first = 1, second = (f record.first) }
    positional = Thing ({ first = 1 })
    labeled = Thing { first = 1 }
"#
    );
}
#[test]
fn literal_patterns() {
    assert_format_snapshot!(
        r##"
    f x = case x of
        42 -> True
        _ -> fail
    g x = case x of
        "hello" -> 1
        _ -> 2
    h x = case x of
        #"00" -> ()
        _ -> ()
"##
    );
}
#[test]
fn multiline_collections() {
    assert_format_snapshot!(
        r#"
    list =
        [ one
        , two
        , three
        ]
    record =
        { first = 1
        , second = 2
        }
    tuple =
        ( 1
        , 2
        , 3
        )
    pipeline value =
        value
            |> first
            |> second
"#
    );
}
#[test]
fn long_lines() {
    assert_format_snapshot!(
        r#"
    module VeryLongModuleName exposing (firstFunction, secondFunction, thirdFunction, fourthFunction, fifthFunction)
    function firstArgument secondArgument thirdArgument = call firstArgument secondArgument thirdArgument "a fairly long final argument"
    values = [firstLongIdentifier, secondLongIdentifier, thirdLongIdentifier, fourthLongIdentifier]
    field = { veryLongFirstFieldName = firstLongIdentifier, veryLongSecondFieldName = secondLongIdentifier }
"#
    );
}
#[test]
fn nested_control_flow() {
    assert_format_snapshot!(
        r#"
    f x = if x == 1 then 2 else if x == 2 then 3 else 4
    g x = (if x then f else g) x
    h x = case x of
        Some (a :: rest) -> case a of
            Some n -> n
            None -> 0
        _ -> 0
    destruct value = let pair(a,b) = value in a + b
"#
    );
}
#[test]
fn comments_in_syntax() {
    assert_format_snapshot!(
        r#"
    module Main exposing (x, -- export
        y)
    import Foo exposing (a, -- import
        b)
    x = f -- argument
        1
    y = { a = -- field
        2, b = [ -- empty
        ] }
"#
    );
}
#[test]
fn documented_declarations() {
    assert_format_snapshot!(
        r#"
    module Main exposing (..)
    {-| Overview
    @docs thing, run
    -}
    {-| A type. -}
    type thing = Thing

    {-| An alias. -}
    type alias other = int

    {-| A trait. -}
    trait Trait 'a where
        method : 'a -> int

    {-| An implementation. -}
    impl Trait int where
        method x = x

    {-| A function. -}
    @inline
    run x = x
"#
    );
}
#[test]
fn validator() {
    assert_format_snapshot!(
        r#"
    validator module Main exposing (main)
    main context = ()
"#
    );
}
#[test]
fn empty_module() {
    assert_eq!(super::format("").unwrap(), "");
}
#[test]
fn comment_only() {
    assert_format_snapshot!("-- hello\n");
}
#[test]
fn parse_error() {
    let source = "f = [1,";
    let report = super::format(source).expect_err("unclosed list");
    insta::with_settings!({description=>source,omit_expression=>true},{insta::assert_snapshot!(nash_report::render_plain(&report,&nash_report::Source::new(source),"Main.nash"));});
}
#[test]
fn documents() {
    use crate::doc::{Doc, cat, text};
    let doc = cat([text("a"), cat([Doc::Line(" "), text("bbb")]).nest()]).group();
    assert_eq!(doc.render(80), "a bbb\n");
    assert_eq!(doc.render(3), "a\n    bbb\n");
    let hard = cat([text("a"), Doc::Hard, text("b")]).group();
    assert_eq!(hard.render(80), "a\nb\n");
}
#[test]
fn do_control_flow() {
    assert_format_snapshot!(
        r#"
        a = do
            x <- read
            let y = x + 1 in pure y
        b = do
            if condition then first else second
        c = do
            trace "message" result
        d = do
            [ firstLongIdentifier, secondLongIdentifier, thirdLongIdentifier, fourthLongIdentifier ]
    "#
    );
}
#[test]
fn comment_whitespace() {
    assert_format_snapshot!("-- leading  \r\nx = 1 -- trailing  \r\n");
}
#[test]
fn crlf_diff() {
    let source = "x = 1\r\n";
    let formatted = super::format(source).unwrap();
    let report = nash_report::format::difference("Main.nash", source, &formatted).unwrap();
    insta::with_settings!({description=>source,omit_expression=>true},{insta::assert_snapshot!(nash_report::render_plain(&report,&nash_report::Source::new(source),"Main.nash"));});
}
#[test]
fn aligned_do_statements() {
    let formatted = assert_format_snapshot!(
        r#"
        main = do
            if condition then
                first
            else
                second
            let
                x = 1
            in
                pure x
            [ 1
            , 2
            ]
            ( 1
            , 2
            )
            { field = 1
            }
            [
            ]
            {
            }
            (
            )
            next
    "#
    );
    let arena = bumpalo::Bump::new();
    let module = nash_parse::Parser::new(&arena, &formatted)
        .module()
        .unwrap();
    let nash_source::Expr::Do { stmts, last } = &module.values[0].value.body.value else {
        panic!("expected do")
    };
    assert_eq!(stmts.len(), 8, "aligned statements must remain separate");
    assert!(matches!(
        last.value,
        nash_source::Expr::Var { name: "next", .. }
    ));
}

#[test]
fn underindented_do_delimiter() {
    let source = indoc!("main = do\n    [ 1\n  ]\n");
    let report = super::format(source).expect_err("closing below statement indentation");
    insta::with_settings!({description=>source,omit_expression=>true},{insta::assert_snapshot!(nash_report::render_plain(&report,&nash_report::Source::new(source),"Main.nash"));});
}
#[test]
fn do_operators() {
    assert_format_snapshot!(
        r#"
        main = do
            a +
                b
            source
                |> transform
                |> consume
            next
    "#
    );
}
#[test]
fn multiline_annotations() {
    assert_format_snapshot!(
        r#"
        x : ( int
            , int
            )
        x = (1,2)
        trait Example 'a where
            method : { first : 'a
                , second : 'a
                } -> int
    "#
    );
}

#[test]
fn definition_layout() {
    assert_format_snapshot!(
        r#"
        run : int -> int
        run n = n

        identity value =
            value

        main = do
            result <- do
                first
                second
            let
                nested = do
                    first
                    second
            result

        commented =
            -- Keep this comment above the block.
            do
                first
                second
    "#
    );
}

#[test]
fn constrained_signatures() {
    assert_format_snapshot!(
        r#"
        append : ( Lift (list 'a) ('f 'a), Lift (list 'a) ('g 'a) ) => 'f 'a ->
                'g 'a -> list 'a
        append xs ys = appendList (lowerOuter xs) (lowerOuter ys)

        render : Show 'a => 'a -> string
        render value = show value

        trait Example 'a where
            append : ( Lift (list 'a) ('f 'a), Lift (list 'a) ('g 'a) ) => 'f 'a -> 'g 'a -> list 'a
    "#
    );
}

#[test]
fn multiline_exposing() {
    assert_format_snapshot!(
        r#"
        module List exposing ( singleton
            , repeat
            , range
            )

        import VeryLongModuleName exposing ( firstLongExportedName, secondLongExportedName, thirdLongExportedName )

        singleton x = [x]
    "#
    );
}
