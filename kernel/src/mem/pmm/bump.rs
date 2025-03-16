//! A simple bump allocator physical memory manager

use core::slice::from_raw_parts_mut;

use limine::memory_map;

use super::super::{PageId, addr_to_page_id, page_id_to_addr};
use super::{PhysAddr, PmmAllocator, PmmError};
use utils::collections::bitmap::Bitmap;

// TODO: Definitely use an UnsafeCell with some locking mechanism here
/// Singleton instance of the bump allocator
pub(super) static mut BUMP_ALLOCATOR: BumpAllocator = BumpAllocator(Bitmap::uninit());

/// Singleton bump allocator implemented using bitmap
pub(super) struct BumpAllocator<'a>(Bitmap<'a>);

impl<'a> PmmAllocator for BumpAllocator<'a> {
    fn alloc_any(&mut self, alignment: PageId, page_count: usize) -> Result<PhysAddr, PmmError> {
        'main: for i in (0..self.0.used_bits_count()).step_by(alignment) {
            // If we would go out of bounds, break
            if i + page_count >= self.0.used_bits_count() {
                break;
            }

            // Check if the block is free. If it isn't go next
            for j in 0..page_count {
                if self.0.get(i + j) != Bitmap::FREE {
                    continue 'main;
                }
            }

            // If it is, set the block as taken and return the address
            for j in 0..page_count {
                self.0.set(i + j);
            }

            return Ok(PhysAddr(page_id_to_addr(i)));
        }

        Err(PmmError::NoAvailableBlock)
    }

    fn alloc_at(&mut self, addr: PhysAddr, page_count: usize) -> Result<(), super::PmmError> {
        let id = addr_to_page_id(addr.0).ok_or(PmmError::InvalidAddress)?;

        if (id + page_count-1) >= self.0.used_bits_count() {
            return Err(PmmError::OutOfBounds);
        }

        // Make sure we can allocate the block. If we can't we propergate the error
        for i in 0..page_count {
            if self.0.get(id + i) != Bitmap::FREE {
                return Err(PmmError::NoAvailableBlock);
            }
        }

        for i in 0..page_count {
            self.0.set(id + i);
        }

        Ok(())
    }

    /// NOTE: `addr` must be a page (4096 bytes) aligned address, otherwise an `PmmError::InvalidAddressAlignment` error is returned.
    unsafe fn free(&mut self, addr: PhysAddr, page_count: usize) -> Result<(), super::PmmError> {
        let id = addr_to_page_id(addr.0).ok_or(PmmError::InvalidAddress)?;

        if (id + page_count-1) >= self.0.used_bits_count() {
            return Err(PmmError::OutOfBounds);
        }

        // For each page ID in the range, try freeing the value. If an error is encountered, stop
        // and return
        for i in 0..page_count {
            if self.0.get(id + i) == Bitmap::FREE {
                return Err(PmmError::FreeOfAlreadyFree);
            }
        }

        for i in 0..page_count {
            self.0.unset(id + i);
        }

        Ok(())
    }

    fn is_page_free(&self, addr: PhysAddr) -> Result<bool, super::PmmError> {
        let id = addr_to_page_id(addr.0).ok_or(PmmError::InvalidAddress)?;

        if id >= self.0.used_bits_count() {
            return Err(PmmError::OutOfBounds);
        }

        Ok(self.0.get(id) == Bitmap::FREE)
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
            BUMP_ALLOCATOR.0 = Bitmap::new(bitmap, page_count);

            // Update the bitmaps contents to the current memory map + entry allocated for the
            // bitmap
            #[allow(static_mut_refs)]
            for entry in mem_map {
                match entry.entry_type {
                    // XXX: Not sure about the BOOTLOADER_RECLAIMABLE
                    memory_map::EntryType::USABLE => {
                        set_for_mem_map_entry(entry.base as usize, entry.length as usize)
                            .for_each(|page_id| BUMP_ALLOCATOR.0.unset(page_id));
                    }
                    _ => (),
                }
            }

            #[allow(static_mut_refs)]
            set_for_mem_map_entry(bitmap_entry.base as usize, bitmap_alloc_size)
                .for_each(|page_id| BUMP_ALLOCATOR.0.set(page_id));
        }
    }
}
