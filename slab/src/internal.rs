//! "Backend" of the slab allocator

use alloc::boxed::Box;
use arch::{
    BASIC_PAGE_SIZE, allocate_pages, free_pages,
    paging::{Flags, PageSize},
};
use core::{alloc::Layout, mem, ptr::NonNull};
use utils::{
    collections::stacklist::{Node, StackList},
    mem::VirtAddr,
    sanity_assert,
};

/// A node that holds a pointer to an object.
/// Pointer is uninitilized when `SlabObjEmbed` is used, but since the lowest size of memory that
/// we allocate in the stack is 2^8 bytes anyway, it doesn't matter
pub(super) type ObjectNode = Node<()>;

/// Errors that the slab allocator might encounter
#[derive(Debug, Copy, Clone)]
pub enum SlabError {
    /// The pointer passed to free is not aligned to the object's alignment
    BadPtrAlignment,
    /// The pointer passed to free is not in the range of the slab
    BadPtrRange,
    /// The pointer passed to free isn't allocated
    DoubleFree,
    /// The slab is full and cannot allocate any more objects
    SlabFullInternalError,
    /// Error while trying to allocate more pages for the slab
    PageAllocationError,
}

/// A slab allocator that allocates objects of a fixed Layout.
pub(super) struct InternalSlabAllocator {
    /// A list of slabs that are completely free
    free_slabs: StackList<Slab>,
    /// A list of slabs that have at least one free object
    partial_slabs: StackList<Slab>,
    /// A list of slabs that are completely full
    full_slabs: StackList<Slab>,
    /// The amount of pages each slab will have
    pages_per_slab: usize,
    /// The layout of the objects that will be allocated
    obj_layout: Layout,
    /// The amount of objects that will be allocated in each slab
    obj_per_slab: usize,
}

/// The core structure of a slab
#[derive(Debug)]
struct Slab {
    /// Pointer to the slab's buffer
    buff_ptr: NonNull<ObjectNode>,
    /// List of objects that are free in this slab
    free_objs: StackList<()>,
}

// TODO: Add an option to embed the slab node in the slab itself as well
impl InternalSlabAllocator {
    /// The size of the `Node<Slab>` struct, which is embedded in the slab buffer.
    const EMBEDDED_SLAB_NODE_SIZE: usize = Layout::new::<Node<Slab>>().pad_to_align().size();

    /// Calculates the amount of pages needed to fit at least one object of the given layout
    const fn pages_per_slab(obj_layout: Layout) -> usize {
        // In case of a ZST
        if obj_layout.size() == 0 || obj_layout.align() == 0 {
            return 0;
        }

        assert!(obj_layout.align() <= BASIC_PAGE_SIZE);

        // The minimum amount of pages that we need to allocate to fit at least one object
        let min_pages_per_slab = usize::div_ceil(obj_layout.size(), BASIC_PAGE_SIZE);

        // Calculate the remainder if we were to allocate `pages_per_slab` pages
        let r = (min_pages_per_slab * BASIC_PAGE_SIZE) % obj_layout.size();
        // If the remainder is less than the size of the `Node<Slab>` struct, we need to allocate
        // an additional page to fit the slab node
        if r < Self::EMBEDDED_SLAB_NODE_SIZE {
            min_pages_per_slab + 1
        } else {
            min_pages_per_slab
        }
    }

    /// Calculates the amount of objects with `obj_layout` that can fit in a slab
    const fn obj_per_slab(obj_layout: Layout, pages_per_slab: usize) -> usize {
        // The amount of objects that can fit in a slab
        (pages_per_slab * BASIC_PAGE_SIZE - Self::EMBEDDED_SLAB_NODE_SIZE) / obj_layout.size()
    }

