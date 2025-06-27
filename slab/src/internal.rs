//! "Backend" of the slab allocator

use alloc::boxed::Box;
use core::{alloc::Layout, mem::{self, ManuallyDrop}, ptr::NonNull};
use kernel::{arch::BASIC_PAGE_SIZE, mem::paging::PagingError};
use utils::{
    collections::{linkedlist::{self, LinkedList}, stacklist::{self, StackList}}, sanity_assert, sync::spinlock::SpinLockable
};

/// A node that holds a pointer to an object.
/// Pointer is uninitilized when `SlabObjEmbed` is used, but since the lowest size of memory that
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
    /// Invalid layout for the slab allocator
    InvalidLayout,
}

/// A slab allocator that allocates objects of a fixed Layout.
#[derive(Debug)]
pub(super) struct InternalSlabAllocator {
    /// The list of the slabs
    slabs: ManuallyDrop<LinkedList<Slab>>,
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
    buffer: NonNull<ObjectNode>,
    /// List of objects that are free in this slab
    objects: StackList<()>,
}

// TODO: Add an option to embed the slab node in the slab itself as well
impl InternalSlabAllocator {
    pub const fn const_new(mut layout: Layout) -> Self {
        // We are placing structs right after another in the slab, so we just pad to align
        layout = layout.pad_to_align();

        assert!(layout.size() >= size_of::<ObjectNode>());

        let page_count = 1; // TODO: Make this configurable
        InternalSlabAllocator {
            slabs: ManuallyDrop::new(LinkedList::new()),
            layout,
            page_count,
            object_count: Self::calculate_object_count(layout, page_count),
        }
    }

    /// Creates a new slab allocator with the given object layout
    /// allocated externally using the kernel's heap
    ///
    /// If you want to create this allocator in a const context, use `const_new` instead.
    /// Just take into account that it fails with an assert is anything does wrong, instead of
    /// returning a `Result`
    pub(super) fn new(mut layout: Layout) -> Result<InternalSlabAllocator, SlabError> {
        // We are placing structs right after another in the slab, so we just pad to align
        layout = layout.pad_to_align();

        if layout.size() < size_of::<ObjectNode>() {
            return Err(SlabError::InvalidLayout);
        }

        let page_count = 1; // TODO: Make this configurable
        Ok(InternalSlabAllocator {
            slabs: ManuallyDrop::new(LinkedList::new()),
            layout,
            page_count,
            object_count: Self::calculate_object_count(layout, page_count),
        })
    }

    /// Allocates an object from the slab allocator and returns a pointer to it. If the slab is
    /// full, it will try to grow the cache and return a pointer to an object in the new slab
    pub(super) fn allocate(&mut self) -> Result<NonNull<()>, SlabError> {
        if let Some(slab) = self.slabs.back_mut()
            && let Ok(ptr) = slab.allocate() {
                return Ok(ptr);
            }

        self.grow()?;
        if let Some(slab) = self.slabs.back_mut()
            && let Ok(ptr) = slab.allocate() {
                return Ok(ptr);
            }

        Err(SlabError::SlabFullInternalError)
    }

    /// Frees the object pointed to by `ptr` from the slab allocator.
    ///
    /// SAFETY: This function is unsafe because the passed pointer needs to be a valid pointer to an
    /// allocated object.
    pub(super) unsafe fn free(&mut self, ptr: NonNull<ObjectNode>) -> Result<(), SlabError> {
        for slab in self.slabs.iter_mut() {
            if slab.is_in_range(ptr, self.object_count, self.layout) {
                unsafe {
                    return slab.free(ptr);
                }
            }
        }

        Err(SlabError::BadPtrRange)
    }

    /// Reap all the slabs that are unused. Should only be invoked by OOM killer, when the system desperately needs memory back.
    // TODO: Maybe pass in the amount of memory needed instead of freeing everything?
    #[cold]
    pub(super) fn reap(&mut self) ->usize {
        let mut count = 0;
        while let Some(slab) = self.slabs.front() && slab.objects.len() == self.object_count {
            // Remove the slab from the list
            let slab = self.slabs.pop_node_front().unwrap();

            count += 1;
            let ptr = slab.data().buffer.cast::<()>();
            mem::forget(slab); // Prevent the slab from being dropped and deallocated, since we
                               // alreay freed the pages
            unsafe {
                free_pages(ptr, self.page_count).unwrap();
            }
        }

        count
    }

