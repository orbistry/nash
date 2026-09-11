//! End-to-end type inference tests: parse → canonicalize → constrain →
//! solve, snapshotting the inferred annotation per top-level value (or the
//! type errors).

use bumpalo::Bump;
use indoc::indoc;
use nash_ast::{Annotation, Type as CanType};
use nash_can::{Annotations, Context};
use nash_constrain::UnionFind;
use nash_constrain::error::Error;
use nash_region::Located;

fn literal_interfaces(bump: &Bump) -> std::collections::BTreeMap<&str, nash_can::Interface<'_>> {
    let mut interfaces =
        std::collections::BTreeMap::from([("Builtin", nash_can::kinds::builtin_interface(bump))]);
    let source = bump.alloc_str(indoc!(
        "
        module Literal exposing (..)
        import Builtin exposing (..)
        trait FromInt 'a where
            fromInt : int -> 'a
        trait FromString 'a where
            fromString : string -> 'a
        trait FromBytes 'a where
            fromBytes : bytes -> 'a
        impl FromInt int where
            fromInt x = x
        impl FromString string where
            fromString x = x
        impl FromBytes bytes where
            fromBytes x = x
    "
    ));
    let module = nash_parse::Parser::new(bump, source).module().unwrap();
    let can = nash_can::canonicalize(
        bump,
        Context {
            package: Some(nash_ast::primitives::CORE),
            interfaces: Some(&interfaces),
        },
        &module,
    )
    .unwrap();
    let mut uf = UnionFind::new();
    let module = &can.module;
    let (annotations, _) = nash_solve::run(bump, &mut uf, module, &can.tables).unwrap();
    interfaces.insert(
        "Literal",
        nash_can::from_module(bump, &can.module, &annotations),
    );
    interfaces
}

fn infer<'a>(bump: &'a Bump, input: &str) -> Result<Annotations<'a>, Vec<Error<'a>>> {
    let src = bump.alloc_str(input);
    let mut parser = nash_parse::Parser::new(bump, src);
    let module = parser.module().expect("expected successful parse");
    let interfaces = literal_interfaces(bump);
    let can_result = nash_can::canonicalize(
        bump,
        Context {
            package: None,
            interfaces: Some(&interfaces),
        },
        &module,
    )
    .expect("expected successful canonicalization");

    let mut uf = UnionFind::new();
    let module = &can_result.module;
    nash_solve::run(bump, &mut uf, module, &can_result.tables).map(|(annotations, _)| annotations)
}

#[test]
fn recovery_collects_independent_mixed_errors_in_both_declaration_orders() {
    let header = "module Main exposing (..)\ntrait Round 'a where\n    create : () -> 'a\n    discard : 'a -> ()\ntype higher 'f = Higher ('f ())\nidfa : 'f 'a -> 'f 'a\nidfa x = x\n";
    let definitions = [
        "mismatch : ()\nmismatch = \\x -> x\n",
        "missing = discard ()\n",
        "constraint : 'a -> ()\nconstraint x = discard x\n",
        "ambiguous = discard (create ())\n",
        "kind = idfa (Higher [])\n",
    ];
    for reverse in [false, true] {
        let bump = Bump::new();
        let mut definitions = definitions.to_vec();
        if reverse {
            definitions.reverse();
        }
        let source = format!("{header}{}", definitions.concat());
        let errors =
            infer(&bump, &source).expect_err("failed solve must not publish solved output");
        assert_eq!(
            errors
                .iter()
                .filter(|error| matches!(error, Error::BadExpr(..)))
                .count(),
            1,
            "{errors:#?}"
        );
        assert_eq!(
            errors
                .iter()
                .filter(|error| matches!(error, Error::MissingImpl { .. }))
                .count(),
            1,
            "{errors:#?}"
        );
        assert_eq!(
            errors
                .iter()
                .filter(|error| matches!(error, Error::MissingConstraint { .. }))
                .count(),
            1,
            "{errors:#?}"
        );
        assert_eq!(
            errors
                .iter()
                .filter(|error| matches!(error, Error::AmbiguousType { .. }))
                .count(),
            1,
            "{errors:#?}"
        );
        assert_eq!(
            errors
                .iter()
                .filter(|error| matches!(error, Error::BadKind { .. }))
                .count(),
            1,
            "{errors:#?}"
        );
        assert_eq!(errors.len(), 5, "{errors:#?}");
        let mut settings = insta::Settings::clone_current();
        settings.set_description(source);
        let _guard = settings.bind_to_scope();
        insta::assert_debug_snapshot!(
            format!(
                "recovery_mixed_errors_{}",
                if reverse { "reversed" } else { "forward" }
            ),
            errors
        );
    }
}

#[test]
fn recovery_reports_an_independent_escaping_annotation_and_blocks_its_uses() {
    let bump = Bump::new();
    let errors = infer(
        &bump,
        indoc!(
            r#"
        module Main exposing (..)
        mismatch : ()
        mismatch = \x -> x
        outer x =
            let
                inner : 'a
                inner = x
            in
            (inner (), x ())
    "#
        ),
    )
    .expect_err("the local annotation is invalid independently of mismatch");
    assert_eq!(
        errors
            .iter()
            .filter(|error| matches!(error, Error::BadExpr(..)))
            .count(),
        1,
        "{errors:#?}"
    );
    assert_eq!(
        errors
            .iter()
            .filter(|error| matches!(error, Error::AnnotationVariableEscapes { .. }))
            .count(),
        1,
        "{errors:#?}"
    );
    assert_eq!(errors.len(), 2, "{errors:#?}");
}

#[test]
fn recovery_blocks_repeated_and_recursive_uses_but_keeps_sibling_errors() {
    for broken in [
        "broken = if True then () else (\\x -> x)\n",
        "broken x = x x\n",
        "broken x = if True then recurse x else (\\y -> y)\nrecurse x = if True then broken x else ()\n",
        "trait Missing 'a where\n    missing : 'a -> 'a\nbroken = missing ()\n",
    ] {
        let bump = Bump::new();
        let source = format!(
            "module Main exposing (..)\nimport Builtin exposing (..)\n{broken}first : ()\nfirst = broken\nsecond : ()\nsecond = broken\nsibling : ()\nsibling = \\x -> x\n"
        );
        let errors = infer(&bump, &source).expect_err("failed dependencies stay blocked");
        assert_eq!(errors.len(), 2, "{source}\n{errors:#?}");
        if broken == "broken x = x x\n" {
            assert_eq!(
                errors
                    .iter()
                    .filter(|error| matches!(error, Error::InfiniteType { .. }))
                    .count(),
                1,
                "{errors:#?}"
            );
        }
    }
}

#[test]
fn recovery_shared_partial_unification_does_not_create_trait_or_call_cascades() {
    let bump = Bump::new();
    let errors = infer(
        &bump,
        indoc!(
            r#"
        module Main exposing (..)
        trait Need 'a where
            need : 'a -> ()
        outer x =
            let
                broken : ((), ())
                broken = (x, \y -> y)
            in
            (x (), need x, broken)
        sibling : ()
        sibling = \x -> x
    "#
        ),
    )
    .expect_err("shared argument depends on the failed tuple check");
    assert_eq!(errors.len(), 2, "{errors:#?}");
    assert!(
        errors
            .iter()
            .all(|error| matches!(error, Error::BadExpr(..))),
        "{errors:#?}"
    );
}

#[test]
fn recovery_final_recursive_evidence_error_survives_an_independent_mismatch() {
    let bump = Bump::new();
    let errors = infer(
        &bump,
        indoc!(
            r#"
        module Main exposing (..)
        type option 'a = Some 'a
        trait Keep 'a where
            keep : 'a -> 'a
        impl Keep 'a => Keep (option 'a) where
            keep xs = xs
        nest : Keep 'a => 'a -> ()
        nest x = nest (Some x)
        sibling : ()
        sibling = \x -> x
    "#
        ),
    )
    .expect_err("both checks must run");
    assert_eq!(
        errors
            .iter()
            .filter(|error| matches!(error, Error::BadExpr(..)))
            .count(),
        1,
        "{errors:#?}"
    );
    assert_eq!(
        errors
            .iter()
            .filter(|error| matches!(error, Error::PolymorphicRecursion { .. }))
            .count(),
        1,
        "{errors:#?}"
    );
    assert_eq!(errors.len(), 2, "{errors:#?}");
}

