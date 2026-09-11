use super::*;
use crate::{
    core::*,
    ty::{ConstTy, Ty},
};
use nash_plutus::{arena::Arena, builtin::DefaultFunction};

fn fixture<'a>(b: &Builder<'a>, leaf: &'a Core<'a>) -> &'a Core<'a> {
    let binder = Binder {
        name: b.fresh("x"),
        ty: Ty::Const(&ConstTy::Int),
    };
    b.constr(
        7,
        &[
            b.var(binder.name),
            b.int(2),
            b.lam(&[binder], leaf),
            b.app(leaf, &[leaf]),
            b.let_(binder, leaf, leaf),
            b.let_rec(
                &[RecBinder {
                    binder,
                    params: b.arena.alloc_slice_copy(&[binder]),
                    static_params: &[0],
                    body: leaf,
                }],
                leaf,
            ),
            b.case(
                CaseKind::Int,
                leaf,
                &[Branch {
                    test: Test::Int(nash_plutus::constant::integer_from(b.arena, 7)),
                    binders: b.arena.alloc_slice_copy(&[binder]),
                    body: leaf,
                }],
                Some(leaf),
            ),
            b.constr(4, &[leaf]),
            b.field(leaf, 0, 1),
            b.builtin(DefaultFunction::AddInteger, &[leaf, leaf]),
            b.cast(
                CastKind::Lift,
                Ty::Const(&ConstTy::Int),
                Ty::Big(&crate::ty::BigTy::Int),
                leaf,
            ),
            b.trace(leaf, leaf),
            b.error(),
            b.delay(leaf),
            b.force(leaf),
        ],
    )
}
#[test]
fn every_variant_is_walked_and_noop_map_reuses_root() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let root = fixture(&b, b.int(1));
    let mut preorder = Vec::new();
    root.walk(&mut |node| preorder.push(node));
    assert!(std::ptr::eq(preorder[0], root));
    let variants = preorder
        .iter()
        .map(|node| std::mem::discriminant(*node))
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(variants.len(), 15);
    let mut postorder = Vec::new();
    let mapped = root.map(&b, &mut |node| {
        postorder.push(node);
        None
    });
    assert!(std::ptr::eq(mapped, root));
    assert_eq!(preorder.len(), postorder.len());
    assert!(std::ptr::eq(*postorder.last().unwrap(), root));
}
#[test]
fn replacement_reaches_all_child_positions_and_preserves_metadata() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let leaf = b.int(1);
    let root = fixture(&b, leaf);
    let replacement = b.int(9);
    let mapped = map(&b, root, &mut |node| {
        std::ptr::eq(node, leaf).then_some(replacement)
    });
    assert!(!std::ptr::eq(mapped, root));
    mapped.walk(&mut |node| assert!(!std::ptr::eq(node, leaf)));
    let (Core::Constr { fields: old, .. }, Core::Constr { tag, fields: new }) = (root, mapped)
    else {
        panic!()
    };
    assert_eq!(*tag, 7);
    assert!(std::ptr::eq(old[0], new[0]));
    assert!(std::ptr::eq(old[1], new[1]));
    let (Core::Lam { params: p, .. }, Core::Lam { params: q, .. }) = (old[2], new[2]) else {
        panic!()
    };
    assert!(std::ptr::eq(*p, *q));
    let (Core::LetRec { binders: p, .. }, Core::LetRec { binders: q, .. }) = (old[5], new[5])
    else {
        panic!()
    };
    assert_eq!(p[0].binder.name, q[0].binder.name);
    assert_eq!(p[0].binder.ty, q[0].binder.ty);
    assert!(std::ptr::eq(p[0].params, q[0].params));
    assert!(std::ptr::eq(p[0].static_params, q[0].static_params));
    let (Core::Case { branches: p, .. }, Core::Case { branches: q, .. }) = (old[6], new[6]) else {
        panic!()
    };
    assert_eq!(p[0].test, q[0].test);
    assert!(std::ptr::eq(p[0].binders, q[0].binders));
}
#[test]
fn unchanged_child_slices_survive_parent_rebuild_and_replacements_are_not_revisited() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let leaf = b.int(1);
    let other = b.int(2);
    let root = b.case(
        CaseKind::Int,
        other,
        &[Branch {
            test: Test::Int(nash_plutus::constant::integer_from(&a, 0)),
            binders: &[],
            body: other,
        }],
        Some(leaf),
    );
    let replacement = b.delay(leaf);
    let mut replacements = 0;
    let mapped = root.map(&b, &mut |node| {
        if std::ptr::eq(node, leaf) {
            replacements += 1;
            Some(replacement)
        } else {
            None
        }
    });
    assert_eq!(replacements, 1);
    let (Core::Case { branches: p, .. }, Core::Case { branches: q, .. }) = (root, mapped) else {
        panic!()
    };
    assert!(std::ptr::eq(*p, *q));
}

#[test]
fn visitor_observes_mapped_children_and_identity_replacement_is_a_noop() {
    let arena = Arena::new();
    let build = Builder::new(&arena);
    let leaf = build.int(1);
    let root = build.delay(leaf);
    assert!(std::ptr::eq(root.map(&build, &mut |node| Some(node)), root));
    let child = build.int(2);
    let result = build.int(3);
    let mapped = root.map(&build, &mut |node| {
        if std::ptr::eq(node, leaf) {
            Some(child)
        } else if matches!(node, Core::Delay(body) if std::ptr::eq(*body, child)) {
            Some(result)
        } else {
            None
        }
    });
    assert!(std::ptr::eq(mapped, result));
}
