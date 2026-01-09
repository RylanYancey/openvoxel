use std::{
    alloc::{Allocator, Layout},
    ptr::NonNull,
};

pub type RegionAlloc = std::alloc::Global;

pub(crate) fn init_region_alloc() -> RegionAlloc {
    std::alloc::Global
}

/// Whether a pointer is owned and needs to be de-allocated on drop,
/// or is borrowed and does not need to de-allocate.
///
/// This is used by the `Voxels` and `Lightmap` structs to
/// point empty or uniform buffers to a static instead of
/// allocating.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u8)]
pub enum AllocationKind {
    /// The pointer is to a shared span of memory.
    /// It is up to you to ensure the memory is
    /// valid.
    Borrowed,

    /// The pointer is owned and should be freed
    /// when the owned drops.
    Owned,
}

impl AllocationKind {
    pub const fn is_borrowed(self) -> bool {
        matches!(self, AllocationKind::Borrowed)
    }

    pub const fn is_owned(self) -> bool {
        matches!(self, AllocationKind::Owned)
    }
}

pub struct MaybeOwnedArray<T: Clone, A: Allocator, const N: usize> {
    ptr_kind: AllocationKind,
    ptr: NonNull<T>,
    alloc: A,
}

impl<T: Clone, A: Allocator, const N: usize> MaybeOwnedArray<T, A, N> {
    pub fn new(alloc: A, fill: T) -> Self {
        Self {
            ptr_kind: AllocationKind::Owned,
            ptr: unsafe {
                let ptr = Self::new_owned_uninit(&alloc);
                for i in 0..N {
                    // write without dropping existing value.
                    ptr.add(i).write(fill.clone());
                }
                ptr
            },
            alloc,
        }
    }

    unsafe fn new_owned_uninit(alloc: &A) -> NonNull<T> {
        let layout = Layout::array::<T>(N).unwrap();
        alloc
            .allocate(layout)
            .unwrap()
            .as_non_null_ptr()
            .cast::<T>()
    }

    pub const unsafe fn get_unchecked(&self, i: usize) -> &T {
        unsafe { self.ptr.add(i).as_ref() }
    }

    pub const unsafe fn get_unchecked_mut(&mut self, i: usize) -> &mut T {
        unsafe { self.ptr.add(i).as_mut() }
    }

    pub unsafe fn set_unchecked(&mut self, i: usize, val: T) {
        unsafe { *self.ptr.add(i).as_mut() = val }
    }

    pub unsafe fn replace_unchecked(&mut self, i: usize, val: T) -> T {
        std::mem::replace(unsafe { self.ptr.add(i).as_mut() }, val)
    }

    pub const fn is_owned(&self) -> bool {
        self.ptr_kind.is_owned()
    }

    pub const fn is_borrowed(&self) -> bool {
        self.ptr_kind.is_borrowed()
    }

    /// Convert to an owned pointer.
    /// Returns "false" if the pointer is already owned.
    pub fn to_owned_ptr(&mut self) -> bool {
        if self.is_borrowed() {
            unsafe {
                let ptr = Self::new_owned_uninit(&self.alloc);
                ptr.copy_from_nonoverlapping(self.ptr, N);
                self.ptr = ptr;
                self.ptr_kind = AllocationKind::Owned;
            }
            true
        } else {
            false
        }
    }

    pub const unsafe fn borrowed(ptr: NonNull<[T; N]>, alloc: A) -> Self {
        Self {
            ptr_kind: AllocationKind::Borrowed,
            ptr: ptr.cast::<T>(),
            alloc,
        }
    }

    pub const unsafe fn owned(ptr: NonNull<[T; N]>, alloc: A) -> Self {
        Self {
            ptr_kind: AllocationKind::Owned,
            ptr: ptr.cast::<T>(),
            alloc,
        }
    }

    pub fn fill(&mut self, value: T) {
        for i in 0..N {
            unsafe { self.set_unchecked(i, value.clone()) };
        }
    }
}

impl<T: Clone, A: Allocator, const N: usize> Drop for MaybeOwnedArray<T, A, N> {
    fn drop(&mut self) {
        if self.ptr_kind.is_owned() {
            let layout = Layout::new::<[T; N]>();
            unsafe { self.alloc.deallocate(self.ptr.cast::<u8>(), layout) }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{alloc::Global, ptr::NonNull};

    use crate::region::alloc::MaybeOwnedArray;

    #[test]
    fn borrowed_to_owned() {
        static BORROWED: [u8; 8] = [0, 1, 2, 3, 4, 5, 6, 7];
        unsafe {
            let ptr = NonNull::new_unchecked(&BORROWED as *const _ as *mut [u8; 8]);
            let mut arr = MaybeOwnedArray::borrowed(ptr, &Global);
            assert_eq!(*arr.get_unchecked(4), 4);
            assert!(arr.to_owned_ptr());
            arr.set_unchecked(4, 99);
            assert_eq!(*arr.get_unchecked(4), 99);
        }
    }
}