#[test]
fn recovery_resolution_limit_keeps_unrelated_obligations() {
    let bump = Bump::new();
    let errors = infer(
        &bump,
        indoc!(
            r#"
        module Main exposing (..)
        trait Keep 'a where
            keep : 'a -> 'a
        trait Missing 'a where
            missing : 'a -> 'a
        impl Keep (list (list 'a)) => Keep (list 'a) where
            keep xs = xs
        value = (keep [()], missing ())
        sibling : ()
        sibling = \x -> x
    "#
        ),
    )
    .expect_err("limit and independent failures");
    assert_eq!(
        errors
            .iter()
            .filter(|error| matches!(error, Error::ImplResolutionLimit { .. }))
            .count(),
        1,
        "{errors:#?}"
    );
    assert_eq!(errors.iter().filter(|error| matches!(error, Error::MissingImpl { trait_, .. } if trait_.name == "Missing")).count(), 1, "{errors:#?}");
    assert_eq!(
        errors
            .iter()
            .filter(|error| matches!(error, Error::BadExpr(..)))
            .count(),
        1,
        "{errors:#?}"
    );
}

#[test]
fn recovery_field_failures_block_dependent_traits_and_keep_independent_traits() {
    for body in [
        "bad : ()\nbad = missing (().field)",
        "type alias record = { a : () }\nbad : ()\nbad = missing ({ a = () }.field)",
        "type thing = Thing { a : () }\nbad : thing -> ()\nbad record = missing record.absent",
        "type thing = Thing { a : () }\nbad : thing -> thing\nbad record = missing { record | a = () }",
        "bad : () -> ()\nbad record = missing { record | a = () }",
    ] {
        let bump = Bump::new();
        let source = format!(
            "module Main exposing (..)\ntrait Missing 'a where\n    missing : 'a -> 'a\n{body}\nsibling = missing ()\n"
        );
        let errors = infer(&bump, &source).expect_err("field failure and independent missing impl");
        assert_eq!(errors.len(), 2, "{source}\n{errors:#?}");
        assert_eq!(
            errors
                .iter()
                .filter(|error| matches!(error, Error::MissingImpl { .. }))
                .count(),
            1,
            "{source}\n{errors:#?}"
        );
    }
}

#[test]
fn recovery_keeps_independent_tuple_siblings_in_both_orders() {
    let header = "module Main exposing (..)\ntrait Missing 'a where\n    missing : 'a -> 'a\ntrait Round 'a where\n    create : () -> 'a\n    discard : 'a -> ()\ntype higher 'f = Higher ('f ())\nidfa : 'f 'a -> 'f 'a\nidfa x = x\n";
    for (first, second, expected) in [
        ("() ()", "missing ()", ["mismatch", "impl"]),
        ("() ()", "discard (create ())", ["mismatch", "ambiguity"]),
        ("idfa (Higher [])", "missing ()", ["kind", "impl"]),
        ("idfa (Higher [])", "idfa (Higher [])", ["kind", "kind"]),
    ] {
        for reverse in [false, true] {
            let bump = Bump::new();
            let (first, second) = if reverse {
                (second, first)
            } else {
                (first, second)
            };
            let source = format!("{header}bad = ({first}, {second})\n");
            let errors = infer(&bump, &source)
                .expect_err("both tuple expressions are independently invalid");
            let mut actual: Vec<_> = errors
                .iter()
                .map(|error| match error {
                    Error::BadExpr(..) => "mismatch",
                    Error::MissingImpl { .. } => "impl",
                    Error::AmbiguousType { .. } => "ambiguity",
                    Error::BadKind { .. } => "kind",
                    _ => "unexpected",
                })
                .collect();
            actual.sort();
            let mut expected = expected;
            expected.sort();
            assert_eq!(actual, expected, "{source}\n{errors:#?}");
        }
    }
}

#[test]
fn recovery_annotated_tuple_keeps_independent_obligations() {
    for body in ["(\\x -> x, missing ())", "(missing (), \\x -> x)"] {
        let bump = Bump::new();
        let source = format!(
            "module Main exposing (..)\ntrait Missing 'a where\n    missing : 'a -> 'a\nbad : ((), ())\nbad = {body}\n"
        );
        let errors =
            infer(&bump, &source).expect_err("both tuple expressions are independently invalid");
        assert_eq!(errors.len(), 2, "{source}\n{errors:#?}");
        assert!(
            errors
                .iter()
                .any(|error| matches!(error, Error::BadExpr(..))),
            "{errors:#?}"
        );
        assert!(
            errors
                .iter()
                .any(|error| matches!(error, Error::MissingImpl { .. })),
            "{errors:#?}"
        );
    }
}

#[test]
fn recovery_poisoned_tuple_child_keeps_independent_type_mismatches() {
    for (annotation, body, field_errors) in [
        ("((), ())", "(().field, \\x -> x)", 1),
        ("((), ())", "(\\x -> x, ().field)", 1),
        ("((), ())", "(() (), \\x -> x)", 0),
        ("((), ())", "(\\x -> x, () ())", 0),
        ("((), ((), ()))", "((), (().field, \\x -> x))", 1),
        ("(((), ()), ())", "((\\x -> x, ().field), ())", 1),
        ("(((), ()), ())", "((().field, ()), \\x -> x)", 1),
        ("((), ((), ()))", "(\\x -> x, ((), ().field))", 1),
    ] {
        let bump = Bump::new();
        let source = format!("module Main exposing (..)\nbad : {annotation}\nbad = {body}\n");
        let errors = infer(&bump, &source).expect_err("both tuple errors must survive");
        assert_eq!(errors.len(), 2, "{source}\n{errors:#?}");
        assert_eq!(
            errors
                .iter()
                .filter(|error| matches!(error, Error::NotARecord { .. }))
                .count(),
            field_errors,
            "{source}\n{errors:#?}"
        );
        assert_eq!(
            errors
                .iter()
                .filter(|error| matches!(error, Error::BadExpr(..)))
                .count(),
            2 - field_errors,
            "{source}\n{errors:#?}"
        );
    }
}

#[test]
fn recovery_field_selection_keeps_errors_independent_of_other_fields() {
    for body in [
        "{ a = ().field, b = (\\x -> x) }.b",
        "{ a = (\\x -> x), b = ().field }.a",
    ] {
        let bump = Bump::new();
        let source = format!(
            "module Main exposing (..)\ntype alias record 'a 'b = {{ a : 'a, b : 'b }}\nbad : ()\nbad = {body}\n"
        );
        let errors =
            infer(&bump, &source).expect_err("selected field remains independently invalid");
        assert_eq!(errors.len(), 2, "{source}\n{errors:#?}");
        assert!(
            errors
                .iter()
                .any(|error| matches!(error, Error::NotARecord { .. })),
            "{source}\n{errors:#?}"
        );
        assert!(
            errors
                .iter()
                .any(|error| matches!(error, Error::BadExpr(..) | Error::FieldMismatch { .. })),
            "{source}\n{errors:#?}"
        );
    }
    for (body, independent) in [
        ("missing ({ a = ().field, b = () }.b)", true),
        ("missing ({ a = (), b = ().field }.b)", false),
    ] {
        let bump = Bump::new();
        let source = format!(
            "module Main exposing (..)\ntype alias record 'a 'b = {{ a : 'a, b : 'b }}\ntrait Missing 'a where\n    missing : 'a -> 'a\nbad : ()\nbad = {body}\n"
        );
        let errors = infer(&bump, &source).expect_err("failed record field");
        assert_eq!(
            errors.len(),
            if independent { 2 } else { 1 },
            "{source}\n{errors:#?}"
        );
        assert_eq!(
            errors
                .iter()
                .filter(|error| matches!(error, Error::MissingImpl { .. }))
                .count(),
            usize::from(independent),
            "{source}\n{errors:#?}"
        );
        assert!(
            errors
                .iter()
                .any(|error| matches!(error, Error::NotARecord { .. })),
            "{source}\n{errors:#?}"
        );
    }
}

#[test]
fn solved_output_records_empty_context_calls_and_preserves_capture_names() {
    let bump = Bump::new();
    let source = indoc!(
        r#"
        module Main exposing (..)
        outer x =
            let
                local y = (x, y)
            in
            (local (), local x)
    "#
    );
    let parsed = nash_parse::Parser::new(&bump, source).module().unwrap();
    let canonical = nash_can::canonicalize(&bump, Context::default(), &parsed).unwrap();
    let mut uf = UnionFind::new();
    let module = &canonical.module;
    let (annotations, solved) = nash_solve::run(&bump, &mut uf, module, &canonical.tables).unwrap();
    assert_eq!(solved.schemes.len(), 2);
    assert_eq!(
        solved.instances.len(),
        2,
        "both local calls need type arguments even with no evidence"
    );
    let nash_ast::Decls::Declare { definition, .. } = canonical.module.decls else {
        panic!("outer declaration")
    };
    let nash_ast::Def::Def { body, .. } = definition else {
        panic!("outer definition")
    };
    let nash_ast::Expr::Let { definition, .. } = body.value else {
        panic!("local let")
    };
    let nash_ast::Def::Def { name, .. } = definition else {
        panic!("local definition")
    };
    assert_eq!(name.value, "local");
    let local = &solved.schemes[&nash_ast::NodeId::def(name)];
    assert_eq!(local.annotation.free_vars.len(), 1);
    let outer = annotations["outer"];
    let CanType::Lambda { from, .. } = outer.typ.value else {
        panic!("outer type")
    };
    let CanType::Var(outer_var) = from.value else {
        panic!("outer variable")
    };
    let CanType::Lambda { to, .. } = local.annotation.typ.value else {
        panic!("local function type")
    };
    let CanType::Tuple { first, .. } = to.value else {
        panic!("local result type")
    };
    assert!(matches!(first.value, CanType::Var(name) if name == outer_var));
    assert_ne!(local.annotation.free_vars[0], outer_var);
    let mut rendered = solved
        .instances
        .values()
        .map(|instance| {
            assert!(instance.evidence.is_empty());
            assert_eq!(instance.type_args.len(), 1);
            render_type(instance.type_args[0], Ctx::None)
        })
        .collect::<Vec<_>>();
    rendered.sort();
    let mut expected = vec!["unit".to_owned(), outer_var.to_owned()];
    expected.sort();
    assert_eq!(rendered, expected);
}

#[test]
fn builtin_list_annotations_match_literals_and_patterns() {
    let bump = Bump::new();
    let source = bump.alloc_str(indoc!(
        r#"
        module Main exposing (..)

        import Builtin exposing (type list)

        empty : list 'a
        empty = []

        first : list 'a -> 'a
        first xs =
            case xs of
                head :: tail -> head
    "#
    ));
    let module = nash_parse::Parser::new(&bump, source).module().unwrap();
    let interfaces =
        std::collections::BTreeMap::from([("Builtin", nash_can::kinds::builtin_interface(&bump))]);
    let canonical = nash_can::canonicalize(
        &bump,
        Context {
            package: None,
            interfaces: Some(&interfaces),
        },
        &module,
    )
    .unwrap();
    let mut uf = UnionFind::new();
    let module = &canonical.module;
    let (annotations, _) = nash_solve::run(&bump, &mut uf, module, &canonical.tables)
        .expect("annotations, list literals, and patterns use the same builtin type");
    insta::assert_snapshot!(render_annotations(&annotations));
}

// RENDER INFERRED TYPES (Elm-style, for readable snapshots)

#[derive(Clone, Copy, PartialEq)]
enum Ctx {
    None,
    Func,
    App,
}

fn ordinary_context_len(annotation: &Annotation<'_>) -> usize {
    annotation
        .context
        .iter()
        .filter(|p| {
            p.trait_ref()
                .is_some_and(|name| nash_ast::primitives::ReprTrait::of(name).is_none())
        })
        .count()
}

fn render_annotations(annotations: &Annotations<'_>) -> String {
    annotations
        .iter()
        .map(|(name, annotation)| format!("{name} : {}", render_annotation(annotation)))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_annotation(annotation: &Annotation<'_>) -> String {
    let mut typ = render_type(annotation.typ, Ctx::None);
    if !annotation.context.is_empty() {
        let predicates = annotation
            .context
            .iter()
            .map(|p| match p {
                nash_ast::Pred::Trait { trait_, args }
                | nash_ast::Pred::Implied { trait_, args } => {
                    render_apply(trait_.name, args, Ctx::None)
                }
                nash_ast::Pred::Apply { head, args } => format!(
                    "Apply {}",
                    render_apply(&render_type(head, Ctx::App), args, Ctx::None)
                ),
            })
            .collect::<Vec<_>>();
        let context = if predicates.len() == 1 {
            predicates[0].clone()
        } else {
            format!("({})", predicates.join(", "))
        };
        typ = format!("{context} => {typ}");
    }
    if annotation.free_vars.is_empty() {
        typ
    } else {
        format!("forall {}. {typ}", annotation.free_vars.join(" "))
    }
}

fn render_type(typ: &Located<CanType<'_>>, ctx: Ctx) -> String {
    match &typ.value {
        CanType::Lambda { from, to } => {
            let rendered = format!(
                "{} -> {}",
                render_type(from, Ctx::Func),
                render_type(to, Ctx::None)
            );
            match ctx {
                Ctx::None => rendered,
                Ctx::Func | Ctx::App => format!("({rendered})"),
            }
        }

        CanType::Var(name) => (*name).to_string(),
        CanType::App { head, args } => render_apply(&render_type(head, Ctx::App), args, ctx),

        CanType::Named { reference, args } => render_apply(reference.name, args, ctx),

        CanType::Record { fields } => {
            let rendered_fields = fields
                .iter()
                .map(|field| format!("{} : {}", field.field, render_type(field.typ, Ctx::None)))
                .collect::<Vec<_>>()
                .join(", ");
            if fields.is_empty() {
                "{}".to_string()
            } else {
                format!("{{ {rendered_fields} }}")
            }
        }

        CanType::Tuple {
            first,
            second,
            rest,
        } => {
            let mut parts = vec![
                render_type(first, Ctx::None),
                render_type(second, Ctx::None),
            ];
            parts.extend(rest.iter().map(|third| render_type(third, Ctx::None)));
            format!("( {} )", parts.join(", "))
        }

        CanType::Alias {
            reference,
            arguments,
            target: _,
            ..
        } => {
            let args: Vec<&Located<CanType<'_>>> =
                arguments.iter().map(|argument| argument.typ).collect();
            render_apply(reference.name, &args, ctx)
        }
    }
}

fn render_apply(name: &str, args: &[&Located<CanType<'_>>], ctx: Ctx) -> String {
    if args.is_empty() {
        name.to_string()
    } else {
        let rendered = format!(
            "{name} {}",
            args.iter()
                .map(|arg| render_type(arg, Ctx::App))
                .collect::<Vec<_>>()
                .join(" ")
        );
        match ctx {
            Ctx::App => format!("({rendered})"),
            Ctx::None | Ctx::Func => rendered,
        }
    }
}

// SNAPSHOT MACROS

macro_rules! assert_inference_snapshot {
    ($input:expr) => {{
        let input = indoc!($input);
        let bump = Bump::new();
        let annotations = infer(&bump, input).expect("expected successful type inference");

        insta::with_settings!({
            description => format!("Code:\n\n{}", input),
            omit_expression => true,
        }, {
            insta::assert_snapshot!(render_annotations(&annotations));
        });
    }};
}

macro_rules! assert_inference_error_snapshot {
    ($input:expr) => {{
        let input = indoc!($input);
        let bump = Bump::new();
        let errors = infer(&bump, input).expect_err("expected type errors");

        insta::with_settings!({
            description => format!("Code:\n\n{}", input),
            omit_expression => true,
        }, {
            insta::assert_debug_snapshot!(errors);
        });
    }};
}

// LITERALS AND SIMPLE VALUES

#[test]
fn literal_method_defaulting_retries_impls_with_the_enclosing_given() {
    for (primitive, trait_name, method) in [
        ("int", "FromInt", "fromInt"),
        ("string", "FromString", "fromString"),
        ("bytes", "FromBytes", "fromBytes"),
    ] {
        for trusted in [true, false] {
            let package = if trusted {
                nash_ast::primitives::CORE
            } else {
                nash_ast::PackageName {
                    author: "example",
                    project: "literal",
                }
            };
            let bump = Bump::new();
            let interfaces = std::collections::BTreeMap::from([(
                "Builtin",
                nash_can::kinds::builtin_interface(&bump),
            )]);
            let literal = indoc!(
                r#"
        module Literal exposing (..)
        import Builtin exposing (..)
        trait FromInt 'a where
            fromInt : int -> 'a
        impl FromInt int where
            fromInt x = x
    "#
            )
            .replace("FromInt", trait_name)
            .replace("fromInt", method)
            .replace("int", primitive);
            let parsed = nash_parse::Parser::new(&bump, &literal).module().unwrap();
            let canonical = nash_can::canonicalize(
                &bump,
                Context {
                    package: Some(package),
                    interfaces: Some(&interfaces),
                },
                &parsed,
            )
            .unwrap();
            let mut uf = UnionFind::new();
            let module = &canonical.module;
            let (annotations, _) =
                nash_solve::run(&bump, &mut uf, module, &canonical.tables).unwrap();
            let interfaces = std::collections::BTreeMap::from([
                ("Builtin", nash_can::kinds::builtin_interface(&bump)),
                (
                    "Literal",
                    nash_can::from_module(&bump, &canonical.module, &annotations),
                ),
            ]);
            let main = indoc!(
                r#"
        module Main exposing (..)
        import Builtin exposing (..)
        import Literal exposing (FromInt, fromInt)
        type Box 'a = Box 'a
        trait Permit 'a where
            permit : 'a -> 'a
        trait Gate 'a 'b where
            gate : 'a -> 'b -> ()
        impl Permit 'a => Gate int (Box 'a) where
            gate x box = ()
        same : 'a -> 'a -> 'a
        same x y = x
        run : Permit 'a => int -> Box 'a -> ()
        run n box = gate (same (fromInt n) (fromInt n)) box
        type option 'a = None | Some 'a
        trait Seed 'a where
            seed : int -> 'a
        impl Seed int where
            seed n = n
        trait Step 'a 'b where
            step : 'a -> 'b -> ()
        impl FromInt 'a => Step int (option 'a) where
            step x box = ()
        chain n = step (fromInt n) (Some (seed n))
    "#
            )
            .replace("FromInt", trait_name)
            .replace("fromInt", method)
            .replace("int", primitive);
            let parsed = nash_parse::Parser::new(&bump, &main).module().unwrap();
            let canonical = nash_can::canonicalize(
                &bump,
                Context {
                    package: None,
                    interfaces: Some(&interfaces),
                },
                &parsed,
            )
            .unwrap();
            let mut uf = UnionFind::new();
            let module = &canonical.module;
            let result = nash_solve::run(&bump, &mut uf, module, &canonical.tables);
            if !trusted {
                let errors =
                    result.expect_err("a same-named trait from another package cannot default");
                assert!(
                    errors
                        .iter()
                        .any(|error| matches!(error, Error::AmbiguousType { .. }))
                );
                continue;
            }
            let (annotations, solved) = result.unwrap();
            assert_eq!(ordinary_context_len(annotations["run"]), 1);
            let gate = solved.instances.values().flat_map(|instance| instance.evidence).find(|evidence| {
        matches!(evidence, nash_ast::Evidence::Impl { impl_, .. } if impl_.key.trait_.name == "Gate")
    }).expect("defaulting selects the actual Gate impl");
            let nash_ast::Evidence::Impl { args, .. } = gate else {
                unreachable!()
            };
            let run_binder = solved
                .schemes
                .values()
                .find(|scheme| std::ptr::eq(scheme.annotation, annotations["run"]))
                .unwrap()
                .binder;
            assert!(
                matches!(args, [nash_ast::Evidence::Given { binder, index: 0 }, nash_ast::Evidence::Given { binder: repr_binder, index: 1 }]
                if *binder == run_binder && *repr_binder == run_binder)
            );
            assert!(solved.instances.values().flat_map(|instance| instance.evidence).any(|evidence| {
        matches!(evidence, nash_ast::Evidence::Impl { impl_, .. }
            if impl_.home.package == Some(nash_ast::primitives::CORE) && impl_.key.trait_.name == trait_name)
    }));
            assert!(annotations["chain"].context.is_empty());
            insta::assert_snapshot!(
                format!("literal_method_defaulting_{primitive}"),
                render_annotations(&annotations)
            );
        }
    }
}

#[test]
fn ambiguous_predicates_keep_distinct_variable_names() {
    let bump = Bump::new();
    let errors = infer(
        &bump,
        indoc!(
            r#"
        module Main exposing (..)
        trait Source 'a where
            create : () -> 'a
        trait Sink 'a where
            consume : 'a -> ()
        value = (consume (create ()), consume (create ()))
        "#
        ),
    )
    .expect_err("both hidden variables are ambiguous");
    let names: Vec<_> = errors
        .iter()
        .map(|error| {
            let Error::AmbiguousType { variable, .. } = error else {
                panic!("ambiguity error")
            };
            let nash_constrain::error_type::ErrorType::FlexVar(name) = variable else {
                panic!("flexible variable")
            };
            *name
        })
        .collect();
    assert_eq!(names.len(), 2);
    assert_ne!(names[0], names[1]);
    insta::assert_debug_snapshot!(errors);
}

#[test]
fn two_literal_traits_do_not_choose_an_arbitrary_default() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        same : 'a -> 'a -> 'a
        same x _ = x
        discard x = ()
        value = discard (same 1 "one")
    "#
    );
}

#[test]
fn hidden_trait_variable_is_reported_at_the_innermost_definition() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        trait Round 'a where
            create : () -> 'a
            discard : 'a -> ()
        outer x =
            let
                hidden = discard (create ())
            in
                (x, hidden)
        "#
    );
}

#[test]
fn inferred_value_rejects_a_hidden_trait_variable() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        trait Round 'a where
            create : () -> 'a
            discard : 'a -> ()
        value = discard (create ())
        "#
    );
}

#[test]
fn annotated_value_rejects_a_hidden_trait_variable() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        trait Round 'a where
            create : () -> 'a
            discard : 'a -> ()
        value : ()
        value = discard (create ())
        "#
    );
}

#[test]
fn recursive_evidence_growth_through_a_nested_helper_is_rejected() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        type option 'a = Some 'a
        trait Keep 'a where
            keep : 'a -> 'a
        impl Keep 'a => Keep (option 'a) where
            keep xs = xs
        nest : Keep 'a => 'a -> ()
        nest x =
            let
                helper : Keep 'b => 'b -> ()
                helper y = nest (Some y)
            in
            helper x
        "#
    );
}

