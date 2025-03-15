use alloc::boxed::Box;
use core::{alloc::Layout, ffi::c_void, ptr::NonNull, usize};

use utils::collections::stacklist::{Node, StackList};

use crate::arch::x86_64::paging::PagingError;

trait SlabConstructable {
    fn slab_init() {}

    fn slab_free() {}
}

//struct SlabAllocatorWrapper<T> where T: Sized + SlabConstructable {
//
//}

static mut SLAB_SLAB_ALLOCATOR: SlabAllocator =
    SlabAllocator::new(Layout::new::<Node<Slab>>(), ObjectStoringScheme::Embedded);

static mut SLAB_OBJECT_ALLOCATOR: SlabAllocator =
    SlabAllocator::new(Layout::new::<Node<Object>>(), ObjectStoringScheme::Embedded);

/// Represents the way the slab allocator will store the `Object` structs, each of which represent a free object available for allocation in the slab.
#[derive(Debug, Copy, Clone)]
pub enum ObjectStoringScheme {
    /// Embed the `Object` struct inside the object's buffer.
    /// This makes the slab allocator more memory efficient, but in turn makes the slab allocator
    /// not able to pre initialize the objects.
    Embedded,
    /// Store the `Object` struct in a separate buffer.
    /// This makes the slab allocator less memory efficient, but in turn makes the slab allocator
    /// able to pre initialize the objects, thus saving precious CPU cycles, performing trivial
    /// initilizations.
    External,
}

#[derive(Debug, Copy, Clone)]
pub enum SlabError {
    BadPtrAlignment,
    BadPtrRange,
    DoubleFree,
    SlabFullInternalError,
    PageAllocationError(PagingError),
}

pub struct SlabAllocator {
    free_slabs: StackList<Slab>,
    partial_slabs: StackList<Slab>,
    full_slabs: StackList<Slab>,
    pages_per_slab: usize,
    obj_layout: Layout,
    obj_count: usize,
    slab_embed: bool,
    obj_storing_scheme: ObjectStoringScheme,
}

impl SlabAllocator {
    const SLAB_EMBED_THRESHOLD: usize = 4 * 1024_usize.pow(2) / 8;

