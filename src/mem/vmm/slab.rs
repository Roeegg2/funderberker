use core::{ffi::c_void, ptr::NonNull, slice::from_raw_parts_mut, usize};
use alloc::collections::linked_list::LinkedList;

trait SlabConstructable {
    fn slab_init() {}

    fn slab_free() {}
}

//struct SlabAllocatorWrapper<T> where T: Sized + SlabConstructable {
//
//}

static mut SLAB_SLAB_ALLOCATOR: SlabAllocator = SlabAllocator::new(size_of::<Slab>(), true);

//static mut SLAB_OBJECT_ALLOCATOR: SlabAllocator = SlabAllocator {
//    free_slabs: None,
//    partial_slabs: None,
//    full_slabs: None,
//    obj_size: size_of::<Object>()
//};

pub struct SlabAllocator {
    free_slabs: LinkedList<Slab>,
    partial_slabs: LinkedList<Slab>,
    full_slabs: LinkedList<Slab>,
    pages_per_slab: usize,
    obj_size: usize,
    obj_count: usize,
    slab_embed: bool,
    obj_embed: bool,
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
        usize::div_ceil(r, (512*c) + d) * c
    }

    const fn calc_slab_embed_n_obj_count(pages_per_slab: usize, obj_size: usize) -> (bool, usize) {
        let slab_embed =(pages_per_slab * 0x1000 % obj_size) >= size_of::<Slab>(); 
        if slab_embed {
            return (slab_embed, ((pages_per_slab * 0x1000)-size_of::<Slab>()) / obj_size);
        }
        
        (slab_embed,(pages_per_slab * 0x1000) / obj_size)
    }

    //fn init(&mut self, obj_size: usize) {
    //    self.obj_size = obj_size;
    //    self.full_slabs = None;
    //    self.partial_slabs = None;
    //    self.
    //    //self.free_slabs = ;
    //}

    pub const fn new(obj_size: usize, obj_embed: bool) -> SlabAllocator {
        let pages_per_slab = Self::calc_pages_per_slab(obj_size);
        let (slab_embed, obj_count) = Self::calc_slab_embed_n_obj_count(pages_per_slab, obj_size);
        SlabAllocator {
            full_slabs: LinkedList::new(),
            partial_slabs: LinkedList::new(),
            free_slabs: LinkedList::new(),
            pages_per_slab,
            obj_size,
            obj_count,
            slab_embed,
            obj_embed,
        }
        // set obj_size
        // full_slabs = None
        // partial_slabs = None
        // cache_grow() // to set free_slabs
    }

    pub fn alloc(&mut self) -> Option<NonNull<c_void>> {
        // try finding a slab from partial_slabs, and then try allocating from it. if you encounter
        // an error then return it.
        // otherwise, convert the allocated value to a Box and return it
        // if the new len is now 0, move the slab to full
        // try doing the same for free_slab (except after allocating, move the slab to the partial
        // slab list)
        //
        // otherwise, return error
    }

    pub unsafe fn free(&mut self, ptr: NonNull<c_void>) -> Option<()> {
        let curr_slab_opt = self.full_slabs;
        while let Some(mut slab_ptr) = curr_slab_opt {
            let slab = unsafe {slab_ptr.as_mut()};
            if ptr.addr() >= slab.ptr.addr() && ptr.as_ptr().addr() < slab.ptr.as_ptr().addr() + self.pages_per_slab * 0x1000 {
                unsafe {
                    super::kalloc::kfree_pages(ptr, self.pages_per_slab)
                };
            }
        }
        // for each full_slab:
        //      if it's in the slabs range:
        //          call free on the slab with that address
        //          move full to partial
        //          return
        // for each partial_slab:
        //      if it's in the slabs range:
        //          call free on the slab with that daddress
        //          if new count is 0, move slab to free
        //          return
        // error
    }

    pub fn reap(&mut self) {
        if self.slab_embed {
            while let Some(free_slab) = self.free_slabs {
                unsafe {super::kalloc::kfree_pages(free_slab.cast::<c_void>(), self.pages_per_slab)};
            }
        } else {
            unreachable!("only support for slab embed reap");
        }
        // for each node in `free_slabs` list:
        //      SLAB_SLAB_ALLOCATOR.free(node)
    }

    pub fn cache_grow(&mut self) -> Option<()> {
        let ptr = super::kalloc::kalloc_pages_any(self.pages_per_slab, 1).ok()?;

        let mut slab_ptr: NonNull<Slab>;
        if self.slab_embed {
            // SAFETY: This is OK since ptr is pointing to a valid, contigious memory of
            // `self.pages_per_slab` bytes.
            let offset_to_slab = self.obj_count * self.obj_size;
            slab_ptr = unsafe {
                ptr.cast::<u8>().add(offset_to_slab).cast::<Slab>()
            };
        } else {
            unreachable!("only support for slab embed cache grow");
            //slab_ptr = unsafe {SLAB_SLAB_ALLOCATOR.alloc().cast::<Slab>()};
        }

        unsafe {*(slab_ptr.as_mut()) = Slab::new(ptr.cast::<Object>(), self.obj_count, self.obj_embed, self.free_slabs)?};

        self.free_slabs = Some(slab_ptr);

        Some(())
        // if obj_size < 2MB * 1/8:
        //      set end of page to Slab struct
        // else
        //      call SLAB_SLAB_ALLOCATOR.alloc()
        // call slab::new()
        // add the returned slab to the free slab list
        //
        //
    }
}

struct Slab {
    ptr: NonNull<c_void>,
    objects: Option<NonNull<Object>>,
    obj_count: usize,
    next: Option<NonNull<Slab>>,
}

impl Slab {
    fn new(ptr: NonNull<Object>, obj_count: usize, obj_embed: bool, next: Option<NonNull<Slab>>) -> Option<Slab> {
        let objects = unsafe { from_raw_parts_mut(ptr.as_ptr(), obj_count) };
        if obj_embed {
            for i in 0..obj_count-1 {
                objects[i].next = Some(NonNull::<Object>::new(core::ptr::from_mut(&mut objects[i+1]))?); 
            }
            objects[obj_count-1].next = None;
        } else {
            todo!("Implement not embeding Object inside itself!");
        }

        Some(Self {
            ptr: ptr.cast::<c_void>(),
            objects: Some(NonNull::<Object>::new(objects.as_mut_ptr())?),
            obj_count,
            next,
        })
        // ptr = alloc new page(s)
        // for each obj:
        //      add new entry to objects lists
        //      call slab_init on that object
        //
    }

    // TODO: Add support for not embedded objects
    fn alloc(&mut self) -> Option<NonNull<c_void>> {
        let ret = self.objects?; 
        self.objects = unsafe {ret.as_ref().next};
        self.obj_count -= 1;
        Some(ret.cast::<c_void>())
        // return the head of `objects` list
    }

    unsafe fn free(&mut self, ptr: NonNull<c_void>) {
        let mut new_obj = ptr.cast::<Object>();
        unsafe {new_obj.as_mut()}.next = self.objects;
        self.objects = Some(new_obj);
        self.obj_count -= 1;
        // add node to end of `object` list
    }

    const fn obj_count(&self) -> usize {
        self.obj_count
    }
}

struct Object {
    next: Option<NonNull<Object>>,
}