#[test]
fn recursive_evidence_allows_unchanged_closed_and_unconstrained_calls() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (..)
        type option 'a = Some 'a
        trait Keep 'a where
            keep : 'a -> 'a
        impl Keep () where
            keep x = x
        impl Keep 'a => Keep (option 'a) where
            keep xs = xs
        same : Keep 'a => 'a -> ()
        same x = same x
        closed : Keep 'a => 'a -> ()
        closed x = closed ()
        plain : 'a -> ()
        plain x = plain (Some x)
        wrap : Keep 'a => 'a -> option 'a
        wrap x = keep (Some x)
        reset : (Keep 'a, Keep 'b) => 'a -> 'b -> ()
        reset x y = reset (Some y) ()
        nested : Keep 'a => 'a -> ()
        nested x =
            let
                helper : Keep 'b => 'b -> ()
                helper y = nested (Some y)
            in
            helper ()
        "#
    );
}

#[test]
fn mutual_recursion_rejects_growing_evidence_from_another_member() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        type option 'a = Some 'a
        trait Keep 'a where
            keep : 'a -> 'a
        impl Keep 'a => Keep (option 'a) where
            keep xs = xs
        left : Keep 'a => 'a -> ()
        left x = right (Some x)
        right : Keep 'a => 'a -> ()
        right x = left x
        "#
    );
}

#[test]
fn recursive_impl_evidence_cannot_grow_from_its_own_given() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        type option 'a = Some 'a
        trait Keep 'a where
            keep : 'a -> 'a
        impl Keep 'a => Keep (option 'a) where
            keep xs = xs
        nest : Keep 'a => 'a -> ()
        nest x = nest (Some x)
        "#
    );
}

#[test]
fn annotated_body_requires_a_given_for_a_rigid_trait_argument() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        trait Keep 'a where
            keep : 'a -> 'a
        missing : 'a -> 'a
        missing x = keep x
    "#
    );
}

#[test]
fn superclass_context_satisfies_an_annotated_body() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (..)
        trait Keep 'a where
            keep : 'a -> 'a
        trait Keep 'a => Strong 'a where
            strong : 'a -> 'a
        present : Strong 'a => 'a -> 'a
        present x = keep x
    "#
    );
}

#[test]
fn local_helper_reports_missing_constraint_on_its_annotated_owner() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        trait Keep 'a where
            keep : 'a -> 'a
        outer : 'a -> 'a
        outer x =
            let
                helper ignored = keep x
            in
            helper ()
    "#
    );
}

#[test]
fn declared_contexts_are_available_at_local_and_recursive_uses() {
    let bump = Bump::new();
    let source = indoc!(
        r#"
        module Main exposing (..)
        trait Keep 'a where
            keep : 'a -> 'a
        type Container 'a = Wrap 'a
        impl Keep () where
            keep x = x
        type Color = Red
        impl Keep Color where
            keep x = x
        impl Keep 'a => Keep (Container 'a) where
            keep (Wrap x) = Wrap (keep x)
        boxed : Keep (Container 'a) => 'a -> 'a
        boxed x = x
        useBox = (boxed (Wrap Red), boxed Red)
        forward : Keep 'a => 'a -> 'a
        forward x = x
        monomorphic : Keep () => ()
        monomorphic = ()
        use = (forward (), monomorphic)
        recursive : Keep 'a => 'a -> 'a
        recursive x = helper x
        helper x = recursive x
    "#
    );
    let annotations = infer(&bump, source).unwrap();
    for name in ["boxed", "forward", "monomorphic", "recursive", "helper"] {
        assert_eq!(ordinary_context_len(annotations[name]), 1, "{name}");
    }
    assert!(annotations["use"].context.is_empty());
    assert!(annotations["useBox"].context.is_empty());
    insta::assert_snapshot!(render_annotations(&annotations));
}

#[test]
fn inferred_context_is_instantiated_independently_at_each_local_use() {
    let bump = Bump::new();
    let source = indoc!(
        r#"
        module Main exposing (..)
        trait Keep 'a where
            keep : 'a -> 'a
        impl Keep () where
            keep x = x
        type Color = Red
        impl Keep Color where
            keep x = x
        forward x = keep x
        pair = (forward (), forward Red)
    "#
    );
    let annotations = infer(&bump, source).unwrap();
    assert_eq!(annotations["forward"].context.len(), 1);
    assert!(annotations["pair"].context.is_empty());
    insta::assert_snapshot!(render_annotations(&annotations));
}

#[test]
fn nested_contexts_defer_outer_variables_and_keep_mixed_scheme_sharing() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (..)
        trait Keep 'a where
            keep : 'a -> 'a
        trait Convert 'a 'b where
            convert : 'a -> 'b
        outer x =
            let
                inner y = keep x
            in
            inner ()
        mixed x =
            let
                inner y = convert x
            in
            (inner (), inner "hello")
    "#
    );
}

#[test]
fn inferred_context_removes_duplicates_and_superclass_requirements() {
    let bump = Bump::new();
    let annotations = infer(
        &bump,
        indoc!(
            r#"
        module Main exposing (..)
        trait Base 'a where
            base : 'a -> 'a
        trait Base 'a => Strong 'a where
            strong : 'a -> 'a
        trait Strong 'a => Top 'a where
            top : 'a -> 'a
        trait Base 'b => Select 'a 'b where
            select : 'a -> 'b -> 'b
        reduced x = (base x, top x, strong (base x))
        reversed x = (top x, strong x, base x)
        distinct x y = (base x, top y, base x)
        permuted x y = (base x, select x y, base y)
    "#
        ),
    )
    .unwrap();
    assert_eq!(annotations["reduced"].context.len(), 1);
    assert_eq!(annotations["distinct"].context.len(), 2);
    assert_eq!(annotations["reversed"].context.len(), 1);
    assert_eq!(annotations["permuted"].context.len(), 2);
    insta::assert_snapshot!(render_annotations(&annotations));
}

#[test]
fn recursive_identity_cannot_change_its_argument_type() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        type Color = Red
        first x = (second x, x)
        second x = case first x of
            (previous, current) -> current
        bad : () -> Color
        bad x = second x
    "#
    );
}

#[test]
fn enclosing_given_waits_for_rank_propagation_through_case_branches() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (..)
        type Choice = First | Second
        trait Keep 'a where
            keep : 'a -> 'a
        impl Keep 'a => Keep (list 'a) where
            keep xs = xs
        f : Keep (list 'a) => list 'a -> list 'a
        f = \xs ->
            let
                g y =
                    case First of
                        First -> keep [y]
                        Second -> xs
            in
            case xs of
                [] -> []
                z :: rest -> g z
    "#
    );
}

#[test]
fn impl_context_failure_reports_the_original_call() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        type Color = Red
        trait Keep 'a where
            keep : 'a -> 'a
        impl Keep () where
            keep x = x
        impl Keep 'a => Keep (list 'a) where
            keep xs = xs
        value = keep [Red]
    "#
    );
}

#[test]
fn enclosing_given_wins_after_lambda_parameter_types_are_known() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (..)
        trait Keep 'a where
            keep : 'a -> 'a
        impl Keep 'a => Keep (list 'a) where
            keep xs = xs
        direct : Keep (list 'a) => 'a -> list 'a
        direct = \x -> keep [x]
        nested : Keep (list 'a) => 'a -> list 'a
        nested = \x ->
            let
                helper ignored = keep [x]
            in
            helper ()
    "#
    );
}

#[test]
fn expanding_impl_context_stops_with_a_diagnostic() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        trait Keep 'a where
            keep : 'a -> 'a
        impl Keep (list (list 'a)) => Keep (list 'a) where
            keep xs = xs
        value = keep [()]
    "#
    );
}

#[test]
fn nominal_record_trait_argument_has_no_impl() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        trait Keep 'a 'b where
            keep : 'a -> 'b -> 'a
        type alias item = { item : unit }
        value = keep () { item = () }
    "#
    );
}

#[test]
fn inferred_context_preserves_constructed_arguments_and_recursive_groups() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (..)
        trait Observe 'a where
            observe : 'a -> ()
        impl Observe 'a => Observe (list 'a) where
            observe xs = ()
        trait Keep 'a where
            keep : 'a -> 'a
        observeList x = observe [x]
        first x = keep (second x)
        second x = first x
    "#
    );
}

#[test]
fn qualified_annotation_keeps_context_only_types_and_reserves_their_names() {
    use nash_ast::{ModuleName, QualifiedName};
    use nash_constrain::type_::{Content, FlatType, make_descriptor, mk_flex_var};
    let bump = Bump::new();
    let mut uf = UnionFind::new();
    let home = ModuleName {
        package: None,
        name: "Main",
    };
    let result = mk_flex_var(&mut uf);
    let context_only = uf.fresh(make_descriptor(Content::FlexVar(Some("a"))));
    let list = uf.fresh(make_descriptor(Content::Structure(FlatType::App1(
        home,
        "List",
        vec![result],
    ))));
    let unit = uf.fresh(make_descriptor(Content::Structure(FlatType::App1(
        nash_ast::primitives::builtin_home(),
        "unit",
        Vec::new(),
    ))));
    let context: &[(QualifiedName<'_>, &[nash_constrain::Variable])] = &[
        (QualifiedName { home, name: "Show" }, &[list]),
        (QualifiedName { home, name: "Keep" }, &[context_only]),
        (
            QualifiedName {
                home,
                name: "Ground",
            },
            &[unit],
        ),
    ];
    let context = context
        .iter()
        .map(|(trait_, args)| nash_solve::preds::Body::Trait {
            trait_: *trait_,
            args: args.to_vec(),
            hidden: false,
        })
        .collect::<Vec<_>>();
    let annotation = nash_solve::to_annotation_with_context(&bump, &mut uf, result, &context);
    assert_eq!(annotation.free_vars, ["a", "b"]);
    assert!(matches!(annotation.typ.value, CanType::Var("b")));
    insta::assert_snapshot!(render_annotation(annotation));
}

#[test]
fn recursive_definition_metadata_preserves_names_types_and_given_variables() {
    let bump = Bump::new();
    let source = indoc!(
        "
        module Main exposing (..)
        trait Keep 'a where
            keep : 'a -> 'a
            keep x = x
        f : Keep 'a => 'a -> 'a
        f x = g x
        g x = h x
        h x = f x
    "
    );
    let parsed = nash_parse::Parser::new(&bump, source).module().unwrap();
    let canonical = nash_can::canonicalize(&bump, Context::default(), &parsed).unwrap();
    let mut definitions = std::collections::BTreeMap::new();
    let mut inferred_group_first = None;
    let mut decls = canonical.module.decls;
    loop {
        let (defs, next) = match decls {
            nash_ast::Decls::Declare { definition, next } => (vec![*definition], next),
            nash_ast::Decls::DeclareRec {
                definition,
                following,
                next,
            } => (
                std::iter::once(*definition)
                    .chain(following.iter().copied())
                    .collect(),
                next,
            ),
            nash_ast::Decls::Empty => break,
        };
        for def in defs {
            let (nash_ast::Def::Def { name, .. } | nash_ast::Def::TypedDef { name, .. }) = def;
            if matches!(def, nash_ast::Def::Def { .. }) {
                inferred_group_first.get_or_insert(nash_ast::NodeId::def(name));
            }
            definitions.insert(name.value, def);
        }
        decls = next;
    }
    let method = canonical.module.traits[0].value.methods[0].default.unwrap();
    let nash_ast::Def::TypedDef {
        name: method_name, ..
    } = method
    else {
        panic!("typed default")
    };
    definitions.insert(method_name.value, method);
    assert_eq!(
        definitions.keys().copied().collect::<Vec<_>>(),
        ["f", "g", "h", "keep"]
    );

    let mut uf = UnionFind::new();
    let (annotations, solved) =
        nash_solve::run(&bump, &mut uf, &canonical.module, &canonical.tables)
            .expect("recursive schemes and default method solve");
    assert_eq!(
        annotations.keys().copied().collect::<Vec<_>>(),
        ["f", "g", "h"]
    );
    assert!(
        !annotations.contains_key("keep"),
        "method must not be published as a lexical binding"
    );
    assert_eq!(
        solved.schemes.len(),
        4,
        "one scheme per original definition"
    );
    let group_binder = inferred_group_first.expect("two inferred recursive members");

    for (text, definition) in definitions {
        let (nash_ast::Def::Def { name, body, .. } | nash_ast::Def::TypedDef { name, body, .. }) =
            definition;
        // Lookup through the original arena name, not a rebuilt name or region.
        let node = nash_ast::NodeId::def(name);
        let scheme = &solved.schemes[&node];
        assert_eq!(
            scheme.binder,
            if text == "g" || text == "h" {
                group_binder
            } else {
                node
            }
        );
        let annotation = scheme.annotation;
        let CanType::Lambda {
            from: argument,
            to: result,
        } = annotation.typ.value
        else {
            panic!("{text}: preserve the full function type")
        };
        let (CanType::Var(argument), CanType::Var(result)) = (&argument.value, &result.value)
        else {
            panic!("{text}: polymorphic identity arguments")
        };
        assert_eq!(
            argument, result,
            "{text}: input and output share one quantified variable"
        );
        assert_eq!(
            annotation.free_vars,
            [*argument],
            "{text}: preserve its quantified variable"
        );
        let [predicate] = annotation.context else {
            panic!("{text}: retain the declared or inferred Keep context")
        };
        assert_eq!(predicate.trait_ref().expect("Keep predicate").name, "Keep");
        let [context_argument] = predicate.args() else {
            panic!("unary Keep")
        };
        assert!(
            matches!(context_argument.value, CanType::Var(variable) if variable == *argument),
            "{text}: predicate, argument, and result share the signature substitution"
        );
        if text != "keep" {
            let nash_ast::Expr::Call { function, .. } = body.value else {
                panic!("recursive call")
            };
            let instance = &solved.instances[&nash_ast::NodeId::expr(function)];
            let [nash_ast::Evidence::Given { binder, index: 0 }] = instance.evidence else {
                panic!("{text}: recursive call must use the enclosing context slot")
            };
            assert_eq!(
                *binder, scheme.binder,
                "{text}: recursive evidence uses the final owning binder"
            );
        }
    }
}

#[test]
fn trait_default_body_must_match_its_annotation() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        trait Keep 'a where
            keep : 'a -> 'a
            keep x = ()
    "#
    );
}

#[test]
fn impl_body_must_match_its_specialized_annotation() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        type Color = Red
        trait Keep 'a where
            keep : 'a -> 'a
        impl Keep Color where
            keep x = ()
    "#
    );
}

#[test]
fn impl_body_requires_its_element_constraint() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        type Box 'a = Box 'a
        trait Keep 'a where
            keep : 'a -> 'a
        impl Keep (Box 'a) where
            keep (Box x) = Box (keep x)
    "#
    );
}

#[test]
fn methods_see_module_helpers_without_becoming_module_bindings() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (..)
        type Color = Red
        trait Keep 'a where
            keep : 'a -> 'a
            keep x = helper x
        impl Keep Color where
            keep x = helper x
        helper x = x
    "#
    );
}

#[test]
fn int_literal() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (main)

        main = 42
    "#
    );
}

#[test]
fn type_variable_names_do_not_imply_constraints() {
    let bump = Bump::new();
    for name in [
        "number",
        "comparable",
        "appendable",
        "compappend",
        "numberish",
        "comparableish",
        "appendableish",
        "compappendish",
    ] {
        let source = format!(
            "module Main exposing (..)\nidentity : '{name} -> '{name}\nidentity x = x\nmain = identity ()\n"
        );
        let annotations = infer(&bump, &source).expect("ordinary type variable accepts unit");
        assert!(annotations["identity"].context.is_empty());
        assert!((annotations["main"].typ.value == CanType::unit()));
    }
}