    /// Grows the cache by allocating a new slab and adding it to the free slabs list.
    pub(super) fn grow(&mut self) -> Result<(), SlabError> {
        let buffer = allocate_pages(self.page_count).map_err(|_| SlabError::PagingError)?.cast::<ObjectNode>();

        let buffer_size = self.object_count * self.layout.size();
        let alignment_offset = (self.page_count * BASIC_PAGE_SIZE.size()) % align_of::<SlabNode>();

        use alloc::format;
        sanity_assert!(buffer_size + alignment_offset + size_of::<SlabNode>() <= self.page_count * BASIC_PAGE_SIZE.size(),
            "Buffer size is too big for the slab allocator ({} + {} + {}) >= {}",
            buffer_size, alignment_offset, size_of::<SlabNode>(), self.page_count * BASIC_PAGE_SIZE.size()
            );

        // TODO: Make sure the sum of all of em is self.pages_per_slab * BASIC_PAGE_SIZE.size()
        unsafe {
            let mut slab_node =
                buffer.byte_add(buffer_size + alignment_offset).cast::<SlabNode>();

            *(slab_node.as_mut()) = linkedlist::Node::new(Slab::new(buffer, self.object_count, self.layout));

            // We push full slabs to the back of the list, the empty ones stay at the front
            self.slabs.push_node_back(Box::from_non_null(slab_node));
        };

            Ok(())

    }

    #[inline]
    #[must_use]
    const fn calculate_object_count(layout: Layout, page_count: usize) -> usize {
        // TODO: Bonwick allocator suggests that we allocate the Node<Slab> in the heap if the
        // object's size is less than 1/8 of a page

        let alignment_offset = (page_count * BASIC_PAGE_SIZE.size()) % align_of::<SlabNode>();
        

        (page_count * BASIC_PAGE_SIZE.size() - alignment_offset - size_of::<SlabNode>()) / layout.size()
    }
}

impl Slab {
    /// Check if the given pointer **to the allocated data** belongs to this slab
    #[inline]
    #[must_use]
    fn is_in_range(
        &self,
        ptr: NonNull<ObjectNode>,
        object_count: usize,
        layout: Layout
    ) -> bool {
        self.buffer <= ptr
            && ptr < unsafe { self.buffer.byte_add(object_count * layout.size()) }
    }

    /// Constructs a new slab with the given parameters.
    ///
    /// SAFETY: This is unsafe because `buff_ptr` must be a valid pointer to a slab of memory
    /// that is at least `object_count` objects in size
    #[inline]
    unsafe fn new(buffer: NonNull<ObjectNode>, count: usize, layout: Layout) -> Self {
        let mut objects = StackList::new();

        for i in 0..count {
            unsafe {
                // SAFETY: This is OK because we already checked to make sure the ptr is aligned,
                // and the size is fine (already checked in the allocator)
                let ptr = buffer.byte_add(i * layout.size());
                objects.push_node(Box::from_non_null(ptr));
            };
        }

        Slab {
            buffer,
            objects,
        }
    }

    /// Allocates an object from the slab
    fn allocate(&mut self) -> Result<NonNull<()>, SlabError> {
        self.objects
            .pop_node()
            .map(|node| Box::into_non_null(node).cast::<()>())
            .ok_or(SlabError::SlabFullInternalError)
    }

    /// Frees an object from the slab
    ///
    /// SAFETY: This function is unsafe because the passed pointer needs to be a valid pointer to
    /// an allocated object.
    unsafe fn free(&mut self, ptr: NonNull<ObjectNode>) -> Result<(), SlabError> {
        if self
            .objects
            .iter_node()
            .any(|node| NonNull::from_ref(node) == ptr)
        {
            return Err(SlabError::DoubleFree);
        }

        // Turns obj_ptr to a new node to add to the list of free objects
        unsafe { self.objects.push_node(Box::from_non_null(ptr)) };

        Ok(())
    }
}

