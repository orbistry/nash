use bumpalo::Bump;
use indoc::indoc;
use nash_can::Context;
use nash_constrain::UnionFind;

pub const LITERAL_SOURCE: &str = indoc!(
    "
        module Literal exposing (..)
        import Primitive exposing (..)
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
        trait FromBool 'a where
            fromBool : bool -> 'a

        trait FromUnit 'a where
            fromUnit : unit -> 'a

        impl FromBool bool where
            fromBool value = value

        impl FromUnit unit where
            fromUnit value = value
    "
);

pub fn literal_interfaces(
    bump: &Bump,
) -> std::collections::BTreeMap<&str, nash_can::Interface<'_>> {
    let mut interfaces =
        std::collections::BTreeMap::from([("Builtin", nash_can::kinds::builtin_interface(bump))]);
    let source = bump.alloc_str(LITERAL_SOURCE);
    let module = nash_parse::Parser::new(bump, source).module().unwrap();
    let can = nash_can::canonicalize(
        bump,
        Context {
            package: Some(nash_ast::primitives::BASE),
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
