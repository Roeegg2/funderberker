//! A simple slab allocator for the kernel heap & custom use

use alloc::boxed::Box;
use core::{alloc::Layout, ffi::c_void, num::NonZero, ptr::NonNull, usize};

use utils::collections::stacklist::{Node, StackList};

use crate::arch::{BASIC_PAGE_SIZE, x86_64::paging::PagingError};

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
    #[allow(dead_code)]
    PageAllocationError(PagingError),
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
    /// Should the object node be embedded in the slab itself?
    obj_embed: bool,
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

    /// Calculates the amount of objects with `obj_layout` that can fit in a slab
    const fn calc_obj_count(pages_per_slab: usize, obj_layout: Layout) -> usize {
        (pages_per_slab * BASIC_PAGE_SIZE - Layout::new::<Node<Slab>>().pad_to_align().size())
            / obj_layout.size()
    }

    /// Creates a new slab allocator with the given object layout
    /// `obj_embed` determines whether the object should be embedded in the slab itself or
    /// allocated externally using the kernel's heap
    ///
    /// NOTE: This is unsafe because the layout must be at least Node<()> size aligned
    // TODO: Find a way to make this safe by returning error?
    pub(super) const unsafe fn new(
        mut obj_layout: Layout,
        obj_embed: bool,
    ) -> InternalSlabAllocator {
        // Get the actual spacing between objects
        obj_layout = obj_layout.pad_to_align();

        let pages_per_slab = Self::calc_pages_per_slab(obj_layout);
        let obj_count = Self::calc_obj_count(pages_per_slab, obj_layout);

        InternalSlabAllocator {
            full_slabs: StackList::new(),
            partial_slabs: StackList::new(),
            free_slabs: StackList::new(),
            pages_per_slab,
            obj_layout,
            obj_count,
            obj_embed,
        }
    }

    /// Allocates an object from the slab allocator and returns a pointer to it. If the slab is
    /// full, it will try to grow the cache and return a pointer to an object in the new slab
    pub(super) fn alloc(&mut self) -> Result<NonNull<ObjectNode>, SlabError> {
        // First, try allocating from the partial slabs
        if let Some(slab_node) = self.partial_slabs.peek_mut() {
            if let ret @ Ok(_) = slab_node.alloc() {
                // If the allocation resulted in the slab being empty, move it to the full slabs
                if slab_node.objects().is_empty() {
                    unsafe {
                        let slab = Box::into_non_null(self.partial_slabs.pop_node().unwrap());
                        self.full_slabs.push_node(slab);
                    };
                }

                return ret;
            }
        }

        // If also the free slabs are all empty, grow the cache
        if self.free_slabs.is_empty() {
            self.cache_grow()?;
        }

        // Try allocating from a free slab
        if let Some(slab_node) = self.free_slabs.peek_mut() {
            let ret = slab_node.alloc()?;
            // If the allocation was successful, move the slab to the partial slabs
            unsafe {
                let slab = Box::into_non_null(self.free_slabs.pop_node().unwrap());
                self.partial_slabs.push_node(slab);
            };

            return Ok(ret);
        }

        unreachable!();
    }

    /// Frees an object from the slab allocator.
    pub(super) unsafe fn free(&mut self, ptr: NonNull<ObjectNode>) -> Result<(), SlabError> {
        // Make sure `ptr` alignment is correct
        if !ptr.is_aligned_to(self.obj_layout.align()) {
            return Err(SlabError::BadPtrAlignment);
        }

        // Check the partial slabs
        for (index, slab) in self.partial_slabs.iter_mut().enumerate() {
            if slab.is_in_range(ptr, self.obj_count, self.obj_layout) {
                unsafe {
                    slab.free(ptr)?;
                    // If the slab is now completely free, move it to the free slabs
                    if slab.objects().len() == self.obj_count {
                        let slab = Box::into_non_null(self.partial_slabs.remove_at(index).unwrap());
                        self.free_slabs.push_node(slab);
                    }
                };

                return Ok(());
            }
        }

        // Check if the slab to whom `ptr` belongs is in the full slabs list
        for (index, slab) in self.full_slabs.iter_mut().enumerate() {
            if slab.is_in_range(ptr, self.obj_count, self.obj_layout) {
                unsafe {
                    slab.free(ptr)?;
                    // Slab is no longer full, so move it to the partial slabs
                    let slab = Box::into_non_null(self.full_slabs.remove_at(index).unwrap());
                    self.partial_slabs.push_node(slab);
                };

                return Ok(());
            }
        }

        // Some invalid address was passed
        Err(SlabError::BadPtrRange)
    }

    /// Reap all the slabs that are free. Should only be invoked by OOM killer, when the system desperately needs memory back.
    // TODO: Maybe pass in the amount of memory needed instead of freeing everything?
    pub(super) fn reap(&mut self) {
        while let Some(slab) = self.free_slabs.pop() {
            unsafe { super::free_pages(slab.buff_ptr().cast::<c_void>(), NonZero::new_unchecked(self.pages_per_slab)) }
                .unwrap();
        }
    }

    /// Grows the cache by allocating a new slab and adding it to the free slabs list.
    pub(super) fn cache_grow(&mut self) -> Result<(), SlabError> {
        let buff_ptr = super::alloc_pages_any(unsafe {NonZero::new_unchecked(self.pages_per_slab)}, unsafe {NonZero::new_unchecked(1)})
            .map_err(|e| SlabError::PageAllocationError(e))?;

        let buff_ptr = buff_ptr.cast::<ObjectNode>();
        unsafe {
            // SAFETY: Size is OK since we allocated the pages_per_slab amount of pages. Alignment
            // is OK since BASIC_PAGE_SIZE * n is always aligned to Node<Slab>, and so BASIC_PAGE_SIZE * n -
            // size_of(Node<Slab>) is also aligned to Node<Slab>
            let slab_ptr = buff_ptr
                .cast::<u8>()
                .add(self.pages_per_slab * BASIC_PAGE_SIZE)
                .sub(Layout::new::<Node<Slab>>().pad_to_align().size())
                .cast::<Node<Slab>>();

            NonNull::write(
                slab_ptr,
                Node::<Slab>::new(Slab::new(
                    buff_ptr.cast::<ObjectNode>(),
                    self.obj_count,
                    self.obj_layout,
                    self.obj_embed,
                )),
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
            unsafe { super::free_pages(slab.buff_ptr().cast::<c_void>(), NonZero::new_unchecked(self.pages_per_slab)) }
                .unwrap();
        }

        // Free the full slabs
        while let Some(slab) = self.full_slabs.pop() {
            unsafe { super::free_pages(slab.buff_ptr().cast::<c_void>(), NonZero::new_unchecked(self.pages_per_slab)) }
                .unwrap();
        }
    }
}

