//! A global heap allocator for the kernel. Structured as a bunch of uninitable object slab allocators

use super::slab::{SlabAllocator, ObjectStoringScheme};

use core::{
    alloc::{GlobalAlloc, Layout},
    cell::UnsafeCell,
    ffi::c_void,
    ptr::{NonNull, null_mut},
};

#[global_allocator]
pub(super) static KERNEL_HEAP_ALLOCATOR: KernelHeapAllocator = KernelHeapAllocator::new();

pub(super) struct KernelHeapAllocator(UnsafeCell<[SlabAllocator; Self::SLAB_ALLOCATOR_COUNT]>);

impl KernelHeapAllocator {
    // TODO: Set this to the actual sizes
    const MIN_OBJ_SIZE: usize = 1;
    const MAX_OBJ_SIZE: usize = 18;
    const SLAB_ALLOCATOR_COUNT: usize = Self::MAX_OBJ_SIZE - Self::MIN_OBJ_SIZE + 1;

    #[rustfmt::skip]
    const fn new() -> Self {
        // TODO: Use a const array::from_fn here!
        Self(UnsafeCell::new([
            SlabAllocator::new(Layout::new::<[u8; 2_usize.pow(1)]>(), ObjectStoringScheme::Embedded),
            SlabAllocator::new(Layout::new::<[u8; 2_usize.pow(2)]>(), ObjectStoringScheme::Embedded),
            SlabAllocator::new(Layout::new::<[u8; 2_usize.pow(3)]>(), ObjectStoringScheme::Embedded),
            SlabAllocator::new(Layout::new::<[u8; 2_usize.pow(4)]>(), ObjectStoringScheme::Embedded),
            SlabAllocator::new(Layout::new::<[u8; 2_usize.pow(5)]>(), ObjectStoringScheme::Embedded),
            SlabAllocator::new(Layout::new::<[u8; 2_usize.pow(6)]>(), ObjectStoringScheme::Embedded),
            SlabAllocator::new(Layout::new::<[u8; 2_usize.pow(7)]>(), ObjectStoringScheme::Embedded),
            SlabAllocator::new(Layout::new::<[u8; 2_usize.pow(8)]>(), ObjectStoringScheme::Embedded),
            SlabAllocator::new(Layout::new::<[u8; 2_usize.pow(9)]>(), ObjectStoringScheme::Embedded),
            SlabAllocator::new(Layout::new::<[u8; 2_usize.pow(10)]>(), ObjectStoringScheme::Embedded),
            SlabAllocator::new(Layout::new::<[u8; 2_usize.pow(11)]>(), ObjectStoringScheme::Embedded),
            SlabAllocator::new(Layout::new::<[u8; 2_usize.pow(12)]>(), ObjectStoringScheme::Embedded),
            SlabAllocator::new(Layout::new::<[u8; 2_usize.pow(13)]>(), ObjectStoringScheme::Embedded),
            SlabAllocator::new(Layout::new::<[u8; 2_usize.pow(14)]>(), ObjectStoringScheme::Embedded),
            SlabAllocator::new(Layout::new::<[u8; 2_usize.pow(15)]>(), ObjectStoringScheme::Embedded),
            SlabAllocator::new(Layout::new::<[u8; 2_usize.pow(16)]>(), ObjectStoringScheme::Embedded),
            SlabAllocator::new(Layout::new::<[u8; 2_usize.pow(17)]>(), ObjectStoringScheme::Embedded),
            SlabAllocator::new(Layout::new::<[u8; 2_usize.pow(18)]>(), ObjectStoringScheme::Embedded),
        ]))
    }
}

unsafe impl Sync for KernelHeapAllocator {}

unsafe impl GlobalAlloc for KernelHeapAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // Use the allocator that is closest to the total size layout requires
        let index = layout.size().next_power_of_two().ilog2() as usize;
        // Try accessing allocators, and then also try to allocate
        if let Some(allocators) = unsafe { self.0.get().as_mut() }
            && let Ok(ptr) = allocators[index].alloc()
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
        if let Some(non_null_ptr) = NonNull::new(ptr.cast::<c_void>())
            && let Some(allocators) = unsafe { self.0.get().as_mut() }
        {
            unsafe { let _ = allocators[index].free(non_null_ptr); };
        }
    }
}
