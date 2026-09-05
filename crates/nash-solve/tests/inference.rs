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

fn infer<'a>(bump: &'a Bump, input: &str) -> Result<Annotations<'a>, Vec<Error<'a>>> {
    let src = bump.alloc_str(input);
    let mut parser = nash_parse::Parser::new(bump, src.as_bytes());
    let module = parser.module().expect("expected successful parse");
    let can_result = nash_can::canonicalize(bump, Context::default(), &module)
        .expect("expected successful canonicalization");

    let mut uf = UnionFind::new();
    let constraint = nash_constrain::constrain(bump, &mut uf, &can_result.module);
    nash_solve::run(bump, &mut uf, &constraint, &can_result.tables)
}

#[test]
fn builtin_list_annotations_match_literals_and_patterns() {
    let bump = Bump::new();
    let source = bump.alloc_str(indoc!(
        r#"
        module Main exposing (..)

        import Builtin exposing (List)

        empty : List 'a
        empty = []

        first : List 'a -> 'a
        first xs =
            case xs of
                head :: tail -> head
    "#
    ));
    let module = nash_parse::Parser::new(&bump, source.as_bytes())
        .module()
        .unwrap();
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
    let constraint = nash_constrain::constrain(&bump, &mut uf, &canonical.module);
    let annotations = nash_solve::run(&bump, &mut uf, &constraint, &canonical.tables)
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

fn render_annotations(annotations: &Annotations<'_>) -> String {
    annotations
        .iter()
        .map(|(name, annotation)| format!("{name} : {}", render_annotation(annotation)))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_annotation(annotation: &Annotation<'_>) -> String {
    let mut tipe = render_type(annotation.typ, Ctx::None);
    if !annotation.context.is_empty() {
        let predicates: Vec<_> = annotation
            .context
            .iter()
            .map(|p| render_apply(p.trait_.name, p.args, Ctx::None))
            .collect();
        let context = if predicates.len() == 1 {
            predicates[0].clone()
        } else {
            format!("({})", predicates.join(", "))
        };
        tipe = format!("{context} => {tipe}");
    }
    if annotation.free_vars.is_empty() {
        tipe
    } else {
        format!("forall {}. {}", annotation.free_vars.join(" "), tipe)
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

        CanType::Record { fields, ext } => {
            let rendered_fields = fields
                .iter()
                .map(|field| format!("{} : {}", field.field, render_type(field.typ, Ctx::None)))
                .collect::<Vec<_>>()
                .join(", ");
            match ext {
                None if fields.is_empty() => "{}".to_string(),
                None => format!("{{ {rendered_fields} }}"),
                Some(ext_name) => format!("{{ {ext_name} | {rendered_fields} }}"),
            }
        }

        CanType::Unit => "()".to_string(),

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
        boxed : Keep (Container 'a) => 'a -> 'a
        boxed x = x
        useBox = (boxed (), boxed "hello")
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
        assert_eq!(annotations[name].context.len(), 1, "{name}");
    }
    assert_eq!(annotations["use"].context.len(), 2);
    assert_eq!(annotations["useBox"].context.len(), 2);
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
        forward x = keep x
        pair = (forward (), forward "hello")
    "#
    );
    let annotations = infer(&bump, source).unwrap();
    assert_eq!(annotations["forward"].context.len(), 1);
    assert_eq!(annotations["pair"].context.len(), 2);
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
fn inferred_context_preserves_constructed_arguments_and_recursive_groups() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (..)
        trait Observe 'a where
            observe : 'a -> ()
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
    let unit = uf.fresh(make_descriptor(Content::Structure(FlatType::Unit1)));
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
    let annotation = nash_solve::to_annotation_with_context(&bump, &mut uf, result, context);
    assert_eq!(annotation.free_vars, ["a", "b"]);
    assert!(matches!(annotation.typ.value, CanType::Var("b")));
    insta::assert_snapshot!(render_annotation(annotation));
}

#[test]
fn recursive_definition_metadata_preserves_names_types_and_given_variables() {
    use nash_constrain::Constraint;
    use nash_constrain::type_::Type;

    fn definitions<'a, 'b>(constraint: &'b Constraint<'a>, found: &mut Vec<&'b Constraint<'a>>) {
        match constraint {
            Constraint::Let {
                definitions: defs,
                header_con,
                body_con,
                ..
            } => {
                if !defs.is_empty() {
                    found.push(constraint);
                }
                definitions(header_con, found);
                definitions(body_con, found);
            }
            Constraint::And(constraints) => {
                for constraint in *constraints {
                    definitions(constraint, found);
                }
            }
            _ => {}
        }
    }

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
    let parsed = nash_parse::Parser::new(&bump, source.as_bytes())
        .module()
        .unwrap();
    let canonical = nash_can::canonicalize(&bump, Context::default(), &parsed).unwrap();
    let mut original_names = Vec::new();
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
            original_names.push(*name);
        }
        decls = next;
    }
    let nash_ast::Def::TypedDef { name, .. } =
        canonical.module.traits[0].value.methods[0].default.unwrap()
    else {
        panic!("typed default")
    };
    original_names.push(*name);
    let mut uf = UnionFind::new();
    let constraint = nash_constrain::constrain(&bump, &mut uf, &canonical.module);
    let mut found = Vec::new();
    definitions(&constraint, &mut found);
    let mut names = Vec::new();
    for constraint in found {
        let Constraint::Let {
            given,
            binder: Some(binder),
            definitions,
            header,
            rigid_vars,
            ..
        } = constraint
        else {
            panic!("each definition scope must have an evidence binder");
        };
        assert!(std::ptr::eq(*binder, definitions[0].name));
        for definition in *definitions {
            assert!(
                original_names
                    .iter()
                    .any(|name| std::ptr::eq(*name, definition.name))
            );
            names.push(definition.name.value);
            assert!(
                matches!(definition.typ, Type::FunN(..)),
                "retain the full function type"
            );
        }
        if binder.value == "f" || binder.value == "keep" {
            assert_eq!(given.len(), 1);
            let Type::VarN(predicate_var) = given[0].args[0] else {
                panic!("predicate variable")
            };
            let Type::FunN(Type::VarN(argument), Type::VarN(result)) = definitions[0].typ else {
                panic!("function type")
            };
            assert_eq!(predicate_var, argument);
            assert_eq!(predicate_var, result);
            assert!(rigid_vars.contains(predicate_var));
            assert!(
                header.is_empty(),
                "methods and recursive typed bodies have no lexical header"
            );
        } else {
            assert!(given.is_empty());
            assert_eq!(
                definitions.len(),
                2,
                "both untyped recursive members share the group binder"
            );
            assert_eq!(header.len(), 2);
        }
    }
    names.sort_unstable();
    assert_eq!(names, ["f", "g", "h", "keep"]);
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
fn string_literal() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (greeting)

        greeting = "hello"
    "#
    );
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

        point = { x = 1, y = 2 }
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
fn record_access() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (getX)

        getX r = r.x
    "#
    );
}