fn allocate_pages(pages_per_slab: usize) -> Result<NonNull<()>, PagingError> {
    #[cfg(test)]
    unsafe {
        use alloc::alloc::alloc_zeroed;
        let layout = Layout::from_size_align(pages_per_slab * BASIC_PAGE_SIZE.size(), BASIC_PAGE_SIZE.size()).unwrap();
        let ptr = alloc_zeroed(layout) as *mut u8;
        if ptr.is_null() {
            return Err(PagingError::OutOfMemory);
        }

        Ok(NonNull::new(ptr).unwrap().cast::<()>())
    }
    #[cfg(not(test))]
    {
        use kernel::mem::paging::Flags;
        kernel::mem::paging::allocate_pages(pages_per_slab, Flags::new().set_read_write(true), BASIC_PAGE_SIZE)
    }
}

unsafe fn free_pages(ptr: NonNull<()>, pages_per_slab: usize) -> Result<(), PagingError> {
    #[cfg(test)]
    unsafe {
        use alloc::alloc::dealloc;
        let layout = Layout::from_size_align(pages_per_slab * BASIC_PAGE_SIZE.size(), BASIC_PAGE_SIZE.size()).unwrap();
        dealloc(ptr.as_ptr().cast::<u8>(), layout);
        Ok(())
    }
    #[cfg(not(test))]
    unsafe {
        kernel::mem::paging::free_pages(ptr, pages_per_slab, BASIC_PAGE_SIZE)
    }
}