#[test]
fn negation_retains_num_evidence() {
    let bump = Bump::new();
    let mut interfaces = literal_interfaces(&bump);
    let num = nash_parse::Parser::new(
        &bump,
        "module Num exposing (Num)\ntrait Num 'a where\n    negate : 'a -> 'a\n",
    )
    .module()
    .unwrap();
    let num = nash_can::canonicalize(
        &bump,
        Context {
            package: Some(nash_ast::primitives::CORE),
            interfaces: None,
        },
        &num,
    )
    .unwrap();
    interfaces.insert(
        "Num",
        nash_can::from_module(&bump, &num.module, &Default::default()),
    );
    let source = bump.alloc_str("module Main exposing (..)\nimport Num as N\nimport Literal exposing (..)\nnegate x = x\nflip x = -x\nnegative = -7\n");
    let parsed = nash_parse::Parser::new(&bump, source).module().unwrap();
    let can = nash_can::canonicalize(
        &bump,
        Context {
            package: None,
            interfaces: Some(&interfaces),
        },
        &parsed,
    )
    .unwrap();
    let mut uf = UnionFind::new();
    let module = &can.module;
    let (annotations, solved) = nash_solve::run(&bump, &mut uf, module, &can.tables).unwrap();
    let [predicate] = annotations["flip"].context else {
        panic!("Num constraint")
    };
    assert_eq!(
        predicate.trait_ref().unwrap().home.package,
        Some(nash_ast::primitives::CORE)
    );
    assert_eq!(predicate.trait_ref().unwrap().home.name, "Num");
    assert_eq!(predicate.trait_ref().unwrap().name, "Num");
    let mut decls = can.module.decls;
    while let nash_ast::Decls::Declare { definition, next } = decls {
        let nash_ast::Def::Def { name, body, .. } = definition else {
            panic!("inferred definition")
        };
        if name.value == "negate" {
            decls = next;
            continue;
        }
        let nash_ast::Expr::Call {
            function,
            arguments: [_],
        } = body.value
        else {
            panic!("ordinary method call")
        };
        assert!(
            matches!(function.value, nash_ast::Expr::VarMethod { trait_, method: "negate", .. } if trait_ == predicate.trait_ref().unwrap())
        );
        let instance = &solved.instances[&nash_ast::NodeId::expr(function)];
        let [nash_ast::Evidence::Given { binder, index }] = instance.evidence else {
            panic!("Num given")
        };
        let scheme = &solved.schemes[&nash_ast::NodeId::def(name)];
        assert_eq!(*binder, scheme.binder);
        assert_eq!(
            scheme.annotation.context[usize::from(*index)]
                .trait_ref()
                .unwrap()
                .name,
            "Num"
        );
        assert_eq!(instance.type_args.len(), 1);
        decls = next;
    }
    insta::assert_snapshot!(render_annotations(&annotations));
}

#[test]
fn string_literal() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (greeting)

        greeting = "hello"
    "#
    );
}

#[test]
fn literal_syntax_records_impls_and_pattern_givens() {
    let bump = Bump::new();
    let mut interfaces = literal_interfaces(&bump);
    let eq_source = bump.alloc_str("module Eq exposing (..)\nimport Builtin exposing (..)\ntrait Eq 'a where eq : 'a -> 'a -> bool\n");
    let eq_module = nash_parse::Parser::new(&bump, eq_source).module().unwrap();
    let eq = nash_can::canonicalize(
        &bump,
        Context {
            package: Some(nash_ast::primitives::CORE),
            interfaces: Some(&interfaces),
        },
        &eq_module,
    )
    .unwrap();
    interfaces.insert(
        "Eq",
        nash_can::from_module(&bump, &eq.module, &Default::default()),
    );
    for (primitive, literal, trait_name) in [
        ("int", "7", "FromInt"),
        ("string", "\"nash\"", "FromString"),
        ("bytes", "#\"00ff\"", "FromBytes"),
    ] {
        let input = bump.alloc_str(&format!("module Main exposing (..)\nimport Builtin exposing (..)\nfixed : {primitive}\nfixed = {literal}\nmatch value =\n    case value of\n        {literal} -> ()\n        _ -> ()\n"));
        let parsed = nash_parse::Parser::new(&bump, input).module().unwrap();
        let can = nash_can::canonicalize(
            &bump,
            Context {
                package: None,
                interfaces: Some(&interfaces),
            },
            &parsed,
        )
        .unwrap();
        let mut uf = UnionFind::new();
        let module = &can.module;
        let (annotations, solved) = nash_solve::run(&bump, &mut uf, module, &can.tables).unwrap();
        let mut decls = can.module.decls;
        while let nash_ast::Decls::Declare { definition, next } = decls {
            match definition {
                nash_ast::Def::TypedDef { body, .. } => {
                    let instance = &solved.instances[&nash_ast::NodeId::expr(body)];
                    let [nash_ast::Evidence::Impl { impl_, .. }] = instance.evidence else {
                        panic!("literal impl evidence")
                    };
                    assert_eq!(impl_.home.package, Some(nash_ast::primitives::CORE));
                    assert_eq!(impl_.key.trait_.name, trait_name);
                }
                nash_ast::Def::Def { name, body, .. } => {
                    let nash_ast::Expr::Case { branches, .. } = body.value else {
                        panic!("literal pattern")
                    };
                    let instance =
                        &solved.instances[&nash_ast::NodeId::pattern(branches[0].pattern)];
                    assert_eq!(instance.evidence.len(), 2);
                    let scheme = &solved.schemes[&nash_ast::NodeId::def(name)];
                    for (evidence, expected) in instance.evidence.iter().zip([trait_name, "Eq"]) {
                        let nash_ast::Evidence::Given { binder, index } = evidence else {
                            panic!("pattern given")
                        };
                        assert_eq!(*binder, scheme.binder);
                        assert_eq!(
                            scheme.annotation.context[usize::from(*index)]
                                .trait_ref()
                                .unwrap()
                                .name,
                            expected
                        );
                    }
                }
            }
            decls = next;
        }
        insta::assert_snapshot!(
            format!("literal_syntax_{primitive}"),
            render_annotations(&annotations)
        );
    }
}

#[test]
fn unit_value() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (nothing)

        nothing = ()
    "#
    );
}

#[test]
fn tuple_value() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (pair)

        pair = ( 1, "two" )
    "#
    );
}

#[test]
fn user_twins_preserve_local_imported_and_pattern_identity() {
    let bump = Bump::new();
    let mut interfaces = std::collections::BTreeMap::new();
    for source in [
        "module Status exposing (..)\ntype Status = Ready | Waiting\ntype status = Ready | Waiting\ntype payload 'a = Payload int 'a\ntype Payload 'a = Payload Int 'a\nsmallPayload x y = Payload x y\nbigPayload x y = Status.Payload x y\nreadSmall (Payload x y) = (x, y)\nreadBig (Status.Payload x y) = (x, y)\nlittle = Ready\nbig = Status.Ready\nlocalLittle x = case x of\n    Ready -> ()\n    Waiting -> ()\nlocalBig x = case x of\n    Status.Ready -> ()\n    Status.Waiting -> ()\n",
        "module Main exposing (..)\nimport Status as S exposing (..)\nsmallPayload x y = Payload x y\nbigPayload x y = S.Payload x y\nreadSmall (Payload x y) = (x, y)\nreadBig (S.Payload x y) = (x, y)\nlittleUse = Ready\nbigUse = S.Ready\nlittlePattern x = case x of\n    Ready -> ()\n    Waiting -> ()\nbigPattern x = case x of\n    S.Ready -> ()\n    S.Waiting -> ()\n",
    ] {
        let source = bump.alloc_str(source);
        let parsed = nash_parse::Parser::new(&bump, source).module().unwrap();
        let canonical = nash_can::canonicalize(
            &bump,
            Context {
                package: None,
                interfaces: Some(&interfaces),
            },
            &parsed,
        )
        .unwrap();
        let mut uf = UnionFind::new();
        let module = &canonical.module;
        let (annotations, _) = nash_solve::run(&bump, &mut uf, module, &canonical.tables).unwrap();
        insta::assert_snapshot!(
            format!("user_twins_{}", canonical.module.name.name),
            render_annotations(&annotations)
        );
        interfaces.insert(
            canonical.module.name.name,
            nash_can::from_module(&bump, &canonical.module, &annotations),
        );
    }
}

#[test]
fn twin_constructor_does_not_follow_the_expected_type() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        type status = Ready
        type Status = Ready
        wrong : Status
        wrong = Ready
    "#
    );
}

#[test]
fn tuple_tail_survives_patterns_annotations_and_instantiation() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (..)

        rotate (a, b, c, d, e) = (e, a, b, c, d)
        keep : ('a, 'b, 'c, 'd, 'e) -> ('a, 'b, 'c, 'd, 'e)
        keep value = value
        result = (keep ((), (), (), 1, "tail"), rotate ((), (), (), "four", 5))
    "#
    );
}

#[test]
fn tuple_fourth_component_mismatch() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        value : (unit, unit, unit, unit)
        value = ((), (), (), \x -> x)
    "#
    );
}

#[test]
fn tuple_arity_mismatch() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        value : (unit, unit, unit, unit)
        value = ((), (), (), (), ())
    "#
    );
}

#[test]
fn tuple_tail_occurs_check() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        loop x = x ((), (), (), x)
    "#
    );
}

#[test]
fn list_of_numbers() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (numbers)

        numbers = [ 1, 2, 3 ]
    "#
    );
}

#[test]
fn record_literal() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (point)

        type alias point = { x : int, y : int }
        keep value = value
        point = keep { x = 1, y = 2 }
    "#
    );
}

// FUNCTIONS

#[test]
fn identity_function() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (id)

        id x = x
    "#
    );
}

#[test]
fn const_function() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (always)

        always x y = x
    "#
    );
}

#[test]
fn apply_function() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (apply)

        apply f x = f x
    "#
    );
}

#[test]
fn compose_lambdas() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (compose)

        compose f g = \x -> g (f x)
    "#
    );
}

#[test]
fn function_application_pins_types() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (id, main)

        id x = x

        main = id 42
    "#
    );
}

// LET

#[test]
fn let_bound_function() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (main)

        main =
            let
                f x = x
            in
            f 42
    "#
    );
}

#[test]
fn let_polymorphism() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (main)

        main =
            let
                id x = x
            in
            ( id 1, id "one" )
    "#
    );
}

#[test]
fn let_destructure() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (main)

        main =
            let
                ( a, b ) = ( 1, "two" )
            in
            b
    "#
    );
}

#[test]
fn destructured_bindings_preserve_contexts_and_polymorphism() {
    let bump = Bump::new();
    let input = bump.alloc_str(indoc!(
        r#"
        module Main exposing (..)
        main =
            let
                (identity, unused) = (\x -> x, \y -> y)
            in
            (identity (), identity (\x -> x))
    "#
    ));
    let parsed = nash_parse::Parser::new(&bump, input).module().unwrap();
    let can = nash_can::canonicalize(&bump, Context::default(), &parsed).unwrap();
    let mut uf = UnionFind::new();
    let module = &can.module;
    let (polymorphic, solved) = nash_solve::run(&bump, &mut uf, module, &can.tables)
        .expect("destructured functions remain polymorphic");
    assert!(polymorphic["main"].context.is_empty());
    let nash_ast::Decls::Declare { definition, .. } = can.module.decls else {
        panic!("main")
    };
    let nash_ast::Def::Def { body, .. } = definition else {
        panic!("inferred main")
    };
    let nash_ast::Expr::LetDestruct { pattern, body, .. } = body.value else {
        panic!("destructure")
    };
    let node = nash_ast::NodeId::pattern(pattern);
    let scheme = &solved.schemes[&node];
    assert_eq!(scheme.binder, node);
    assert_eq!(
        scheme.annotation.free_vars.len(),
        2,
        "both aggregate quantifiers are retained"
    );
    let nash_ast::Expr::Tuple { first, second, .. } = body.value else {
        panic!("two uses")
    };
    for call in [first, second] {
        let nash_ast::Expr::Call { function, .. } = call.value else {
            panic!("identity call")
        };
        let instance = &solved.instances[&nash_ast::NodeId::expr(function)];
        assert_eq!(
            instance.type_args.len(),
            2,
            "even the unused component's quantifier is instantiated"
        );
        assert!(instance.evidence.is_empty());
    }
    let invalid = infer(
        &bump,
        indoc!(
            r#"
        module Main exposing (..)
        main =
            let
                (unused, value) = ((), 7)
            in
            value ()
    "#
        ),
    )
    .expect_err("using a destructured literal must preserve its FromInt requirement");
    assert!(
        invalid.iter().any(|error| matches!(error,
            Error::MissingImpl { trait_, .. } if trait_.name == "FromInt"
        )),
        "{invalid:?}"
    );
}

// IF

#[test]
fn if_picks_branch_type() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (pick)

        pick b x y = if b then x else y
    "#
    );
}

// UNIONS, CASE, AND PATTERNS

#[test]
fn union_constructors() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (Maybe(..), just, nothing)

        type Maybe 'a
            = Just 'a
            | Nothing

        just = Just

        nothing = Nothing
    "#
    );
}

#[test]
fn case_with_default() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (Maybe(..), withDefault)

        type Maybe 'a
            = Just 'a
            | Nothing

        withDefault default maybe =
            case maybe of
                Just value ->
                    value

                Nothing ->
                    default
    "#
    );
}

#[test]
fn tuple_pattern_arg() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (swap)

        swap ( a, b ) = ( b, a )
    "#
    );
}

#[test]
fn cons_pattern() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (head)

        head fallback list =
            case list of
                first :: rest ->
                    first

                [] ->
                    fallback
    "#
    );
}

// RECORD ACCESS AND UPDATE

#[test]
fn record_access_needs_known_type() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (getX)

        getX r = r.x
    "#
    );
}

#[test]
fn record_accessor_needs_known_type() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (getName)

        getName = .name
    "#
    );
}

#[test]
fn record_update_needs_known_type() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (bump)

        bump r = { r | x = 1 }
    "#
    );
}

// TYPE ANNOTATIONS

#[test]
fn typed_identity() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (id)

        id : 'a -> 'a
        id x = x
    "#
    );
}

#[test]
fn typed_union_function() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (Shape(..), rotate)

        type Shape
            = Circle
            | Square

        rotate : Shape -> Shape
        rotate shape = shape
    "#
    );
}

#[test]
fn nominal_records_do_not_unify() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        type alias point = { x : int }
        type alias other = { x : int }
        f : point -> other
        f r = r
    "#
    );
}

#[test]
fn transparent_record_aliases_unify() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (..)
        type alias point = { x : int }
        type alias wrapped = point
        type alias identity 'a = 'a
        f : point -> wrapped
        f r = r
        g : wrapped -> point
        g r = r
        h : identity point -> point
        h r = r
        i : point -> identity point
        i r = r
    "#
    );
}

#[test]
fn transparent_wrappers_preserve_distinct_record_identity() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        type alias point = { x : int }
        type alias other = { x : int }
        type alias wrapped = point
        type alias wrappedOther = other
        f : wrapped -> wrappedOther
        f r = r
    "#
    );
}

