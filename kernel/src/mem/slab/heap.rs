//! A global heap allocator for the kernel. Structured as a bunch of uninitable object slab allocators

use super::internal::{InternalSlabAllocator, ObjectNode};

use core::{
    alloc::{GlobalAlloc, Layout},
    cell::UnsafeCell,
    ptr::{NonNull, null_mut},
};

/// The global instance of the kernel heap allocator
#[global_allocator]
pub(super) static KERNEL_HEAP_ALLOCATOR: KernelHeapAllocator = KernelHeapAllocator::new();

/// A global heap allocator for the kernel. Structured as a bunch of uninitable object slab
/// allocators
#[derive(Debug)]
pub(super) struct KernelHeapAllocator([UnsafeCell<InternalSlabAllocator>; Self::SIZE]);

/// A macro to make creating slab allocators easier
macro_rules! create_slab_allocators {
    ($($size:expr),*) => {
        [
            $(UnsafeCell::new( InternalSlabAllocator::new(Layout::new::<[u8; $size]>())),)*
        ]
    };
}

impl KernelHeapAllocator {
    const MIN_POW: usize = 16_usize.ilog2() as usize;
    const MAX_POW: usize = 16384_usize.ilog2() as usize;
    const SIZE: usize = Self::MAX_POW - Self::MIN_POW + 1;

    /// Create a new instance of the kernel heap allocator
    #[rustfmt::skip]
    const fn new() -> Self {
        // TODO: Use a const array::from_fn here!
        // TODO: Benchmark and possibly change the slab allocator sizes
        Self(create_slab_allocators!(
            2_usize.pow(Self::MIN_POW as u32),
            2_usize.pow(Self::MIN_POW as u32 + 1),
            2_usize.pow(Self::MIN_POW as u32 + 2),
            2_usize.pow(Self::MIN_POW as u32 + 3),
            2_usize.pow(Self::MIN_POW as u32 + 4),
            2_usize.pow(Self::MIN_POW as u32 + 5),
            2_usize.pow(Self::MIN_POW as u32 + 6),
            2_usize.pow(Self::MIN_POW as u32 + 7),
            2_usize.pow(Self::MIN_POW as u32 + 8),
            2_usize.pow(Self::MIN_POW as u32 + 9),
            2_usize.pow(Self::MIN_POW as u32 + 10)
        ))
    }

    /// Get the index of the allocator that is closest to the total size layout requires
    #[inline]
    const fn get_matching_allocator_index(layout: Layout) -> usize {
        let pow = layout.pad_to_align().size().next_power_of_two().ilog2() as usize;
        assert!(pow <= Self::MAX_POW);
        if pow <= Self::MIN_POW {
            return 0;
        }

        pow - Self::MIN_POW
    }
}

// XXX: Make this actually Syncable
unsafe impl Sync for KernelHeapAllocator {}

unsafe impl GlobalAlloc for KernelHeapAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // Use the allocator that is closest to the total size layout requires
        let index = KernelHeapAllocator::get_matching_allocator_index(layout);

        // Try accessing allocators, and then also try to allocate
        if let Some(allocator) = unsafe { self.0[index].get().as_mut() }
            && let Ok(ptr) = allocator.allocate()
        {
            return ptr.as_ptr().cast::<u8>();
        }

        // Returning NULL indicates an error
        null_mut()
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // Use the allocator that is closest to the total size layout requires
        let index = layout.size().next_power_of_two().ilog2() as usize;

        // Convert ptr to a NonNull one + cast, get the allocator and pass the pointer to the
        // allocator
        if let Some(non_null_ptr) = NonNull::new(ptr.cast::<ObjectNode>())
            && let Some(allocator) = unsafe { self.0[index].get().as_mut() }
        {
            unsafe {
                let _ = allocator.free(non_null_ptr);
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::{boxed::Box, string::ToString, vec::Vec};
    use macros::test_fn;

    #[test_fn]
    fn test_heap_generic_allocs() {
        let a = Box::new(5_usize);
        let string = Box::new("Hello, World!".to_string());
        assert_eq!(*a, 5);
        assert_eq!(*string, "Hello, World!".to_string());
        drop(a);
        drop(string);

        let a = Box::new([100_usize, 12_usize, 42_usize]);
        let part1 = Box::new("Hello there");
        {
            let part2 = Box::new("General Kenobi");
            let b = Box::new(200);
            let mut v = Vec::new();

            for i in 0..100 {
                v.push(Box::new(i));
            }

            for i in 0..100 {
                assert_eq!(*v.pop().unwrap(), 99 - i);
            }

            assert_eq!(*b, 200);
            assert_eq!(*part2, "General Kenobi");
        }

        assert_eq!(*a, [100, 12, 42]);
        assert_eq!(*part1, "Hello there");

        let a = Box::new(());
        assert_eq!(*a, ());
    }
}