/// A node that holds a pointer to an object.
/// Pointer is uninitilized when `SlabObjEmbed` is used, but since the lowest size of memory that
/// we allocate in the stack is 2^8 bytes anyway, it doesn't matter
pub(super) type ObjectNode = Node<NonNull<()>>;

/// The core structure of a slab
#[derive(Debug)]
struct SlabCore {
    /// Pointer to the slab's buffer
    buff_ptr: NonNull<ObjectNode>,
    /// List of objects that are free in this slab
    free_objs: StackList<NonNull<ObjectNode>>,
}

/// The slab structure that holds the core and the type of slab
#[derive(Debug)]
enum Slab {
    /// The slab where the Node<ObjectNodes> are embedded in the slab itself
    SlabObjEmbed(SlabCore),
    /// The slab where the Node<ObjectNodes> are stored in the kernel's heap (i.e. using
    /// global allocator )
    SlabObjExtern(SlabCore),
}

impl Slab {
    /// Get the list of free objects in the slab. This is a simple getter function.
    const fn objects(&self) -> &StackList<NonNull<ObjectNode>> {
        match self {
            Slab::SlabObjEmbed(slab) => &slab.free_objs,
            Slab::SlabObjExtern(slab) => &slab.free_objs,
        }
    }

    /// Get the pointer to the allocated data. This is a simple getter function.
    const fn buff_ptr(&self) -> NonNull<ObjectNode> {
        match self {
            Slab::SlabObjEmbed(slab) => slab.buff_ptr,
            Slab::SlabObjExtern(slab) => slab.buff_ptr,
        }
    }

    /// Check if the given pointer **to the allocated data** belongs to this slab
    fn is_in_range(&self, ptr: NonNull<ObjectNode>, obj_count: usize, obj_layout: Layout) -> bool {
        self.buff_ptr() <= ptr
            && ptr < unsafe { utils::ptr_add_layout!(ptr, obj_count, obj_layout, ObjectNode) }
    }

    /// Constructs a new slab with the given parameters. This is unsafe because the layout must be
    /// at least Node<ObjectNode> size aligned.
    /// This is a simple wrapper around the `new_obj_embed` and `new_obj_extern` functions
    #[inline]
    unsafe fn new(
        buff_ptr: NonNull<ObjectNode>,
        obj_count: usize,
        obj_layout: Layout,
        obj_embed: bool,
    ) -> Self {
        unsafe {
            if obj_embed {
                Slab::new_obj_embed(buff_ptr, obj_count, obj_layout)
            } else {
                Slab::new_obj_extern(buff_ptr, obj_count, obj_layout)
            }
        }
    }

