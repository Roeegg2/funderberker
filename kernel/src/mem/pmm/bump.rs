//! A simple bump allocator physical memory manager

use core::num::NonZero;
use core::slice::from_raw_parts_mut;

use limine::memory_map;

use crate::arch::BASIC_PAGE_SIZE;
use crate::boot::limine::get_page_count_from_mem_map;

use super::super::{addr_to_page_id, page_id_to_addr};
use super::{PhysAddr, PmmAllocator, PmmError};
use utils::collections::static_bitmap::StaticBitmap;

// TODO: Definitely use an UnsafeCell with some locking mechanism here
/// Singleton instance of the bump allocator
pub(super) static mut BUMP_ALLOCATOR: BumpAllocator = BumpAllocator(StaticBitmap::uninit());

/// Singleton bump allocator implemented using bitmap
pub(super) struct BumpAllocator<'a>(StaticBitmap<'a>);

impl<'a> PmmAllocator for BumpAllocator<'a> {
    fn alloc_any(
        &mut self,
        alignment: NonZero<usize>,
        page_count: NonZero<usize>,
    ) -> Result<PhysAddr, PmmError> {
        'main: for i in (0..self.0.used_bits_count()).step_by(alignment.get()) {
            // If we would go out of bounds, break
            if i + page_count.get() >= self.0.used_bits_count() {
                break;
            }

            // Check if the block is free. If it isn't go next
            for j in 0..page_count.get() {
                if self.0.get(i + j) != StaticBitmap::FREE {
                    continue 'main;
                }
            }

            // If it is, set the block as taken and return the address
            for j in 0..page_count.get() {
                self.0.set(i + j);
            }

            return Ok(PhysAddr(page_id_to_addr(i)));
        }

        Err(PmmError::NoAvailableBlock)
    }

    fn alloc_at(
        &mut self,
        addr: PhysAddr,
        page_count: NonZero<usize>,
    ) -> Result<(), super::PmmError> {
        let id = addr_to_page_id(addr.0).ok_or(PmmError::InvalidAddress)?;

        if (id + page_count.get() - 1) >= self.0.used_bits_count() {
            return Err(PmmError::OutOfBounds);
        }

        // Make sure we can allocate the block. If we can't we propergate the error
        for i in 0..page_count.get() {
            if self.0.get(id + i) != StaticBitmap::FREE {
                return Err(PmmError::NoAvailableBlock);
            }
        }

        for i in 0..page_count.get() {
            self.0.set(id + i);
        }

        Ok(())
    }

    /// NOTE: `addr` must be a page (4096 bytes) aligned address, otherwise an `PmmError::InvalidAddressAlignment` error is returned.
    unsafe fn free(
        &mut self,
        addr: PhysAddr,
        page_count: NonZero<usize>,
    ) -> Result<(), super::PmmError> {
        let id = addr_to_page_id(addr.0).ok_or(PmmError::InvalidAddress)?;

        if (id + page_count.get() - 1) >= self.0.used_bits_count() {
            return Err(PmmError::OutOfBounds);
        }

        // For each page ID in the range, try freeing the value. If an error is encountered, stop
        // and return
        for i in 0..page_count.get() {
            if self.0.get(id + i) == StaticBitmap::FREE {
                return Err(PmmError::FreeOfAlreadyFree);
            }
        }

        for i in 0..page_count.get() {
            self.0.unset(id + i);
        }

        Ok(())
    }

    fn is_page_free(
        &self,
        addr: PhysAddr,
        page_count: NonZero<usize>,
    ) -> Result<bool, super::PmmError> {
        let id = addr_to_page_id(addr.0).ok_or(PmmError::InvalidAddress)?;

        if id + page_count.get() >= self.0.used_bits_count() {
            return Err(PmmError::OutOfBounds);
        }

        for i in 0..page_count.get() {
            if self.0.get(id + i) != StaticBitmap::FREE {
                return Ok(false);
            }
        }

        Ok(true)
    }

    // Ugly code. But if it ain't broke, don't try to fix it
    #[cfg(feature = "limine")]
    #[inline]
    unsafe fn init_from_limine(mem_map: &[&limine::memory_map::Entry]) {
        #[inline]
        unsafe fn set_for_mem_map_entry(base: usize, len: usize) -> core::ops::Range<usize> {
            {
                let start_id = addr_to_page_id(base).unwrap();
                // If the length is page aligned, set iterate over the next page as well.
                // NOTE: This is OK, since this never happens with `USEABLE` or `BOOTLOADER_RECLAIMABLE`. And
                // it's a must for allocating the page table, since it's almost always not page aligned
                let end_id = start_id + ((len + 0xfff) / BASIC_PAGE_SIZE);

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
                let ptr = bitmap_virt_addr.0 as *mut u8;

                // Set all of memory to taken by default
                utils::mem::memset(ptr, StaticBitmap::BLOCK_TAKEN, bitmap_alloc_size as usize);
                // Convert to a bitmap slice
                from_raw_parts_mut(ptr, bitmap_alloc_size as usize)
            };

            (bitmap, bitmap_entry)
        }

        // Get the last allocatable memory descriptor - that'll decide the bitmap size (yes, we'll
        // still have some "holes" (ie. reserved & shit memory) but it's negligible. Not worth the
        // hasstle of maintaining multiple bitmaps etc)
        let page_count = get_page_count_from_mem_map(mem_map);

        unsafe {
            // Get the size we need to allocate to the bitmap (that's `what we use` + `rounding up` to byte alignment)
            #[allow(static_mut_refs)]
            let bitmap_alloc_size = (page_count.get() + 7) / 8;

            // Find a suitable bitmap & initilize it with 1's
            let (bitmap, bitmap_entry) = new_bitmap_from_limine(mem_map, bitmap_alloc_size as u64);

            // Set the actual bitmap size to the page count, since these are the pages we can
            // actually use
            BUMP_ALLOCATOR.0 = StaticBitmap::new(bitmap, page_count.get());

            // Update the bitmaps contents to the current memory map + entry allocated for the
            // bitmap
            #[allow(static_mut_refs)]
            for entry in mem_map {
                if entry.entry_type == memory_map::EntryType::USABLE {
                        set_for_mem_map_entry(entry.base as usize, entry.length as usize)
                            .for_each(|page_id| BUMP_ALLOCATOR.0.unset(page_id));
                }
            }

            #[allow(static_mut_refs)]
            set_for_mem_map_entry(bitmap_entry.base as usize, bitmap_alloc_size)
                .for_each(|page_id| BUMP_ALLOCATOR.0.set(page_id));
        }
    }
}