    /// Creates a new slab allocator with the given object layout
    /// allocated externally using the kernel's heap
    ///
    // TODO: Possibly return an error instead of asserting
    pub(super) const fn new(mut obj_layout: Layout) -> InternalSlabAllocator {
        // Get the actual spacing between objects
        obj_layout = obj_layout.pad_to_align();

        let pages_per_slab = Self::pages_per_slab(obj_layout);

        assert!(
            obj_layout.size() >= size_of::<ObjectNode>(),
            "Object size is too small"
        );
        assert!(
            (BASIC_PAGE_SIZE * pages_per_slab) % obj_layout.align() == 0,
            "Object alignment is not valid"
        );

        let obj_per_slab = Self::obj_per_slab(obj_layout, pages_per_slab);
        assert!(
            obj_per_slab > 0,
            "Slab allocator cannot allocate less than one object"
        );

        InternalSlabAllocator {
            full_slabs: StackList::new(),
            partial_slabs: StackList::new(),
            free_slabs: StackList::new(),
            pages_per_slab,
            obj_layout,
            obj_per_slab,
        }
    }

    /// Allocates an object from the slab allocator and returns a pointer to it. If the slab is
    /// full, it will try to grow the cache and return a pointer to an object in the new slab
    pub(super) fn allocate(&mut self) -> Result<NonNull<()>, SlabError> {
        // First, try allocating from the partial slabs
        if let Some(partial_slab) = self.partial_slabs.peek_mut()
            && let ret @ Ok(_) = partial_slab.allocate()
        {
            // If the allocation resulted in the slab being empty, move it to the full slabs
            if partial_slab.free_objs.is_empty() {
                self.partial_slabs.pop_into(&mut self.full_slabs);
            }

            return ret;
        }

        // If also the free slabs are all empty, grow the cache
        if self.free_slabs.is_empty() {
            self.cache_grow()?;
        }

        // Try allocating from a free slab
        if let Some(free_slab) = self.free_slabs.peek_mut() {
            let ret = free_slab.allocate()?;
            self.free_slabs.pop_into(&mut self.partial_slabs);

            return Ok(ret);
        }

        unreachable!();
    }

    /// Frees the object pointed to by `ptr` from the slab allocator.
    ///
    /// SAFETY: This function is unsafe because the passed pointer needs to be a valid pointer to an
    /// allocated object.
    pub(super) unsafe fn free(&mut self, ptr: NonNull<ObjectNode>) -> Result<(), SlabError> {
        // Make sure `ptr` alignment is correct
        if !ptr.is_aligned_to(self.obj_layout.align()) {
            return Err(SlabError::BadPtrAlignment);
        }

        // Check the partial slabs
        for (index, slab) in self.partial_slabs.iter_mut().enumerate() {
            if slab.is_in_range(ptr, self.obj_per_slab, self.obj_layout.size()) {
                unsafe { slab.free(ptr)? };
                // If the slab is now completely free, move it to the free slabs
                if slab.free_objs.len() == self.obj_per_slab {
                    self.partial_slabs.remove_into(&mut self.free_slabs, index);
                }

                return Ok(());
            }
        }

        // Check if the slab to whom `ptr` belongs is in the full slabs list
        for (index, slab) in self.full_slabs.iter_mut().enumerate() {
            if slab.is_in_range(ptr, self.obj_per_slab, self.obj_layout.size()) {
                unsafe { slab.free(ptr)? };
                // Slab is no longer full, so move it to the partial slabs
                self.full_slabs.remove_into(&mut self.partial_slabs, index);

                return Ok(());
            }
        }

        // Pointer is not in any of the slabs
        Err(SlabError::BadPtrRange)
    }

    /// Reap all the slabs that are free. Should only be invoked by OOM killer, when the system desperately needs memory back.
    // TODO: Maybe pass in the amount of memory needed instead of freeing everything?
    pub(super) fn reap(&mut self) {
        while let Some(slab) = self.free_slabs.pop() {
            let offset = slab.buff_ptr.addr().get() % BASIC_PAGE_SIZE;
            let addr = VirtAddr(slab.buff_ptr.addr().get() - offset);
            unsafe {
                free_pages(addr, self.pages_per_slab, PageSize::size_4kb()).unwrap();
            };

            // IMPORTANT!!! We don't want to call the destructor on the slab, since it's already
            // been freed by `free_pages`!!
            mem::forget(slab);
        }
    }

