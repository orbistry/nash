//! Port of Elm's `Type.Constrain.Module`.
//!
//! Nash has no ports or effect managers, so this is just the declaration
//! walk terminated by `CSaveTheEnvironment`.

use bumpalo::Bump;
use nash_ast::{Decls, Module as CanModule};

use crate::expression;
use crate::type_::Constraint;
use crate::union_find::UnionFind;

pub fn constrain<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    module: &CanModule<'a>,
) -> Constraint<'a> {
    let definitions = module
        .traits
        .iter()
        .flat_map(|trait_| {
            trait_
                .value
                .methods
                .iter()
                .filter_map(|method| method.default)
        })
        .chain(
            module
                .impls
                .iter()
                .flat_map(|impl_| impl_.value.methods.iter().copied()),
        );
    let mut methods: Vec<_> = definitions
        .map(|definition| expression::constrain_method(bump, uf, definition))
        .collect();
    methods.push(Constraint::SaveTheEnvironment);
    constrain_decls(
        bump,
        uf,
        module.decls,
        Constraint::And(bump.alloc_slice_fill_iter(methods)),
    )
}

fn constrain_decls<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    decls: &Decls<'a>,
    final_constraint: Constraint<'a>,
) -> Constraint<'a> {
    match decls {
        Decls::Declare { definition, next } => {
            let next_con = constrain_decls(bump, uf, next, final_constraint);
            expression::constrain_def(bump, uf, &expression::Rtv::new(), definition, next_con)
        }

        Decls::DeclareRec {
            definition,
            following,
            next,
        } => {
            let next_con = constrain_decls(bump, uf, next, final_constraint);
            let mut defs = Vec::with_capacity(1 + following.len());
            defs.push(*definition);
            defs.extend(following.iter().copied());
            expression::constrain_recursive_defs(bump, uf, &expression::Rtv::new(), &defs, next_con)
        }

        Decls::Empty => final_constraint,
    }
}