#[test]
fn parameterized_transparent_alias_preserves_record_identity() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        type alias point = { x : int }
        type alias other = { x : int }
        type alias identity 'a = 'a
        f : identity point -> identity other
        f r = r
    "#
    );
}

#[test]
fn typed_alias_function() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (Point, getX)

        type alias Point =
            { x : Int }

        type Int
            = Int

        getX : Point -> Int
        getX point = point.x
    "#
    );
}

// RECURSION

#[test]
fn recursive_function() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (forever)

        forever x = forever x
    "#
    );
}

#[test]
fn mutual_recursion() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (ping, pong)

        ping x = pong x

        pong x = ping x
    "#
    );
}

// ERRORS

#[test]
fn if_condition_must_be_bool() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (main)

        main = if 1 then 2 else 3
    "#
    );
}

#[test]
fn mixed_literal_branches_retain_both_traits() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (pick)

        pick b = if b then 1 else "two"
    "#
    );
}

#[test]
fn rigid_vars_do_not_unify() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (cast)

        cast : 'a -> 'b
        cast x = x
    "#
    );
}

#[test]
fn local_annotation_cannot_generalize_an_outer_argument() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        outer x =
            let
                inner : 'a
                inner = x
            in
            inner
    "#
    );
}

#[test]
fn infinite_type() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (selfApply)

        selfApply f = f f
    "#
    );
}

#[test]
fn string_literal_requires_an_impl_for_the_result_type() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (Msg(..), broken)

        type Msg
            = Ping

        broken : Msg
        broken = "not a msg"
    "#
    );
}

#[test]
fn operator_methods_preserve_provider_and_backing_method() {
    let bump = Bump::new();
    let mut interfaces = std::collections::BTreeMap::new();
    let sources = [
        (
            "Methods",
            "module Methods exposing (..)\ninfix left 5 (<+>) = select\ntrait Select 'a where\n    select : 'a -> 'a -> 'a\nimpl Select () where\n    select x _ = x\nplain : 'a -> 'a -> 'a\nplain x _ = x\n",
        ),
        (
            "Operators",
            "module Operators exposing ((<*>), (<|>))\nimport Methods exposing (Select, plain)\ninfix left 5 (<*>) = select\ninfix left 5 (<|>) = plain\n",
        ),
        (
            "Main",
            "module Main exposing (..)\nimport Methods exposing ((<+>))\nimport Operators exposing ((<*>))\npick x y = x <*> y\nleft = () <+> ()\nright = () <*> ()\nsection = (() <*>)\noperator = (<*>)\n",
        ),
        (
            "OnlyOperators",
            "module OnlyOperators exposing (..)\nimport Operators exposing ((<*>), (<|>))\nvalue = () <*> ()\nordinary = () <|> ()\n",
        ),
    ];
    let mut output = Vec::new();
    for (name, source) in sources {
        let module = nash_parse::Parser::new(&bump, source).module().unwrap();
        let canonical = nash_can::canonicalize(
            &bump,
            Context {
                package: None,
                interfaces: Some(&interfaces),
            },
            &module,
        )
        .unwrap();
        assert!(
            canonical.warnings.is_empty(),
            "{name}: {:?}",
            canonical.warnings
        );
        let mut uf = UnionFind::new();
        let module = &canonical.module;
        let (annotations, solved) =
            nash_solve::run(&bump, &mut uf, module, &canonical.tables).unwrap();
        let interface = nash_can::from_module(&bump, &canonical.module, &annotations);
        for binop in interface.binops {
            assert_eq!(binop.function.home.name, "Methods");
            if binop.symbol == "<|>" {
                assert_eq!(binop.function.name, "plain");
                assert!(binop.annotation.context.is_empty());
            } else {
                assert_eq!(binop.function.name, "select");
                assert_eq!(
                    binop.annotation.context[0].trait_ref().unwrap().home.name,
                    "Methods"
                );
            }
        }
        if name == "Main" || name == "OnlyOperators" {
            let mut decls = canonical.module.decls;
            let mut checked = 0;
            while let nash_ast::Decls::Declare { definition, next } = decls {
                let nash_ast::Def::Def { name, body, .. } = definition else {
                    panic!("inferred operator use")
                };
                let body = if let nash_ast::Expr::Lambda { body, .. } = body.value {
                    body
                } else {
                    body
                };
                let (operator_home, reference) = match body.value {
                    nash_ast::Expr::Binop {
                        operator_home,
                        reference,
                        ..
                    }
                    | nash_ast::Expr::VarOperator {
                        operator_home,
                        reference,
                        ..
                    } => (operator_home, reference),
                    _ => panic!("operator use or section body"),
                };
                assert_eq!(reference.home.name, "Methods");
                let ordinary = name.value == "ordinary";
                assert_eq!(reference.name, if ordinary { "plain" } else { "select" });
                assert_eq!(
                    operator_home.name,
                    if name.value == "left" {
                        "Methods"
                    } else {
                        "Operators"
                    }
                );
                assert_eq!(
                    solved.instances[&nash_ast::NodeId::expr(body)]
                        .evidence
                        .len(),
                    if ordinary { 0 } else { 1 }
                );
                checked += 1;
                decls = next;
            }
            assert_eq!(checked, if name == "Main" { 5 } else { 2 });
            assert_eq!(solved.instances.values().filter(|instance| matches!(instance.evidence, [nash_ast::Evidence::Impl { impl_, .. }] if impl_.home.name == "Methods" && impl_.key.trait_.name == "Select")).count(), if name == "Main" { 3 } else { 1 });
            output.push(format!("{name}:\n{}", render_annotations(&annotations)));
        }
        interfaces.insert(name, interface);
    }
    insta::assert_snapshot!(output.join("\n"));
}

#[test]
fn builtin_value_schemes_preserve_container_contexts() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (..)
        import Builtin exposing (type unit)
        tag data = Builtin.fstPair (Builtin.unConstrData data)
        fields data = Builtin.sndPair (Builtin.unConstrData data)
        cons x xs = Builtin.mkCons x xs
        choose flag x y = Builtin.ifThenElse flag x y
        unit x = Builtin.chooseUnit () x
        namedUnit : unit
        namedUnit = ()
        trait Keep 'a where
            keep : 'a -> 'a
        impl Keep unit where
            keep x = Builtin.chooseUnit x ()
        kept = keep ()
    "#
    );
}

#[test]
fn builtin_list_rejects_function_elements() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        import Builtin
        bad xs = Builtin.mkCons (\x -> x) xs
    "#
    );
}

#[test]
fn abstract_map_rejects_non_storable_builtin_list_results() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        import Builtin
        trait Functor 'f where
            map : ('a -> 'b) -> 'f 'a -> 'f 'b
        impl Functor list where
            map f xs =
                case xs of
                    [] -> []
                    x :: rest -> Builtin.mkCons (f x) (map f rest)
        bad = map (\x -> (x, x)) [()]
    "#
    );
}

#[test]
fn inline_representation_predicates_survive_aliases_and_function_annotations() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (..)
        type alias Record 'a = ({ field : ('a : Big) } : Big)
        wrap : (('a : Big) -> Record 'a : Term)
        wrap x = Record x
        identity : ('a : Big) -> 'a
        identity x = x
        preserve : Record 'a -> Record 'a
        preserve x = identity x
        type Color = Red
        concrete = wrap Red
        concrete2 = preserve concrete
    "#
    );
}

#[test]
fn inline_representation_predicate_rejects_const_at_a_big_use() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        identity : ('a : Big) -> 'a
        identity x = x
        rejected = identity ()
    "#
    );
}

#[test]
fn list_data_rejects_const_elements() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        import Builtin
        badList = Builtin.listData [()]
    "#
    );
}

#[test]
fn recursive_impl_heads_select_disjoint_concrete_arguments() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (..)
        trait Keep 'a where
            keep : 'a -> 'a
        impl Keep (list int) where
            keep x = x
        impl Keep (list bytes) where
            keep x = x
        integers : list int -> list int
        integers = keep
        byteStrings : list bytes -> list bytes
        byteStrings = keep
    "#
    );
}

#[test]
fn repeated_impl_variables_wait_without_equating_inferred_types() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (..)
        type pairish 'a 'b = Both 'a 'b
        trait Keep 'a where
            keep : 'a -> 'a
        impl Keep (pairish 'a 'a) where
            keep x = x
        route x y = keep (Both x y)
        accepted = route () ()
    "#
    );
}

#[test]
fn repeated_impl_variables_reject_distinct_inferred_types() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        type pairish 'a 'b = Both 'a 'b
        trait Keep 'a where
            keep : 'a -> 'a
        impl Keep (pairish 'a 'a) where
            keep x = x
        route x y = keep (Both x y)
        rejected = route () ((), ())
    "#
    );
}

#[test]
fn list_literal_rejects_function_elements() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        bad = [\x -> x]
    "#
    );
}

#[test]
fn builtin_constructors_match_conditions_and_data_fields() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (..)
        import Builtin exposing (type bool(..), Data(..))
        choice flag = if flag then True else False
        invert flag =
            case flag of
                False -> True
                True -> False
        rewrap data =
            case data of
                Constr tag fields -> Constr tag fields
                Map pairs -> Map pairs
                List values -> List values
                I n -> I n
                B payload -> B payload
        integer n = I n
        bytes b = B b
    "#
    );
}

#[test]
fn source_basics_bool_is_an_ordinary_union() {
    assert_inference_snapshot!(
        r#"
        module Basics exposing (..)
        type Bool = False | True
        invert flag =
            case flag of
                False -> True
                True -> False
    "#
    );
}

#[test]
fn nested_operator_sections_apply() {
    let bump = Bump::new();
    let operators = "module Operators exposing (..)\n\ninfix left 6 (+) = first\n\nfirst x y = x\n";
    let annotations = infer(&bump, operators).expect("operator module infers");
    let source = bump.alloc_str(operators);
    let module = nash_parse::Parser::new(&bump, source).module().unwrap();
    let canonical = nash_can::canonicalize(&bump, Context::default(), &module).unwrap();
    let interface = nash_can::from_module(&bump, &canonical.module, &annotations);
    let mut interfaces = literal_interfaces(&bump);
    interfaces.insert("Operators", interface);
    let input = indoc!(
        r#"
        module Main exposing (..)

        import Operators exposing (..)

        right = (+ ((+ 1) 2)) "right"
        left = (((() +) 2) +) "left"
    "#
    );
    let source = bump.alloc_str(input);
    let module = nash_parse::Parser::new(&bump, source).module().unwrap();
    let canonical = nash_can::canonicalize(
        &bump,
        Context {
            package: None,
            interfaces: Some(&interfaces),
        },
        &module,
    )
    .expect("nested sections canonicalize");
    let mut uf = UnionFind::new();
    let module = &canonical.module;
    let (annotations, _) =
        nash_solve::run(&bump, &mut uf, module, &canonical.tables).expect("nested sections infer");
    let rendered = render_annotations(&annotations);
    assert!(rendered.contains("FromString a => a"), "{rendered}");
    assert!(rendered.contains("left : unit"), "{rendered}");
    insta::assert_snapshot!(rendered);
}

#[test]
fn solved_alias_retains_its_closed_parameterized_body() {
    let bump = Bump::new();
    let annotations = infer(
        &bump,
        indoc!(
            r#"
        module Main exposing (..)
        type Color = Red | Blue
        type alias Pair 'left 'right = { first : 'left, second : 'right }
        pair : Pair Color Color
        pair = Pair Red Blue
    "#
        ),
    )
    .unwrap();
    let pair = annotations.get("pair").unwrap();
    let CanType::Alias {
        target: nash_ast::AliasType::Filled { body, typ },
        arguments,
        ..
    } = &pair.typ.value
    else {
        panic!("solved nominal alias")
    };
    assert_eq!(arguments.len(), 2);
    let CanType::Record { fields, .. } = &body.value else {
        panic!("closed record body")
    };
    assert!(matches!(fields[0].typ.value, CanType::Var("left")));
    assert!(matches!(fields[1].typ.value, CanType::Var("right")));
    let CanType::Record { fields, .. } = &typ.value else {
        panic!("instantiated record body")
    };
    assert!(fields.iter().all(|field| matches!(&field.typ.value, CanType::Named { reference, .. } if reference.name == "Color")));
    insta::assert_snapshot!(render_annotations(&annotations));
}

#[test]
fn higher_kinded_partial_alias_retains_its_nominal_impl() {
    let bump = Bump::new();
    let mut interfaces = std::collections::BTreeMap::new();
    let mut output = Vec::new();
    for (module_name, source) in [
        (
            "Higher",
            indoc!(
                r#"
            module Higher exposing (..)
            type alias Pair 'left 'right = { first : 'left, second : 'right }
            trait Keep 'f where
                keep : 'f 'a -> 'f 'a
            impl Keep (Pair 'a) where
                keep x = x
            through x = keep x
        "#
            ),
        ),
        (
            "Main",
            indoc!(
                r#"
            module Main exposing (..)
            import Higher exposing (..)
            type Color = Red | Blue
            type Mood = Calm | Busy
            pair : Pair Color Color
            pair = { first = Red, second = Blue }
            other : Pair Mood Color
            other = Pair Calm Red
            kept = through pair
            keptOther = through other
        "#
            ),
        ),
        (
            "Reject",
            indoc!(
                r#"
            module Reject exposing (..)
            import Higher exposing (..)
            type Color = Red | Blue
            type alias Other 'left 'right = { first : 'left, second : 'right }
            bad = through (Other Red Blue)
        "#
            ),
        ),
    ] {
        let source = bump.alloc_str(source);
        let parsed = nash_parse::Parser::new(&bump, source).module().unwrap();
        let canonical = nash_can::canonicalize(
            &bump,
            Context {
                package: None,
                interfaces: Some(&interfaces),
            },
            &parsed,
        )
        .unwrap();
        let mut uf = UnionFind::new();
        let module = &canonical.module;
        let result = nash_solve::run(&bump, &mut uf, module, &canonical.tables);
        if module_name == "Reject" {
            let errors =
                result.expect_err("structurally identical aliases retain distinct impl heads");
            assert!(
                matches!(&errors[..], [Error::MissingImpl { trait_, .. }] if trait_.name == "Keep")
            );
            insta::assert_debug_snapshot!("partial_alias_nominal_mismatch", errors);
            continue;
        }
        let (annotations, solved) = result.unwrap();
        if module_name == "Main" {
            let mut declarations = canonical.module.decls;
            let mut checked = 0;
            while let nash_ast::Decls::Declare { definition, next } = declarations {
                if let nash_ast::Def::Def { name, body, .. } = definition {
                    let expected = match name.value {
                        "kept" => Some("Color"),
                        "keptOther" => Some("Mood"),
                        _ => None,
                    };
                    if let Some(expected) = expected {
                        let nash_ast::Expr::Call { function, .. } = body.value else {
                            panic!("through call")
                        };
                        let instance = &solved.instances[&nash_ast::NodeId::expr(function)];
                        let [
                            nash_ast::Evidence::Impl {
                                impl_,
                                type_args,
                                args,
                            },
                        ] = instance.evidence
                        else {
                            panic!("alias impl evidence")
                        };
                        assert_eq!(impl_.key.trait_.name, "Keep");
                        let [
                            nash_ast::Head::Named {
                                reference: head, ..
                            },
                        ] = impl_.key.heads
                        else {
                            panic!("nominal alias head")
                        };
                        assert_eq!((head.home.name, head.name), ("Higher", "Pair"));
                        assert!(
                            args.iter()
                                .all(|arg| matches!(arg, nash_ast::Evidence::Repr { .. }))
                        );
                        assert!(
                            matches!(type_args, [typ] if matches!(&typ.value, CanType::Named { reference, .. } if reference.name == expected))
                        );
                        assert!(instance.type_args.iter().any(|typ| matches!(&typ.value, CanType::Alias { reference, arguments, remaining, .. } if reference.name == "Pair" && arguments.len() == 1 && *remaining == ["right"])));
                        checked += 1;
                    }
                }
                declarations = next;
            }
            assert_eq!(checked, 2);
        }
        output.push(format!(
            "{module_name}:\n{}",
            render_annotations(&annotations)
        ));
        interfaces.insert(
            module_name,
            nash_can::from_module(&bump, &canonical.module, &annotations),
        );
    }
    insta::assert_snapshot!(output.join("\n"));
}

#[test]
fn datatype_context_is_enforced_at_an_inferred_call_site() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        import Builtin exposing (..)
        type option 'a = None | Some 'a
        first : 'a -> list 'a -> 'a
        first x xs = x
        bad xs = first (Some ()) xs
    "#
    );
}

