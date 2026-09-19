//! The solver keeps nominal record identity in the error types it produces.

use std::collections::BTreeMap;

use nash_ast::primitives;
use nash_constrain::ErrorType;
use nash_constrain::type_::make_descriptor;
use nash_constrain::{Content, FlatType, UnionFind};

#[test]
fn solver_error_producer_retains_record_identity() {
    let arena = bumpalo::Bump::new();
    let mut uf = UnionFind::new();
    let body = nash_region::Located::at_zero(nash_ast::Type::Record { fields: &[] });
    let real = uf.fresh(make_descriptor(Content::Structure(FlatType::Record1(
        BTreeMap::new(),
    ))));
    let little = uf.fresh(make_descriptor(Content::Alias {
        home: primitives::builtin_home(),
        name: "littleRecord",
        args: vec![],
        real,
        body: &body,
    }));
    let produced = nash_solve::to_error_type(&arena, &mut uf, little);
    assert!(matches!(
        produced,
        ErrorType::Alias { home, name: "littleRecord", real: ErrorType::Record { .. }, .. }
            if *home == primitives::builtin_home()
    ));
}