    /// Grows the cache by allocating a new slab and adding it to the free slabs list.
    pub(super) fn cache_grow(&mut self) -> Result<(), SlabError> {
        // Allocate the pages for the buffer + the `Node<Slab>` struct
        let objs_ptr: NonNull<()> = allocate_pages(
            self.pages_per_slab,
            Flags::new().set_read_write(true),
            PageSize::size_4kb(),
        )
        .unwrap()
        .try_into()
        .map_err(|()| SlabError::PageAllocationError)?;

        // XXX: Make the slab node be at the end and the objects at the beginning of the buffer

        unsafe {
            let offset = objs_ptr
                .byte_add(self.obj_layout.size() * self.obj_per_slab)
                .align_offset(align_of::<Node<Slab>>());
            let slab_node_ptr = objs_ptr
                .byte_add(self.obj_layout.size() * self.obj_per_slab)
                .byte_add(offset)
                .cast::<Node<Slab>>();

            // Sanity check to make sure there is enough space on the buffer for the slab node
            sanity_assert!(
                slab_node_ptr.addr().get() - objs_ptr.addr().get() >= Self::EMBEDDED_SLAB_NODE_SIZE
            );

            NonNull::write(
                slab_node_ptr,
                Node::<Slab>::new(Slab::new(
                    objs_ptr,
                    self.obj_per_slab,
                    self.obj_layout.size(),
                )),
            );

            self.free_slabs.push_node(slab_node_ptr);
        };

        Ok(())
    }
}

impl Slab {
    /// Check if the given pointer **to the allocated data** belongs to this slab
    #[inline]
    fn is_in_range(
        &self,
        ptr: NonNull<ObjectNode>,
        obj_per_slab: usize,
        obj_padded_size: usize,
    ) -> bool {
        self.buff_ptr <= ptr
            && ptr < unsafe { self.buff_ptr.byte_add(obj_per_slab * obj_padded_size) }
    }

    /// Constructs a new slab with the given parameters.
    ///
    /// SAFETY: This is unsafe because `buff_ptr` must be a valid pointer to a slab of memory
    /// that is at least `obj_per_slab` objects in size
    #[inline]
    unsafe fn new(buff_ptr: NonNull<()>, obj_per_slab: usize, obj_padded_size: usize) -> Self {
        let mut free_objs = StackList::new();

        let buff_ptr = buff_ptr.cast::<ObjectNode>();
        for i in 0..obj_per_slab {
            unsafe {
                // SAFETY: This is OK because we already checked to make sure the ptr is aligned,
                // and the size is fine (already checked in the allocator)
                let ptr = buff_ptr.byte_add(i * obj_padded_size);

                free_objs.push_node(ptr);
            };
        }

        Slab {
            buff_ptr,
            free_objs,
        }
    }

    /// Allocates an object from the slab
    fn allocate(&mut self) -> Result<NonNull<()>, SlabError> {
        self.free_objs
            .pop_node()
            .map(|node| Box::into_non_null(node).cast::<()>())
            .ok_or(SlabError::SlabFullInternalError)
    }

    /// Frees an object from the slab
    ///
    /// SAFETY: This function is unsafe because the passed pointer needs to be a valid pointer to
    /// an allocated object.
    unsafe fn free(&mut self, obj_ptr: NonNull<ObjectNode>) -> Result<(), SlabError> {
        if self
            .free_objs
            .iter_node()
            .any(|node| NonNull::from_ref(node).cast::<ObjectNode>() == obj_ptr)
        {
            return Err(SlabError::DoubleFree);
        }

        // Turns obj_ptr to a new node to add to the list of free objects
        unsafe { self.free_objs.push_node(obj_ptr) };

        Ok(())
    }
}

impl Drop for InternalSlabAllocator {
    /// Frees all the slabs that were allocated by the allocator
    fn drop(&mut self) {
        assert!(
            self.partial_slabs.pop().is_none(),
            "Freeing a used slab (a partial slab). This is bad."
        );
        assert!(
            self.full_slabs.pop().is_none(),
            "Freeing a used slab (a full slab). This is bad."
        );

        // Free the free_slabs
        self.reap();
    }
}