#[test]
fn inferred_wrapper_preserves_the_callees_representation_requirement() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        import Builtin exposing (..)
        type option 'a = None | Some 'a
        first : 'a -> list 'a -> 'a
        first x xs = x
        wrapper x xs = first x xs
        bad xs = wrapper (Some ()) xs
    "#
    );
}

#[test]
fn imported_values_retain_declared_and_inferred_representation_contexts() {
    let bump = Bump::new();
    let mut interfaces = literal_interfaces(&bump);
    let source = bump.alloc_str(indoc!(
        "
        module Source exposing (first, wrapper)
        import Builtin exposing (..)
        first : 'a -> list 'a -> 'a
        first x xs = wrapper x xs
        wrapper x xs = first x xs
    "
    ));
    let module = nash_parse::Parser::new(&bump, source).module().unwrap();
    let canonical = nash_can::canonicalize(
        &bump,
        Context {
            package: None,
            interfaces: Some(&interfaces),
        },
        &module,
    )
    .unwrap();
    let mut uf = UnionFind::new();
    let module = &canonical.module;
    let (annotations, solved) = nash_solve::run(&bump, &mut uf, module, &canonical.tables).unwrap();
    let context = annotations["first"].context;
    assert!(
        matches!(context, [pred] if pred.trait_ref() == Some(nash_ast::primitives::ReprTrait::Storable.qualified()))
    );
    assert_eq!(
        annotations["wrapper"]
            .context
            .iter()
            .map(|p| p.key())
            .collect::<Vec<_>>(),
        context.iter().map(|p| p.key()).collect::<Vec<_>>()
    );
    assert!(solved.schemes.values().all(|scheme| {
        scheme
            .annotation
            .context
            .iter()
            .any(|p| p.trait_ref() == Some(nash_ast::primitives::ReprTrait::Storable.qualified()))
    }));
    interfaces.insert(
        "Source",
        nash_can::from_module(&bump, &canonical.module, &annotations),
    );
    let mut results = Vec::new();
    for name in ["first", "wrapper"] {
        let source = bump.alloc_str(&format!(
            indoc!(
                "
        module Main exposing (..)
        import Source exposing ({name})
        type option 'a = Some 'a
        bad xs = {name} (Some ()) xs
    "
            ),
            name = name
        ));
        let module = nash_parse::Parser::new(&bump, source).module().unwrap();
        let canonical = nash_can::canonicalize(
            &bump,
            Context {
                package: None,
                interfaces: Some(&interfaces),
            },
            &module,
        )
        .unwrap();
        let mut uf = UnionFind::new();
        let module = &canonical.module;
        let errors = nash_solve::run(&bump, &mut uf, module, &canonical.tables).unwrap_err();
        assert!(
            errors.iter().all(|error| matches!(error, Error::MissingImpl { trait_, .. } if *trait_ == nash_ast::primitives::ReprTrait::Storable.qualified())) && errors.iter().any(|error| matches!(error, Error::MissingImpl { name: actual, .. } if *actual == name)),
            "{errors:?}"
        );
        results.push((name, errors));
    }
    insta::assert_snapshot!(format!(
        "{}\n{results:#?}",
        render_annotations(&annotations)
    ));
}

#[test]
fn do_infers_monad() {
    let bump = Bump::new();
    let source = indoc!(
        r#"
        module Monad exposing (..)
        trait Functor 'f where
            map : ('a -> 'b) -> 'f 'a -> 'f 'b
        trait Functor 'f => Applicative 'f where
            pure : 'a -> 'f 'a
            apply : 'f ('a -> 'b) -> 'f 'a -> 'f 'b
        trait Applicative 'm => Monad 'm where
            bind : 'm 'a -> ('a -> 'm 'b) -> 'm 'b
    "#
    );
    let module = nash_parse::Parser::new(&bump, source).module().unwrap();
    let canonical = nash_can::canonicalize(
        &bump,
        Context {
            package: Some(nash_ast::primitives::CORE),
            interfaces: None,
        },
        &module,
    )
    .unwrap();
    let interfaces = std::collections::BTreeMap::from([(
        "Monad",
        nash_can::from_module(&bump, &canonical.module, &Default::default()),
    )]);
    let source = indoc!(
        r#"
        module Main exposing (..)
        import Monad exposing (..)
        run m = do
            x <- m
            y <- m
            pure (x, y)
        expanded m = bind m (\x -> bind m (\y -> pure (x, y)))
    "#
    );
    let module = nash_parse::Parser::new(&bump, source).module().unwrap();
    let canonical = nash_can::canonicalize(
        &bump,
        Context {
            package: None,
            interfaces: Some(&interfaces),
        },
        &module,
    )
    .unwrap();
    let mut uf = UnionFind::new();
    let module = &canonical.module;
    let (annotations, solved) = nash_solve::run(&bump, &mut uf, module, &canonical.tables).unwrap();
    assert!(
        annotations["run"]
            .context
            .iter()
            .any(|pred| pred.trait_ref() == Some(nash_ast::primitives::monad_trait()))
            && ordinary_context_len(annotations["run"]) == 1
    );
    assert_eq!(
        render_annotation(annotations["run"]),
        render_annotation(annotations["expanded"])
    );
    assert_eq!(
        solved
            .instances
            .values()
            .filter(|instance| matches!(instance.evidence, [nash_ast::Evidence::Given { .. }]))
            .count(),
        4
    );
    assert_eq!(
        solved
            .instances
            .values()
            .filter(|instance| matches!(instance.evidence, [nash_ast::Evidence::Super { .. }]))
            .count(),
        2
    );
    insta::assert_snapshot!(render_annotations(&annotations));
}

#[test]
fn higher_kinded_bind_chain_retains_its_monad_constraint() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (..)
        trait Functor 'f where
            map : ('a -> 'b) -> 'f 'a -> 'f 'b
        trait Functor 'f => Applicative 'f where
            pure : 'a -> 'f 'a
        trait Applicative 'm => Monad 'm where
            bind : 'm 'a -> ('a -> 'm 'b) -> 'm 'b
        chain f g mx = bind (bind mx f) g
    "#
    );
}

#[test]
fn nested_use_requires_owners_storable_constraint() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        import Builtin exposing (..)
        first : 'a -> list 'a -> 'a
        first x xs = x
        ignore value = ()
        bad : 'a -> ()
        bad x =
            let
                helper ignored = ignore (first x)
            in
            helper ()
    "#
    );
}

fn lift_interface(bump: &Bump, core: bool) -> nash_can::Interface<'_> {
    let module = nash_parse::Parser::new(bump, "module Lift exposing (Lift)\ntrait Lift 'small 'big where\n    lift : 'small -> 'big\n    lower : 'big -> 'small\nimpl Lift () () where\n    lift x = x\n    lower x = x\n").module().unwrap();
    let canonical = nash_can::canonicalize(
        bump,
        Context {
            package: core.then_some(nash_ast::primitives::CORE),
            interfaces: None,
        },
        &module,
    )
    .unwrap();
    let mut uf = UnionFind::new();
    let module = &canonical.module;
    let (annotations, _) = nash_solve::run(bump, &mut uf, module, &canonical.tables).unwrap();
    nash_can::from_module(bump, &canonical.module, &annotations)
}

#[test]
fn reflexive_lift_retains_big_evidence() {
    let bump = Bump::new();
    let interfaces = std::collections::BTreeMap::from([("Lift", lift_interface(&bump, true))]);
    let source = indoc!(
        r#"
        module Main exposing (..)
        import Lift exposing (Lift)
        type Color = Red
        type alias bigIdentity ('a : Big) = 'a -> 'a
        concrete : Color -> Color
        concrete x = lift x
        rigid : bigIdentity 'a
        rigid x = lift x
        same : 'a -> 'a -> 'a
        same x y = x
        inferred x = (rigid x, same x (lift x))
        explicit : () -> ()
        explicit x = lift x
        type Box 'a = Box 'a
        trait Keep 'a where
            keep : 'a -> 'a
        impl Lift 'a 'a => Keep (Box 'a) where
            keep x = x
        nested = keep (Box Red)
    "#
    );
    let module = nash_parse::Parser::new(&bump, source).module().unwrap();
    let canonical = nash_can::canonicalize(
        &bump,
        Context {
            package: None,
            interfaces: Some(&interfaces),
        },
        &module,
    )
    .unwrap();
    let mut uf = UnionFind::new();
    let module = &canonical.module;
    let (annotations, solved) = nash_solve::run(&bump, &mut uf, module, &canonical.tables).unwrap();
    assert!(
        ["concrete", "explicit", "nested"]
            .iter()
            .all(|name| annotations[name].context.is_empty())
    );
    for name in ["rigid", "inferred"] {
        assert!(
            matches!(annotations[name].context, [pred] if pred.trait_ref() == Some(nash_ast::primitives::ReprTrait::Big.qualified())),
            "{name}: {:?}",
            annotations[name].context
        );
    }
    let mut proofs: Vec<_> = solved
        .instances
        .values()
        .flat_map(|instance| instance.evidence.iter())
        .collect();
    assert!(
        proofs
            .iter()
            .any(|proof| matches!(proof, nash_ast::Evidence::Given { .. }))
    );
    proofs.retain(|proof| {
        !matches!(
            proof,
            nash_ast::Evidence::Given { .. } | nash_ast::Evidence::Repr { .. }
        )
    });
    assert_eq!(proofs.len(), 5);
    assert_eq!(
        proofs
            .iter()
            .filter(|proof| matches!(proof, nash_ast::Evidence::ReflexiveLift { .. }))
            .count(),
        3
    );
    assert_eq!(
        proofs
            .iter()
            .filter(|proof| matches!(proof, nash_ast::Evidence::Impl { .. }))
            .count(),
        2
    );
    assert!(proofs.iter().any(|proof| matches!(proof, nash_ast::Evidence::Impl { args, .. } if args.iter().any(|arg| matches!(arg, nash_ast::Evidence::ReflexiveLift { .. })))));
    proofs.sort_by_key(|proof| format!("{proof:?}"));
    insta::assert_debug_snapshot!(proofs);
}

#[test]
fn reflexive_lift_neither_narrows_types_nor_uses_foreign_identity() {
    let bump = Bump::new();
    let mut results = Vec::new();
    for (core, annotation) in [
        (true, "'a -> 'a"),
        (true, "'a -> 'b"),
        (false, "Color -> Color"),
    ] {
        let interfaces = std::collections::BTreeMap::from([("Lift", lift_interface(&bump, core))]);
        let source = bump.alloc_str(&format!("module Main exposing (..)\nimport Lift exposing (Lift)\ntype Color = Red\nbad : {annotation}\nbad x = lift x\n"));
        let module = nash_parse::Parser::new(&bump, source).module().unwrap();
        let canonical = nash_can::canonicalize(
            &bump,
            Context {
                package: None,
                interfaces: Some(&interfaces),
            },
            &module,
        )
        .unwrap();
        let mut uf = UnionFind::new();
        let module = &canonical.module;
        let errors = nash_solve::run(&bump, &mut uf, module, &canonical.tables).unwrap_err();
        assert!(matches!(
            errors.as_slice(),
            [Error::MissingConstraint { .. }] | [Error::MissingImpl { .. }]
        ));
        results.push((core, annotation, errors));
    }
    insta::assert_debug_snapshot!(results);
}

#[test]
fn incompatible_representations_cannot_escape_in_an_inferred_scheme() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        useBig : ('a : Big) -> ()
        useBig x = ()
        useTerm : ('a : Term) -> ()
        useTerm x = ()
        bad x = (useBig x, useTerm x)
    "#
    );
}

#[test]
fn declared_body_rejects_big_term_representation() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        type alias bigUse ('a : Big) = 'a -> ()
        type alias termUse ('a : Term) = 'a -> ()
        useBig : bigUse 'a
        useBig x = ()
        useTerm : termUse 'a
        useTerm x = ()
        loop x = loop x
        discard x y = ()
        bad : ()
        bad = (\x -> discard (useBig x) (useTerm x)) (loop ())
    "#
    );
}

#[test]
fn higher_kinded_value_inference_preserves_partial_heads() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (..)
        identityK : 'f 'a -> 'f 'a
        identityK x = x
        two : 'g 'b 'a -> 'g 'b 'a
        two x = identityK x
        reverse : 'f 'a -> 'f 'b -> ( 'f 'b, 'f 'a )
        reverse x y = (identityK y, identityK x)
    "#
    );
}

#[test]
fn higher_kinded_rigid_heads_cannot_specialize() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        type Box 'a = Box 'a
        wrong : 'f 'a -> Box 'a
        wrong x = x
    "#
    );
}

#[test]
fn higher_kinded_traits_resolve_distinct_constructors() {
    let source = indoc!(
        r#"
        module Main exposing (..)
        type option 'a = None | Some 'a
        type Box 'a = Box 'a
        type Color = Red | Blue
        trait Functor 'f where
            map : ('a -> 'b) -> 'f 'a -> 'f 'b
        impl Functor option where
            map f xs =
                case xs of
                    None -> None
                    Some x -> Some (f x)
        impl Functor Box where
            map f (Box x) = Box (f x)
        twice f xs = map f (map f xs)
        little = map (\x -> x) (Some ())
        changedRepresentation = map (\x -> (x, x)) (Some ())
        big = map (\x -> x) (Box Red)
    "#
    );
    let bump = Bump::new();
    let source = bump.alloc_str(source);
    let parsed = nash_parse::Parser::new(&bump, source).module().unwrap();
    let canonical = nash_can::canonicalize(
        &bump,
        Context {
            package: None,
            interfaces: None,
        },
        &parsed,
    )
    .unwrap();
    let mut uf = UnionFind::new();
    let module = &canonical.module;
    let (annotations, solved) = nash_solve::run(&bump, &mut uf, module, &canonical.tables).unwrap();
    let mut decls = canonical.module.decls;
    let mut checked = 0;
    while let nash_ast::Decls::Declare { definition, next } = decls {
        if let nash_ast::Def::Def { name, body, .. } = definition {
            let expected = match name.value {
                "little" | "changedRepresentation" => Some("option"),
                "big" => Some("Box"),
                _ => None,
            };
            if let Some(expected) = expected {
                let nash_ast::Expr::Call { function, .. } = body.value else {
                    panic!("map call")
                };
                let instance = &solved.instances[&nash_ast::NodeId::expr(function)];
                let [nash_ast::Evidence::Impl { impl_, args, .. }] = instance.evidence else {
                    panic!("resolved Functor evidence")
                };
                assert_eq!(impl_.key.trait_.name, "Functor");
                let [
                    nash_ast::Head::Named {
                        reference: head, ..
                    },
                ] = impl_.key.heads
                else {
                    panic!("nominal constructor")
                };
                assert_eq!(head.name, expected);
                assert!(args.is_empty());
                assert!(instance.type_args.iter().any(|arg| matches!(&arg.value, CanType::Named { reference, args } if reference.name == expected && args.is_empty())));
                checked += 1;
            }
        }
        decls = next;
    }
    assert_eq!(checked, 3);
    insta::assert_snapshot!(render_annotations(&annotations));
}

