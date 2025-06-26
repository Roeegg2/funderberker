//! A global heap allocator for the kernel. Structured as a bunch of uninitable object slab allocators

use utils::{collections::stacklist::Node, sync::spinlock::{SpinLock, SpinLockGuard}};

use super::internal::InternalSlabAllocator;

use core::{
    alloc::{GlobalAlloc, Layout},
    ptr::{NonNull, null_mut},
};

/// A global heap allocator for the kernel. Structured as a bunch of uninitable object slab
/// allocators
#[derive(Debug)]
pub struct Heap {
    slab_64: SpinLock<InternalSlabAllocator>,
    slab_128: SpinLock<InternalSlabAllocator>,
    slab_256: SpinLock<InternalSlabAllocator>,
    slab_512: SpinLock<InternalSlabAllocator>,
    slab_1024: SpinLock<InternalSlabAllocator>,
    slab_2048: SpinLock<InternalSlabAllocator>,
    slab_4096: SpinLock<InternalSlabAllocator>,
}

impl Heap {
    pub const fn new() -> Self {
        unsafe {
            Self {
                slab_64: SpinLock::new(InternalSlabAllocator::new(Layout::from_size_align_unchecked(64, 8))),
                slab_128: SpinLock::new(InternalSlabAllocator::new(Layout::from_size_align_unchecked(128, 8))),
                slab_256: SpinLock::new(InternalSlabAllocator::new(Layout::from_size_align_unchecked(256, 8))),
                slab_512: SpinLock::new(InternalSlabAllocator::new(Layout::from_size_align_unchecked(512, 8))),
                slab_1024: SpinLock::new(InternalSlabAllocator::new(Layout::from_size_align_unchecked(1024, 8))),
                slab_2048: SpinLock::new(InternalSlabAllocator::new(Layout::from_size_align_unchecked(2048, 8))),
                slab_4096: SpinLock::new(InternalSlabAllocator::new(Layout::from_size_align_unchecked(4096, 8))),
            }
        }
    }

    fn layout_to_allocator(&self, layout: Layout) -> SpinLockGuard<'_, InternalSlabAllocator> {
        if layout.size() <= 64 {
            self.slab_64.lock()
        } else if layout.size() <= 128 {
            self.slab_128.lock()
        } else if layout.size() <= 256 {
            self.slab_256.lock()
        } else if layout.size() <= 512 {
            self.slab_512.lock()
        } else if layout.size() <= 1024 {
            self.slab_1024.lock()
        } else if layout.size() <= 2048 {
            self.slab_2048.lock()
        } else if layout.size() <= 4096 {
            self.slab_4096.lock()
        } else {
            panic!("No allocator for size {} and alignment {}", layout.size(), layout.align());
        }
        
    }
}


unsafe impl GlobalAlloc for Heap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut allocator = self.layout_to_allocator(layout);

        if let Ok(ptr) = allocator.allocate() {
            return ptr.as_ptr().cast::<u8>()
        }

        null_mut()
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let mut allocator = self.layout_to_allocator(layout);
        
        let ptr = NonNull::new(ptr).expect("Tried to deallocate a null pointer").cast::<Node<()>>();
        // SAFETY: We are deallocating a pointer that was allocated by this allocator
        unsafe {
            allocator.free(ptr).unwrap();
        }

    }
}

unsafe impl Sync for Heap {}


impl Default for Heap {
    fn default() -> Self {
        Self::new()
    }
}