/// Implementing `Drop` manually here since when the `StackList` field gets drop, it's popping and
/// trying to free all the `Node`s, and that's undefined behaviour because:
/// 1. `Drop` on `Slab` should only ever be called when `reap`ing. And when reaping, we manually `unmap` the object pages (on which `Slab` is also allocated)
/// 2. The `Node`s here are allocated manually, so calling a traditional free on them is UB anyway
///
/// NOTE: Technically it's more correct to implement the freeing behaviour here, but that would
/// require the slab to hold `pages_per_slab` as a field, but we want to keep `Slab` as small as
/// possible so we can stuff more objects in each slab buffer
impl Drop for Slab {
    fn drop(&mut self) {
        unreachable!("Drop called on slab. This shouldn't have happened");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    use core::alloc::Layout;
    use core::ptr::NonNull;
    use std::collections::HashSet;

    // Helper function to create a test layout
    fn test_layout(size: usize, align: usize) -> Layout {
        Layout::from_size_align(size, align).unwrap()
    }

    // Helper function to allocate and track pointers
    fn allocate_multiple(
        allocator: &mut InternalSlabAllocator,
        count: usize,
    ) -> Result<Vec<NonNull<()>>, SlabError> {
        let mut ptrs = Vec::new();
        for _ in 0..count {
            ptrs.push(allocator.allocate()?);
        }
        Ok(ptrs)
    }

    #[test]
    fn test_new_allocator_creation() {
        // Test creating allocators with different layouts
        let layouts = [
            test_layout(8, 8),
            test_layout(16, 8),
            test_layout(32, 16),
            test_layout(64, 32),
            test_layout(128, 64),
            test_layout(256, 128),
            test_layout(512, 256),
            test_layout(1024, 512),
        ];

        for layout in layouts.iter() {
            let allocator = InternalSlabAllocator::new(*layout);
            assert_eq!(allocator.obj_layout, layout.pad_to_align());
            assert!(allocator.pages_per_slab > 0);
            assert!(allocator.obj_per_slab > 0);
        }
    }

    #[test]
    fn test_pages_per_slab_calculation() {
        // Test various object sizes and their page requirements
        let test_cases = [
            (8, 8),    // Small objects
            (64, 64),  // Medium objects
            (512, 512), // Large objects
            (2048, 1024), // Very large objects
            (4096, 4096), // Page-sized objects
        ];

        for (size, align) in test_cases.iter() {
            let layout = test_layout(*size, *align);
            let pages = InternalSlabAllocator::pages_per_slab(layout);
            assert!(pages > 0, "Pages per slab should be positive for size {}", size);
            
            // Verify we can fit at least one object
            let total_size = pages * BASIC_PAGE_SIZE;
            assert!(
                total_size >= size + InternalSlabAllocator::EMBEDDED_SLAB_NODE_SIZE,
                "Should have space for at least one object and slab node"
            );
        }
    }

    #[test]
    fn test_obj_per_slab_calculation() {
        let layout = test_layout(64, 64);
        let pages = InternalSlabAllocator::pages_per_slab(layout);
        let objs = InternalSlabAllocator::obj_per_slab(layout, pages);
        
        assert!(objs > 0, "Should be able to fit at least one object");
        
        // Verify the calculation makes sense
        let total_space = pages * BASIC_PAGE_SIZE - InternalSlabAllocator::EMBEDDED_SLAB_NODE_SIZE;
        let expected_objs = total_space / layout.size();
        assert_eq!(objs, expected_objs);
    }

    #[test]
    fn test_basic_allocation_and_free() {
        let mut allocator = InternalSlabAllocator::new(test_layout(64, 64));
        
        // Allocate a single object
        let ptr = allocator.allocate().unwrap();
        
        // Free the object
        unsafe {
            allocator.free(ptr.cast()).unwrap();
        }
    }

    #[test]
    fn test_multiple_allocations() {
        let mut allocator = InternalSlabAllocator::new(test_layout(32, 32));
        let count = 10;
        
        let ptrs = allocate_multiple(&mut allocator, count).unwrap();
        
        // Verify all pointers are unique and properly aligned
        let mut unique_addrs = HashSet::new();
        for ptr in &ptrs {
            assert!(ptr.as_ptr().is_aligned_to(32));
            assert!(unique_addrs.insert(ptr.as_ptr() as usize), "Duplicate pointer detected");
        }
        
        // Free all objects
        for ptr in ptrs {
            unsafe {
                allocator.free(ptr.cast()).unwrap();
            }
        }
    }

    #[test]
    fn test_fill_entire_slab() {
        let layout = test_layout(64, 64);
        let mut allocator = InternalSlabAllocator::new(layout);
        let obj_per_slab = allocator.obj_per_slab;
        
        // Allocate exactly the number of objects that fit in one slab
        let ptrs = allocate_multiple(&mut allocator, obj_per_slab)
            .unwrap();
        
        assert_eq!(ptrs.len(), obj_per_slab);
        
        // Free all objects
        for ptr in ptrs {
            unsafe {
                allocator.free(ptr.cast()).unwrap();
            }
        }
    }

    #[test]
    fn test_cache_grow_on_overflow() {
        let layout = test_layout(128, 128);
        let mut allocator = InternalSlabAllocator::new(layout);
        let obj_per_slab = allocator.obj_per_slab;
        
        // Allocate more objects than fit in one slab to trigger cache growth
        let total_objects = obj_per_slab * 2 + 5;
        let ptrs = allocate_multiple(&mut allocator, total_objects)
            .unwrap();
        
        assert_eq!(ptrs.len(), total_objects);
        
        // Free all objects
        for ptr in ptrs {
            unsafe {
                allocator.free(ptr.cast()).unwrap();
            }
        }
    }

    #[test]
    fn test_random_allocation_pattern() {
        use std::collections::VecDeque;
        
        let mut allocator = InternalSlabAllocator::new(test_layout(64, 64));
        let mut allocated_ptrs = VecDeque::new();
        
        // Simulate random allocation/deallocation pattern
        for i in 0..100 {
            if i % 3 == 0 && !allocated_ptrs.is_empty() {
                // Free a random object
                let ptr: NonNull<()> = allocated_ptrs.pop_front().unwrap();
                unsafe {
                    allocator.free(ptr.cast()).unwrap();
                }
            } else {
                // Allocate a new object
                let ptr = allocator.allocate().unwrap();
                allocated_ptrs.push_back(ptr);
            }
        }
        
        // Free remaining objects
        while let Some(ptr) = allocated_ptrs.pop_front() {
            unsafe {
                allocator.free(ptr.cast()).unwrap();
            }
        }
    }

    #[test]
    fn test_different_object_sizes() {
        let sizes = [8, 16, 32, 64, 128, 256, 512, 1024];
        
        for size in sizes.iter() {
            let layout = test_layout(*size, *size);
            let mut allocator = InternalSlabAllocator::new(layout);
            
            // Allocate some objects
            let ptrs = allocate_multiple(&mut allocator, 5)
                .unwrap();
            
            // Verify alignment
            for ptr in &ptrs {
                assert!(ptr.as_ptr().is_aligned_to(*size));
            }
            
            // Free objects
            for ptr in ptrs {
                unsafe {
                    allocator.free(ptr.cast()).unwrap();
                }
            }
        }
    }

    #[test]
    fn test_bad_ptr_alignment_error() {
        let mut allocator = InternalSlabAllocator::new(test_layout(64, 64));
        
        // Create a misaligned pointer
        let misaligned_ptr = NonNull::new(0x1001 as *mut ObjectNode).unwrap(); // Not 64-byte aligned
        
        unsafe {
            let result = allocator.free(misaligned_ptr);
            assert!(matches!(result, Err(SlabError::BadPtrAlignment)));
        }
    }

    #[test]
    fn test_bad_ptr_range_error() {
        let mut allocator = InternalSlabAllocator::new(test_layout(64, 64));
        
        // Create a pointer that's aligned but not from our allocator
        let external_ptr = NonNull::new(0x10000 as *mut ObjectNode).unwrap(); // Properly aligned but external
        
        unsafe {
            let result = allocator.free(external_ptr);
            assert!(matches!(result, Err(SlabError::BadPtrRange)));
        }
    }

    #[test]
    fn test_double_free_error() {
        let mut allocator = InternalSlabAllocator::new(test_layout(64, 64));
        
        // Allocate an object
        let ptr = allocator.allocate().unwrap();
        
        // Free it once (should succeed)
        unsafe {
            allocator.free(ptr.cast()).unwrap();
        }
        
        // Try to free it again (should fail)
        unsafe {
            let result = allocator.free(ptr.cast());
            assert!(matches!(result, Err(SlabError::DoubleFree)));
        }
    }

    #[test]
    fn test_slab_transitions() {
        let layout = test_layout(64, 64);
        let mut allocator = InternalSlabAllocator::new(layout);
        let obj_per_slab = allocator.obj_per_slab;
        
        // Initially all lists should be empty
        assert!(allocator.free_slabs.is_empty());
        assert!(allocator.partial_slabs.is_empty());
        assert!(allocator.full_slabs.is_empty());
        
        // Allocate one object (should create a slab and move it to partial)
        let ptr1 = allocator.allocate().unwrap();
        assert!(allocator.free_slabs.is_empty());
        assert!(!allocator.partial_slabs.is_empty());
        assert!(allocator.full_slabs.is_empty());
        
        // Fill the rest of the slab (should move it to full)
        let mut ptrs = vec![ptr1];
        for _ in 1..obj_per_slab {
            ptrs.push(allocator.allocate().unwrap());
        }
        assert!(allocator.free_slabs.is_empty());
        assert!(allocator.partial_slabs.is_empty());
        assert!(!allocator.full_slabs.is_empty());
        
        // Free one object (should move slab back to partial)
        unsafe {
            allocator.free(ptrs.pop().unwrap().cast()).unwrap();
        }
        assert!(allocator.free_slabs.is_empty());
        assert!(!allocator.partial_slabs.is_empty());
        assert!(allocator.full_slabs.is_empty());
        
        // Free all remaining objects (should move slab to free)
        while let Some(ptr) = ptrs.pop() {
            unsafe {
                allocator.free(ptr.cast()).unwrap();
            }
        }
        assert!(!allocator.free_slabs.is_empty());
        assert!(allocator.partial_slabs.is_empty());
        assert!(allocator.full_slabs.is_empty());
    }

    #[test]
    fn test_cache_grow_functionality() {
        let mut allocator = InternalSlabAllocator::new(test_layout(64, 64));
        
        // Initially no slabs
        assert!(allocator.free_slabs.is_empty());
        
        // Manually trigger cache growth
        allocator.cache_grow().unwrap();
        
        // Should now have one free slab
        assert!(!allocator.free_slabs.is_empty());
        assert_eq!(allocator.free_slabs.len(), 1);
    }

    #[test]
    fn test_reap_functionality() {
        let mut allocator = InternalSlabAllocator::new(test_layout(64, 64));
        
        // Create some slabs by allocating and then freeing everything
        let obj_per_slab = allocator.obj_per_slab;
        let ptrs = allocate_multiple(&mut allocator, obj_per_slab * 3)
            .expect("Should allocate multiple slabs");
        
        // Free everything to get multiple free slabs
        for ptr in ptrs {
            unsafe {
                allocator.free(ptr.cast()).unwrap();
            }
        }
        
        // Should have free slabs now
        let free_slabs_count = allocator.free_slabs.len();
        assert!(free_slabs_count > 0);
        
        // Reap should free all free slabs
        allocator.reap();
        assert!(allocator.free_slabs.is_empty());
    }

    #[test]
    fn test_slab_is_in_range() {
        let layout = test_layout(64, 64);
        let mut allocator = InternalSlabAllocator::new(layout);
        
        // Allocate an object to create a slab
        let ptr = allocator.allocate().unwrap();
        
        // The pointer should be in range of the slab that allocated it
        // We can't directly test this without accessing private fields,
        // so we test indirectly by ensuring free works
        unsafe {
            allocator.free(ptr.cast()).unwrap();
        }
    }

    #[test]
    fn test_mixed_size_stress_test() {
        // Test with different alignments and sizes
        let test_cases = [
            (8, 8),
            (16, 16), 
            (32, 32),
            (64, 64),
            (128, 128),
        ];
        
        for (size, align) in test_cases.iter() {
            let mut allocator = InternalSlabAllocator::new(test_layout(*size, *align));
            let mut allocated = Vec::new();
            
            // Allocate many objects
            for _ in 0..50 {
                let ptr = allocator.allocate().unwrap();
                allocated.push(ptr);
            }
            
            // Free every other object
            let mut to_free = Vec::new();
            for (i, ptr) in allocated.iter().enumerate() {
                if i % 2 == 0 {
                    to_free.push(*ptr);
                }
            }
            
            for ptr in to_free {
                unsafe {
                    allocator.free(ptr.cast()).unwrap();
                }
            }
            
            // Allocate more objects in the gaps
            for _ in 0..25 {
                let ptr = allocator.allocate().unwrap();
                allocated.push(ptr);
            }
            
            // Free everything
            for ptr in allocated {
                unsafe {
                    allocator.free(ptr.cast()).unwrap();
                }
            }
        }
    }

    #[test]
    fn test_memory_layout_consistency() {
        let layout = test_layout(128, 128);
        let mut allocator = InternalSlabAllocator::new(layout);
        
        // Allocate several objects
        let ptrs = allocate_multiple(&mut allocator, 10).unwrap();
        
        // Check that objects are properly spaced
        let mut addresses: Vec<usize> = ptrs.iter().map(|p| p.as_ptr() as usize).collect();
        addresses.sort();
        
        // Adjacent objects should be at least obj_layout.size() apart
        for window in addresses.windows(2) {
            let diff = window[1] - window[0];
            assert!(diff >= layout.size(), "Objects not properly spaced: diff={}, size={}", diff, layout.size());
        }
        
        // Free all objects
        for ptr in ptrs {
            unsafe {
                allocator.free(ptr.cast()).unwrap();
            }
        }
    }

    #[test]
    fn test_allocator_drop_with_free_slabs() {
        let layout = test_layout(64, 64);
        let mut allocator = InternalSlabAllocator::new(layout);
        
        // Allocate and free some objects to create free slabs
        let ptrs = allocate_multiple(&mut allocator, 10).unwrap();
        for ptr in ptrs {
            unsafe {
                allocator.free(ptr.cast()).unwrap();
            }
        }
        
        // Allocator should drop cleanly even with free slabs
        drop(allocator);
    }

    #[test]
    #[should_panic(expected = "Freeing a used slab")]
    fn test_allocator_drop_with_allocated_objects_panics() {
        let mut allocator = InternalSlabAllocator::new(test_layout(64, 64));
        
        // Allocate an object but don't free it
        let _ptr = allocator.allocate().unwrap();
        
        // This should panic when the allocator is dropped
        drop(allocator);
    }

    #[test]
    fn test_edge_case_large_objects() {
        // Test with objects that are close to page size
        let layout = test_layout(3072, 1024); // 3KB objects
        let mut allocator = InternalSlabAllocator::new(layout);
        
        // Should still be able to allocate at least one object
        let ptr = allocator.allocate().unwrap();
        
        unsafe {
            allocator.free(ptr.cast()).unwrap();
        }
    }

    #[test]
    fn test_alignment_requirements() {
        let alignments = [8, 16, 32, 64, 128, 256, 512, 1024];
        
        for align in alignments.iter() {
            let layout = test_layout(*align * 2, *align); // Size is 2x alignment
            let mut allocator = InternalSlabAllocator::new(layout);
            
            let ptr = allocator.allocate().unwrap();
            assert!(ptr.as_ptr().is_aligned_to(*align), "Incorrect alignment for {}", align);
            
            unsafe {
                allocator.free(ptr.cast()).unwrap();
            }
        }
    }

    #[test]
    fn test_fragmentation_and_coalescing() {
        let mut allocator = InternalSlabAllocator::new(test_layout(64, 64));
        let obj_per_slab = allocator.obj_per_slab;
        
        // Allocate two full slabs worth of objects
        let ptrs = allocate_multiple(&mut allocator, obj_per_slab * 2)
            .unwrap();
        
        // Free every other object to create fragmentation
        let mut freed_ptrs = Vec::new();
        let mut kept_ptrs = Vec::new();
        
        for (i, ptr) in ptrs.into_iter().enumerate() {
            if i % 2 == 0 {
                freed_ptrs.push(ptr);
                unsafe {
                    allocator.free(ptr.cast()).unwrap();
                }
            } else {
                kept_ptrs.push(ptr);
            }
        }
        
        // Allocate again - should reuse the freed slots
        let new_ptrs = allocate_multiple(&mut allocator, freed_ptrs.len())
            .unwrap();
        
        // Clean up
        for ptr in kept_ptrs.into_iter().chain(new_ptrs.into_iter()) {
            unsafe {
                allocator.free(ptr.cast()).unwrap();
            }
        }
    }
}
