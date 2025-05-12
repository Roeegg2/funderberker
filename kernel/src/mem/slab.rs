//! A simple slab allocator for the kernel heap & custom use

use alloc::boxed::Box;
use core::mem;
use core::{alloc::Layout, ptr::NonNull};
use utils::sanity_assert;

use utils::collections::stacklist::{Node, StackList};

use crate::arch::BASIC_PAGE_SIZE;
use crate::arch::x86_64::paging::Entry;
use crate::mem::vmm::{allocate_pages, free_pages};

use super::VirtAddr;

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
    obj_count: usize,
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
            if slab.is_in_range(ptr, self.obj_count, self.obj_layout.size()) {
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
            if slab.is_in_range(ptr, self.obj_count, self.obj_layout.size()) {
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
            unsafe { free_pages(addr, self.pages_per_slab); };

            // IMPORTANT!!! We don't want to call the destructor on the slab, since it's already
            // been freed by `free_pages`!!
            mem::forget(slab);
        }
    }

    /// Grows the cache by allocating a new slab and adding it to the free slabs list.
    pub(super) fn cache_grow(&mut self) -> Result<(), SlabError> {
        // Allocate the pages for the buffer + the `Node<Slab>` struct
        let ptr: NonNull<()> = allocate_pages(self.pages_per_slab, Entry::FLAG_RW)
            .try_into()
            .map_err(|()| SlabError::PageAllocationError)?;

        let slab_node_ptr = ptr.cast::<Node<Slab>>();
        unsafe {
            let offset = slab_node_ptr.add(1).align_offset(self.obj_layout.align());
            let objs_ptr = slab_node_ptr.add(1).byte_add(offset).cast::<()>();

            // Sanity check to make sure there is enough space on the buffer for self.obj_count
            {
                // The total size of pages we allocated minus the size of the `Node<Slab>` and any
                // of the padding needed
                let actually_obj_space = (BASIC_PAGE_SIZE * self.pages_per_slab)
                    - (objs_ptr.addr().get() - ptr.addr().get());

                // Just making sure they actually match
                sanity_assert!(actually_obj_space / self.obj_layout.size() == self.obj_count);
            }

            // SAFETY: This is OK since `slab_node_ptr` is at least `BASIC_PAGE_SIZE` page aligned and sized, which of course satisfies `Node<Slab>`
            // And we also aligned `obj_ptr` to the object's alignment and we already calculated so
            // size should be fine as well
            NonNull::write(
                slab_node_ptr,
                Node::<Slab>::new(Slab::new(objs_ptr, self.obj_count, self.obj_layout.size())),
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
        obj_count: usize,
        obj_padded_size: usize,
    ) -> bool {
        self.buff_ptr <= ptr && ptr < unsafe { self.buff_ptr.byte_add(obj_count * obj_padded_size) }
    }

    /// Constructs a new slab with the given parameters.
    ///
    /// SAFETY: This is unsafe because `buff_ptr` must be a valid pointer to a slab of memory
    /// that is at least `obj_count` objects in size
    #[inline]
    unsafe fn new(buff_ptr: NonNull<()>, obj_count: usize, obj_padded_size: usize) -> Self {
        let mut free_objs = StackList::new();

        let buff_ptr = buff_ptr.cast::<ObjectNode>();
        for i in 0..obj_count {
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
/// 1. `Drop` on `Slab` should only ever be called when `reap`ing. And when reaping, we manually 
///     `unmap` the object pages (on which `Slab` is also allocated)
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
    use alloc::vec;
    use alloc::{format, vec::Vec};
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
            let ptr = allocator.allocate().expect("Allocation failed");
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
        let ptr = allocator.allocate().expect("Allocation after free failed");
        unsafe {
            allocator
                .free(ptr.cast::<ObjectNode>())
                .expect("Free failed");
        };
    }

    #[test_fn]
    fn test_slab_alloc_free_large_objects() {
        let layout = Layout::from_size_align(512, 16).unwrap();
        let mut allocator = InternalSlabAllocator::new(layout);

        // Allocate 5 large objects
        let mut pointers = vec![];
        for _ in 0..5 {
            let ptr = allocator.allocate().expect("Allocation failed");
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
            };
        }

        // Allocate one more to test partial slab
        let ptr = allocator.allocate().expect("Allocation after free failed");
        unsafe {
            allocator
                .free(ptr.cast::<ObjectNode>())
                .expect("Free failed");
        };
    }

    #[test_fn]
    fn test_slab_mixed_layout_sizes() {
        // Define test layouts for small, medium, and large allocations
        let layouts = [
            Layout::from_size_align(8, 4).expect("Invalid small layout"),
            Layout::from_size_align(64, 8).expect("Invalid medium layout"),
            Layout::from_size_align(256, 16).expect("Invalid large layout"),
        ];

        for layout in layouts {
            let mut allocator = InternalSlabAllocator::new(layout);
            let mut pointers = Vec::with_capacity(12); // Pre-allocate for 8 + 4 pointers

            for i in 0..8 {
                let ptr = allocator
                    .allocate()
                    .expect(&format!("Allocation {} failed for layout {:?}", i, layout));
                assert!(
                    ptr.is_aligned_to(layout.align()),
                    "Pointer {} not aligned to {} for layout {:?}",
                    i,
                    layout.align(),
                    layout
                );
                pointers.push(ptr);
            }

            // Step 2: Free every other object (indices 0, 2, 4, 6)
            for (i, ptr) in pointers.iter().enumerate().step_by(2) {
                unsafe {
                    allocator.free(ptr.cast::<ObjectNode>()).expect(&format!(
                        "Free failed for pointer {} in layout {:?}",
                        i, layout
                    ));
                }
            }

            // Step 3: Allocate 4 more objects and verify alignment
            let mut new_pointers = Vec::with_capacity(4);
            for i in 0..4 {
                let ptr = allocator.allocate().expect(&format!(
                    "Allocation {} after partial free failed for layout {:?}",
                    i, layout
                ));
                assert!(
                    ptr.is_aligned_to(layout.align()),
                    "Pointer {} after partial free not aligned to {} for layout {:?}",
                    i,
                    layout.align(),
                    layout
                );
                new_pointers.push(ptr);
            }

            // Step 4: Free every other object (indices 0, 2, 4, 6)
            for (i, ptr) in pointers.iter().enumerate().skip(1).step_by(2) {
                unsafe {
                    allocator.free(ptr.cast::<ObjectNode>()).expect(&format!(
                        "Free failed for pointer {} in layout {:?}",
                        i, layout
                    ));
                }
            }

            // Step 5: Free the 4 other allocated pointers
            for (i, ptr) in new_pointers.iter().enumerate() {
                unsafe {
                    allocator.free(ptr.cast::<ObjectNode>()).expect(&format!(
                        "Free failed for pointer {} in layout {:?}",
                        i, layout
                    ));
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
            let ptr = allocator.allocate().expect("Allocation failed");
            assert!(ptr.is_aligned_to(layout.align()), "Pointer not aligned");
            pointers.push(ptr);
        }

        // Allocate one more to trigger cache growth
        let extra_ptr = allocator
            .allocate()
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