    /// Constructs a new slab where the Node<ObjectNodes> are stored in the kernel's heap
    /// This is unsafe because the layout must be at least Node<ObjectNode> size aligned.
    #[inline]
    unsafe fn new_obj_extern(
        buff_ptr: NonNull<ObjectNode>,
        obj_count: usize,
        obj_layout: Layout,
    ) -> Self {
        let mut free_objs = StackList::new();

        for i in 0..obj_count {
            // Get the ptr for the object
            let ptr = unsafe { utils::ptr_add_layout!(buff_ptr, i, obj_layout, ObjectNode) };
            //let ptr = unsafe {buff_ptr.cast::<u8>().add(i * obj_layout.size()).cast::<c_void>()};
            free_objs.push(ptr);
        }

        Slab::SlabObjExtern(SlabCore {
            buff_ptr,
            free_objs,
        })
    }

    // Constructs a new slab where the Node<ObjectNodes> are embedded in the slab itself.
    // This is unsafe because the layout must be at least Node<ObjectNode> size aligned.
    #[inline]
    unsafe fn new_obj_embed(
        buff_ptr: NonNull<ObjectNode>,
        obj_count: usize,
        obj_layout: Layout,
    ) -> Self {
        let mut free_objs = StackList::new();
        {
            for i in 0..obj_count {
                unsafe {
                    // Cast buff_ptr to u8, then add the size of obj in bytes, and then cast this
                    // all to a pointer to a Node in the free objects linked list
                    // SAFETY: This is OK because we already checked in the allocator to make sure
                    // T has at least same alignment and size as Node<NonNull<c_void>>
                    let ptr =
                        utils::ptr_add_layout!(buff_ptr, i, obj_layout, Node<NonNull<ObjectNode>>);
                    free_objs.push_node(ptr);
                };
            }
        }

        Slab::SlabObjEmbed(SlabCore {
            buff_ptr,
            free_objs,
        })
    }

    /// Allocates an object from the slab
    fn alloc(&mut self) -> Result<NonNull<ObjectNode>, SlabError> {
        match self {
            // Node<NonNull<ObjectNode>> -> NonNull<Object> since the address of the node is the
            // (to be) address of the object
            Slab::SlabObjEmbed(slab) => slab
                .free_objs
                .pop_node()
                .map(|node| Box::into_non_null(node).cast::<ObjectNode>())
                .ok_or(SlabError::SlabFullInternalError),
            // just return the NonNull<ObjectNode> since thats the address of the object
            Slab::SlabObjExtern(slab) => slab
                .free_objs
                .pop()
                .map(|node| node)
                .ok_or(SlabError::SlabFullInternalError),
        }
    }

    /// Frees an object from the slab
    fn free(&mut self, obj_ptr: NonNull<ObjectNode>) -> Result<(), SlabError> {
        match self {
            Slab::SlabObjEmbed(slab) => {
                // Interpret the pointer to the node as the pointer to the slab (since we are
                // using the embedded scheme)
                if slab
                    .free_objs
                    .iter_node()
                    .find(|&node| NonNull::from_ref(node).cast::<ObjectNode>() == obj_ptr)
                    .is_some()
                {
                    return Err(SlabError::DoubleFree);
                }

                // Turns obj_ptr to a new node to add to the list of free objects
                unsafe {
                    slab.free_objs
                        .push_node(obj_ptr.cast::<Node<NonNull<ObjectNode>>>())
                };
            }
            Slab::SlabObjExtern(slab) => {
                if slab
                    .free_objs
                    .iter()
                    .find(|&node| *node == obj_ptr)
                    .is_some()
                {
                    return Err(SlabError::DoubleFree);
                }

                // Allocate and add a new node with ptr to the freed object
                slab.free_objs.push(obj_ptr);
            }
        };

        Ok(())
    }
}

