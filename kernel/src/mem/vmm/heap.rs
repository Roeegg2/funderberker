//! A global heap allocator for the kernel. Structured as a bunch of uninitable object slab allocators

use super::slab::{InternalSlabAllocator, Object};

use core::{
    alloc::{GlobalAlloc, Layout},
    cell::UnsafeCell,
    ptr::{NonNull, null_mut},
};

#[global_allocator]
pub(super) static KERNEL_HEAP_ALLOCATOR: KernelHeapAllocator = KernelHeapAllocator::new();

#[derive(Debug)]
pub(super) struct KernelHeapAllocator(
    UnsafeCell<[InternalSlabAllocator; Self::SLAB_ALLOCATOR_COUNT]>,
);

impl KernelHeapAllocator {
    // TODO: Set this to the actual sizes
    const MIN_OBJ_SIZE: usize = 8;
    const MAX_OBJ_SIZE: usize = 29;
    const SLAB_ALLOCATOR_COUNT: usize = Self::MAX_OBJ_SIZE - Self::MIN_OBJ_SIZE;

    #[rustfmt::skip]
    const fn new() -> Self {
        // TODO: Use a const array::from_fn here!
        Self(UnsafeCell::new([
            unsafe {InternalSlabAllocator::new(Layout::new::<[u8; 2_usize.pow(8)]>(), true)},
            unsafe {InternalSlabAllocator::new(Layout::new::<[u8; 2_usize.pow(9)]>(), true)},
            unsafe {InternalSlabAllocator::new(Layout::new::<[u8; 2_usize.pow(10)]>(), true)},
            unsafe {InternalSlabAllocator::new(Layout::new::<[u8; 2_usize.pow(11)]>(), true)},
            unsafe {InternalSlabAllocator::new(Layout::new::<[u8; 2_usize.pow(12)]>(), true)},
            unsafe {InternalSlabAllocator::new(Layout::new::<[u8; 2_usize.pow(13)]>(), true)},
            unsafe {InternalSlabAllocator::new(Layout::new::<[u8; 2_usize.pow(14)]>(), true)},
            unsafe {InternalSlabAllocator::new(Layout::new::<[u8; 2_usize.pow(15)]>(), true)},
            unsafe {InternalSlabAllocator::new(Layout::new::<[u8; 2_usize.pow(16)]>(), true)},
            unsafe {InternalSlabAllocator::new(Layout::new::<[u8; 2_usize.pow(17)]>(), true)},
            unsafe {InternalSlabAllocator::new(Layout::new::<[u8; 2_usize.pow(19)]>(), true)},
            unsafe {InternalSlabAllocator::new(Layout::new::<[u8; 2_usize.pow(20)]>(), true)},
            unsafe {InternalSlabAllocator::new(Layout::new::<[u8; 2_usize.pow(21)]>(), true)},
            unsafe {InternalSlabAllocator::new(Layout::new::<[u8; 2_usize.pow(22)]>(), true)},
            unsafe {InternalSlabAllocator::new(Layout::new::<[u8; 2_usize.pow(23)]>(), true)},
            unsafe {InternalSlabAllocator::new(Layout::new::<[u8; 2_usize.pow(24)]>(), true)},
            unsafe {InternalSlabAllocator::new(Layout::new::<[u8; 2_usize.pow(25)]>(), true)},
            unsafe {InternalSlabAllocator::new(Layout::new::<[u8; 2_usize.pow(26)]>(), true)},
            unsafe {InternalSlabAllocator::new(Layout::new::<[u8; 2_usize.pow(27)]>(), true)},
            unsafe {InternalSlabAllocator::new(Layout::new::<[u8; 2_usize.pow(28)]>(), true)},
            unsafe {InternalSlabAllocator::new(Layout::new::<[u8; 2_usize.pow(29)]>(), true)},
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
        if let Some(non_null_ptr) = NonNull::new(ptr.cast::<Object>())
            && let Some(allocators) = unsafe { self.0.get().as_mut() }
        {
            unsafe {
                let _ = allocators[index].free(non_null_ptr);
            };
        }
    }
}