#[test]
fn imported_higher_kinded_value_preserves_application() {
    let bump = Bump::new();
    let module = nash_parse::Parser::new(
        &bump,
        "module Higher exposing (value)\nvalue : 'f 'a -> 'f 'a\nvalue x = x\n",
    )
    .module()
    .unwrap();
    let canonical = nash_can::canonicalize(
        &bump,
        Context {
            package: None,
            interfaces: None,
        },
        &module,
    )
    .unwrap();
    let mut uf = UnionFind::new();
    let module = &canonical.module;
    let (producer, _) = nash_solve::run(&bump, &mut uf, module, &canonical.tables).unwrap();
    let annotation = producer["value"];
    let a = annotation
        .free_vars
        .iter()
        .position(|name| *name == "a")
        .unwrap();
    let f = annotation
        .free_vars
        .iter()
        .position(|name| *name == "f")
        .unwrap();
    let [nash_ast::Pred::Apply { head, args: [arg] }] = annotation.context else {
        panic!("retained higher-kinded application")
    };
    assert!(matches!(head.value, CanType::Var(name) if name == annotation.free_vars[f]));
    assert!(matches!(arg.value, CanType::Var(name) if name == annotation.free_vars[a]));
    let interface = nash_can::from_module(&bump, &canonical.module, &producer);
    let interfaces = std::collections::BTreeMap::from([("Higher", interface)]);
    let source =
        bump.alloc_str("module Main exposing (..)\n\nimport Higher\n\nvalue = Higher.value\n");
    let module = nash_parse::Parser::new(&bump, source).module().unwrap();
    let canonical = nash_can::canonicalize(
        &bump,
        Context {
            package: None,
            interfaces: Some(&interfaces),
        },
        &module,
    )
    .unwrap();
    let mut uf = UnionFind::new();
    let module = &canonical.module;
    let (annotations, solved) = nash_solve::run(&bump, &mut uf, module, &canonical.tables)
        .expect("imported higher-kinded applications infer");
    let nash_ast::Decls::Declare { definition, .. } = canonical.module.decls else {
        panic!("value declaration")
    };
    let nash_ast::Def::Def { body, .. } = definition else {
        panic!("value definition")
    };
    let instance = &solved.instances[&nash_ast::NodeId::expr(body)];
    assert_eq!(instance.type_args.len(), 2);
    assert!(instance.evidence.is_empty());
    assert_eq!(annotations["value"].free_vars, annotation.free_vars);
    assert_eq!(
        annotations["value"]
            .context
            .iter()
            .map(|p| p.key())
            .collect::<Vec<_>>(),
        annotation
            .context
            .iter()
            .map(|p| p.key())
            .collect::<Vec<_>>()
    );
    insta::assert_snapshot!(render_annotations(&annotations));
}

#[test]
fn core_cast_schemes_preserve_nominal_source_and_target_types() {
    let bump = Bump::new();
    let source = indoc!(
        "
        module Casts exposing (..)
        import Builtin
        lift : int -> Int
        lift = Builtin.castLift
        lower : Int -> int
        lower = Builtin.castLower
        erase : Int -> Data
        erase = Builtin.castToData
        shallow : Data -> Int
        shallow = Builtin.castFromDataShallow
        validate : Data -> Int
        validate = Builtin.castValidateData
    "
    );
    let parsed = nash_parse::Parser::new(&bump, source).module().unwrap();
    let interfaces = literal_interfaces(&bump);
    let canonical = nash_can::canonicalize(
        &bump,
        Context {
            package: Some(nash_ast::primitives::CORE),
            interfaces: Some(&interfaces),
        },
        &parsed,
    )
    .unwrap();
    let mut uf = UnionFind::new();
    let module = &canonical.module;
    let (annotations, solved) = nash_solve::run(&bump, &mut uf, module, &canonical.tables).unwrap();
    assert_eq!(solved.instances.len(), 5);
    insta::assert_snapshot!(render_annotations(&annotations));
}

#[test]
fn literal_impls_preserve_little_defaults_with_big_and_utf8_candidates() {
    let bump = Bump::new();
    let mut interfaces =
        std::collections::BTreeMap::from([("Builtin", nash_can::kinds::builtin_interface(&bump))]);
    for (name, source, package) in [
        (
            "Literal",
            indoc!(
                "
            module Literal exposing (..)
            import Builtin
            trait FromInt 'a where
                fromInt : int -> 'a
            trait FromBytes 'a where
                fromBytes : bytes -> 'a
            trait FromString 'a where
                fromString : string -> 'a
            impl FromInt int where
                fromInt = Builtin.identity
            impl FromInt Int where
                fromInt = Builtin.castLift
            impl FromBytes bytes where
                fromBytes = Builtin.identity
            impl FromBytes Bytes where
                fromBytes = Builtin.castLift
            impl FromString string where
                fromString = Builtin.identity
            impl FromString bytes where
                fromString = Builtin.encodeUtf8
        "
            ),
            Some(nash_ast::primitives::CORE),
        ),
        (
            "Main",
            indoc!(
                r#"
            module Main exposing (..)
            import Literal
            integer = 42
            bytes = #"ff"
            string = "Nash"
            discardedInteger = (\_ -> ()) 42
            discardedBytes = (\_ -> ()) #"ff"
            discardedString = (\_ -> ()) "Nash"
            bigInteger : Int
            bigInteger = 42
            bigBytes : Bytes
            bigBytes = #"ff"
            utf8 : bytes
            utf8 = "Nash"
        "#
            ),
            None,
        ),
    ] {
        let parsed = nash_parse::Parser::new(&bump, source).module().unwrap();
        let canonical = nash_can::canonicalize(
            &bump,
            Context {
                package,
                interfaces: Some(&interfaces),
            },
            &parsed,
        )
        .unwrap();
        let mut uf = UnionFind::new();
        let module = &canonical.module;
        let (annotations, solved) =
            nash_solve::run(&bump, &mut uf, module, &canonical.tables).unwrap();
        if name == "Main" {
            for (trait_name, primitive) in [
                ("FromInt", "int"),
                ("FromBytes", "bytes"),
                ("FromString", "string"),
            ] {
                assert!(solved.instances.values().flat_map(|instance| instance.evidence).any(|evidence| {
                    matches!(evidence, nash_ast::Evidence::Impl { impl_, .. }
                        if impl_.key.trait_.name == trait_name
                        && matches!(impl_.key.heads, [nash_ast::Head::Named { reference: head, .. }]
                            if head.home == nash_ast::primitives::builtin_home() && head.name == primitive))
                }), "discarded literal must default to {primitive}");
            }
            insta::assert_snapshot!(render_annotations(&annotations));
        }
        interfaces.insert(
            name,
            nash_can::from_module(&bump, &canonical.module, &annotations),
        );
    }
}

#[test]
fn big_equality_is_automatic_and_retains_structural_evidence() {
    let bump = Bump::new();
    let mut interfaces =
        std::collections::BTreeMap::from([("Builtin", nash_can::kinds::builtin_interface(&bump))]);
    for (name, source, package) in [
        (
            "Eq",
            indoc!(
                "
            module Eq exposing (Eq)
            trait Eq 'a where
                eq : 'a -> 'a -> bool
        "
            ),
            Some(nash_ast::primitives::CORE),
        ),
        (
            "Main",
            indoc!(
                "
            module Main exposing (..)
            import Eq exposing (Eq)
            type Token = Token Int
            type alias Box = { token : Token }
            same : Token -> Token -> bool
            same = eq
            sameBox : Box -> Box -> bool
            sameBox = eq
            sameList : List 'a -> List 'a -> bool
            sameList = eq
            trait Eq 'a => Compare 'a where
                compare : 'a -> 'a -> bool
            impl Compare Token where
                compare = eq
        "
            ),
            None,
        ),
    ] {
        let parsed = nash_parse::Parser::new(&bump, source).module().unwrap();
        let canonical = nash_can::canonicalize(
            &bump,
            Context {
                package,
                interfaces: Some(&interfaces),
            },
            &parsed,
        )
        .unwrap();
        let mut uf = UnionFind::new();
        let module = &canonical.module;
        let (annotations, solved) =
            nash_solve::run(&bump, &mut uf, module, &canonical.tables).unwrap();
        if name == "Main" {
            let mut evidence: Vec<_> = solved
                .instances
                .values()
                .flat_map(|i| i.evidence)
                .filter_map(|e| {
                    if let nash_ast::Evidence::StructuralEq { typ } = e {
                        Some(render_type(typ, Ctx::None))
                    } else {
                        None
                    }
                })
                .collect();
            evidence.sort();
            assert!(evidence.len() >= 3);
            for name in ["same", "sameBox"] {
                let CanType::Lambda { from, .. } = annotations[name].typ.value else {
                    panic!("function annotation")
                };
                let pred = nash_ast::Pred::Trait {
                    trait_: nash_ast::primitives::eq_trait(),
                    args: bump.alloc_slice_copy(&[from]),
                };
                assert!(matches!(
                    nash_solve::evidence::resolve(&bump, &canonical.tables, &pred).unwrap(),
                    Some(nash_ast::Evidence::StructuralEq { .. })
                ));
            }
            insta::assert_snapshot!(format!(
                "{}\nStructural evidence: {}",
                render_annotations(&annotations),
                evidence.join(", ")
            ));
        }
        interfaces.insert(
            name,
            nash_can::from_module(&bump, &canonical.module, &annotations),
        );
    }
}

#[test]
fn nominal_record_access_resolved_later() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (..)
        type alias point = { x : int }
        g : point -> int
        g p = p.x
        f p = (p.x, g p)
    "#
    );
}

#[test]
fn nominal_record_access_fixed_point() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (..)
        type alias point = { x : int }
        type alias outer = { inner : point }
        g : outer -> int
        g p = p.inner.x
        f p = (p.inner.x, g p)
    "#
    );
}

#[test]
fn nominal_record_local_accessor_cannot_generalize() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        type alias point = { x : int }
        f p =
            let
                get r = r.x
            in
            get (point p)
    "#
    );
}

#[test]
fn nominal_record_captured_field_resolves_in_outer_scope() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (..)
        type alias point = { x : int }
        g : point -> int
        g p = p.x
        f p =
            let
                get ignored = p.x
            in
            (get (), g p)
    "#
    );
}

#[test]
fn nominal_record_captured_field_does_not_generalize_result() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        type alias point = { x : int }
        f : point -> (int, string)
        f p =
            let
                get ignored = p.x
            in
            (get (), get ())
    "#
    );
}

#[test]
fn empty_record_pattern_rejects_non_record() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        f : int -> ()
        f {} = ()
    "#
    );
}

#[test]
fn empty_record_pattern_accepts_known_record() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (..)
        type alias point = { x : int }
        f : point -> ()
        f {} = ()
    "#
    );
}

#[test]
fn empty_record_pattern_needs_known_record() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        f {} = ()
    "#
    );
}

#[test]
fn deferred_captured_record_field_cannot_escape() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        type alias point = { x : int }
        g : point -> ()
        g p = ()
        asInt : int -> ()
        asInt x = ()
        asString : string -> ()
        asString x = ()
        f p =
            let
                get ignored = p.x
            in
            (asInt (get ()), asString (get ()), g p)
    "#
    );
}

#[test]
fn nominal_record_operations_preserve_parameters() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (..)
        type alias box 'a = { item : 'a }
        value = { item = () }
        get : box 'a -> 'a
        get b = b.item
        accessor : box 'a -> 'a
        accessor = .item
        update : box 'a -> 'a -> box 'a
        update b item = { b | item = item }
        pattern : box 'a -> 'a
        pattern { item } = item
    "#
    );
}

#[test]
fn nominal_record_missing_field_errors() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        type alias point = { x : int }
        get : point -> int
        get p = p.y
    "#
    );
}

#[test]
fn nominal_record_update_preserves_field_type() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        type alias point = { x : int }
        change : point -> string -> point
        change p s = { p | x = s }
    "#
    );
}

#[test]
fn nominal_record_constructor_disambiguates_identical_fields() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (..)
        type alias first = { x : unit }
        type alias second = { x : unit }
        a = first ()
        b = second ()
    "#
    );
}

#[test]
fn deferred_captured_field_preserves_trait_evidence() {
    let bump = Bump::new();
    let source = indoc!(
        r#"
        module Main exposing (..)
        type alias point = { x : int }
        trait Read 'a where
            read : 'a -> 'a
        impl Read int where
            read x = x
        g : point -> ()
        g p = ()
        f p =
            let
                get ignored = read p.x
            in
            (get (), g p)
    "#
    );
    let module = nash_parse::Parser::new(&bump, source).module().unwrap();
    let canonical = nash_can::canonicalize(&bump, Context::default(), &module).unwrap();
    let mut uf = UnionFind::new();
    let module = &canonical.module;
    let (annotations, solved) = nash_solve::run(&bump, &mut uf, module, &canonical.tables).unwrap();
    assert_eq!(
        render_annotation(annotations["f"]),
        "point -> ( int, unit )"
    );
    assert!(solved.instances.values().flat_map(|instance| instance.evidence).any(|evidence| {
        matches!(evidence, nash_ast::Evidence::Impl { impl_, .. } if impl_.key.trait_.name == "Read")
    }));
}

#[test]
fn big_builtin_types_in_scope() {
    assert_inference_snapshot!(
        "module Main exposing (..)\nf : Builtin.List Builtin.Int -> List Int\nf x = x\n"
    );
}

#[test]
fn user_type_shadows_builtin_unqualified() {
    let bump = Bump::new();
    let annotations = infer(&bump, "module Main exposing (..)\ntype int = Mine\nmain = Mine\nidentity : Builtin.int -> Builtin.int\nidentity x = x\n").unwrap();
    assert!(
        matches!(annotations["main"].typ.value, CanType::Named { reference, .. } if reference.home.name == "Main" && reference.name == "int")
    );
    let CanType::Lambda { from, to } = annotations["identity"].typ.value else {
        panic!("expected identity")
    };
    for typ in [from, to] {
        assert!(
            matches!(typ.value, CanType::Named { reference, .. } if reference.home == nash_ast::primitives::builtin_home() && reference.name == "int")
        );
    }
}

#[test]
fn unit_impl_syntax_matches_named_builtin() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (..)
        trait Keep 'a where
            keep : 'a -> 'a
        impl Keep () where
            keep x = x
        main : Builtin.unit
        main = keep ()
    "#
    );
}

#[test]
fn labeled_ctor_access_and_construction() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (..)
        type box 'a = Box { z : 'a, count : int }
        get : box 'a -> 'a
        get b = b.z
        make value = Box { count = 1, z = value }
        positional value = Box value 1
        pattern (Box { z }) = z
        accessor = .z (Box () 1)
    "#
    );
}

#[test]
fn labeled_ctor_update_error() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        type box 'a = Box { value : 'a }
        change : box unit -> box unit
        change b = { b | value = () }
    "#
    );
}

