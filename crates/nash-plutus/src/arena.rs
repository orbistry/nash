use std::any::type_name;

use append_only_vec::AppendOnlyVec;
use bumpalo::Bump;

use crate::constant::Integer;

pub struct Arena {
    bump: Bump,
    integers: AppendOnlyVec<Integer>,
}

impl Arena {
    pub fn new() -> Self {
        Self {
            bump: Bump::new(),
            integers: AppendOnlyVec::new(),
        }
    }

    pub fn from_bump(bump: Bump) -> Self {
        Self {
            bump,
            integers: AppendOnlyVec::new(),
        }
    }

    pub fn alloc<T>(&self, value: T) -> &mut T {
        if cfg!(debug_assertions) {
            assert!(
                type_name::<T>() != type_name::<Integer>(),
                "use alloc_integer for Integer types"
            );
        }
        self.bump.alloc(value)
    }

    pub fn alloc_slice_copy<T: Copy>(&self, values: &[T]) -> &[T] {
        self.bump.alloc_slice_copy(values)
    }

    pub fn alloc_slice_fill_iter<T, I>(&self, values: I) -> &mut [T]
    where
        I: IntoIterator<Item = T>,
        I::IntoIter: ExactSizeIterator,
    {
        self.bump.alloc_slice_fill_iter(values)
    }

    pub fn alloc_integer(&self, value: Integer) -> &Integer {
        let idx = self.integers.push(value);
        &self.integers[idx]
    }

    /// Borrow the allocation arena for canonical compiler metadata.
    pub fn as_bump(&self) -> &Bump {
        &self.bump
    }

    pub fn reset(&mut self) {
        // Drop all allocated integers
        self.integers = AppendOnlyVec::new();
        self.bump.reset();
    }
}

impl Default for Arena {
    fn default() -> Self {
        Self::new()
    }
}
