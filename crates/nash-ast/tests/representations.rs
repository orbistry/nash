use nash_ast::Kind;
use nash_ast::primitives::{self, Repr, ReprTrait};
use nash_ast::{Pred, Type};
use nash_region::{Located, Region};

#[test]
fn obligation_identity_ignores_locations_and_visibility_but_keeps_apply_head() {
    let a = Located::at_zero(Type::Var("a"));
    let relocated = Located::at(Region::one(), Type::Var("a"));
    let first = [&a];
    let second = [&relocated];
    let written = Pred::Trait {
        trait_: ReprTrait::Big.qualified(),
        args: &first,
    };
    let implied = Pred::Implied {
        trait_: ReprTrait::Big.qualified(),
        args: &second,
    };
    assert_eq!(written.key(), implied.key());
    let f = Located::at_zero(Type::Var("f"));
    let g = Located::at_zero(Type::Var("g"));
    let apply_f = Pred::Apply {
        head: &f,
        args: &first,
    };
    let apply_g = Pred::Apply {
        head: &g,
        args: &first,
    };
    assert_ne!(apply_f.key(), apply_g.key());
    assert_eq!(apply_f.types().count(), 2);
    assert_eq!(
        std::collections::HashSet::from([
            written.key(),
            implied.key(),
            apply_f.key(),
            apply_g.key()
        ])
        .len(),
        3
    );
}

#[test]
fn container_kinds_do_not_encode_representation() {
    let get = |name| {
        primitives::PRIMITIVES
            .iter()
            .find(|p| p.name == name)
            .unwrap()
    };
    assert_eq!(get("List").kind, get("list").kind);
    assert_eq!(get("List").repr, Repr::Big);
    assert_eq!(get("list").repr, Repr::Const);
    assert_eq!(get("List").context, &[(0, ReprTrait::Big)]);
    assert_eq!(get("list").context, &[(0, ReprTrait::Storable)]);
    assert_eq!(
        get("pair").context,
        &[(0, ReprTrait::Storable), (1, ReprTrait::Storable)]
    );
}

#[test]
fn higher_order_kind_structure_is_not_just_arity() {
    let typ = Kind::Type;
    let unary = Kind::Arrow(&typ, &typ);
    let higher = Kind::Arrow(&unary, &typ);
    assert_eq!(unary.arity(), higher.arity());
    assert_ne!(unary, higher);
}

#[test]
fn representation_identity_is_qualified_and_implication_is_directional() {
    assert_eq!(
        ReprTrait::of(ReprTrait::Big.qualified()),
        Some(ReprTrait::Big)
    );
    let mut foreign = ReprTrait::Big.qualified();
    foreign.home.package = None;
    assert_eq!(ReprTrait::of(foreign), None);
    assert_eq!(ReprTrait::Big.supers(), &[ReprTrait::Storable]);
    assert_eq!(
        ReprTrait::Const.supers(),
        &[ReprTrait::Storable, ReprTrait::Little]
    );
    assert!(ReprTrait::Storable.supers().is_empty());
    assert!(
        ReprTrait::Big
            .admits()
            .intersect(ReprTrait::Little.admits())
            .is_empty()
    );
    let narrow = ReprTrait::Storable
        .admits()
        .intersect(ReprTrait::Little.admits());
    assert!(narrow.contains(Repr::Const));
    assert!(!narrow.contains(Repr::Big));
    assert!(!narrow.contains(Repr::Term));
}