#[cfg(test)]
mod tests {
    use macros::test_fn;

    use super::*;

    #[test_fn]
    fn test_bump_alloc() {
        let allocator = unsafe {
            #[allow(static_mut_refs)]
            &mut BUMP_ALLOCATOR
        };

        let addr0 = allocator
            .alloc_any(unsafe { NonZero::new_unchecked(1) }, unsafe {
                NonZero::new_unchecked(1)
            })
            .unwrap();
        let addr1 = allocator
            .alloc_any(unsafe { NonZero::new_unchecked(2) }, unsafe {
                NonZero::new_unchecked(10)
            })
            .unwrap();
        assert!(addr1.0 % 0x2000 == 0);

        let addr2 = allocator
            .alloc_any(unsafe { NonZero::new_unchecked(1) }, unsafe {
                NonZero::new_unchecked(2)
            })
            .unwrap();
        unsafe { allocator.free(addr0, NonZero::new_unchecked(1)).unwrap() };

        for _ in 0..10 {
            let addr = allocator
                .alloc_any(unsafe { NonZero::new_unchecked(1) }, unsafe {
                    NonZero::new_unchecked(3)
                })
                .unwrap();
            unsafe { allocator.free(addr, NonZero::new_unchecked(3)).unwrap() };
        }

        unsafe { allocator.free(addr1, NonZero::new_unchecked(10)).unwrap() };
        unsafe { allocator.free(addr2, NonZero::new_unchecked(2)).unwrap() };
    }

    #[test_fn]
    fn test_bump_error() {
        let allocator = unsafe {
            #[allow(static_mut_refs)]
            &mut BUMP_ALLOCATOR
        };

        let addr = allocator
            .alloc_any(unsafe { NonZero::new_unchecked(2) }, unsafe {
                NonZero::new_unchecked(10)
            })
            .unwrap();

        assert_eq!(
            allocator.alloc_at(addr, unsafe { NonZero::new_unchecked(10) }),
            Err(PmmError::NoAvailableBlock)
        );

        unsafe { allocator.free(addr, NonZero::new_unchecked(5)).unwrap() };

        unsafe {
            assert_eq!(
                allocator.free(addr, NonZero::new_unchecked(5)),
                Err(PmmError::FreeOfAlreadyFree)
            )
        };

        unsafe {
            allocator
                .free(
                    PhysAddr(addr.0 + 5 * BASIC_PAGE_SIZE),
                    NonZero::new_unchecked(5),
                )
                .unwrap()
        };
    }

    // TODO: Need to test alloc_at
}
