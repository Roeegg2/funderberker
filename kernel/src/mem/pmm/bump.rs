//! A simple ring-buffer style bump allocator physical memory manager

use core::slice::from_raw_parts_mut;

use limine::memory_map;

use super::super::{PageId, addr_to_page_id, page_id_to_addr};
use super::{PhysAddr, PmmAllocator, PmmError};
use utils::collections::bitmap::Bitmap;

// TODO: Definitely use an UnsafeCell with some locking mechanism here
/// Singleton instance of the bump allocator
pub(super) static mut BUMP_ALLOCATOR: BumpAllocator = BumpAllocator {
    bitmap: Bitmap::uninit(),
    ptr: 0,
};

/// Singleton ring-buffer bump allocator implemented using bitmap
pub(super) struct BumpAllocator<'a> {
    /// The bitmap representing the status of each page (`1` meaning used, `0` meaning free)
    bitmap: Bitmap<'a>,
    /// The ring buffer ptr for finding new pages to allocate
    ptr: PageId,
}

impl<'a> PmmAllocator for BumpAllocator<'a> {
    fn alloc_any(&mut self, alignment: usize, page_count: usize) -> Result<PhysAddr, PmmError> {
        let mut ptr = inc_ring_buff_ptr(self.ptr, 1, self.bitmap.used_bits_count());

        if alignment >= self.bitmap.used_bits_count() || alignment == 0 {
            return Err(PmmError::InvalidAlignment);
        }

        'main: loop {
            // Couldn't find suitable block
            if ptr == self.ptr {
                return Err(PmmError::NoAvailableBlock);
            }

            // This is an optimization. Instead of incrememting ptr until it gets to the start
            // of the ring buffer, we set it to 0 now
            if ptr + page_count >= self.bitmap.used_bits_count() {
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
                    ptr = inc_ring_buff_ptr(ptr, diff, self.bitmap.used_bits_count());
                    continue;
                }
            }

            // Check if all entries are available. If at least one isn't, go next
            for i in 0..page_count {
                if self.bitmap.get(ptr + i) != Bitmap::FREE {
                    ptr = inc_ring_buff_ptr(ptr, 1, self.bitmap.used_bits_count());
                    continue 'main;
                }
            }

            // We can allocate! So break out of loop
            break;
        }

        // They are, so mark them as taken
        for i in 0..page_count {
            self.bitmap.set(ptr + i);
        }

        self.ptr = ptr;

        return Ok(PhysAddr(page_id_to_addr(ptr)));
    }

    fn alloc_at(&mut self, addr: PhysAddr, page_count: usize) -> Result<(), super::PmmError> {
        let id = addr_to_page_id(addr.0).ok_or(PmmError::InvalidAddress)?;

        // Make sure we can allocate the block. If we can't we propergate the error
        (id..(id + page_count)).try_for_each(|i| {
            if i >= self.bitmap.used_bits_count() {
                return Err(PmmError::OutOfBounds);
            } else if self.bitmap.get(id) != Bitmap::FREE {
                return Err(PmmError::NoAvailableBlock);
            }

            Ok(())
        })?;

        // If we can, then we do!
        (id..page_count).for_each(|page_id| {
            self.bitmap.set(page_id);
        });

        Ok(())
    }

    /// NOTE: `addr` must be a page (4096 bytes) aligned address, otherwise an `PmmError::InvalidAddressAlignment` error is returned.
    unsafe fn free(&mut self, addr: PhysAddr, page_count: usize) -> Result<(), super::PmmError> {
        let id = addr_to_page_id(addr.0).ok_or(PmmError::InvalidAddress)?;

        // For each page ID in the range, try freeing the value. If an error is encountered, stop
        // and return
        (id..(id + page_count)).try_for_each(|page_id| {
            if page_id >= self.bitmap.used_bits_count() {
                return Err(PmmError::OutOfBounds);
            } else if self.bitmap.get(id) == Bitmap::FREE {
                return Err(PmmError::FreeOfAlreadyFree);
            }

            self.bitmap.unset(page_id);

            Ok(())
        })
    }

    fn is_page_free(&self, addr: PhysAddr) -> Result<bool, super::PmmError> {
        let id = addr_to_page_id(addr.0).ok_or(PmmError::InvalidAddress)?;

        if id >= self.bitmap.used_bits_count() {
            return Err(PmmError::OutOfBounds);
        }

        Ok(self.bitmap.get(id) == Bitmap::FREE)
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
                utils::mem::memset(ptr, Bitmap::BLOCK_TAKEN, bitmap_alloc_size as usize);
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

            // Find a suitable bitmap & initilize it with 1's
            let (bitmap, bitmap_entry) = new_bitmap_from_limine(mem_map, bitmap_alloc_size as u64);

            // Set the actual bitmap size to the page count, since these are the pages we can
            // actually use
            BUMP_ALLOCATOR.bitmap = Bitmap::new(bitmap, page_count);

            // Update the bitmaps contents to the current memory map + entry allocated for the
            // bitmap
            #[allow(static_mut_refs)]
            for entry in mem_map {
                match entry.entry_type {
                    // XXX: Not sure about the BOOTLOADER_RECLAIMABLE
                    memory_map::EntryType::USABLE => {
                        set_for_mem_map_entry(entry.base as usize, entry.length as usize)
                            .for_each(|page_id| BUMP_ALLOCATOR.bitmap.unset(page_id));
                    }
                    _ => (),
                }
            }

            #[allow(static_mut_refs)]
            set_for_mem_map_entry(bitmap_entry.base as usize, bitmap_alloc_size)
                .for_each(|page_id| BUMP_ALLOCATOR.bitmap.set(page_id));
        }
    }
}

// TODO: Move this to a more fitting place
const fn inc_ring_buff_ptr(ring_buff: PageId, amount: usize, ring_buff_size: usize) -> PageId {
    (ring_buff + amount) % ring_buff_size
}
