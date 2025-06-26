//! "Backend" of the slab allocator

use alloc::boxed::Box;
use core::{alloc::Layout, mem, ptr::NonNull};
use kernel::{
    arch::BASIC_PAGE_SIZE, mem::paging::PagingError};
use utils::{
    collections::stacklist::{Node, StackList},
    mem::VirtAddr,
    sanity_assert, sync::spinlock::SpinLockable,
};

/// A node that holds a pointer to an object.
/// Pointer is uninitilized when `SlabObjEmbed` is used, but since the lowest size of memory that
/// we allocate in the stack is 2^8 bytes anyway, it doesn't matter
pub(super) type ObjectNode = Node<()>;

/// Errors that the slab allocator might encounter
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
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
    /// Paging error occurred
    PagingError(PagingError),
}

/// A slab allocator that allocates objects of a fixed Layout.
#[derive(Debug)]
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

        assert!(obj_layout.align() <= BASIC_PAGE_SIZE.size());

        // The minimum amount of pages that we need to allocate to fit at least one object
        let min_pages_per_slab = usize::div_ceil(obj_layout.size(), BASIC_PAGE_SIZE.size());

        // Calculate the remainder if we were to allocate `pages_per_slab` pages
        let r = (min_pages_per_slab * BASIC_PAGE_SIZE.size()) % obj_layout.size();
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
        (pages_per_slab * BASIC_PAGE_SIZE.size() - Self::EMBEDDED_SLAB_NODE_SIZE) / obj_layout.size()
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
            (BASIC_PAGE_SIZE.size() * pages_per_slab) % obj_layout.align() == 0,
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
            let offset = slab.buff_ptr.addr().get() % BASIC_PAGE_SIZE.size();
            let addr = VirtAddr(slab.buff_ptr.addr().get() - offset);
            unsafe {
                free_pages(addr, self.pages_per_slab).unwrap();
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

            // assert_eq!(slab_node_ptr.addr(), NonNull::new(4 as *mut ()).unwrap().addr());
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

fn allocate_pages(pages_per_slab: usize) -> Result<VirtAddr, SlabError> {
    #[cfg(test)]
    unsafe {
        use alloc::alloc::alloc_zeroed;
        let layout = Layout::from_size_align(pages_per_slab * BASIC_PAGE_SIZE.size(), BASIC_PAGE_SIZE.size()).unwrap();
        let ptr = alloc_zeroed(layout) as *mut u8;
        if ptr.is_null() {
            return Err(SlabError::PagingError(PagingError::OutOfMemory));
        }
        Ok(VirtAddr(ptr as usize))
    }
    #[cfg(not(test))]
    {
        use kernel::mem::paging::Flags;
        kernel::mem::paging::allocate_pages(pages_per_slab, Flags::new().set_read_write(true), BASIC_PAGE_SIZE)
        .map_err(|e| SlabError::PagingError(e))
    }
}

unsafe fn free_pages(addr: VirtAddr, pages_per_slab: usize) -> Result<(), SlabError> {
    #[cfg(test)]
    unsafe {
        use alloc::alloc::dealloc;
        let layout = Layout::from_size_align(pages_per_slab * BASIC_PAGE_SIZE.size(), BASIC_PAGE_SIZE.size()).unwrap();
        dealloc(addr.into(), layout);
        Ok(())
    }
    #[cfg(not(test))]
    unsafe {
        kernel::mem::paging::free_pages(addr, pages_per_slab, BASIC_PAGE_SIZE)
            .map_err(|e| SlabError::PagingError(e))
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

impl SpinLockable for InternalSlabAllocator {}

#[cfg(test)]
mod tests{

use super::*;
use alloc::{alloc::Layout, vec::Vec};
use core::ptr::NonNull;

/// Helper to create a dummy layout
fn dummy_layout(size: usize, align: usize) -> Layout {
    Layout::from_size_align(size, align).unwrap()
}

/// Helper to allocate and return an InternalSlabAllocator for a given object size
fn make_allocator(obj_size: usize, obj_align: usize) -> InternalSlabAllocator {
    InternalSlabAllocator::new(dummy_layout(obj_size, obj_align))
}

#[test]
fn test_basic_allocation_and_free() {
    let mut allocator = make_allocator(64, 8);
    let mut ptrs = Vec::new();

    // Allocate all objects in one slab
    for _ in 0..allocator.obj_per_slab {
        let ptr = allocator.allocate().unwrap();
        ptrs.push(ptr);
    }

    // Slab should be full now, next allocation should allocate a new slab
    let _ = allocator.allocate().unwrap();

    // Free all in first slab
    for ptr in ptrs.iter() {
        unsafe { allocator.free(ptr.cast::<ObjectNode>()).unwrap(); }
    }

    // Freeing again should error (double free)
    let double_free_ptr = ptrs[0].cast::<ObjectNode>();
    assert_eq!(
        unsafe { allocator.free(double_free_ptr) },
        Err(SlabError::BadPtrRange).or(Err(SlabError::DoubleFree))
    );
}

#[test]
fn test_alignment_check() {
    let mut allocator = make_allocator(64, 16); // 16-aligned
    let ptr = allocator.allocate().unwrap();
    // Make a misaligned pointer
    let misaligned = unsafe { NonNull::new_unchecked(ptr.as_ptr().byte_add(1)) }.cast();
    assert_eq!(
        unsafe { allocator.free(misaligned) },
        Err(SlabError::BadPtrAlignment)
    );
}

#[test]
fn test_bad_ptr_range() {
    let mut allocator = make_allocator(32, 8);
    let bogus_ptr: NonNull<Node<()>> = NonNull::dangling();
    // Should not be in range
    assert_eq!(
        unsafe { allocator.free(bogus_ptr) },
        Err(SlabError::BadPtrRange)
    );
}

#[test]
fn test_double_free() {
    let mut allocator = make_allocator(64, 8);
    let ptr = allocator.allocate().unwrap().cast();
    unsafe {
        allocator.free(ptr).unwrap();
        assert_eq!(allocator.free(ptr), Err(SlabError::BadPtrRange).or(Err(SlabError::DoubleFree)));
    }
}

#[test]
fn test_slab_growth_and_reap() {
    let mut allocator = make_allocator(32, 8);
    let mut ptrs = Vec::new();

    // Allocate more than one slab worth
    for _ in 0..(allocator.obj_per_slab * 3) {
        let ptr = allocator.allocate().unwrap();
        ptrs.push(ptr);
    }
    // Free everything
    for ptr in ptrs.drain(..) {
        unsafe { allocator.free(ptr.cast()).unwrap(); }
    }

    // Reap should free all slabs
    allocator.reap();

    // Should be able to allocate again (it will grow)
    let _p = allocator.allocate().unwrap();
}

#[test]
fn test_full_partial_free_state_transitions() {
    let mut allocator = make_allocator(64, 8);
    let mut ptrs = Vec::new();

    // Allocate a full slab
    for _ in 0..allocator.obj_per_slab {
        let ptr = allocator.allocate().unwrap();
        ptrs.push(ptr);
    }

    // Slab is now full, next allocation grows
    let extra = allocator.allocate().unwrap();

    // Free one from the first slab, should move to partial
    unsafe { allocator.free(ptrs[0].cast()).unwrap(); }

    // Free the rest from the first slab
    for ptr in ptrs.into_iter().skip(1) {
        unsafe { allocator.free(ptr.cast()).unwrap(); }
    }

    // Free extra from the second slab
    unsafe { allocator.free(extra.cast()).unwrap(); }

    // All slabs should be free again
    allocator.reap();
}

#[test]
#[should_panic]
fn test_zero_size_type_layout_panics() {
    let layout = dummy_layout(0, 1);
    // Should panic/assert
        InternalSlabAllocator::new(layout);
    // assert!(result.is_err());
}

#[test]
#[should_panic]
fn test_too_small_obj_size_panics() {
    let layout = dummy_layout(1, 1); // less than Node size
        InternalSlabAllocator::new(layout);
    // assert!(result.is_err());
}

#[test]
#[should_panic]
fn test_unaligned_layout_panics() {
    let layout = dummy_layout(48, 16);
    // Force an invalid alignment by using a non-multiple of page size
    // (Assuming BASIC_PAGE_SIZE.size() is 4096)
    let mut bad_layout = layout;
    if BASIC_PAGE_SIZE.size() > 1 {
        bad_layout = dummy_layout(48, BASIC_PAGE_SIZE.size() + 1);
    }
        InternalSlabAllocator::new(bad_layout);
    // assert!(resulr.is_err());
}

/// Compile-time check: InternalSlabAllocator implements SpinLockable
fn _spinlockable_trait_check(alloc: &InternalSlabAllocator) {
    let _ = alloc as &dyn SpinLockable;
}
}