#[cfg(test)]
pub mod tests {
    #[test_case]
    fn test0() {
        let mut allocator = unsafe {
            super::InternalSlabAllocator::new(core::alloc::Layout::new::<[u8; 10]>(), true)
        };
        let ptr = allocator.alloc().unwrap();
        assert_eq!(allocator.free_slabs.len(), 0);
        assert_eq!(allocator.partial_slabs.len(), 1);
        assert_eq!(allocator.full_slabs.len(), 0);
        unsafe { allocator.free(ptr).unwrap() };

        let ptr = allocator.alloc().unwrap();
        let ptr2 = allocator.alloc().unwrap();
        let ptr3 = allocator.alloc().unwrap();
        assert_eq!(allocator.free_slabs.len(), 0);
        assert_eq!(allocator.partial_slabs.len(), 1);
        assert_eq!(allocator.full_slabs.len(), 0);
        unsafe { allocator.free(ptr).unwrap() };
        assert_eq!(allocator.free_slabs.len(), 0);
        assert_eq!(allocator.partial_slabs.len(), 1);
        assert_eq!(allocator.full_slabs.len(), 0);
        unsafe { allocator.free(ptr3).unwrap() };
        unsafe { allocator.free(ptr2).unwrap() };
        assert_eq!(allocator.free_slabs.len(), 1);
        assert_eq!(allocator.partial_slabs.len(), 0);
        assert_eq!(allocator.full_slabs.len(), 0);
    }

    #[test_case]
    fn test1() {
        let mut allocator = unsafe {
            super::InternalSlabAllocator::new(core::alloc::Layout::new::<[u64; 4]>(), true)
        };
        for _ in 0..allocator.obj_count {
            allocator.alloc().unwrap();
        }
        assert_eq!(allocator.free_slabs.len(), 0);
        assert_eq!(allocator.partial_slabs.len(), 0);
        assert_eq!(allocator.full_slabs.len(), 1);

        let ptr = allocator.alloc().unwrap();
        assert_eq!(allocator.free_slabs.len(), 0);
        assert_eq!(allocator.partial_slabs.len(), 1);
        assert_eq!(allocator.full_slabs.len(), 1);

        unsafe { allocator.free(ptr).unwrap() };
        assert_eq!(allocator.free_slabs.len(), 1);
        assert_eq!(allocator.partial_slabs.len(), 0);
        assert_eq!(allocator.full_slabs.len(), 1);

        let ptr0 = allocator.alloc().unwrap();
        let ptr1 = allocator.alloc().unwrap();
        let ptr2 = allocator.alloc().unwrap();
        let ptr3 = allocator.alloc().unwrap();
        assert_eq!(allocator.free_slabs.len(), 0);
        assert_eq!(allocator.partial_slabs.len(), 1);
        assert_eq!(allocator.full_slabs.len(), 1);

        for _ in 0..allocator.obj_count {
            allocator.alloc().unwrap();
        }
        for _ in 0..allocator.obj_count {
            allocator.alloc().unwrap();
        }

        assert_eq!(allocator.free_slabs.len(), 0);
        assert_eq!(allocator.partial_slabs.len(), 1);
        assert_eq!(allocator.full_slabs.len(), 3);

        unsafe { allocator.free(ptr0).unwrap() };
        unsafe { allocator.free(ptr1).unwrap() };
        unsafe { allocator.free(ptr2).unwrap() };
        unsafe { allocator.free(ptr3).unwrap() };

        assert_eq!(allocator.free_slabs.len(), 0);
        assert_eq!(allocator.partial_slabs.len(), 2);
        assert_eq!(allocator.full_slabs.len(), 2);
    }

    #[test_case]
    fn test2() {
        let mut allocator = unsafe {
            super::InternalSlabAllocator::new(core::alloc::Layout::new::<[u16; 20]>(), true)
        };
        allocator.cache_grow().unwrap();

        assert_eq!(allocator.free_slabs.len(), 1);
        assert_eq!(allocator.partial_slabs.len(), 0);
        assert_eq!(allocator.full_slabs.len(), 0);

        allocator.cache_grow().unwrap();

        assert_eq!(allocator.free_slabs.len(), 2);
        assert_eq!(allocator.partial_slabs.len(), 0);
        assert_eq!(allocator.full_slabs.len(), 0);

        allocator.cache_grow().unwrap();

        assert_eq!(allocator.free_slabs.len(), 3);
        assert_eq!(allocator.partial_slabs.len(), 0);
        assert_eq!(allocator.full_slabs.len(), 0);

        allocator.reap();

        assert_eq!(allocator.free_slabs.len(), 0);
        assert_eq!(allocator.partial_slabs.len(), 0);
        assert_eq!(allocator.full_slabs.len(), 0);

        allocator.alloc().unwrap();
        assert_eq!(allocator.free_slabs.len(), 0);
        assert_eq!(allocator.partial_slabs.len(), 1);
        assert_eq!(allocator.full_slabs.len(), 0);
    }
}
