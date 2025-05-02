//! A simple slab allocator for the kernel heap & custom use

use alloc::boxed::Box;
use core::{alloc::Layout, num::NonZero, ptr::NonNull, usize};

use utils::collections::stacklist::{Node, StackList};

use crate::arch::BASIC_PAGE_SIZE;
use crate::arch::x86_64::paging::Entry;
use crate::mem::vmm::{allocate_pages, free_pages};

use super::pmm::{self, PmmAllocator};

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
    obj_count: usize,
}

// TODO: Add an option to embed the slab node in the slab itself as well
impl InternalSlabAllocator {
    /// Calculates the amount of pages needed to fit at least one object of the given layout
    const fn calc_pages_per_slab(obj_layout: Layout) -> usize {
        // The initial (r)emainder (i.e. unused space due to internal fragmentation)
        let r = BASIC_PAGE_SIZE % obj_layout.size();

        // If allocating a single page is enough to get internal fragmentation under 12.5% (1/8)
        // XXX: Might need to multiple here by 10000 or something
        if (r * 100000 / BASIC_PAGE_SIZE) <= (1 * 100000 / 8) {
            return 1;
        }

        // Find the page (c)ount that would let us fit another obj inside
        let c = usize::div_ceil(obj_layout.size(), r);
        // Find the (d)ifference by which internal fragmentation decreases every time we allocate
        // an addtional `c` pages
        let d = r - ((r * c) - obj_layout.size());

        // Formula to get the minimal amount of `c` pages that would result in internal
        // fragmentation being less than 12.5%.
        // This formula is a simplification of this expression:
        //
        //    r - d * x       1
        // -------------- <= ---
        // BASIC_PAGE_SIZE * c * x     8
        //
        // (where `x` is the amount of `c` blocks we want to find)
        //
        // To this:
        //
        //       r
        // ------------- <= x
        //  512 * q + d
        //
        // And since `x` is a whole number (a physical amount of blocks), we need to round up.
        // And then after we calculated x, to get the actual final page amount we return
        // (rounded up `x`) * c
        //
        usize::div_ceil(r, (512 * c) + d) * c
    }

    /// Creates a new slab allocator with the given object layout
    /// allocated externally using the kernel's heap
    ///
    // TODO: Possibly return an error instead of asserting
    pub(super) const fn new(mut obj_layout: Layout) -> InternalSlabAllocator {
        /// Calculates the amount of objects with `obj_layout` that can fit in a slab
        const fn calc_obj_count(pages_per_slab: usize, obj_size: usize) -> usize {
            (pages_per_slab * BASIC_PAGE_SIZE - Layout::new::<Node<Slab>>().pad_to_align().size())
                / obj_size
        }

        // Get the actual spacing between objects
        obj_layout = obj_layout.pad_to_align();

        let pages_per_slab = Self::calc_pages_per_slab(obj_layout);

        assert!(
            obj_layout.size() >= size_of::<ObjectNode>(),
            "Object size is too small"
        );
        assert!(
            (BASIC_PAGE_SIZE * pages_per_slab) % obj_layout.align() == 0,
            "Object alignment is not valid"
        );

        let obj_count = calc_obj_count(pages_per_slab, obj_layout.size());

        InternalSlabAllocator {
            full_slabs: StackList::new(),
            partial_slabs: StackList::new(),
            free_slabs: StackList::new(),
            pages_per_slab,
            obj_layout,
            obj_count,
        }
    }

    /// Allocates an object from the slab allocator and returns a pointer to it. If the slab is
    /// full, it will try to grow the cache and return a pointer to an object in the new slab
    pub(super) fn alloc(&mut self) -> Result<NonNull<()>, SlabError> {
        // First, try allocating from the partial slabs
        if let Some(partial_slab) = self.partial_slabs.peek_mut()
            && let ret @ Ok(_) = partial_slab.alloc()
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
            let ret = free_slab.alloc()?;
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
            if slab.is_in_range(ptr, self.obj_count, self.obj_layout) {
                unsafe { slab.free(ptr)? };
                // If the slab is now completely free, move it to the free slabs
                if slab.free_objs.len() == self.obj_count {
                    self.partial_slabs.remove_into(&mut self.free_slabs, index);
                }

                return Ok(());
            }
        }

