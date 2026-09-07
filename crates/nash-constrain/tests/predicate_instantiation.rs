use bumpalo::Bump;
use nash_ast::{Pred, Type as Canonical, primitives::ReprTrait};
use nash_constrain::{Type, UnionFind, instantiate, type_};
use nash_region::Located;
use std::collections::BTreeMap;

#[test]
fn apply_head_and_arguments_share_the_signature_substitution() {
    let bump = Bump::new();
    let mut uf = UnionFind::new();
    let f = type_::mk_flex_var(&mut uf);
    let a = type_::mk_flex_var(&mut uf);
    let f_type = &*bump.alloc(Type::VarN(f));
    let a_type = &*bump.alloc(Type::VarN(a));
    let scope = BTreeMap::from([("f", f_type), ("a", a_type)]);
    let head = &*bump.alloc(Located::at_zero(Canonical::Var("f")));
    let arg = &*bump.alloc(Located::at_zero(Canonical::Var("a")));
    let args = bump.alloc_slice_copy(&[arg]);
    let context = [
        Pred::Apply { head, args },
        Pred::Implied {
            trait_: ReprTrait::Big.qualified(),
            args,
        },
    ];
    let lowered = instantiate::from_src_context(&bump, &scope, &context);
    let type_::Pred::Apply {
        head: actual_head,
        args: actual_args,
    } = lowered[0]
    else {
        panic!("Apply preserved")
    };
    assert!(std::ptr::eq(actual_head, f_type));
    assert!(std::ptr::eq(actual_args[0], a_type));
    let type_::Pred::Trait { hidden, args, .. } = lowered[1] else {
        panic!("representation predicate preserved")
    };
    assert!(hidden);
    assert!(std::ptr::eq(args[0], a_type));
    let signature = Located::at_zero(Canonical::App {
        head,
        args: bump.alloc_slice_copy(&[arg]),
    });
    let Type::AppVarN(type_head, type_args) = instantiate::from_src_type(&bump, &scope, &signature)
    else {
        panic!("type application preserved")
    };
    assert!(std::ptr::eq(*type_head, actual_head));
    assert!(std::ptr::eq(type_args[0], actual_args[0]));
}
