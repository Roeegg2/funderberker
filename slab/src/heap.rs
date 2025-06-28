//! A global heap allocator for the kernel. Structured as a bunch of uninitable object slab allocators

use utils::sync::spinlock::{SpinLock, SpinLockGuard};

use super::internal::InternalSlabAllocator;

use core::{
    alloc::{GlobalAlloc, Layout},
    ptr::{NonNull, null_mut},
};

/// A global heap allocator for the kernel. Structured as a bunch of uninitable object slab
/// allocators
#[derive(Debug)]
pub struct Heap {
    slab_32: SpinLock<InternalSlabAllocator>,
    slab_64: SpinLock<InternalSlabAllocator>,
    slab_128: SpinLock<InternalSlabAllocator>,
    slab_256: SpinLock<InternalSlabAllocator>,
    slab_512: SpinLock<InternalSlabAllocator>,
    slab_1024: SpinLock<InternalSlabAllocator>,
    slab_2048: SpinLock<InternalSlabAllocator>,
    slab_4096: SpinLock<InternalSlabAllocator>,
}

impl Default for Heap {
    fn default() -> Self {
        Self::new()
    }
}

impl Heap {
    #[must_use]
    pub const fn new() -> Self {
        // SAFETY: The sizes and alignments are guaranteed to be valid for the slab allocator
        unsafe {
            Self {
                slab_32: SpinLock::new(InternalSlabAllocator::new(
                    Layout::from_size_align_unchecked(32, 32),
                )),
                slab_64: SpinLock::new(InternalSlabAllocator::new(
                    Layout::from_size_align_unchecked(64, 64),
                )),
                slab_128: SpinLock::new(InternalSlabAllocator::new(
                    Layout::from_size_align_unchecked(128, 128),
                )),
                slab_256: SpinLock::new(InternalSlabAllocator::new(
                    Layout::from_size_align_unchecked(256, 256),
                )),
                slab_512: SpinLock::new(InternalSlabAllocator::new(
                    Layout::from_size_align_unchecked(512, 512),
                )),
                slab_1024: SpinLock::new(InternalSlabAllocator::new(
                    Layout::from_size_align_unchecked(1024, 1024),
                )),
                slab_2048: SpinLock::new(InternalSlabAllocator::new(
                    Layout::from_size_align_unchecked(2048, 2048),
                )),
                slab_4096: SpinLock::new(InternalSlabAllocator::new(
                    Layout::from_size_align_unchecked(4096, 4096),
                )),
            }
        }
    }

    #[must_use]
    fn layout_to_allocator(&self, mut layout: Layout) -> SpinLockGuard<'_, InternalSlabAllocator> {
        // Padding since we're storing the slabs sequentially in memory
        layout = layout.pad_to_align();
        if layout.size() <= 32 && layout.align() <= 32 {
            self.slab_32.lock()
        } else if layout.size() <= 64 && layout.align() <= 64 {
            self.slab_64.lock()
        } else if layout.size() <= 128 && layout.align() <= 128 {
            self.slab_128.lock()
        } else if layout.size() <= 256 && layout.align() <= 256 {
            self.slab_256.lock()
        } else if layout.size() <= 512 && layout.align() <= 512 {
            self.slab_512.lock()
        } else if layout.size() <= 1024 && layout.align() <= 1024 {
            self.slab_1024.lock()
        } else if layout.size() <= 2048 && layout.align() <= 2048 {
            self.slab_2048.lock()
        } else if layout.size() <= 4096 && layout.align() <= 4096 {
            self.slab_4096.lock()
        } else {
            panic!(
                "No allocator for size {} and alignment {}",
                layout.size(),
                layout.align()
            );
        }
    }

    #[cold]
    #[must_use]
    pub fn reap(&self) -> usize {
        let reap_n_sum = |allocator: &SpinLock<InternalSlabAllocator>| {
            let mut allocator = allocator.lock();
            allocator.reap()
        };

        let mut total_reaped = 0;
        total_reaped += reap_n_sum(&self.slab_64);
        total_reaped += reap_n_sum(&self.slab_128);
        total_reaped += reap_n_sum(&self.slab_256);
        total_reaped += reap_n_sum(&self.slab_512);
        total_reaped += reap_n_sum(&self.slab_1024);
        total_reaped += reap_n_sum(&self.slab_2048);
        total_reaped += reap_n_sum(&self.slab_4096);

        total_reaped
    }
}

unsafe impl GlobalAlloc for Heap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut allocator = self.layout_to_allocator(layout);

        if let Ok(ptr) = allocator.allocate() {
            return ptr.as_ptr().cast::<u8>();
        }

        null_mut()
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let mut allocator = self.layout_to_allocator(layout);

        let ptr = NonNull::new(ptr).expect("Tried to deallocate a null pointer");
        // SAFETY: We are deallocating a pointer that was allocated by this allocator
        unsafe {
            allocator.free(ptr.cast()).unwrap();
        };
    }
}

unsafe impl Sync for Heap {}
