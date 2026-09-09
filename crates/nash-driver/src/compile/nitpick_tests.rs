use super::*;

fn url(name: &str) -> Url {
    Url::parse(&format!("file:///{name}.nash")).unwrap()
}

fn rejected(source: &str) -> String {
    let bump = Bump::new();
    let interfaces = BTreeMap::from([("Builtin", nash_can::kinds::builtin_interface(&bump))]);
    let (output, compiled) = compile_module(
        &url("Main"),
        None,
        &Ok(source.to_owned()),
        &bump,
        &interfaces,
    );
    assert!(
        compiled.is_none(),
        "rejected module must not publish an interface or solved module"
    );
    let ModuleResult::Failed { message } = output.result else {
        panic!("expected a failed module")
    };
    message
}

#[test]
fn incomplete_case_fails_module() {
    let source = indoc::indoc!(
        r#"
        module Main exposing (..)
        import Builtin exposing (type bool(..))
        f x =
            case x of
                True -> ()
    "#
    );
    let message = rejected(source);
    assert!(
        message.contains("Incomplete") && message.contains("False"),
        "{message}"
    );
    insta::assert_snapshot!(message);
}

#[test]
fn redundant_case_fails_module() {
    let source = indoc::indoc!(
        r#"
        module Main exposing (..)
        import Builtin exposing (type bool(..))
        f x =
            case x of
                _ -> ()
                True -> ()
    "#
    );
    let message = rejected(source);
    assert!(
        message.contains("Redundant") && message.contains("index: 2"),
        "{message}"
    );
    insta::assert_snapshot!(message);
}

#[test]
fn unsafe_argument_fails_module() {
    let message = rejected("module Main exposing (..)\nf (x :: _) = x\n");
    assert!(message.contains("BadArg"), "{message}");
    insta::assert_snapshot!(message);
}

#[test]
fn unsafe_destructure_fails_module() {
    let message = rejected(indoc::indoc!(
        r#"
        module Main exposing (..)
        f xs =
            let
                (x :: rest) = xs
            in
            x
    "#
    ));
    assert!(message.contains("BadDestruct"), "{message}");
    insta::assert_snapshot!(message);
}

#[test]
fn trait_default_without_top_level_definitions_fails_module() {
    let message = rejected(indoc::indoc!(
        r#"
        module Main exposing (..)
        import Builtin exposing (type bool(..))
        trait Choose 'a where
            choose : 'a -> bool -> unit
            choose _ flag =
                case flag of
                    True -> ()
    "#
    ));
    assert!(
        message.contains("Incomplete") && message.contains("False"),
        "{message}"
    );
    insta::assert_snapshot!(message);
}

#[test]
fn impl_method_fails_module() {
    let message = rejected(indoc::indoc!(
        r#"
        module Main exposing (..)
        import Builtin exposing (type bool(..))
        trait Choose 'a where
            choose : 'a -> unit
        impl Choose bool where
            choose True = ()
    "#
    ));
    assert!(
        message.contains("Incomplete") && message.contains("BadArg"),
        "{message}"
    );
    insta::assert_snapshot!(message);
}

#[test]
fn type_errors_precede_nitpick() {
    let message = rejected(indoc::indoc!(
        r#"
        module Main exposing (..)
        import Builtin exposing (type bool(..))
        f : bool -> unit
        f True = True
    "#
    ));
    assert!(message.contains("BadExpr"), "{message}");
    assert!(!message.contains("Incomplete"), "{message}");
}

#[test]
fn rejected_module_publishes_no_interface_to_dependents() {
    let result = build_sync(vec![
        (
            url("Base"),
            None,
            Ok(
                "module Base exposing (..)\nimport Builtin exposing (type bool(..))\nf True = ()\n"
                    .to_owned(),
            ),
        ),
        (
            url("Main"),
            None,
            Ok("module Main exposing (..)\nimport Base\nf = Base.f\n".to_owned()),
        ),
        (
            url("Good"),
            None,
            Ok("module Good exposing (..)\nf x = x\n".to_owned()),
        ),
    ]);
    assert_eq!(result.total, 3);
    assert_eq!(result.failed, 2, "{result:?}");
    assert_eq!(result.success, 1, "{result:?}");
    assert_eq!(result.interfaces.len(), 1);
    assert!(result.interfaces.contains_key(&url("Good")));
    let ModuleResult::Failed { message } = &result.modules[&url("Base")] else {
        panic!("base must fail")
    };
    assert!(message.contains("Incomplete"), "{message}");
    let ModuleResult::Failed { message } = &result.modules[&url("Main")] else {
        panic!("dependent must fail")
    };
    assert!(message.contains("ImportNotFound"), "{message}");
}

#[test]
fn exhaustive_imported_union_publishes_interfaces() {
    let result = build_sync(vec![
        (
            url("Base"),
            None,
            Ok("module Base exposing (type choice(..))\ntype choice = A | B\n".to_owned()),
        ),
        (
            url("Main"),
            None,
            Ok(indoc::indoc!(
                r#"
            module Main exposing (..)
            import Base
            f x =
                case x of
                    Base.A -> ()
                    Base.B -> ()
        "#
            )
            .to_owned()),
        ),
    ]);
    assert_eq!(result.success, 2, "{result:?}");
    assert_eq!(result.interfaces.len(), 2);
}