#[test]
fn labeled_ctor_multi_access_error() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        type choice = First { value : unit } | Second { value : unit }
        get : choice -> unit
        get b = b.value
    "#
    );
}

#[test]
fn positional_ctor_record_operations_stay_positional() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (..)
        type alias point = { x : unit }
        type wrapper = Wrap point
        main = Wrap { x = () }
        get (Wrap { x }) = x
    "#
    );
}

#[test]
fn grouped_record_is_positional_argument_to_labeled_ctor() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (..)
        type alias point = { x : unit }
        type wrapper = Wrap { inner : point }
        positional = Wrap ({ x = () })
        labeled = Wrap { inner = { x = () } }
        get (Wrap inner) = inner.x
    "#
    );
}

#[test]
fn labeled_ctor_higher_kinded_projection_preserves_application() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (..)
        type holder 'f 'a = Holder { value : 'f 'a }
        type alias wrapped 'f 'a = holder 'f 'a
        get : wrapped 'f 'a -> 'f 'a
        get h = h.value
        main = get (Holder [()])
    "#
    );
}

#[test]
fn labeled_ctor_twins_preserve_labeled_construction() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (..)
        type Box 'a = Box { z : 'a }
        type box 'a = Box { z : 'a }
        little : box unit
        little = Box { z = () }
        get : box 'a -> 'a
        get b = b.z
    "#
    );
}

#[test]
fn labeled_ctor_captured_projection_keeps_outer_parameter() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        type box 'a = Box { value : 'a }
        bad : box 'a -> ('a, unit)
        bad b =
            let
                get ignored = b.value
            in
            (get (), get ())
    "#
    );
}

#[test]
fn labeled_ctor_empty_pattern_and_missing_projection() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (..)
        type box = Box { value : unit }
        make = Box { value = () }
        get : box -> unit
        get {} = ()
    "#
    );
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        type box = Box { value : unit }
        bad : box -> unit
        bad e = e.missing
    "#
    );
}

#[test]
fn labeled_ctor_big_construction_projection_and_pattern() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (..)
        type Datum = Datum { owner : Bytes, deadline : Int }
        make : Bytes -> Int -> Datum
        make o d = Datum { deadline = d, owner = o }
        positional : Bytes -> Int -> Datum
        positional o d = Datum o d
        owner : Datum -> Bytes
        owner d = d.owner
        due (Datum { deadline }) = deadline
    "#
    );
}

#[test]
fn labeled_ctor_multi_constructor_sugar_and_case() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (..)
        type choice = First { x : unit, y : unit } | Second { x : unit }
        first = First { y = (), x = () }
        second = Second { x = () }
        pick : choice -> unit
        pick c =
            case c of
                First { y } -> y
                Second { x } -> x
    "#
    );
}

#[test]
fn labeled_ctor_big_multi_constructor_sugar_and_case() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (..)
        type Redeemer = Claim { owner : Bytes, amount : Int } | Cancel { owner : Bytes }
        claim : Bytes -> Int -> Redeemer
        claim o a = Claim { amount = a, owner = o }
        cancel : Bytes -> Redeemer
        cancel o = Cancel { owner = o }
        who : Redeemer -> Bytes
        who r =
            case r of
                Claim { owner } -> owner
                Cancel { owner } -> owner
    "#
    );
}

#[test]
fn labeled_ctor_big_multi_access_error() {
    assert_inference_error_snapshot!(
        r#"
        module Main exposing (..)
        type Redeemer = Claim { owner : Bytes } | Cancel { owner : Bytes }
        who : Redeemer -> Bytes
        who r = r.owner
    "#
    );
}

#[test]
fn recovery_direct_recursive_occurs_precedes_generalization() {
    assert_inference_error_snapshot!(
        r#"module Main exposing (..)
f x = f
use = f ()
bad : ()
bad = \x -> x
"#
    );
}

#[test]
fn recovery_direct_annotated_if_keeps_both_mismatches() {
    assert_inference_error_snapshot!(
        r#"module Main exposing (..)
import Builtin exposing (..)
f : ()
f = if True then (\x -> x) else (\y -> y)
"#
    );
}

#[test]
fn recovery_direct_annotated_case_keeps_both_mismatches() {
    assert_inference_error_snapshot!(
        r#"module Main exposing (..)
import Builtin exposing (..)
f : ()
f = case True of
    True -> (\x -> x)
    False -> (\y -> y)
"#
    );
}

#[test]
fn recovery_direct_cons_tail_keeps_independent_mismatch() {
    assert_inference_error_snapshot!(
        r#"module Main exposing (..)
import Builtin exposing (..)
f : () -> ()
f (x :: ()) = ()
"#
    );
}

#[test]
fn recovery_direct_alias_bool_keeps_header_type() {
    assert_inference_error_snapshot!(
        r#"module Main exposing (..)
import Builtin exposing (..)
f : () -> ()
f (True as whole) = whole ()
"#
    );
}

#[test]
fn recovery_direct_alias_nested_keeps_header_type() {
    assert_inference_error_snapshot!(
        r#"module Main exposing (..)
import Builtin exposing (..)
f : ((), ()) -> ()
f (((True as a), (() as b)) as whole) = whole ()
"#
    );
}

#[test]
fn recovery_direct_alias_ctor_keeps_header_type() {
    assert_inference_error_snapshot!(
        r#"module Main exposing (..)
import Builtin exposing (..)
type box = Box bool
f : box -> ()
f (Box (() as whole)) = whole ()
"#
    );
}

// Original canonical nodes must all be usable by codegen, including the
// branch/let nodes that the annotated inference path handles separately.
#[derive(Default)]
struct MetadataNodes<'a> {
    exprs: Vec<&'a Located<nash_ast::Expr<'a>>>,
    patterns: Vec<&'a Located<nash_ast::Pattern<'a>>>,
}

impl<'a> MetadataNodes<'a> {
    fn pattern(&mut self, pattern: &'a Located<nash_ast::Pattern<'a>>) {
        use nash_ast::Pattern;
        self.patterns.push(pattern);
        match &pattern.value {
            Pattern::Alias { pattern, .. } => self.pattern(pattern),
            Pattern::Tuple {
                first,
                second,
                rest,
            } => {
                self.pattern(first);
                self.pattern(second);
                for pattern in *rest {
                    self.pattern(pattern);
                }
            }
            Pattern::List(patterns) => {
                for pattern in *patterns {
                    self.pattern(pattern);
                }
            }
            Pattern::Cons { head, tail } => {
                self.pattern(head);
                self.pattern(tail);
            }
            Pattern::Constructor(ctor) => {
                for arg in ctor.arguments {
                    self.pattern(arg.pattern);
                }
            }
            _ => {}
        }
    }

    fn definition(&mut self, def: &'a nash_ast::Def<'a>) {
        use nash_ast::Def;
        match def {
            Def::Def { args, body, .. } => {
                for pattern in *args {
                    self.pattern(pattern);
                }
                self.expr(body);
            }
            Def::TypedDef { args, body, .. } => {
                for arg in *args {
                    self.pattern(arg.pattern);
                }
                self.expr(body);
            }
        }
    }

    fn expr(&mut self, expr: &'a Located<nash_ast::Expr<'a>>) {
        use nash_ast::Expr;
        self.exprs.push(expr);
        match &expr.value {
            Expr::List(items) => {
                for item in *items {
                    self.expr(item);
                }
            }
            Expr::Binop { left, right, .. } => {
                self.expr(left);
                self.expr(right);
            }
            Expr::Lambda { parameters, body } => {
                for pattern in *parameters {
                    self.pattern(pattern);
                }
                self.expr(body);
            }
            Expr::Call {
                function,
                arguments,
            } => {
                self.expr(function);
                for arg in *arguments {
                    self.expr(arg);
                }
            }
            Expr::If {
                branches,
                final_else,
            } => {
                for branch in *branches {
                    self.expr(branch.condition);
                    self.expr(branch.then_branch);
                }
                self.expr(final_else);
            }
            Expr::Let { definition, body } => {
                self.definition(definition);
                self.expr(body);
            }
            Expr::LetRec { definitions, body } => {
                for def in *definitions {
                    self.definition(def);
                }
                self.expr(body);
            }
            Expr::LetDestruct {
                pattern,
                value,
                body,
            } => {
                self.pattern(pattern);
                self.expr(value);
                self.expr(body);
            }
            Expr::Case {
                scrutinee,
                branches,
            } => {
                self.expr(scrutinee);
                for branch in *branches {
                    self.pattern(branch.pattern);
                    self.expr(branch.body);
                }
            }
            Expr::Access { record, .. } => self.expr(record),
            Expr::Update { base, fields, .. } => {
                self.expr(base);
                for field in *fields {
                    self.expr(field.value);
                }
            }
            Expr::Record { fields, .. } => {
                for field in *fields {
                    self.expr(field.value);
                }
            }
            Expr::Tuple {
                first,
                second,
                rest,
            } => {
                self.expr(first);
                self.expr(second);
                for item in *rest {
                    self.expr(item);
                }
            }
            _ => {}
        }
    }
}

fn metadata_fixture<'a>(
    bump: &'a Bump,
    source: &str,
) -> (
    Annotations<'a>,
    nash_solve::SolvedTypes<'a>,
    MetadataNodes<'a>,
) {
    let source = bump.alloc_str(source);
    let parsed = nash_parse::Parser::new(bump, source).module().unwrap();
    let mut interfaces = literal_interfaces(bump);
    let eq_source = bump.alloc_str("module Eq exposing (..)\nimport Builtin exposing (..)\ntrait Eq 'a where\n    eq : 'a -> 'a -> bool\nimpl Eq int where\n    eq a b = True\n");
    let eq_parsed = nash_parse::Parser::new(bump, eq_source).module().unwrap();
    let eq_can = nash_can::canonicalize(
        bump,
        Context {
            package: Some(nash_ast::primitives::CORE),
            interfaces: Some(&interfaces),
        },
        &eq_parsed,
    )
    .unwrap();
    let (eq_annotations, _) =
        nash_solve::run(bump, &mut UnionFind::new(), &eq_can.module, &eq_can.tables).unwrap();
    interfaces.insert(
        "Eq",
        nash_can::from_module(bump, &eq_can.module, &eq_annotations),
    );
    let canonical = nash_can::canonicalize(
        bump,
        Context {
            package: None,
            interfaces: Some(&interfaces),
        },
        &parsed,
    )
    .unwrap();
    let mut uf = UnionFind::new();
    let (annotations, solved) =
        nash_solve::run(bump, &mut uf, &canonical.module, &canonical.tables).unwrap();
    let mut nodes = MetadataNodes::default();
    let mut decls = canonical.module.decls;
    loop {
        match decls {
            nash_ast::Decls::Declare { definition, next } => {
                nodes.definition(definition);
                decls = next;
            }
            nash_ast::Decls::DeclareRec {
                definition,
                following,
                next,
            } => {
                nodes.definition(definition);
                for def in *following {
                    nodes.definition(def);
                }
                decls = next;
            }
            nash_ast::Decls::Empty => break,
        }
    }
    for trait_ in canonical.module.traits {
        for method in trait_.value.methods {
            if let Some(def) = method.default {
                nodes.definition(def);
            }
        }
    }
    for impl_ in canonical.module.impls {
        for def in impl_.value.methods {
            nodes.definition(def);
        }
    }
    (annotations, solved, nodes)
}

#[test]
fn solved_metadata_covers_original_nodes_in_recursive_and_annotated_bodies() {
    let bump = Bump::new();
    let (_, solved, nodes) = metadata_fixture(
        &bump,
        indoc!(
            r#"
        module Main exposing (..)
        import Builtin exposing (..)
        type option 'a = None | Some 'a
        trait Keep 'a where
            keep : 'a -> 'a
        typed : 'a -> 'a
        typed (x as alias) =
            if True then
                let
                    captured y = (alias, y)
                    (a, b) = captured ()
                in
                a
            else
                case Some x of
                    Some z -> typed z
                    None -> x
        untyped x = untyped x
        qualified : Keep 'a => 'a -> 'a
        qualified x =
            let
                helper y = keep y
            in
            helper x
        literal : int -> ()
        literal x =
            case x of
                0 -> ()
                _ -> ()
        tuple (a, b) = (a, b)
        list (x :: xs) = xs
        lambda = \x -> x
    "#
        ),
    );
    let expr_ids: std::collections::HashSet<_> = nodes
        .exprs
        .iter()
        .map(|expr| nash_ast::NodeId::expr(expr))
        .collect();
    let pattern_ids: std::collections::HashSet<_> = nodes
        .patterns
        .iter()
        .map(|pattern| nash_ast::NodeId::pattern(pattern))
        .collect();
    assert_eq!(
        solved
            .exprs
            .keys()
            .copied()
            .collect::<std::collections::HashSet<_>>(),
        expr_ids
    );
    assert_eq!(
        solved
            .patterns
            .keys()
            .copied()
            .collect::<std::collections::HashSet<_>>(),
        pattern_ids
    );
    for expr in nodes.exprs {
        if let nash_ast::Expr::Unit = expr.value {
            assert_eq!(
                solved.exprs[&nash_ast::NodeId::expr(expr)].value,
                CanType::unit()
            );
        }
    }
}

#[test]
fn solved_metadata_names_captures_consistently_with_schemes_and_instances() {
    let bump = Bump::new();
    let (annotations, solved, nodes) = metadata_fixture(
        &bump,
        indoc!(
            r#"
        module Main exposing (..)
        outer captured =
            let
                local argument = (captured, argument)
            in
            (local (), captured)
    "#
        ),
    );
    let CanType::Lambda { from, .. } = annotations["outer"].typ.value else {
        panic!("function");
    };
    let CanType::Var(capture_name) = from.value else {
        panic!("generic capture");
    };
    for expr in &nodes.exprs {
        if matches!(expr.value, nash_ast::Expr::VarLocal("captured")) {
            assert_eq!(
                solved.exprs[&nash_ast::NodeId::expr(expr)].value,
                CanType::Var(capture_name)
            );
        }
    }
    for pattern in nodes.patterns {
        if matches!(pattern.value, nash_ast::Pattern::Var("captured")) {
            assert_eq!(
                solved.patterns[&nash_ast::NodeId::pattern(pattern)].value,
                CanType::Var(capture_name)
            );
        }
    }
    let local = solved.schemes.values().find(|scheme| {
        !scheme.annotation.free_vars.contains(&capture_name)
            && matches!(scheme.annotation.typ.value, CanType::Lambda { to, .. } if matches!(to.value, CanType::Tuple { .. }))
    }).unwrap();
    let CanType::Lambda { to, .. } = local.annotation.typ.value else {
        unreachable!()
    };
    let CanType::Tuple { first, .. } = to.value else {
        unreachable!()
    };
    assert_eq!(first.value, CanType::Var(capture_name));
    assert!(!local.annotation.free_vars.contains(&capture_name));
    let use_ = nodes
        .exprs
        .iter()
        .find(|expr| matches!(expr.value, nash_ast::Expr::VarLocal("local")))
        .unwrap();
    let node = nash_ast::NodeId::expr(use_);
    let CanType::Lambda { from, to } = solved.exprs[&node].value else {
        panic!("local function use");
    };
    assert_eq!(from.value, CanType::unit());
    let CanType::Tuple { first, second, .. } = to.value else {
        panic!("local result");
    };
    assert_eq!(first.value, CanType::Var(capture_name));
    assert_eq!(second.value, CanType::unit());
    assert_eq!(solved.instances[&node].type_args.len(), 1);
    assert_eq!(solved.instances[&node].type_args[0].value, from.value);
}
