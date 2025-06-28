//! "Backend" of the slab allocator

use core::{alloc::Layout, ptr::NonNull};
use kernel::{arch::BASIC_PAGE_SIZE, mem::paging::PagingError};
use utils::{
    collections::{
        linkedlist::{self, LinkedList},
        stacklist::{self, StackList},
    },
    const_max, sanity_assert,
    sync::spinlock::SpinLockable,
};

/// A node that holds a pointer to an object.
/// Pointer is uninitialized when `SlabObjEmbed` is used, but since the lowest size of memory that
/// we allocate in the stack is 2^8 bytes anyway, it doesn't matter
pub(super) type ObjectNode = stacklist::Node<()>;

type SlabNode = linkedlist::Node<Slab>;

/// Errors that the slab allocator might encounter
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SlabError {
    /// The pointer passed to free is not in the range of the slab
    BadPtrRange,
    /// The pointer passed to free isn't allocated
    DoubleFree,
    /// The slab is full and cannot allocate any more objects
    SlabFullInternalError,
    /// Paging error occurred
    PagingError,
    /// Invalid layout
    InvalidLayout,
}

/// A slab allocator that allocates objects of a fixed Layout.
#[derive(Debug)]
pub(super) struct InternalSlabAllocator {
    /// The list of the slabs
    slabs: LinkedList<Slab>,
    /// The amount of pages each slab will have
    page_count: usize,
    /// The amount of objects that will be allocated in each slab
    object_count: usize,
    /// The layout of the objects that will be allocated
    layout: Layout,
}

/// The core structure of a slab
#[derive(Debug)]
struct Slab {
    /// Pointer to the slab's buffer
    buffer: NonNull<u8>,
    /// List of objects that are free in this slab
    free_objects: StackList<()>,
    /// Number of allocated objects in this slab
    allocated_count: usize,
    /// Total capacity of this slab
    capacity: usize,
}

impl InternalSlabAllocator {
    /// Creates a new slab allocator with const evaluation (unsafe version)
    pub(super) const fn new(layout: Layout) -> Self {
        let adjusted_layout = Self::adjust_layout(layout);
        let page_count = 1; // TODO: Make this configurable

        Self {
            slabs: LinkedList::new(),
            layout: adjusted_layout,
            page_count,
            object_count: Self::calculate_object_count(adjusted_layout, page_count),
        }
    }

    /// Adjusts layout to meet minimum requirements (const version for unsafe const_new)
    const fn adjust_layout(layout: Layout) -> Layout {
        let size = const_max!(layout.size(), size_of::<ObjectNode>());
        let align = const_max!(layout.align(), align_of::<ObjectNode>());

        // SAFETY: This is only used in const_new which is marked unsafe
        unsafe { Layout::from_size_align_unchecked(size, align) }
    }

    /// Allocates an object from the slab allocator
    pub(super) fn allocate(&mut self) -> Result<NonNull<()>, SlabError> {
        // Try to allocate from existing slabs (prefer partially filled ones)
        for slab in self.slabs.iter_mut() {
            if let Ok(ptr) = slab.allocate(self.layout) {
                return Ok(ptr);
            }
        }

        // No space in existing slabs, grow the cache
        self.grow()?;

        // Try again with the new slab
        if let Some(slab) = self.slabs.back_mut() {
            if let Ok(ptr) = slab.allocate(self.layout) {
                return Ok(ptr);
            }
        }

        Err(SlabError::SlabFullInternalError)
    }

    /// Frees the object pointed to by `ptr` from the slab allocator.
    ///
    /// SAFETY: This function is unsafe because the passed pointer needs to be a valid pointer to an
    /// allocated object that was returned by this allocator.
    pub(super) unsafe fn free(&mut self, ptr: NonNull<()>) -> Result<(), SlabError> {
        // Check basic alignment first
        if !ptr.as_ptr().is_aligned_to(self.layout.align()) {
            return Err(SlabError::BadPtrRange);
        }

        // Find the slab that contains this pointer and free it
        for slab in self.slabs.iter_mut() {
            if slab.contains_ptr(ptr, self.layout) {
                return unsafe { slab.free(ptr, self.layout) };
            }
        }

        Err(SlabError::BadPtrRange)
    }