        // Check if the slab to whom `ptr` belongs is in the full slabs list
        for (index, slab) in self.full_slabs.iter_mut().enumerate() {
            if slab.is_in_range(ptr, self.obj_count, self.obj_layout) {
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
            unsafe { free_pages(slab.buff_ptr.into(), self.pages_per_slab) }
        }
    }

    /// Grows the cache by allocating a new slab and adding it to the free slabs list.
    pub(super) fn cache_grow(&mut self) -> Result<(), SlabError> {
        let buff_ptr: NonNull<()> = allocate_pages(self.pages_per_slab, Entry::FLAG_RW)
            .try_into()
            .map_err(|_| SlabError::PageAllocationError)?;

        // let phys_addr = pmm::get().alloc_any(NonZero::new(1).unwrap(), NonZero::new(self.pages_per_slab).unwrap()).unwrap();
        // let buff_ptr: NonNull<()> = phys_addr
        //     .add_hhdm_offset()
        //     .try_into()
        //     .map_err(|_| SlabError::PageAllocationError)?;

        unsafe {
            // SAFETY: Size is OK since we allocated the pages_per_slab amount of pages. Alignment
            // is OK since BASIC_PAGE_SIZE * n is always aligned to Node<Slab>, and so BASIC_PAGE_SIZE * n -
            // size_of(Node<Slab>) is also aligned to Node<Slab>
            let slab_ptr = buff_ptr
                .byte_add(
                    self.pages_per_slab * BASIC_PAGE_SIZE
                        - Layout::new::<Node<Slab>>().pad_to_align().size(),
                )
                .cast::<Node<Slab>>();

            NonNull::write(
                slab_ptr,
                Node::<Slab>::new(Slab::new(buff_ptr, self.obj_count, self.obj_layout).unwrap()),
            );

            self.free_slabs.push_node(slab_ptr);
        }

        Ok(())
    }
}

impl Drop for InternalSlabAllocator {
    /// Frees all the slabs that were allocated by the allocator
    fn drop(&mut self) {
        // Free the free_slabs
        self.reap();

        // Free the partial slabs
        while let Some(slab) = self.partial_slabs.pop() {
            unsafe {
                free_pages(slab.buff_ptr.into(), self.pages_per_slab);
            }
        }

        // Free the full slabs
        while let Some(slab) = self.full_slabs.pop() {
            unsafe {
                free_pages(slab.buff_ptr.into(), self.pages_per_slab);
            }
        }
    }
}

/// A node that holds a pointer to an object.
/// Pointer is uninitilized when `SlabObjEmbed` is used, but since the lowest size of memory that
/// we allocate in the stack is 2^8 bytes anyway, it doesn't matter
pub(super) type ObjectNode = Node<()>;

/// The core structure of a slab
#[derive(Debug)]
struct Slab {
    /// Pointer to the slab's buffer
    buff_ptr: NonNull<ObjectNode>,
    /// List of objects that are free in this slab
    free_objs: StackList<()>,
}

impl Slab {
    /// Check if the given pointer **to the allocated data** belongs to this slab
    #[inline]
    fn is_in_range(&self, ptr: NonNull<ObjectNode>, obj_count: usize, obj_layout: Layout) -> bool {
        self.buff_ptr <= ptr
            && ptr < unsafe { self.buff_ptr.byte_add(obj_count * obj_layout.size()) }
    }

    /// Constructs a new slab with the given parameters.
    ///
    /// SAFETY: This is unsafe because `buff_ptr` must be a valid pointer to a slab of memory
    /// that is at least `obj_count` objects in size
    #[inline]
    unsafe fn new(
        buff_ptr: NonNull<()>,
        obj_count: usize,
        obj_layout: Layout,
    ) -> Result<Self, SlabError> {
        let mut free_objs = StackList::new();

        let buff_ptr = buff_ptr.cast::<ObjectNode>();
        for i in 0..obj_count {
            unsafe {
                // SAFETY: This is OK because we already checked to make sure the ptr is aligned,
                // and the size is fine (already checked in the allocator)
                let ptr = buff_ptr.byte_add(i * obj_layout.size());

                free_objs.push_node(ptr);
            };
        }

        Ok(Slab {
            buff_ptr,
            free_objs,
        })
    }

