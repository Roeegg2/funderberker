use alloc::{alloc::Allocator, boxed::Box};
use core::{alloc::Layout, cell::UnsafeCell, ffi::c_void, ptr::NonNull, usize};

use utils::collections::stacklist::{Node, StackList};

use crate::arch::x86_64::paging::PagingError;

/// Errors that the slab allocator might encounter
#[derive(Debug, Copy, Clone)]
pub enum SlabError {
    BadPtrAlignment,
    BadPtrRange,
    DoubleFree,
    SlabFullInternalError,
    PageAllocationError(PagingError),
}

pub unsafe trait SlabConstructable: Default {
    unsafe fn slab_init(&mut self) {
        *self = Default::default();
    }
}

pub struct SlabAllocator<T>
where
    T: SlabConstructable,
{
    slab_allocator: UnsafeCell<InternalSlabAllocator>,
    _phantom: core::marker::PhantomData<T>,
}

impl<T> SlabAllocator<T>
where
    T: SlabConstructable,
{
    pub const fn new() -> Self {
        let layout = {
            let size = utils::const_max!(Layout::new::<T>().size(), Layout::new::<Object>().size());
            let align = utils::const_max!(Layout::new::<T>().align(), Layout::new::<Object>().align());
            unsafe {Layout::from_size_align_unchecked(size, align)}
        };

        SlabAllocator {
            slab_allocator: unsafe {
                UnsafeCell::new(InternalSlabAllocator::new(
                    layout,
                    false,
                ))
            },
            _phantom: core::marker::PhantomData,
        }
    }

    pub fn cache_grow(&self) -> Result<(), SlabError> {
        unsafe { self.slab_allocator.get().as_mut().unwrap().cache_grow() }
    }

    pub fn reap(&self) {
        unsafe { self.slab_allocator.get().as_mut().unwrap().reap() }
    }
}

unsafe impl<T> Allocator for SlabAllocator<T>
where
    T: SlabConstructable,
{
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, alloc::alloc::AllocError> {
        assert_eq!(
            layout.size(),
            core::mem::size_of::<T>(),
            "Layout size must be equal to size of T"
        );
        assert_eq!(
            layout.align(),
            core::mem::align_of::<T>(),
            "Layout alignment must be equal to alignment of T"
        );

        let ptr = unsafe { self.slab_allocator.get().as_mut().unwrap().alloc() }
            .map_err(|_| alloc::alloc::AllocError)?;

        unsafe {
            ptr.cast::<T>().as_ptr().as_mut().unwrap().slab_init();
        }

        Ok(NonNull::new(ptr.as_ptr() as *mut [u8]).unwrap())
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        assert_eq!(
            layout.size(),
            core::mem::size_of::<T>(),
            "Layout size must be equal to size of T"
        );
        assert_eq!(
            layout.align(),
            core::mem::align_of::<T>(),
            "Layout alignment must be equal to alignment of T"
        );

        unsafe {
            self.slab_allocator
                .get()
                .as_mut()
                .unwrap()
                .free(ptr.cast::<Object>())
                .expect("Invalid pointer passed to deallocate")
        };
    }
}

//impl<T> Clone for SlabAllocator<T> where T: SlabConstructable {
//    fn clone(&self) -> Self {
//        SlabAllocator {
//            slab_allocator: self.slab_allocator,
//            _phantom: core::marker::PhantomData,
//        }
//    }
//}

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
    obj_embed: bool,
}

impl InternalSlabAllocator {
    const SLAB_EMBED_THRESHOLD: usize = 4 * 1024_usize.pow(2) / 8;

