//! Fixed frontend constants do not acquire native literal or equality constraints.
use bumpalo::Bump;
use nash_region::Located;
use nash_source::{Constant, Expr, Pattern};

fn infer<'a>(
    arena: &'a Bump,
    source: &'a str,
    body: &'a Located<Expr<'a>>,
) -> Result<nash_can::Annotations<'a>, Vec<nash_constrain::error::Error<'a>>> {
    let mut module = nash_parse::Parser::new(arena, source).module().unwrap();
    let original = &module.values[0].value;
    module.values =
        arena.alloc_slice_copy(&[&*arena.alloc(Located::at_zero(nash_source::Value {
            name: original.name,
            arguments: original.arguments,
            body,
            annotation: original.annotation,
            attributes: original.attributes,
        }))]);
    let interfaces =
        std::collections::BTreeMap::from([("Builtin", nash_can::kinds::builtin_interface(arena))]);
    let can = nash_can::canonicalize(
        arena,
        nash_can::Context {
            package: None,
            interfaces: Some(&interfaces),
        },
        &module,
    )
    .unwrap();
    nash_solve::run(
        arena,
        &mut nash_constrain::UnionFind::new(),
        &can.module,
        &can.tables,
    )
    .map(|(annotations, _)| annotations)
}

#[test]
fn fixed_literals_infer_primitive_types_without_literal_modules() {
    let arena = Bump::new();
    for value in [
        Constant::Int(1),
        Constant::Bytes(&[0xff]),
        Constant::Str("hello"),
    ] {
        let annotations = infer(
            &arena,
            "module Main exposing (..)\nvalue = ()\n",
            arena.alloc(Located::at_zero(Expr::Constant(value))),
        )
        .unwrap();
        let annotation = annotations["value"];
        assert!(annotation.context.is_empty());
        assert!(
            matches!(&annotation.typ.value, nash_ast::Type::Named { reference, args }
            if reference.home == nash_ast::primitives::builtin_home() && reference.name == value.builtin_name() && args.is_empty()),
            "{annotation:#?}"
        );
    }
    let error = infer(
        &arena,
        "module Main exposing (..)\nvalue : bytes\nvalue = ()\n",
        arena.alloc(Located::at_zero(Expr::Constant(Constant::Int(1)))),
    )
    .unwrap_err();
    assert!(
        matches!(
            error.as_slice(),
            [nash_constrain::error::Error::BadExpr(..)]
        ),
        "{error:#?}"
    );
}

#[test]
fn fixed_pattern_checks_primitive_without_equality_or_literal_instances() {
    let arena = Bump::new();
    let pattern = arena.alloc(Located::at_zero(Pattern::Constant(Constant::Int(0))));
    let expr = arena.alloc(Located::at_zero(Expr::Constant(Constant::Bytes(&[1, 2]))));
    let body = arena.alloc(Located::at_zero(Expr::Lambda {
        parameters: arena.alloc_slice_copy(&[&*pattern]),
        body: expr,
    }));
    let annotations = infer(
        &arena,
        "module Main exposing (..)\nvalue : int -> bytes\nvalue = ()\n",
        body,
    )
    .unwrap();
    assert!(annotations["value"].context.is_empty());
    let errors = infer(
        &arena,
        "module Main exposing (..)\nvalue : string -> bytes\nvalue = ()\n",
        body,
    )
    .unwrap_err();
    assert!(
        matches!(
            errors.as_slice(),
            [nash_constrain::error::Error::BadExpr(..)]
        ),
        "{errors:#?}"
    );
}
