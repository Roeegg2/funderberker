//! A simple VMM slab allocator

use core::{ffi::c_void, slice::from_raw_parts_mut, ptr::NonNull};
use crate::lib::bitmap::Bitmap;

static mut SLAB_CACHE_ALLOCATOR: SlabAllocator = SlabAllocator(None);

pub enum SlabAllocatorError {}

pub trait SlabConstructable {
    fn slab_init(&mut self) {}
}

pub struct SlabAllocator(Option<NonNull<c_void>>);

impl SlabAllocator {
    pub fn new<T>() -> SlabAllocator
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

        Self(SlabCache::new(size_of::<T>()))
    }

    fn alloc_core(&mut self) -> Option<NonNull<c_void>> {
        let curr_slab_cache_opt = self.0;
        loop {
            if let Some(curr_slab_cache) = curr_slab_cache_opt {
                if curr_slab_cache
            } else {
                *curr_slab_cache_opt = SlabCache::new();
            }
        }
        //
        //
        //
        
        //loop {
        //    if slab_cache.is_none() {
        //        *slab_cache = SlabCache::new(size_of::<SlabCache>());
        //    }
        //    let cache = unsafe { (*slab_cache)?.as_mut() };
        //    let slab = cache.alloc_slab();
        //
        //    if slab.is_none() {
        //        slab_cache = &mut cache.next;
        //        continue;
        //    }
        //
        //    return slab;
        //}
    }
}

struct SlabCache {
    cache_ptr: NonNull<c_void>,
    len: usize,
    // TODO: Pretty sure I shouldn't use static lifetime here?
    bitmap: Bitmap<'static>,
    obj_size: usize,
    next: Option<NonNull<SlabCache>>,
}

impl SlabCache {
    // NOTE: Maybe using generics for this function instead of passing in `object_size` would be better?
    // TODO: Add mechanism to choose the right amount of pages to allocate for a cache (could
    // do that by calculating the remainder, then checking to see if the remainder divided by
    // the size of the object is )
    fn calc_bitmap_size(page_count: usize, object_size: usize) -> (usize, usize, usize) {
        // Get the amount of leftover bytes (e.g. size_of::<T> = 5, so well have at least 1 byte
        // spare anyway, which we could use for the bitmap)
        let leftover_size = (page_count * 0x1000) % object_size;

        // If we didn't allocate any bitmaps, whats the largest amount of objects we could've
        // allocated?
        let amount_of_obj = (page_count * 0x1000) / object_size;
        // Find the largest amount of objects we could allocate, but that would still leave us with
        // enough bytes for the bitmap
        // TODO: Find a more efficient way to compute this?
        let amount_to_remove = (0..amount_of_obj).into_iter().find(|amount| {
            8 * ((amount * object_size) + leftover_size) >= amount_of_obj
        }).unwrap();

        // Return that amount (the amount of objects we give up for the bitmap + any leftover byte
        // count)
        let bitmap_size = (amount_to_remove * object_size) + leftover_size;
        let actual_obj_amount = amount_of_obj - amount_to_remove;
        (bitmap_size, actual_obj_amount, (8 * bitmap_size) % actual_obj_amount)
    }

    fn init_cache_as<T>(&mut self) where T: SlabConstructable {
        let cache = unsafe {from_raw_parts_mut(self.cache_ptr.as_ptr().cast::<T>(), self.len)};
        
        // Init each object in the cache
        cache.iter_mut().map(|entry| {
            entry.slab_init();
        });
    }

    fn init(&mut self, obj_size: usize) -> Option<()> {
        self.next = None;
        self.obj_size = obj_size;
        // Allocate page(s) for bitmap + cache
        self.cache_ptr = super::kalloc::kalloc_pages_any(1, 1).ok()?;
        // Calculate the bitmap size for this `obj_size`
        let (bitmap_size, obj_count, used_bits_count) = Self::calc_bitmap_size(1, obj_size);

        self.len = obj_count;
        {
            let bitmap = unsafe {from_raw_parts_mut(self.cache_ptr.add(obj_count).as_ptr().cast::<u8>(), bitmap_size)};
            unsafe { crate::lib::mem::memset(bitmap.as_mut_ptr(), Bitmap::FREE, bitmap_size) };
            self.bitmap = Bitmap::new(bitmap, used_bits_count);
        }

        for i in bitmap_size-1..(bitmap_size-1 + used_bits_count) {
            self.bitmap.set(i);
        }

        Some(())
    }

    /// Allocates a new slab cache from `SLAB_CACHE_ALLOCATOR`, initilizes it, and returns it
    fn new(obj_size: usize) -> Option<NonNull<Self>> {
        let mut slab_cache_ptr = unsafe { SLAB_CACHE_ALLOCATOR.alloc()?.cast::<SlabCache>()};

        let slab_cache = unsafe { slab_cache_ptr.as_mut() };
        slab_cache.init(obj_size)?;

        Some(slab_cache_ptr)
    }

    /// Allocates a some free slab from the cache
    fn alloc_slab(&mut self) -> Option<NonNull<c_void>> {
        for i in 0..self.bitmap.used_bits_count() {
            if self.bitmap.get(i) == Bitmap::FREE {
                self.bitmap.set(1);
                unsafe { return Some(self.cache_ptr.add(i as usize)) };
            }
        }

        None
    }
}
