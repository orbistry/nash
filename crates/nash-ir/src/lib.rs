//! Nash Core IR and arena constructors.
pub mod analysis;
pub mod anf;
pub mod beta;
pub mod build;
pub mod core;
pub mod dead_code;
pub mod force_delay;
pub mod hygiene;
pub mod known_case;
pub mod pretty;
pub mod propagate;
pub mod single_use;
pub mod small_inline;
pub mod static_lift;
pub mod ty;
pub mod unused_params;

mod traverse;

#[cfg(test)]
mod test_support {
    use crate::{
        build::Builder,
        core::Core,
        ty::{RuntimeTy, Ty},
    };
    pub fn constr<'a>(b: &Builder<'a>, tag: u16, fields: &[&'a Core<'a>]) -> &'a Core<'a> {
        let types: Vec<_> = fields.iter().map(|f| f.ty).collect();
        let ty = Ty::Runtime(b.arena.alloc(RuntimeTy::Constr {
            tag,
            fields: b.arena.alloc_slice_copy(&types),
        }));
        b.constr(tag, fields, ty)
    }
}

pub mod constant_fold;
pub mod inverse;

pub mod pair_projection;
