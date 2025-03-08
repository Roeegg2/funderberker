//! A simple VMM slab allocator

use core::{ffi::c_void, ptr::NonNull};

pub enum SlabAllocatorError {}

pub trait SlabConstructable {
    fn init() {}

    fn disable() {}
}

static mut SLAB_CACHE_ALLOCATOR: SlabAllocator = SlabAllocator(None);

//static mut SLAB_CACHE_ALLOCATOR: SlabAllocator = SlabAllocator(Some(SlabCache {
//    ptr: NonNull::dangling(),
//    bitmap: &mut [],
//    obj_size: size_of::<SlabCache>(),
//    len: 0,
//    next: None,
//}));

//impl SlabAllocator {
//    fn new<T>() where T: SlabConstructable -> SlabAllocator {
//
//    }

//fn alloc(&mut self) {
//    // go over each slab cache. if there are entries left, mark one as used and return it. If
//    // there isn't allocate a new slab, set it to the tail and return the first entry
//    let mut curr = &mut self.0;
//    loop {
//        if let Some(mut ret) = unsafe {curr.as_mut()}.alloc_slab() {
//            unsafe {return ret.as_mut()};
//            // else: allocate new cache, add to cache list and
//
//        } else {
//
//            // allocate new cache
//            // set add it to the list
//            // allocate an entry from it
//            // return the entry
//        }
//    }
//}
//
//pub fn free() {
//
//}
//}

pub struct SlabAllocator(Option<NonNull<SlabCache>>);

impl SlabAllocator {
    pub fn new<T>() -> Option<SlabAllocator>
    where
        T: SlabConstructable,
    {
        // 1. you allocate a cache for type T
        // 2. for each element in the cache, you call T.init() (that can be called, since T
        // implements the `SlabConstructable` trait)
        // 3. call SLAB_CACHE_ALLOCATOR.alloc() to get the `SlabCache` for this cache
        // 4. construct a new SlabAllocator with Some(*the_above `SlabCache`) and return it


        // TODO: Add mechanism to choose the right amount of pages to allocate for a cache
        // TODO: Return error here instead of None?
        let count = size_of::<T>() / 
        let mut cache_ptr = super::kalloc::kalloc_pages_any(size_of::<T>() / 0x1000, 1).ok()?.cast::<T>();

        let cache = unsafe { cache_ptr.as_mut() };
    

        Some(
            SlabAllocator(
                Some(
                    cache_ptr,
                )
            )
        )
    }

    pub fn alloc(&mut self) -> Option<NonNull<c_void>> {
        let mut curr_cache = &mut self.0;
        loop {
            if curr_cache.is_none() {
                *curr_cache = Some(unsafe { SLAB_CACHE_ALLOCATOR.alloc()? }.cast::<SlabCache>());
            }
            let cache = unsafe { curr_cache.unwrap_unchecked().as_mut() };
            let slab = cache.alloc_slab();

            if slab.is_none() {
                curr_cache = &mut cache.next;
                continue;
            }

            return slab;
        }
    }
}

struct SlabCache {
    ptr: NonNull<c_void>,
    bitmap: &'static mut [u8],
    obj_size: usize,
    len: usize,
    next: Option<NonNull<SlabCache>>,
}

impl SlabCache {
    fn alloc_slab(&mut self) -> Option<NonNull<c_void>> {
        self.bitmap.iter_mut().find_map(|&mut mut i| {
            if i == 0 {
                i = 1;
                unsafe { return Some(self.ptr.add(i as usize)) };
            }
            None
        })
    }
}
