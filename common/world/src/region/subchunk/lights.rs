use std::alloc::{Allocator, Global};
use std::ptr::NonNull;

use crate::region::alloc::MaybeOwnedArray;
use crate::voxel::Light;

static UNIFORM_AMBIENT_NONE: [Light; 32768] = [Light::AMBIENT_NONE; 32768];
static UNIFORM_AMBIENT_FULL: [Light; 32768] = [Light::AMBIENT_FULL; 32768];

pub struct Lights<A: Allocator = Global>(MaybeOwnedArray<Light, A, 32768>);

impl<A: Allocator> Lights<A> {
    pub const fn uniform_ambient_none(alloc: A) -> Self {
        let ptr = unsafe { NonNull::new_unchecked(&UNIFORM_AMBIENT_NONE as *const _ as *mut _) };
        Self(unsafe { MaybeOwnedArray::borrowed(ptr, alloc) })
    }

    pub const fn uniform_ambient_full(alloc: A) -> Self {
        let ptr = unsafe { NonNull::new_unchecked(&UNIFORM_AMBIENT_FULL as *const _ as *mut _) };
        Self(unsafe { MaybeOwnedArray::borrowed(ptr, alloc) })
    }

    /// Construct a new lightmap with a fill value and allocator.
    /// If the fill value is Light::AMBIENT_FULL or Light::AMBIENT_NONE,
    /// no pointer will be allocated and the lightmap will point to a static
    /// uniform buffer.
    #[inline]
    pub fn new(fill: Light, alloc: A) -> Self {
        if fill == Light::AMBIENT_FULL {
            Self::uniform_ambient_full(alloc)
        } else if fill == Light::AMBIENT_NONE {
            Self::uniform_ambient_none(alloc)
        } else {
            let mut v = Self::uniform_ambient_full(alloc);
            v.fill(fill);
            v
        }
    }

    /// Assign a value to every index.
    ///
    /// If the lght value is Light::AMBIENT_FULL or Light::AMBIENT_NONE,
    /// the buffer will be de-allocated (if owned) and replaced with a pointer
    /// to a static uniform buffer.
    pub fn fill(&mut self, light: Light) {
        unsafe {
            // If the lightmap is borrowed, it is uniform, and may
            // already be filled with the desired light value.
            if self.0.is_borrowed() {
                if *self.0.get_unchecked(0) == light {
                    return;
                } else {
                    self.0.to_owned_ptr();
                }
            }

            // write fill value to each element.
            self.0.fill(light)
        }
    }

    /// Get the light value of the voxel at this index.
    /// i must be less than 32768.
    #[inline(always)]
    pub const unsafe fn get(&self, i: usize) -> Light {
        debug_assert!(i < 32768);
        unsafe { *self.0.get_unchecked(i) }
    }

    /// Assign the light value of the voxel at this index.
    /// i must be less than 32768.
    #[inline(always)]
    pub unsafe fn set(&mut self, i: usize, v: Light) {
        unsafe {
            self.replace(i, v);
        }
    }

    /// Assign the light value of the voxel at this index.
    /// i must be less than 32768.
    #[inline(always)]
    pub unsafe fn replace(&mut self, i: usize, v: Light) -> Light {
        debug_assert!(i < 32768);
        unsafe {
            let curr = *self.0.get_unchecked(i);
            if self.0.is_borrowed() && v != curr {
                self.0.to_owned_ptr();
            }
            self.0.set_unchecked(i, v);
            curr
        }
    }
}