    /// Reap all the slabs that are unused
    #[cold]
    pub(super) fn reap(&mut self) -> usize {
        let mut reaped_count = 0;

        // Remove all completely empty slabs from the front
        while let Some(slab) = self.slabs.front() {
            if slab.is_empty() {
                let slab_node = self.slabs.pop_node_front().unwrap();
                let slab_data = unsafe { slab_node.as_ref().data() };

                sanity_assert!(slab_data.buffer.as_ptr().addr() % BASIC_PAGE_SIZE.size() == 0);

                // SAFETY: Buffer is a valid pointer to allocated pages
                unsafe {
                    free_pages(slab_data.buffer.cast(), self.page_count).unwrap();
                }

                reaped_count += 1;
            } else {
                break;
            }
        }

        reaped_count
    }

    /// Grows the cache by allocating a new slab
    fn grow(&mut self) -> Result<(), SlabError> {
        let pages = allocate_pages(self.page_count).map_err(|_| SlabError::PagingError)?;

        unsafe {
            let buffer = pages.cast::<u8>();

            // Calculate where to place the slab metadata
            let objects_size = self.object_count * self.layout.size();
            let metadata_offset = align_up(objects_size, align_of::<SlabNode>());
            let slab_node_ptr = buffer.byte_add(metadata_offset).cast::<SlabNode>();

            // Verify we have enough space
            sanity_assert!(
                metadata_offset + size_of::<SlabNode>() <= self.page_count * BASIC_PAGE_SIZE.size(),
                "Slab metadata doesn't fit in allocated pages"
            );

            // Create the slab
            let slab = Slab::new(buffer, self.object_count, self.layout)?;

            // Initialize the node
            slab_node_ptr.write(linkedlist::Node::new(slab));

            // Add to our slab list
            self.slabs.push_node_back(slab_node_ptr);
        }

        Ok(())
    }

    /// Calculate how many objects can fit in a slab
    #[must_use]
    const fn calculate_object_count(layout: Layout, page_count: usize) -> usize {
        let total_size = page_count * BASIC_PAGE_SIZE.size();
        let metadata_size = size_of::<SlabNode>();

        // Account for alignment of metadata
        let available_for_objects = total_size - metadata_size;
        let alignment_waste = available_for_objects % align_of::<SlabNode>();
        let usable_space = available_for_objects - alignment_waste;

        usable_space / layout.size()
    }
}

impl Slab {
    /// Creates a new slab
    unsafe fn new(buffer: NonNull<u8>, capacity: usize, layout: Layout) -> Result<Self, SlabError> {
        let mut free_objects = StackList::new();

        // Initialize all objects as free
        for i in 0..capacity {
            let offset = i * layout.size();
            let object_ptr = unsafe { buffer.byte_add(offset).cast::<ObjectNode>() };

            // Verify alignment
            if !object_ptr.as_ptr().is_aligned_to(layout.align()) {
                return Err(SlabError::InvalidLayout);
            }

            unsafe {
                free_objects.push_node(object_ptr);
            }
        }

        Ok(Slab {
            buffer,
            free_objects,
            allocated_count: 0,
            capacity,
        })
    }

    /// Check if the slab contains the given pointer
    fn contains_ptr(&self, ptr: NonNull<()>, layout: Layout) -> bool {
        let ptr_addr = ptr.as_ptr().addr();
        let buffer_start = self.buffer.as_ptr().addr();
        let buffer_end = buffer_start + (self.capacity * layout.size());

        ptr_addr >= buffer_start && ptr_addr < buffer_end
    }

    /// Check if this slab is completely empty
    fn is_empty(&self) -> bool {
        self.allocated_count == 0
    }

    /// Allocate an object from this slab
    fn allocate(&mut self, layout: Layout) -> Result<NonNull<()>, SlabError> {
        if let Some(node_ptr) = self.free_objects.pop_node() {
            self.allocated_count += 1;

            // Verify the pointer is properly aligned
            let ptr = node_ptr.cast::<()>();
            if !ptr.as_ptr().is_aligned_to(layout.align()) {
                return Err(SlabError::InvalidLayout);
            }

            Ok(ptr)
        } else {
            Err(SlabError::SlabFullInternalError)
        }
    }