impl SpinLockable for InternalSlabAllocator {}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{format, vec::Vec, vec};
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
        d: u64,
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
    fn test_const_new_basic() {
        let allocator = InternalSlabAllocator::const_new(small_layout());
        assert_eq!(allocator.layout, small_layout());
        assert_eq!(allocator.page_count, 1);
        assert!(allocator.object_count > 0);
    }

    #[test]
    fn test_new_valid_layouts() {
        let layouts = vec![
            small_layout(),
            medium_layout(),
            aligned_layout(),
            large_layout(),
            min_valid_layout(),
        ];

        for layout in layouts {
            let result = InternalSlabAllocator::new(layout);
            assert!(result.is_ok(), "Failed to create allocator for layout: {:?}", layout);
            
            let allocator = result.unwrap();
            assert_eq!(allocator.layout, layout);
            assert!(allocator.object_count > 0);
        }
    }

    #[test]
    fn test_new_invalid_layout_too_small() {
        let invalid_layout = Layout::from_size_align(1, 1).unwrap();
        let result = InternalSlabAllocator::new(invalid_layout);
        assert_eq!(result.unwrap_err(), SlabError::InvalidLayout);
    }

    #[test]
    fn test_calculate_object_count() {
        let layout = small_layout();
        let page_count = 1;
        let object_count = InternalSlabAllocator::calculate_object_count(layout, page_count);
        
        // Should be able to fit multiple objects in a page
        assert!(object_count > 1);
        
        // Verify the calculation makes sense
        let alignment_offset = (page_count * BASIC_PAGE_SIZE.size()) % align_of::<SlabNode>();
        let expected_count = (page_count * BASIC_PAGE_SIZE.size() - alignment_offset - size_of::<SlabNode>()) / layout.size();
        assert_eq!(object_count, expected_count);
    }

    #[test]
    fn test_single_allocation_and_free() {
        let mut allocator = InternalSlabAllocator::new(small_layout()).unwrap();
        
        // First allocation should trigger slab creation
        let ptr1 = allocator.allocate().expect("Failed to allocate");
        
        // Free the allocation
        unsafe {
            allocator.free(ptr1.cast()).expect("Failed to free");
        }
    }

    #[test]
    fn test_multiple_allocations() {
        let mut allocator = InternalSlabAllocator::new(small_layout()).unwrap();
        let mut pointers = Vec::new();
        
        // Allocate multiple objects
        for i in 0..10 {
            let ptr = allocator.allocate().expect(&format!("Failed to allocate object {}", i));
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
        let mut allocator = InternalSlabAllocator::new(small_layout()).unwrap();
        let object_count = allocator.object_count;
        let mut pointers = Vec::new();
        
        // Fill the entire first slab
        for i in 0..object_count {
            let ptr = allocator.allocate().expect(&format!("Failed to allocate object {}", i));
            pointers.push(ptr);
        }
        
        // Next allocation should trigger slab growth
        let overflow_ptr = allocator.allocate().expect("Failed to allocate after slab full");
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
        let mut allocator = InternalSlabAllocator::new(small_layout()).unwrap();
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
        for (i, ptr) in pointers.iter().enumerate() {
            if i % 2 == 1 {
                unsafe {
                    allocator.free(ptr.cast()).expect("Failed to free remaining");
                }
            }
        }
    }

    #[test]
    fn test_double_free_error() {
        let mut allocator = InternalSlabAllocator::new(small_layout()).unwrap();
        
        let ptr = allocator.allocate().unwrap();
        
        // First free should succeed
        unsafe {
            allocator.free(ptr.cast()).expect("First free should succeed");
        }
        
        // Second free should fail with DoubleFree error
        unsafe {
            let result = allocator.free(ptr.cast());
            assert_eq!(result.unwrap_err(), SlabError::DoubleFree);
        }
    }

    #[test]
    fn test_bad_ptr_range_error() {
        let mut allocator = InternalSlabAllocator::new(small_layout()).unwrap();
        
        // Create a fake pointer that's not from our allocator
        let fake_ptr: NonNull<ObjectNode> = NonNull::dangling();
        
        unsafe {
            let result = allocator.free(fake_ptr);
            assert_eq!(result.unwrap_err(), SlabError::BadPtrRange);
        }
    }

    #[test]
    fn test_slab_growth_multiple_times() {
        let mut allocator = InternalSlabAllocator::new(small_layout()).unwrap();
        let objects_per_slab = allocator.object_count;
        let total_slabs = 3;
        let mut all_pointers = Vec::new();
        
        // Allocate enough objects to force multiple slab growths
        for slab_num in 0..total_slabs {
            for obj_num in 0..objects_per_slab {
                let ptr = allocator.allocate().expect(&format!(
                    "Failed to allocate object {} in slab {}", obj_num, slab_num
                ));
                all_pointers.push(ptr);
            }
        }
        
        // Verify we have the expected number of allocations
        assert_eq!(all_pointers.len(), total_slabs * objects_per_slab);
        
        // Free all allocations
        for ptr in all_pointers {
            unsafe {
                allocator.free(ptr.cast()).expect("Failed to free during cleanup");
            }
        }
    }

    #[test]
    fn test_stress_allocation_free_patterns() {
        let mut allocator = InternalSlabAllocator::new(medium_layout()).unwrap();
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
            let mut allocator = InternalSlabAllocator::new(layout).expect(&format!("Failed to create {} allocator", name));
            let mut pointers = Vec::new();
            
            // Allocate several objects
            for i in 0..5 {
                let ptr = allocator.allocate().expect(&format!("Failed to allocate {} object {}", name, i));
                pointers.push(ptr);
            }
            
            // Free all objects
            for ptr in pointers {
                unsafe {
                    allocator.free(ptr.cast()).expect(&format!("Failed to free {} object", name));
                }
            }
        }
    }

    #[test]
    fn test_reap_functionality() {
        let mut allocator = InternalSlabAllocator::new(small_layout()).unwrap();
        
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
        let mut allocator = InternalSlabAllocator::new(small_layout()).unwrap();
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
        let mut allocator = InternalSlabAllocator::new(min_layout).unwrap();
        
        // Should be able to allocate even with minimum size
        let ptr = allocator.allocate().unwrap();
        
        unsafe {
            allocator.free(ptr.cast()).unwrap();
        }
    }

    #[test]
    fn test_large_number_of_allocations() {
        let mut allocator = InternalSlabAllocator::new(small_layout()).unwrap();
        let num_allocations = 1000;
        let mut pointers = Vec::with_capacity(num_allocations);
        
        // Allocate many objects
        for i in 0..num_allocations {
            let ptr = allocator.allocate().expect(&format!("Failed allocation {}", i));
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
        let mut allocator = InternalSlabAllocator::new(aligned_layout).unwrap();
        
        // Allocate several aligned objects
        for _ in 0..5 {
            let ptr = allocator.allocate().unwrap();
            
            // Check alignment
            let addr = ptr.as_ptr().addr();
            assert_eq!(addr % aligned_layout.align(), 0, "Allocated pointer is not properly aligned");
            
            unsafe {
                allocator.free(ptr.cast()).unwrap();
            }
        }
    }

    #[test]
    fn test_fragmentation_and_coalescing() {
        let mut allocator = InternalSlabAllocator::new(small_layout()).unwrap();
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
        let mut allocator = InternalSlabAllocator::new(small_layout()).unwrap();
        
        // Test BadPtrRange error
        let bad_ptr: NonNull<ObjectNode> = NonNull::dangling();
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
        
        assert!(total_used <= available, 
            "Memory layout calculation is incorrect: used {} bytes but only {} available", 
            total_used, available);
    }

    #[test]
    fn test_allocator_state_consistency() {
        let mut allocator = InternalSlabAllocator::new(small_layout()).unwrap();
        
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
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use core::alloc::Layout;
//     use core::mem::{align_of, size_of};
//     use alloc::vec::Vec;
//
//     // Helper function to create a valid layout for testing
//     fn create_test_layout(size: usize, align: usize) -> Layout {
//         Layout::from_size_align(size, align).unwrap()
//     }
//
//     #[test]
//     fn test_new_allocator_valid_layout() {
//         let layout = create_test_layout(size_of::<ObjectNode>(), align_of::<ObjectNode>());
//         let allocator = InternalSlabAllocator::new(layout);
//         assert!(allocator.is_ok());
//         let allocator = allocator.unwrap();
//         assert_eq!(allocator.layout, layout);
//         assert_eq!(allocator.page_count, 1);
//         assert_eq!(allocator.slabs.is_empty(), true);
//     }
//
//     #[test]
//     fn test_new_allocator_invalid_layout() {
//         // Layout size smaller than ObjectNode
//         let layout = create_test_layout(size_of::<ObjectNode>() - 1, align_of::<ObjectNode>());
//         let result = InternalSlabAllocator::new(layout);
//         if let Err(SlabError::InvalidLayout) = result {
//             // Expected error
//         } else {
//             panic!("Expected InvalidLayout error, got {:?}", result);
//         }
//     }
//
//     #[test]
//     fn test_const_new_allocator() {
//         const LAYOUT: Layout = unsafe {Layout::from_size_align_unchecked(size_of::<ObjectNode>(), align_of::<ObjectNode>())};
//         let allocator = InternalSlabAllocator::const_new(LAYOUT);
//         assert_eq!(allocator.layout, LAYOUT);
//         assert_eq!(allocator.page_count, 1);
//         assert_eq!(allocator.slabs.is_empty(), true);
//     }
//
//     #[test]
//     #[should_panic]
//     fn test_const_new_invalid_layout() {
//         const LAYOUT: Layout = unsafe { Layout::from_size_align_unchecked(size_of::<ObjectNode>() - 1, align_of::<ObjectNode>()) };
//         let _ = InternalSlabAllocator::const_new(LAYOUT);
//     }
//
//     #[test]
//     fn test_calculate_object_count() {
//         let layout = create_test_layout(size_of::<ObjectNode>(), align_of::<ObjectNode>());
//         let page_count = 1;
//         let object_count = InternalSlabAllocator::calculate_object_count(layout, page_count);
//         let expected_count = {
//             let alignment_offset = (page_count * BASIC_PAGE_SIZE.size()) % align_of::<SlabNode>();
//             (page_count * BASIC_PAGE_SIZE.size() - alignment_offset - size_of::<SlabNode>()) / layout.size()
//         };
//         assert_eq!(object_count, expected_count);
//     }
//
//     #[test]
//     fn test_allocate_and_free() {
//         let layout = create_test_layout(size_of::<ObjectNode>(), align_of::<ObjectNode>());
//         let mut allocator = InternalSlabAllocator::new(layout).unwrap();
//
//         // Allocate an object
//         let ptr = allocator.allocate().unwrap();
//
//         // Free the object
//         unsafe {
//             let result = allocator.free(ptr.cast::<ObjectNode>());
//             assert_eq!(result, Ok(()));
//         }
//     }
//
//     #[test]
//     fn test_allocate_multiple_objects() {
//         let layout = create_test_layout(size_of::<ObjectNode>(), align_of::<ObjectNode>());
//         let mut allocator = InternalSlabAllocator::new(layout).unwrap();
//         let object_count = allocator.object_count;
//
//         // Allocate all objects in the first slab
//         let mut pointers = Vec::new();
//         for _ in 0..object_count {
//             let ptr = allocator.allocate();
//             assert!(ptr.is_ok(), "Allocation failed at iteration {}", pointers.len());
//             pointers.push(ptr.unwrap());
//         }
//
//         // Next allocation should trigger a new slab
//         let result = allocator.allocate();
//         assert!(result.is_ok(), "Failed to allocate after growing slab");
//
//         // Free all objects
//         for ptr in pointers {
//             unsafe {
//                 assert_eq!(allocator.free(ptr.cast::<ObjectNode>()), Ok(()));
//             }
//         }
//     }
//
//     #[test]
//     fn test_free_invalid_pointer_range() {
//         let layout = create_test_layout(size_of::<ObjectNode>(), align_of::<ObjectNode>());
//         let mut allocator = InternalSlabAllocator::new(layout).unwrap();
//
//         // Create a pointer outside the slab's range
//         let invalid_ptr = NonNull::new(0xDEAD_BEEF as *mut ObjectNode).unwrap();
//         unsafe {
//             let result = allocator.free(invalid_ptr);
//             assert_eq!(result, Err(SlabError::BadPtrRange));
//         }
//     }
//
//     #[test]
//     fn test_double_free() {
//         let layout = create_test_layout(size_of::<ObjectNode>(), align_of::<ObjectNode>());
//         let mut allocator = InternalSlabAllocator::new(layout).unwrap();
//
//         // Allocate and free an object
//         let ptr = allocator.allocate().unwrap();
//         unsafe {
//             assert_eq!(allocator.free(ptr.cast::<ObjectNode>()), Ok(()));
//             // Try to free it again
//             let result = allocator.free(ptr.cast::<ObjectNode>());
//             assert_eq!(result, Err(SlabError::DoubleFree));
//         }
//     }
//
//     #[test]
//     fn test_grow_slab() {
//         let layout = create_test_layout(size_of::<ObjectNode>(), align_of::<ObjectNode>());
//         let mut allocator = InternalSlabAllocator::new(layout).unwrap();
//         let object_count = allocator.object_count;
//
//         // Fill the first slab
//         for _ in 0..object_count {
//             assert!(allocator.allocate().is_ok());
//         }
//
//         // This should trigger a new slab
//         let result = allocator.allocate();
//         assert!(result.is_ok());
//         assert_eq!(allocator.slabs.len(), 2);
//     }
//
//     #[test]
//     fn test_slab_is_in_range() {
//         let layout = create_test_layout(size_of::<ObjectNode>(), align_of::<ObjectNode>());
//         let mut allocator = InternalSlabAllocator::new(layout).unwrap();
//         let object_count = allocator.object_count;
//
//         // Allocate to create a slab
//         let ptr = allocator.allocate().unwrap();
//         let slab = allocator.slabs.front().unwrap();
//
//         // Check if the pointer is in range
//         assert!(slab.is_in_range(ptr.cast::<ObjectNode>(), object_count));
//
//         // Check an invalid pointer
//         let invalid_ptr = NonNull::new(0xDEAD_BEEF as *mut ObjectNode).unwrap();
//         assert!(!slab.is_in_range(invalid_ptr, object_count));
//     }
//
//     #[test]
//     fn test_allocate_after_free() {
//         let layout = create_test_layout(size_of::<ObjectNode>(), align_of::<ObjectNode>());
//         let mut allocator = InternalSlabAllocator::new(layout).unwrap();
//
//         // Allocate and free an object
//         let ptr = allocator.allocate().unwrap();
//         unsafe {
//             assert_eq!(allocator.free(ptr.cast::<ObjectNode>()), Ok(()));
//         }
//
//         // Allocate again, should reuse the freed slot
//         let new_ptr = allocator.allocate().unwrap();
//         assert_eq!(ptr, new_ptr); // Should reuse the same memory
//     }
//
//     #[test]
//     fn test_slab_full_error() {
//         let layout = create_test_layout(size_of::<ObjectNode>(), align_of::<ObjectNode>());
//         let mut allocator = InternalSlabAllocator::new(layout).unwrap();
//         let object_count = allocator.object_count;
//
//         // Allocate all objects
//         for _ in 0..object_count {
//             assert!(allocator.allocate().is_ok());
//         }
//
//         // Simulate allocation failure by mocking allocate_pages to fail
//         // This would require mocking the allocate_pages function, which is complex in this context
//         // Instead, we rely on the fact that grow() will eventually succeed or fail gracefully
//         let result = allocator.allocate();
//         assert!(result.is_ok()); // Should grow a new slab
//     }
// }