#[test]
fn record_accessor_function() {
    assert_inference_snapshot!(
        r#"
        module Main exposing (getName)

        getName = .name
    "#
    );
}

#[test]
fn record_update() {
    assert_inference_snapshot!(
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
fn branch_mismatch() {
    assert_inference_error_snapshot!(
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
fn number_cannot_be_string() {
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
fn nested_operator_sections_apply() {
    let bump = Bump::new();
    let operators = "module Operators exposing (..)\n\ninfix left 6 (+) = first\n\nfirst x y = x\n";
    let annotations = infer(&bump, operators).expect("operator module infers");
    let source = bump.alloc_str(operators);
    let module = nash_parse::Parser::new(&bump, source.as_bytes())
        .module()
        .unwrap();
    let canonical = nash_can::canonicalize(&bump, Context::default(), &module).unwrap();
    let interface = nash_can::from_module(&bump, &canonical.module, &annotations);
    let interfaces = std::collections::BTreeMap::from([("Operators", interface)]);
    let input = indoc!(
        r#"
        module Main exposing (..)

        import Operators exposing (..)

        right = (+ ((+ 1) 2)) "right"
        left = (((() +) 2) +) "left"
    "#
    );
    let source = bump.alloc_str(input);
    let module = nash_parse::Parser::new(&bump, source.as_bytes())
        .module()
        .unwrap();
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
    let constraint = nash_constrain::constrain(&bump, &mut uf, &canonical.module);
    let annotations = nash_solve::run(&bump, &mut uf, &constraint, &canonical.tables)
        .expect("nested sections infer");
    let rendered = render_annotations(&annotations);
    assert!(rendered.contains("right : String"), "{rendered}");
    assert!(rendered.contains("left : ()"), "{rendered}");
    insta::assert_snapshot!(rendered);
}

#[test]
fn higher_kinded_value_inference_is_explicitly_deferred() {
    let bump = Bump::new();
    let errors = infer(
        &bump,
        "module Main exposing (..)\n\nf : 'f 'a -> 'f 'a\nf x = x\n",
    )
    .expect_err("higher-kinded value unification belongs to plan 03");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, Error::UnsupportedTypeApplication { .. }))
    );
    insta::assert_debug_snapshot!(errors);
}

#[test]
fn imported_higher_kinded_value_inference_is_explicitly_deferred() {
    let bump = Bump::new();
    let head = bump.alloc(Located::at_zero(CanType::Var("f")));
    let arg = bump.alloc(Located::at_zero(CanType::Var("a")));
    let typ = bump.alloc(Located::at_zero(CanType::App {
        head,
        args: bump.alloc_slice_copy(&[&*arg]),
    }));
    let annotation = bump.alloc(Annotation {
        context: &[],
        free_vars: &["f", "a"],
        typ,
    });
    let interface = nash_can::Interface {
        impls: &[],
        traits: &[],
        home: nash_ast::ModuleName {
            package: None,
            name: "Higher",
        },
        values: bump.alloc_slice_copy(&[nash_can::InterfaceValue {
            name: "value",
            annotation,
        }]),
        unions: &[],
        aliases: &[],
        binops: &[],
    };
    let interfaces = std::collections::BTreeMap::from([("Higher", interface)]);
    let source =
        bump.alloc_str("module Main exposing (..)\n\nimport Higher\n\nvalue = Higher.value\n");
    let module = nash_parse::Parser::new(&bump, source.as_bytes())
        .module()
        .unwrap();
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
    let constraint = nash_constrain::constrain(&bump, &mut uf, &canonical.module);
    let errors = nash_solve::run(&bump, &mut uf, &constraint, &canonical.tables)
        .expect_err("imported HKT must not silently become a value type");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, Error::UnsupportedTypeApplication { .. }))
    );
    insta::assert_debug_snapshot!(errors);
}