    const fn calc_pages_per_slab(obj_layout: Layout) -> usize {
        // The initial (r)emainder (i.e. unused space due to internal fragmentation)
        let r = 0x1000 % obj_layout.size();

        // If allocating a single page is enough to get internal fragmentation under 12.5% (1/8)
        // XXX: Might need to multiple here by 10000 or something
        if (r * 100000 / 0x1000) <= (1 * 100000 / 8) {
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
        // 0x1000 * c * x     8
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

    const fn calc_obj_count(pages_per_slab: usize, obj_layout: Layout, slab_embed: bool) -> usize {
        if slab_embed {
            (pages_per_slab * 0x1000 - Layout::new::<Node<Slab>>().pad_to_align().size())
                / obj_layout.size()
        } else {
            (pages_per_slab * 0x1000) / obj_layout.size()
        }
    }

    /// NOTE: This is unsafe because the layout must be at least Node<()> size aligned
    /// TODO: Find a way to make this safe by returning error
    pub(super) const unsafe fn new(mut layout: Layout, obj_embed: bool) -> InternalSlabAllocator {
        layout = layout.pad_to_align();
        let pages_per_slab = Self::calc_pages_per_slab(layout);
        let obj_count = Self::calc_obj_count(pages_per_slab, layout, obj_embed);

        InternalSlabAllocator {
            full_slabs: StackList::new(),
            partial_slabs: StackList::new(),
            free_slabs: StackList::new(),
            pages_per_slab,
            obj_layout: layout,
            obj_count,
            obj_embed,
        }
    }

    pub(super) fn alloc(&mut self) -> Result<NonNull<Object>, SlabError> {
        // First, try allocating from the partial slabs
        if let Some(slab_node) = self.partial_slabs.front_mut() {
            if let ret @ Ok(_) = slab_node.alloc() {
                // If the allocation resulted in the slab being empty, move it to the full slabs
                if slab_node.objects().is_empty() {
                    // move top slab from partial to full
                    unsafe {
                        let slab = Box::into_non_null(self.partial_slabs.pop_front_node().unwrap());
                        // doesn't matter if we push to front or back
                        self.full_slabs.push_front_node(slab);
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
        if let Some(slab_node) = self.free_slabs.front_mut() {
            if let ret @ Ok(_) = slab_node.alloc() {
                // If the allocation was successful, move the slab to the partial slabs
                unsafe {
                    let slab = Box::into_non_null(self.free_slabs.pop_front_node().unwrap());
                    self.partial_slabs.push_front_node(slab);
                };

                return ret;
            }
        }

        unreachable!("No slab was able to allocate an object!");
    }

    pub(super) unsafe fn free(&mut self, ptr: NonNull<Object>) -> Result<(), SlabError> {
        // TODO: Maybe trying the partial slabs first would be better?

        // Make sure `ptr` alignment is correct
        if ptr.as_ptr().addr() % self.obj_layout.align() != 0 {
            return Err(SlabError::BadPtrAlignment);
        }

        // Check the partial slabs
        for (index, slab) in self.partial_slabs.iter_mut().enumerate() {
            if slab.is_in_range(ptr, self.obj_count, self.obj_layout) {
                // Again, free the ptr. Then check if the slab is now completely free, and if so,
                // move it to the free slabs
                //if !ptr.is_aligned_to(self.obj_layout.align()) {
                //    break;
                //}

                unsafe {
                    slab.free(ptr)?;
                    if slab.objects().len() == self.obj_count {
                        let slab = Box::into_non_null(self.partial_slabs.remove_at(index).unwrap());
                        self.free_slabs.push_front_node(slab);
                    }
                };

                return Ok(());
            }
        }

        // Check if the slab to whom `ptr` belongs is in the full slabs list
        for (index, slab) in self.full_slabs.iter_mut().enumerate() {
            if slab.is_in_range(ptr, self.obj_count, self.obj_layout) {
                //if !ptr.is_aligned_to(self.obj_layout.align()) {
                //    break;
                //}
                unsafe {
                    // If it is, free the object and move the slab to the partial slabs
                    slab.free(ptr)?;
                    let slab = Box::into_non_null(self.full_slabs.remove_at(index).unwrap());
                    self.partial_slabs.push_front_node(slab);
                };

                return Ok(());
            }
        }

        // Some invalid address was passed
        Err(SlabError::BadPtrRange)
    }

    // TODO: Maybe pass in the amount of memory needed instead of freeing everything?
    pub(super) fn reap(&mut self) {
        while let Some(slab) = self.free_slabs.pop_front() {
            unsafe {
                super::kernel::free_pages(slab.buff_ptr().cast::<c_void>(), self.pages_per_slab)
            }
            .unwrap();
        }
    }

    pub(super) fn cache_grow(&mut self) -> Result<(), SlabError> {
        let buff_ptr = super::kernel::alloc_pages_any(self.pages_per_slab, 1)
            .map_err(|e| SlabError::PageAllocationError(e))?;

        // TODO: Take care of slab embed/extern

        let buff_ptr = buff_ptr.cast::<Object>();
        unsafe {
            //let buff_ptr = utils::ptr_add_layout!(ptr.add(1), 1, ptr.;
            let slab_ptr = buff_ptr
                .cast::<u8>()
                .add(self.pages_per_slab * 0x1000)
                .sub(Layout::new::<Node<Slab>>().pad_to_align().size())
                .cast::<Node<Slab>>();

            NonNull::write(
                slab_ptr,
                Node::<Slab>::new(Slab::new(
                    buff_ptr.cast::<Object>(),
                    self.obj_count,
                    self.obj_layout,
                    self.obj_embed,
                )),
            );

            self.free_slabs.push_front_node(slab_ptr);
        }

        Ok(())
    }
}

pub type Object = [u8; 16];

#[derive(Debug)]
struct SlabCore {
    buff_ptr: NonNull<Object>,
    /// List of objects that are free in this slab
    free_objs: StackList<NonNull<Object>>,
}

#[derive(Debug)]
enum Slab {
    SlabObjEmbed(SlabCore),
    SlabObjExtern(SlabCore),
}

impl Slab {
    const fn objects(&self) -> &StackList<NonNull<Object>> {
        match self {
            Slab::SlabObjEmbed(slab) => &slab.free_objs,
            Slab::SlabObjExtern(slab) => &slab.free_objs,
        }
    }

    const fn buff_ptr(&self) -> NonNull<Object> {
        match self {
            Slab::SlabObjEmbed(slab) => slab.buff_ptr,
            Slab::SlabObjExtern(slab) => slab.buff_ptr,
        }
    }

    /// Check if the given pointer **to the allocated data** belongs to this slab
    fn is_in_range(&self, ptr: NonNull<Object>, obj_count: usize, obj_layout: Layout) -> bool {
        self.buff_ptr() <= ptr
            && ptr < unsafe { utils::ptr_add_layout!(ptr, obj_count, obj_layout, Object) }
    }

    #[inline]
    unsafe fn new(
        buff_ptr: NonNull<Object>,
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

    /// Constructs a new slab where the Node<Objects> are stored in the kernel's heap
    #[inline]
    unsafe fn new_obj_extern(
        buff_ptr: NonNull<Object>,
        obj_count: usize,
        obj_layout: Layout,
    ) -> Self {
        let mut free_objs = StackList::new();

        for i in 0..obj_count {
            // Get the ptr for the object
            let ptr = unsafe { utils::ptr_add_layout!(buff_ptr, i, obj_layout, Object) };
            //let ptr = unsafe {buff_ptr.cast::<u8>().add(i * obj_layout.size()).cast::<c_void>()};
            free_objs.push_back(ptr);
        }

        Slab::SlabObjExtern(SlabCore {
            buff_ptr,
            free_objs,
        })
    }

    // Constructs a new slab where the Node<Objects> are embedded in the slab itself
    #[inline]
    unsafe fn new_obj_embed(
        buff_ptr: NonNull<Object>,
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
                        utils::ptr_add_layout!(buff_ptr, i, obj_layout, Node<NonNull<Object>>);
                    free_objs.push_back_node(ptr);
                };
            }
        }

        Slab::SlabObjEmbed(SlabCore {
            buff_ptr,
            free_objs,
        })
    }

    fn alloc(&mut self) -> Result<NonNull<Object>, SlabError> {
        match self {
            // Node<NonNull<Object>> -> NonNull<Object> since the address of the node is the
            // (to be) address of the object
            Slab::SlabObjEmbed(slab) => slab
                .free_objs
                .pop_front_node()
                .map(|node| Box::into_non_null(node).cast::<Object>())
                .ok_or(SlabError::SlabFullInternalError),
            // just return the NonNull<Object> since thats the address of the object
            Slab::SlabObjExtern(slab) => slab
                .free_objs
                .pop_front()
                .map(|node| node)
                .ok_or(SlabError::SlabFullInternalError),
        }
    }

    fn free(&mut self, obj_ptr: NonNull<Object>) -> Result<(), SlabError> {
        match self {
            Slab::SlabObjEmbed(slab) => {
                if slab
                    .free_objs
                    .iter_node()
                    .find(|&node| NonNull::from_ref(node).cast::<Object>() == obj_ptr)
                    .is_some()
                {
                    return Err(SlabError::DoubleFree);
                }

                // Turns obj_ptr to a new node to add to the list of free objects
                unsafe {
                    slab.free_objs
                        .push_front_node(obj_ptr.cast::<Node<NonNull<Object>>>())
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
                slab.free_objs.push_back(obj_ptr);
            }
        };

        Ok(())
    }
}

#[cfg(feature = "test")]
pub mod tests {
    use alloc::boxed::Box;

    pub fn test() {
        test0();
        test1();
        test2();
        test3();
        println!("Slab tests passed!");
    }

    fn test0() {
        let mut allocator = unsafe {
            super::InternalSlabAllocator::new(core::alloc::Layout::new::<[u8; 10]>(), true)
        };
        println!("obj_count {:?}", allocator.obj_count);
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

    fn test1() {
        println!("Testing max alloc and reap");
        let mut allocator = unsafe {
            super::InternalSlabAllocator::new(core::alloc::Layout::new::<[u64; 4]>(), true)
        };
        println!("TYOOO {:?}", allocator.pages_per_slab);
        println!(
            "SIZE OF NODE SLAB {:?}",
            core::mem::size_of::<super::Node<super::Slab>>()
        );
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
        println!("allocated ptr0 {:?}", ptr0);
        let ptr1 = allocator.alloc().unwrap();
        println!("allocated ptr1 {:?}", ptr1);
        let ptr2 = allocator.alloc().unwrap();
        println!("allocated ptr2 {:?}", ptr2);
        let ptr3 = allocator.alloc().unwrap();
        println!("allocated ptr3 {:?}", ptr3);
        assert_eq!(allocator.free_slabs.len(), 0);
        assert_eq!(allocator.partial_slabs.len(), 1);
        assert_eq!(allocator.full_slabs.len(), 1);

        for i in 0..allocator.obj_count {
            let a = allocator.alloc().unwrap();
            println!("allocated {i} {:?}", a);
        }
        for i in 0..allocator.obj_count {
            let a = allocator.alloc().unwrap();
            println!("allocated {i} {:?}", a);
        }

        assert_eq!(allocator.free_slabs.len(), 0);
        assert_eq!(allocator.partial_slabs.len(), 1);
        assert_eq!(allocator.full_slabs.len(), 3);

        println!("GOT HERE!");
        unsafe { allocator.free(ptr0).unwrap() };
        unsafe { allocator.free(ptr1).unwrap() };
        unsafe { allocator.free(ptr2).unwrap() };
        unsafe { allocator.free(ptr3).unwrap() };

        assert_eq!(allocator.free_slabs.len(), 0);
        assert_eq!(allocator.partial_slabs.len(), 2);
        assert_eq!(allocator.full_slabs.len(), 2);
    }

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

        let ptr = allocator.alloc().unwrap();
        assert_eq!(allocator.free_slabs.len(), 0);
        assert_eq!(allocator.partial_slabs.len(), 1);
        assert_eq!(allocator.full_slabs.len(), 0);
    }
}