    /// Allocates an object from the slab
    fn alloc(&mut self) -> Result<NonNull<()>, SlabError> {
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
            .find(|&node| NonNull::from_ref(node).cast::<ObjectNode>() == obj_ptr)
            .is_some()
        {
            return Err(SlabError::DoubleFree);
        }

        // Turns obj_ptr to a new node to add to the list of free objects
        unsafe { self.free_objs.push_node(obj_ptr) };

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;
    use macros::test_fn;

    use super::*;
    use core::alloc::Layout;

    #[test_fn]
    fn test_slab_alloc_free_small_objects() {
        let layout = Layout::from_size_align(16, 4).unwrap();
        let mut allocator = InternalSlabAllocator::new(layout);

        // Allocate 10 objects
        let mut pointers = vec![];
        for _ in 0..10 {
            let ptr = allocator.alloc().expect("Allocation failed");
            assert!(ptr.is_aligned_to(layout.align()), "Pointer not aligned");
            pointers.push(ptr);
        }

        // Free in reverse order
        for ptr in pointers.iter().rev() {
            unsafe {
                allocator
                    .free(ptr.cast::<ObjectNode>())
                    .expect("Free failed");
            }
        }

        // Allocate again to ensure slab reuse
        let ptr = allocator.alloc().expect("Allocation after free failed");
        unsafe {
            allocator
                .free(ptr.cast::<ObjectNode>())
                .expect("Free failed");
        }
    }

    #[test_fn]
    fn test_slab_alloc_free_large_objects() {
        let layout = Layout::from_size_align(512, 16).unwrap();
        let mut allocator = InternalSlabAllocator::new(layout);

        // Allocate 5 large objects
        let mut pointers = vec![];
        for _ in 0..5 {
            let ptr = allocator.alloc().expect("Allocation failed");
            assert!(ptr.is_aligned_to(layout.align()), "Pointer not aligned");
            pointers.push(ptr);
        }

        // Free in random order
        let free_order = [2, 0, 4, 1, 3];
        for &i in &free_order {
            unsafe {
                allocator
                    .free(pointers[i].cast::<ObjectNode>())
                    .expect("Free failed");
            }
        }

        // Allocate one more to test partial slab
        let ptr = allocator.alloc().expect("Allocation after free failed");
        unsafe {
            allocator
                .free(ptr.cast::<ObjectNode>())
                .expect("Free failed");
        }
    }

    #[test_fn]
    fn test_slab_mixed_layout_sizes() {
        // Test with small, medium, and large layouts
        let layouts = [
            Layout::from_size_align(8, 4).unwrap(),
            Layout::from_size_align(64, 8).unwrap(),
            Layout::from_size_align(256, 16).unwrap(),
        ];

        for &layout in &layouts {
            let mut allocator = InternalSlabAllocator::new(layout);

            // Allocate 8 objects
            let mut pointers = vec![];
            for _ in 0..8 {
                let ptr = allocator.alloc().expect("Allocation failed");
                assert!(ptr.is_aligned_to(layout.align()), "Pointer not aligned");
                pointers.push(ptr);
            }

            // Free every other object
            for i in (0..8).step_by(2) {
                unsafe {
                    allocator
                        .free(pointers[i].cast::<ObjectNode>())
                        .expect("Free failed");
                }
            }

            // Allocate 4 more
            for _ in 0..4 {
                let ptr = allocator
                    .alloc()
                    .expect("Allocation after partial free failed");
                assert!(ptr.is_aligned_to(layout.align()), "Pointer not aligned");
                pointers.push(ptr);
            }

            // Free all remaining
            for ptr in pointers.iter().skip(1).step_by(2) {
                unsafe {
                    allocator
                        .free(ptr.cast::<ObjectNode>())
                        .expect("Free failed");
                }
            }
        }
    }

    #[test_fn]
    fn test_slab_fill_slab_and_free() {
        let layout = Layout::from_size_align(32, 8).unwrap();
        let mut allocator = InternalSlabAllocator::new(layout);

        // Fill the slab completely
        let obj_count = allocator.obj_count;
        let mut pointers = vec![];
        for _ in 0..obj_count {
            let ptr = allocator.alloc().expect("Allocation failed");
            assert!(ptr.is_aligned_to(layout.align()), "Pointer not aligned");
            pointers.push(ptr);
        }

        // Allocate one more to trigger cache growth
        let extra_ptr = allocator
            .alloc()
            .expect("Allocation after full slab failed");
        pointers.push(extra_ptr);

        // Free in forward order
        for ptr in pointers {
            unsafe {
                allocator
                    .free(ptr.cast::<ObjectNode>())
                    .expect("Free failed");
            }
        }
    }
}
