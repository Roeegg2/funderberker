//! A global heap allocator for the kernel. Structured as a bunch of uninitable object slab allocators 

use core::{alloc::{GlobalAlloc, Layout}, array, cell::UnsafeCell, ffi::c_void, mem::MaybeUninit, ptr::{null_mut, NonNull}};
use crate::mem::vmm::slab::SlabAllocator;

#[global_allocator]
pub(super) static KERNEL_HEAP_ALLOCATOR: KernelHeapAllocator = KernelHeapAllocator::new();

pub(super) struct KernelHeapAllocator(UnsafeCell<[SlabAllocator; Self::SLAB_ALLOCATOR_COUNT]>);

impl KernelHeapAllocator {
    // TODO: Set this to the actual sizes 
    const MIN_OBJ_SIZE: usize = 4;
    const MAX_OBJ_SIZE: usize = 18;
    const SLAB_ALLOCATOR_COUNT: usize = Self::MAX_OBJ_SIZE - Self::MIN_OBJ_SIZE + 1;

    const fn new() -> Self {
        // TODO: Use a const array::from_fn here!
        Self(UnsafeCell::new([
                SlabAllocator::new(2_usize.pow(4), true),
                SlabAllocator::new(2_usize.pow(5), true),
                SlabAllocator::new(2_usize.pow(6), true),
                SlabAllocator::new(2_usize.pow(7), true),
                SlabAllocator::new(2_usize.pow(8), true),
                SlabAllocator::new(2_usize.pow(9), true),
                SlabAllocator::new(2_usize.pow(10), true),
                SlabAllocator::new(2_usize.pow(11), true),
                SlabAllocator::new(2_usize.pow(12), true),
                SlabAllocator::new(2_usize.pow(13), true),
                SlabAllocator::new(2_usize.pow(14), true),
                SlabAllocator::new(2_usize.pow(15), true),
                SlabAllocator::new(2_usize.pow(16), true),
                SlabAllocator::new(2_usize.pow(17), true),
                SlabAllocator::new(2_usize.pow(18), true),
        ]))
    }
}

unsafe impl Sync for KernelHeapAllocator {}

unsafe impl GlobalAlloc for KernelHeapAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // Use the allocator that is closest to the total size layout requires
        let index = layout.size().next_power_of_two().ilog2() as usize;
        // Try accessing allocators, and then also try to allocate
        if let Some(allocators) = unsafe {self.0.get().as_mut()} && let Some(ptr) = allocators[index].alloc() {
            return ptr.as_ptr().cast::<u8>()
        }

        // Returning NULL indicates an error 
        null_mut()
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // Use the allocator that is closest to the total size layout requires
        let index = layout.size().next_power_of_two().ilog2() as usize;

        // Convert ptr to a NonNull one + cast, get the allocator and pass the pointer to the
        // allocator
        if let Some(non_null_ptr) = NonNull::new(ptr.cast::<c_void>()) && 
            let Some(allocators) = unsafe {self.0.get().as_mut()} {
            unsafe {allocators[index].free(non_null_ptr)};
        }
    }
}