    /// Free an object back to this slab
    ///
    /// SAFETY: ptr must be a valid pointer that was allocated from this slab
    unsafe fn free(&mut self, ptr: NonNull<()>, layout: Layout) -> Result<(), SlabError> {
        let ptr_addr = ptr.as_ptr().addr();
        let buffer_start = self.buffer.as_ptr().addr();

        // Verify the pointer is aligned to an object boundary
        let offset = ptr_addr - buffer_start;
        if offset % layout.size() != 0 {
            return Err(SlabError::BadPtrRange);
        }

        let object_ptr = ptr.cast::<ObjectNode>();

        // Check for double-free by looking for this pointer in the free list
        for node in self.free_objects.iter_node() {
            if NonNull::from_ref(node) == object_ptr {
                return Err(SlabError::DoubleFree);
            }
        }

        // Add back to free list
        unsafe {
            self.free_objects.push_node(object_ptr);
        }

        self.allocated_count = self.allocated_count.saturating_sub(1);
        Ok(())
    }
}

/// Helper function to align a value up to the next multiple of align
const fn align_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}

fn allocate_pages(pages_per_slab: usize) -> Result<NonNull<()>, PagingError> {
    #[cfg(test)]
    unsafe {
        use alloc::alloc::alloc_zeroed;
        let layout = Layout::from_size_align(
            pages_per_slab * BASIC_PAGE_SIZE.size(),
            BASIC_PAGE_SIZE.size(),
        )
        .unwrap();
        let ptr = alloc_zeroed(layout) as *mut u8;
        if ptr.is_null() {
            return Err(PagingError::OutOfMemory);
        }

        Ok(NonNull::new(ptr).unwrap().cast::<()>())
    }
    #[cfg(not(test))]
    {
        use kernel::mem::paging::Flags;
        kernel::mem::paging::allocate_pages(
            pages_per_slab,
            Flags::new().set_read_write(true),
            BASIC_PAGE_SIZE,
        )
    }
}

unsafe fn free_pages(ptr: NonNull<()>, pages_per_slab: usize) -> Result<(), PagingError> {
    #[cfg(test)]
    unsafe {
        use alloc::alloc::dealloc;
        let layout = Layout::from_size_align(
            pages_per_slab * BASIC_PAGE_SIZE.size(),
            BASIC_PAGE_SIZE.size(),
        )
        .unwrap();
        dealloc(ptr.as_ptr().cast::<u8>(), layout);
        Ok(())
    }
    #[cfg(not(test))]
    unsafe {
        kernel::mem::paging::free_pages(ptr, pages_per_slab, BASIC_PAGE_SIZE)
    }
}

impl Drop for InternalSlabAllocator {
    fn drop(&mut self) {
        // Reap all slabs
        let _reaped = self.reap();

        // Check if there are any remaining slabs with allocated objects
        let mut has_leaks = false;
        for slab in self.slabs.iter() {
            if !slab.is_empty() {
                has_leaks = true;
                break;
            }
        }

        // Force free any remaining slabs (this would be a leak, but we need to clean up)
        while let Some(slab_node) = self.slabs.pop_node_front() {
            let slab_data = unsafe { slab_node.as_ref().data() };
            unsafe {
                free_pages(slab_data.buffer.cast(), self.page_count).unwrap();
            }
        }

        assert!(
            !has_leaks,
            "Slabs are still allocated but the allocator is being dropped. This is a memory leak!"
        );
    }
}

