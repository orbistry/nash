use bumpalo::Bump;
use nash_ast::{Kind, Pred, Type, primitives::ReprTrait};
use nash_can::{Interface, InterfaceUnion, UnionVisibility};
use nash_driver::interface::Interface as Cached;
use nash_region::Located;

#[test]
fn private_constructor_context_changes_invalidate_the_interface() {
    let bump = Bump::new();
    let a = &*bump.alloc(Located::at_zero(Type::Var("a")));
    let make = |context| Interface {
        home: nash_ast::ModuleName {
            package: None,
            name: "Types",
        },
        impls: &[],
        traits: &[],
        values: &[],
        aliases: &[],
        binops: &[],
        unions: &*bump.alloc_slice_copy(&[InterfaceUnion {
            name: "hidden",
            kind: &Kind::Arrow(&Kind::Type, &Kind::Type),
            context,
            parameters: &["a"],
            ctors: &[],
            alternatives: 0,
            options: nash_ast::CtorOpts::Normal,
            visibility: UnionVisibility::Private,
        }]),
    };
    let unbounded = make(&[]);
    let bounded = make(bump.alloc_slice_copy(&[Pred::Implied {
        trait_: ReprTrait::Storable.qualified(),
        args: bump.alloc_slice_copy(&[a]),
    }]));
    let first = Cached::from_canonical(&unbounded);
    let second = Cached::from_canonical(&bounded);
    assert_eq!(first.exports, second.exports);
    assert!(first.differs_from(&second));
    assert!(!second.differs_from(&Cached::from_canonical(&bounded)));
}

#[test]
fn application_argument_order_changes_the_interface_fingerprint() {
    let bump = Bump::new();
    let variable = |name| &*bump.alloc(Located::at_zero(Type::Var(name)));
    let head = variable("f");
    let a = variable("a");
    let b = variable("b");
    let make = |args| Interface {
        home: nash_ast::ModuleName {
            package: None,
            name: "Types",
        },
        impls: &[],
        traits: &[],
        aliases: &[],
        binops: &[],
        unions: &[],
        values: bump.alloc_slice_fill_iter([nash_can::InterfaceValue {
            name: "apply",
            annotation: bump.alloc(nash_ast::Annotation {
                free_vars: &["f", "a", "b"],
                context: bump.alloc_slice_fill_iter([Pred::Apply { head, args }]),
                typ: head,
            }),
        }]),
    };
    let forward = make(bump.alloc_slice_copy(&[a, b]));
    let reverse = make(bump.alloc_slice_copy(&[b, a]));
    let first = Cached::from_canonical(&forward);
    let second = Cached::from_canonical(&reverse);
    assert_eq!(first.exports, second.exports);
    assert!(first.differs_from(&second));
    assert!(!first.differs_from(&Cached::from_canonical(&forward)));
}