    const fn calc_pages_per_slab(obj_size: usize) -> usize {
        // The initial (r)emainder (i.e. unused space due to internal fragmentation)
        let r = 0x1000 % obj_size;

        // If allocating a single page is enough to get internal fragmentation under 12.5% (1/8)
        // XXX: Might need to multiple here by 10000 or something
        if (r / 0x1000) <= (1 / 8) {
            return 1;
        }

        // Find the page (c)ount that would let us fit another obj inside
        let c = usize::div_ceil(obj_size, r);
        // Find the (d)ifference by which internal fragmentation decreases every time we allocate
        // an addtional `c` pages
        let d = r - ((r * c) - obj_size);

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

    const fn calc_slab_embed_n_obj_count(pages_per_slab: usize, obj_size: usize) -> (bool, usize) {
        //let slab_embed = (pages_per_slab * 0x1000 % obj_size) >= size_of::<Slab>();
        let slab_embed = true;
        if slab_embed {
            return (
                slab_embed,
                ((pages_per_slab * 0x1000) - size_of::<Slab>()) / obj_size,
            );
        }

        (slab_embed, (pages_per_slab * 0x1000) / obj_size)
    }

    pub const fn new(layout: Layout, obj_storing_scheme: ObjectStoringScheme) -> SlabAllocator {
        let pages_per_slab = Self::calc_pages_per_slab(layout.size());
        let (slab_embed, obj_count) =
            Self::calc_slab_embed_n_obj_count(pages_per_slab, layout.size());

        SlabAllocator {
            full_slabs: StackList::new(),
            partial_slabs: StackList::new(),
            free_slabs: StackList::new(),
            pages_per_slab,
            obj_layout: layout,
            obj_count,
            slab_embed,
            obj_storing_scheme,
        }
    }

    pub fn alloc(&mut self) -> Result<NonNull<c_void>, SlabError> {
        // First, try allocating from the partial slabs
        if let Some(slab_node) = self.partial_slabs.front_mut() {
            if let ret @ Ok(_) = slab_node.alloc() {
                // If the allocation resulted in the slab being empty, move it to the full slabs
                if slab_node.objects.is_empty() {
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

    pub unsafe fn free(&mut self, ptr: NonNull<c_void>) -> Result<(), SlabError> {
        // TODO: Maybe trying the partial slabs first would be better?

        // Make sure `ptr` alignment is correct
        if ptr.as_ptr() as usize % self.obj_layout.align() != 0 {
            return Err(SlabError::BadPtrAlignment);
        }

        // Check if the slab to whom `ptr` belongs is in the full slabs list
        for slab in self.full_slabs.iter_mut() {
            if slab.ptr <= ptr && ptr < unsafe { slab.ptr.add(self.pages_per_slab * 0x1000) } {
                // If it is, free the object and move the slab to the partial slabs
                unsafe { slab.free(ptr) }?;
                unsafe {
                    let slab = Box::into_non_null(self.full_slabs.pop_front_node().unwrap());
                    self.partial_slabs.push_front_node(slab);
                };

                return Ok(());
            }
        }

        // Check the partial slabs
        for slab in self.partial_slabs.iter_mut() {
            if slab.ptr <= ptr && ptr < unsafe { slab.ptr.add(self.pages_per_slab * 0x1000) } {
                // Again, free the ptr. Then check if the slab is now completely free, and if so,
                // move it to the free slabs
                unsafe { slab.free(ptr) }?;
                if slab.objects.len() == self.obj_count {
                    unsafe {
                        let slab = Box::into_non_null(self.partial_slabs.pop_front_node().unwrap());
                        self.free_slabs.push_front_node(slab);
                    };
                }

                return Ok(());
            }
        }

        // Some invalid address was passed
        Err(SlabError::BadPtrRange)
    }

    // TODO: Maybe pass in the amount of memory needed instead of freeing everything?
    pub fn reap(&mut self) {
        while let Some(slab) = self.free_slabs.pop_front() {
            unsafe { super::kernel::free_pages(slab.ptr.cast::<c_void>(), self.pages_per_slab) }
                .unwrap();
        }
    }

    pub fn cache_grow(&mut self) -> Result<(), SlabError> {
        println!("pages_per_slab: {}", self.pages_per_slab);
        let ptr = super::kernel::alloc_pages_any(self.pages_per_slab, 1)
            .map_err(|e| SlabError::PageAllocationError(e))?;

        let mut slab_ptr: NonNull<Node<Slab>>;
        if self.slab_embed {
            let offset_to_slab = self.pages_per_slab * 0x1000 - size_of::<Node<Slab>>();
            // SAFETY: This is OK since ptr is pointing to a valid, contigious memory of
            slab_ptr = unsafe { ptr.cast::<u8>().add(offset_to_slab).cast::<Node<Slab>>() };
        } else {
            unreachable!("only support for slab embed cache grow");
        }

        unsafe {
            *(slab_ptr.as_mut()) = Node::<Slab>::new(Slab::new(
                ptr.cast::<u8>(),
                self.obj_count,
                self.obj_layout.size(),
            ))
        };

        unsafe { self.free_slabs.push_front_node(slab_ptr) };

        Ok(())
    }
}

type Object = c_void;

struct Slab {
    ptr: NonNull<c_void>,
    objects: StackList<Object>,
}

impl Slab {
    #[inline]
    fn new(ptr: NonNull<u8>, obj_count: usize, obj_size: usize) -> Slab {
        let mut objects = StackList::new();
        for i in 0..obj_count {
            unsafe {
                let node = ptr.add(i * obj_size).cast::<Node<Object>>();
                objects.push_front_node(node);
            }
        }

        Self {
            ptr: ptr.cast::<c_void>(),
            objects,
        }
    }

    // TODO: Add support for not embedded objects
    fn alloc(&mut self) -> Result<NonNull<c_void>, SlabError> {
        self.objects
            .pop_front_node()
            .map(|node| Box::into_non_null(node).cast::<c_void>())
            .ok_or(SlabError::SlabFullInternalError)
    }

    unsafe fn free(&mut self, ptr: NonNull<c_void>) -> Result<(), SlabError> {
        if self
            .objects
            .iter()
            .find(|&node| core::ptr::from_ref(node).addr() == ptr.addr().into())
            .is_some()
        {
            return Err(SlabError::DoubleFree); // Double free
        }

        println!("freeing ptr: {:?}", ptr);

        unsafe { self.objects.push_front_node(ptr.cast::<Node<Object>>()) };

        Ok(())
    }
}