impl SpinLockable for InternalSlabAllocator {}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{format, vec, vec::Vec};
    use core::alloc::Layout;

    // Test structures of different sizes and alignments
    #[repr(C)]
    struct SmallStruct {
        a: u32,
        b: u32,
    }

    #[repr(C)]
    struct MediumStruct {
        a: u64,
        b: u64,
        c: u64,
    }

    #[repr(C, align(16))]
    struct AlignedStruct {
        data: [u8; 32],
    }

    #[repr(C)]
    struct LargeStruct {
        data: [u8; 256],
    }

    // Helper function to create test layouts
    fn small_layout() -> Layout {
        Layout::new::<SmallStruct>()
    }

    fn medium_layout() -> Layout {
        Layout::new::<MediumStruct>()
    }

    fn aligned_layout() -> Layout {
        Layout::new::<AlignedStruct>()
    }

    fn large_layout() -> Layout {
        Layout::new::<LargeStruct>()
    }

    fn min_valid_layout() -> Layout {
        Layout::from_size_align(size_of::<ObjectNode>(), align_of::<ObjectNode>()).unwrap()
    }

    #[test]
    fn test_basic() {
        let mut allocator = InternalSlabAllocator::new(small_layout());

        let obj1 = allocator.allocate().unwrap();
        let obj2 = allocator.allocate().unwrap();
        let obj3 = allocator.allocate().unwrap();

        unsafe {
            allocator.free(obj1).unwrap();
            allocator.free(obj2).unwrap();
            allocator.free(obj3).unwrap();
        }
    }

    #[test]
    fn test_single_allocation_and_free() {
        let mut allocator = InternalSlabAllocator::new(small_layout());

        // First allocation should trigger slab creation
        let ptr1 = allocator.allocate().expect("Failed to allocate");

        // Free the allocation
        unsafe {
            allocator.free(ptr1.cast()).expect("Failed to free");
        }
    }

    #[test]
    fn test_multiple_allocations() {
        let mut allocator = InternalSlabAllocator::new(small_layout());
        let mut pointers = Vec::new();

        // Allocate multiple objects
        for i in 0..10 {
            let ptr = allocator
                .allocate()
                .expect(&format!("Failed to allocate object {}", i));
            pointers.push(ptr);
        }

        // Free all allocations
        for ptr in pointers {
            unsafe {
                allocator.free(ptr.cast()).expect("Failed to free");
            }
        }
    }

    #[test]
    fn test_fill_entire_slab() {
        let mut allocator = InternalSlabAllocator::new(small_layout());
        let object_count = allocator.object_count;
        let mut pointers = Vec::new();

        // Fill the entire first slab
        for i in 0..object_count {
            let ptr = allocator
                .allocate()
                .expect(&format!("Failed to allocate object {}", i));
            pointers.push(ptr);
        }

        // Next allocation should trigger slab growth
        let overflow_ptr = allocator
            .allocate()
            .expect("Failed to allocate after slab full");
        pointers.push(overflow_ptr);

        // Free all allocations
        for ptr in pointers {
            unsafe {
                allocator.free(ptr.cast()).expect("Failed to free");
            }
        }
    }

    #[test]
    fn test_allocation_after_partial_free() {
        let mut allocator = InternalSlabAllocator::new(small_layout());
        let mut pointers = Vec::new();

        // Allocate several objects
        for _ in 0..5 {
            pointers.push(allocator.allocate().unwrap());
        }

        // Free every other allocation
        let mut freed_indices = Vec::new();
        for (i, ptr) in pointers.iter().enumerate() {
            if i % 2 == 0 {
                unsafe {
                    allocator.free(ptr.cast()).expect("Failed to free");
                }
                freed_indices.push(i);
            }
        }

        // Allocate new objects (should reuse freed slots)
        for _ in freed_indices {
            let _ = allocator.allocate().expect("Failed to reallocate");
        }

        // Clean up remaining allocations
        for ptr in pointers.iter() {
            unsafe {
                allocator
                    .free(ptr.cast())
                    .expect("Failed to free remaining");
            }
        }
    }

    #[test]
    fn test_double_free_error() {
        let mut allocator = InternalSlabAllocator::new(small_layout());

        let ptr = allocator.allocate().unwrap();

        // First free should succeed
        unsafe {
            allocator
                .free(ptr.cast())
                .expect("First free should succeed");
        }

        // Second free should fail with DoubleFree error
        unsafe {
            let result = allocator.free(ptr.cast());
            assert_eq!(result.unwrap_err(), SlabError::DoubleFree);
        }
    }

    #[test]
    fn test_bad_ptr_range_error() {
        let mut allocator = InternalSlabAllocator::new(small_layout());

        // Create a fake pointer that's not from our allocator
        let fake_ptr: NonNull<()> = NonNull::dangling();

        unsafe {
            let result = allocator.free(fake_ptr);
            assert_eq!(result.unwrap_err(), SlabError::BadPtrRange);
        }
    }

    #[test]
    fn test_slab_growth_multiple_times() {
        let mut allocator = InternalSlabAllocator::new(small_layout());
        let objects_per_slab = allocator.object_count;
        let total_slabs = 3;
        let mut all_pointers = Vec::new();

        // Allocate enough objects to force multiple slab growths
        for slab_num in 0..total_slabs {
            for obj_num in 0..objects_per_slab {
                let ptr = allocator.allocate().expect(&format!(
                    "Failed to allocate object {} in slab {}",
                    obj_num, slab_num
                ));
                all_pointers.push(ptr);
            }
        }

        // Verify we have the expected number of allocations
        assert_eq!(all_pointers.len(), total_slabs * objects_per_slab);

        // Free all allocations
        for ptr in all_pointers {
            unsafe {
                allocator
                    .free(ptr.cast())
                    .expect("Failed to free during cleanup");
            }
        }
    }

    #[test]
    fn test_stress_allocation_free_patterns() {
        let mut allocator = InternalSlabAllocator::new(medium_layout());
        let mut active_pointers = Vec::new();

        // Pattern 1: Allocate many, free all
        for _ in 0..50 {
            active_pointers.push(allocator.allocate().unwrap());
        }

        for ptr in active_pointers.drain(..) {
            unsafe {
                allocator.free(ptr.cast()).unwrap();
            }
        }

        // Pattern 2: Interleaved allocation and freeing
        for _ in 0..10 {
            // Allocate 5 objects
            for _ in 0..5 {
                active_pointers.push(allocator.allocate().unwrap());
            }

            // Free 3 objects
            for _ in 0..3 {
                if let Some(ptr) = active_pointers.pop() {
                    unsafe {
                        allocator.free(ptr.cast()).unwrap();
                    }
                }
            }
        }

        // Clean up remaining
        for ptr in active_pointers {
            unsafe {
                allocator.free(ptr.cast()).unwrap();
            }
        }
    }

    #[test]
    fn test_different_sized_objects() {
        let test_cases = vec![
            ("small", small_layout()),
            ("medium", medium_layout()),
            ("aligned", aligned_layout()),
            ("large", large_layout()),
        ];

        for (name, layout) in test_cases {
            let mut allocator = InternalSlabAllocator::new(layout);
            let mut pointers = Vec::new();

            // Allocate several objects
            for i in 0..5 {
                let ptr = allocator
                    .allocate()
                    .expect(&format!("Failed to allocate {} object {}", name, i));
                pointers.push(ptr);
            }

            // Free all objects
            for ptr in pointers {
                unsafe {
                    allocator
                        .free(ptr.cast())
                        .expect(&format!("Failed to free {} object", name));
                }
            }
        }
    }

    #[test]
    fn test_reap_functionality() {
        let mut allocator = InternalSlabAllocator::new(small_layout());

        // Initially, no slabs exist, so reap should return 0
        let reaped = allocator.reap();
        assert_eq!(reaped, 0);

        // Allocate and immediately free to create an empty slab
        let ptr = allocator.allocate().unwrap();
        unsafe {
            allocator.free(ptr.cast()).unwrap();
        }

        // Now reap should free the empty slab
        let reaped = allocator.reap();
        assert!(reaped > 0);
    }

    #[test]
    fn test_concurrent_like_operations() {
        // Simulate what might happen in concurrent scenarios
        let mut allocator = InternalSlabAllocator::new(small_layout());
        let mut operations = Vec::new();

        // Mixed operations that might occur
        for i in 0..100 {
            match i % 4 {
                0 | 1 => {
                    // Allocate
                    if let Ok(ptr) = allocator.allocate() {
                        operations.push(ptr);
                    }
                }
                2 => {
                    // Free if we have something to free
                    if let Some(ptr) = operations.pop() {
                        unsafe {
                            let _ = allocator.free(ptr.cast());
                        }
                    }
                }
                3 => {
                    // Attempt reap
                    let _ = allocator.reap();
                }
                _ => unreachable!(),
            }
        }

        // Clean up remaining allocations
        for ptr in operations {
            unsafe {
                allocator.free(ptr.cast()).unwrap();
            }
        }
    }

    #[test]
    fn test_edge_case_minimum_object_size() {
        let min_layout = min_valid_layout();
        let mut allocator = InternalSlabAllocator::new(min_layout);

        // Should be able to allocate even with minimum size
        let ptr = allocator.allocate().unwrap();

        unsafe {
            allocator.free(ptr.cast()).unwrap();
        }
    }

    #[test]
    fn test_large_number_of_allocations() {
        let mut allocator = InternalSlabAllocator::new(small_layout());
        let num_allocations = 1000;
        let mut pointers = Vec::with_capacity(num_allocations);

        // Allocate many objects
        for i in 0..num_allocations {
            let ptr = allocator
                .allocate()
                .expect(&format!("Failed allocation {}", i));
            pointers.push(ptr);
        }

        // Free all in reverse order
        for ptr in pointers.into_iter().rev() {
            unsafe {
                allocator.free(ptr.cast()).expect("Failed to free");
            }
        }
    }

    #[test]
    fn test_allocation_alignment() {
        let aligned_layout = aligned_layout();
        let mut allocator = InternalSlabAllocator::new(aligned_layout);

        // Allocate several aligned objects
        for _ in 0..5 {
            let ptr = allocator.allocate().unwrap();

            // Check alignment
            let addr = ptr.as_ptr().addr();
            assert_eq!(
                addr % aligned_layout.align(),
                0,
                "Allocated pointer is not properly aligned"
            );

            unsafe {
                allocator.free(ptr.cast()).unwrap();
            }
        }
    }

    #[test]
    fn test_fragmentation_and_coalescing() {
        let mut allocator = InternalSlabAllocator::new(small_layout());
        let mut pointers = Vec::new();

        // Fill up a slab
        let object_count = allocator.object_count;
        for _ in 0..object_count {
            pointers.push(allocator.allocate().unwrap());
        }

        // Free every other object to create fragmentation
        let mut kept_pointers = Vec::new();
        for (i, ptr) in pointers.into_iter().enumerate() {
            if i % 2 == 0 {
                unsafe {
                    allocator.free(ptr.cast()).unwrap();
                }
            } else {
                kept_pointers.push(ptr);
            }
        }

        // Allocate new objects - should reuse freed slots
        let realloc_count = object_count / 2;
        for _ in 0..realloc_count {
            let ptr = allocator.allocate().unwrap();
            kept_pointers.push(ptr);
        }

        // Clean up
        for ptr in kept_pointers {
            unsafe {
                allocator.free(ptr.cast()).unwrap();
            }
        }
    }

    #[test]
    fn test_error_propagation() {
        // Test that errors are properly propagated through the call chain
        let mut allocator = InternalSlabAllocator::new(small_layout());

        // Test BadPtrRange error
        let bad_ptr: NonNull<()> = NonNull::dangling();
        unsafe {
            match allocator.free(bad_ptr) {
                Err(SlabError::BadPtrRange) => (),
                other => panic!("Expected BadPtrRange error, got {:?}", other),
            }
        }

        // Test DoubleFree error
        let ptr = allocator.allocate().unwrap();
        unsafe {
            allocator.free(ptr.cast()).unwrap(); // First free
            match allocator.free(ptr.cast()) {
                Err(SlabError::DoubleFree) => (),
                other => panic!("Expected DoubleFree error, got {:?}", other),
            }
        }
    }

    #[test]
    fn test_memory_layout_assumptions() {
        // Verify our assumptions about memory layout are correct
        let layout = small_layout();
        let page_count = 1;

        let object_count = InternalSlabAllocator::calculate_object_count(layout, page_count);
        let buffer_size = object_count * layout.size();
        let alignment_offset = (page_count * BASIC_PAGE_SIZE.size()) % align_of::<SlabNode>();
        let total_used = buffer_size + alignment_offset + size_of::<SlabNode>();
        let available = page_count * BASIC_PAGE_SIZE.size();

        assert!(
            total_used <= available,
            "Memory layout calculation is incorrect: used {} bytes but only {} available",
            total_used,
            available
        );
    }

    #[test]
    fn test_allocator_state_consistency() {
        let mut allocator = InternalSlabAllocator::new(small_layout());

        // Initial state
        assert_eq!(allocator.page_count, 1);
        assert!(allocator.object_count > 0);

        // After allocation, state should remain consistent
        let ptr = allocator.allocate().unwrap();
        assert_eq!(allocator.page_count, 1);
        assert!(allocator.object_count > 0);

        // After free, state should remain consistent
        unsafe {
            allocator.free(ptr.cast()).unwrap();
        }
        assert_eq!(allocator.page_count, 1);
        assert!(allocator.object_count > 0);
    }

    // Not testing this on Miri, since it will panic and Miri will report the intentional memory
    // leak as an error
    #[test]
    #[should_panic]
    #[cfg_attr(miri, ignore)]
    fn test_allocator_drop_behavior() {
        let mut allocator = InternalSlabAllocator::new(small_layout());

        // Fill the allocator with objects
        for _ in 0..5 {
            allocator.allocate().unwrap();
        }

        // Intentionally we don't drop, to make sure it panics if there are still slabs allocated
    }
}
