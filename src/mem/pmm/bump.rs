//! A simple ring-buffer style bump allocator physical memory manager

use core::slice::from_raw_parts_mut;

use limine::memory_map;

use super::super::{PageId, addr_to_page_id, page_id_to_addr};
use super::{PhysAddr, PmmAllocator, PmmError};

// TODO: Definitely use an UnsafeCell with some locking mechanism here
/// Singleton instance of the bump allocator
pub(super) static mut BUMP_ALLOCATOR: BumpAllocator = BumpAllocator {
    bitmap: &mut [],
    ptr: 0,
    used_bitmap_size: 0,
};

/// Singleton ring-buffer bump allocator implemented using bitmap
pub(super) struct BumpAllocator<'a> {
    /// The bitmap representing the status of each page (`1` meaning used, `0` meaning free)
    bitmap: &'a mut [u8],
    /// The ring buffer ptr for finding new pages to allocate
    ptr: PageId,
    /// The size of the used bitmap. Amount of pages doesn't have to be 8 aligned, and in such
    /// case, we allocate an additional entry that has some entries which aren't used.
    /// NOTE: WHEN EVER REFERING TO ADDERSABLE/VALID ENTRIES, **ALWAYS** USE THIS VALUE. NOT THE
    /// BIMTAP SLICE LENGTH!!!
    used_bitmap_size: usize,
}

impl<'a> PmmAllocator for BumpAllocator<'a> {
    fn alloc_any(&mut self, alignment: usize, page_count: usize) -> Result<PhysAddr, PmmError> {
        let mut ptr = inc_ring_buff_ptr(self.ptr, 1, self.used_bitmap_size);

        if alignment >= self.used_bitmap_size || alignment == 0 {
            return Err(PmmError::InvalidAlignment);
        }

        'main: loop {
            // Couldn't find suitable block
            if ptr == self.ptr {
                return Err(PmmError::NoAvailableBlock);
            }

            // This is an optimization. Instead of incrememting ptr until it gets to the start
            // of the ring buffer, we set it to 0 now
            if ptr + page_count >= self.used_bitmap_size {
                // Taking care of the case in which if we chose to procede with regular
                // iteration, we would've encountered self.ptr again
                if ptr + page_count >= self.ptr {
                    return Err(PmmError::NoAvailableBlock);
                }
                ptr = 0;
                continue;
            }

            // If alignment doesn't match, go to next alignment available block
            {
                let diff = ptr % alignment;
                if diff != 0 {
                    ptr = inc_ring_buff_ptr(ptr, diff, self.used_bitmap_size);
                    continue;
                }
            }

            // Check if all entries are available. If at least one isn't, go next
            for i in 0..page_count {
                if self.bitmap_get(ptr + i) != Self::FREE {
                    ptr = inc_ring_buff_ptr(ptr, 1, self.used_bitmap_size);
                    continue 'main;
                }
            }

            // We can allocate! So break out of loop
            break;
        }

        // They are, so mark them as taken
        for i in 0..page_count {
            self.bitmap_set(ptr + i);
        }

        self.ptr = ptr;

        return Ok(PhysAddr(page_id_to_addr(ptr)));
    }

    fn alloc_at(&mut self, addr: PhysAddr, page_count: usize) -> Result<(), super::PmmError> {
        let id = addr_to_page_id(addr.0).ok_or(PmmError::InvalidAddress)?;

        // Make sure we can allocate the block. If we can't we propergate the error
        (id..(id + page_count)).try_for_each(|i| {
            if i >= self.used_bitmap_size {
                return Err(PmmError::OutOfBounds);
            } else if self.bitmap_get(id) != Self::FREE {
                return Err(PmmError::NoAvailableBlock);
            }

            Ok(())
        })?;

        // If we can, then we do!
        (id..page_count).for_each(|page_id| {
            self.bitmap_set(page_id);
        });

        Ok(())
    }

    /// NOTE: `addr` must be a page (4096 bytes) aligned address, otherwise an `PmmError::InvalidAddressAlignment` error is returned.
    fn free(&mut self, addr: PhysAddr, page_count: usize) -> Result<(), super::PmmError> {
        let id = addr_to_page_id(addr.0).ok_or(PmmError::InvalidAddress)?;

        // For each page ID in the range, try freeing the value. If an error is encountered, stop
        // and return
        (id..(id + page_count)).try_for_each(|page_id| {
            if page_id >= self.used_bitmap_size {
                return Err(PmmError::OutOfBounds);
            } else if self.bitmap_get(id) == Self::FREE {
                return Err(PmmError::FreeOfAlreadyFree);
            }

            self.bitmap_unset(page_id);

            Ok(())
        })
    }

    fn is_page_free(&self, addr: PhysAddr) -> Result<bool, super::PmmError> {
        let id = addr_to_page_id(addr.0).ok_or(PmmError::InvalidAddress)?;

        if id >= self.used_bitmap_size {
            return Err(PmmError::OutOfBounds);
        }

        Ok(self.bitmap_get(id) == Self::FREE)
    }

    #[inline]
    unsafe fn init_from_limine(mem_map: &[&limine::memory_map::Entry]) {
        #[inline]
        unsafe fn set_for_mem_map_entry(base: usize, len: usize) -> core::ops::Range<usize> {
            {
                let start_id = addr_to_page_id(base).unwrap();
                // If the length is page aligned, set iterate over the next page as well.
                // NOTE: This is OK, since this never happens with `USEABLE` or `BOOTLOADER_RECLAIMABLE`. And
                // it's a must for allocating the page table, since it's almost always not page aligned
                let end_id = start_id + ((len + 0xfff) / 0x1000);

                start_id..end_id
            }
            .into_iter()
        }
        #[inline]
        unsafe fn new_bitmap_from_limine<'b, 'c>(
            mem_map: &[&'c limine::memory_map::Entry],
            bitmap_alloc_size: u64,
        ) -> (&'b mut [u8], &'c limine::memory_map::Entry) {
            // Find a suitable block to allocate the bitmap in.
            let bitmap_entry = mem_map
                .iter()
                .find(|&entry| match entry.entry_type {
                    limine::memory_map::EntryType::USABLE if entry.length >= bitmap_alloc_size => {
                        true
                    }
                    _ => false,
                })
                .expect("Unreachable, can't find entry for bitmap");

            let bitmap = unsafe {
                // Convert the block's physical address to VirtAddr using HHDM, then convert that
                // to a valid pointer
                let bitmap_virt_addr = PhysAddr(bitmap_entry.base as usize).add_hhdm_offset();
                let ptr = core::ptr::without_provenance_mut::<u8>(bitmap_virt_addr.0);

                // Set all of memory to taken by default
                crate::utils::memset(ptr, BumpAllocator::TAKEN, bitmap_alloc_size as usize);
                // Convert to a bitmap slice
                from_raw_parts_mut(ptr, bitmap_alloc_size as usize)
            };

            (bitmap, bitmap_entry)
        }

        // Get the last allocatable memory descriptor - that'll decide the bitmap size (yes, we'll
        // still have some "holes" (ie. reserved & shit memory) but it's negligible. Not worth the
        // hasstle of maintaining multiple bitmaps etc)
        let page_count = {
            let last_descr = mem_map
                .iter()
                .rev()
                .find(|&entry| match entry.entry_type {
                    limine::memory_map::EntryType::USABLE
                    | limine::memory_map::EntryType::BOOTLOADER_RECLAIMABLE
                    | limine::memory_map::EntryType::ACPI_RECLAIMABLE
                    | limine::memory_map::EntryType::KERNEL_AND_MODULES => true,
                    _ => false,
                })
                .unwrap();
            (last_descr.base + last_descr.length) as usize
        } / 0x1000;

        unsafe {
            // Get the size we need to allocate to the bitmap (that's `what we use` + `rounding up` to byte alignment)
            #[allow(static_mut_refs)]
            let bitmap_alloc_size = (page_count + 7) / 8;
            // But set the actual bitmap size to the page count, since these are the pages we can
            // actually use
            BUMP_ALLOCATOR.used_bitmap_size = page_count;

            // Find a suitable bitmap & initilize it with 1's
            let bitmap_entry: &limine::memory_map::Entry;
            (BUMP_ALLOCATOR.bitmap, bitmap_entry) =
                new_bitmap_from_limine(mem_map, bitmap_alloc_size as u64);

            // Update the bitmaps contents to the current memory map + entry allocated for the
            // bitmap
            #[allow(static_mut_refs)]
            for entry in mem_map {
                match entry.entry_type {
                    // XXX: Not sure about the BOOTLOADER_RECLAIMABLE
                    memory_map::EntryType::USABLE => {
                        set_for_mem_map_entry(entry.base as usize, entry.length as usize)
                            .for_each(|page_id| BUMP_ALLOCATOR.bitmap_unset(page_id));
                    }
                    _ => (),
                }
            }

            #[allow(static_mut_refs)]
            set_for_mem_map_entry(bitmap_entry.base as usize, bitmap_alloc_size)
                .for_each(|page_id| BUMP_ALLOCATOR.bitmap_set(page_id));
        }
    }
}

impl<'a> BumpAllocator<'a> {
    const FREE: u8 = 0x0;
    const TAKEN: u8 = 0xff;

    /// Index into the bitmap and unset the status of a page
    const fn bitmap_unset(&mut self, index: PageId) {
        self.bitmap[index / 8] &= !(1 << (index % 8));
    }

    /// Index into the bitmap and set the status of a page
    const fn bitmap_set(&mut self, index: PageId) {
        self.bitmap[index / 8] |= 1 << (index % 8);
    }

    /// Index into the bitmap and get the status of a page
    const fn bitmap_get(&self, index: PageId) -> u8 {
        self.bitmap[index / 8] & (1 << (index % 8))
    }
}

// TODO: Move this to a more fitting place
const fn inc_ring_buff_ptr(ring_buff: PageId, amount: usize, ring_buff_size: usize) -> PageId {
    (ring_buff + amount) % ring_buff_size
}
