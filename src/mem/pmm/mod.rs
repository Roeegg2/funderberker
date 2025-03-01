//! A simple ring-buffer style bump allocator physical memory manager

use core::slice::from_raw_parts_mut;

use super::{PageId, PhysAddr, addr_to_page_id, page_id_to_addr};

// TODO: Definitely use an UnsafeCell with some locking mechanism here
/// Singleton instance of the bump allocator
pub static mut BUMP_ALLOCATOR: BumpAllocator = BumpAllocator {
    bitmap: &mut [],
    ptr: 0,
    bitmap_size: 0,
};

/// Errors that the bump allocator might encounter
#[derive(Debug)]
pub enum PmmError {
    NoAvailableBlock,
    FreeOfAlreadyFree,
    InvalidAddressAlignment,
}

pub struct BumpAllocator {
    /// The bitmap representing the status of each page (`1` meaning used, `0` meaning free)
    bitmap: &'static mut [u8],
    /// The ring buffer ptr for finding new pages to allocate
    ptr: PageId,
    /// The size of the used bitmap. Amount of pages doesn't have to be 8 aligned, and in such
    /// case, we allocate an additional entry that has some entries which aren't used.
    /// NOTE: WHEN EVER REFERING TO ADDERSABLE/VALID ENTRIES, **ALWAYS** USE THIS VALUE. NOT THE
    /// BIMTAP SLICE LENGTH!!!
    bitmap_size: usize,
}

impl BumpAllocator {
    /// Initilizes the singleton page bump allocator. SHOULD ONLY BE CALLED ONCE EARLY AT BOOT!
    #[cfg(feature = "limine")]
    pub unsafe fn init_from_limine(mem_map: &[&limine::memory_map::Entry]) {
        // Get the total page count of memory region
        {
            let mut page_count: u64 = 0;
            mem_map.iter().for_each(|&entry| {
                page_count += entry.length;
            });
            page_count /= 0x1000;

            // Calculate bitmap size
            let bitmap_size = (page_count + 7) / 8;
            // Allocate space for bitmap
            let bitmap_entry = mem_map
                .iter()
                .find(|&entry| entry.length >= bitmap_size)
                .expect("Couldn't find memory area to allocate bitmap!");

            // XXX: Unsafe casts here!
            unsafe {
                BUMP_ALLOCATOR.bitmap_size = page_count as usize;
                BUMP_ALLOCATOR.bitmap = {
                    let bitmap_virt_addr = PhysAddr(bitmap_entry.base as usize).add_hhdm_offset();
                    let ptr = core::ptr::without_provenance_mut::<u8>(bitmap_virt_addr.0);
                    //memset(ptr, 0, bitmap_size as usize);
                    from_raw_parts_mut(ptr, bitmap_size as usize)
                };
            }
        }

        mem_map.iter().for_each(|&entry| {
            // NOTE: On AMD processors, this is the start of CPU hypertransport memory map
            #[cfg(feature = "amd")]
            if entry.base == 0xfd00000000 {
                return;
            }

            // Get the range of pages this entry maps
            let page_range = {
                let start_id = addr_to_page_id(entry.base as usize).unwrap();
                let end_id = start_id + addr_to_page_id(entry.length as usize).unwrap();

                start_id..end_id
            };

            // Mark it as used/unused in the bitmap depending on the type
            match entry.entry_type {
                limine::memory_map::EntryType::USABLE
                | limine::memory_map::EntryType::BOOTLOADER_RECLAIMABLE => {
                    page_range.into_iter().for_each(|page| unsafe {
                        #[allow(static_mut_refs)]
                        BUMP_ALLOCATOR.bitmap_unset(page)
                    });
                }
                _ => {
                    page_range.into_iter().for_each(|page| unsafe {
                        #[allow(static_mut_refs)]
                        BUMP_ALLOCATOR.bitmap_unset(page)
                    });
                }
            }
        });

        log!("PMM Bump allocator initilized successfully");
    }

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

    /// Tries to allocates a contiguious block of pages of size `page_count` which satisfy the passed `alignment`. If allocation if successfull, the physical address of the start of the block is returned.
    pub fn allocate_any(
        &mut self,
        alignment: usize,
        page_count: usize,
    ) -> Result<PhysAddr, PmmError> {
        let mut ptr = self.ptr;

        'main: loop {
            // Couldn't find suitable block
            if ptr == self.ptr {
                return Err(PmmError::NoAvailableBlock); 
            }

            // This is an optimization. Instead of incrememting ptr until it gets to the start
            // of the ring buffer, we set it to 0 now
            if ptr + page_count >= self.bitmap_size {
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
                    ptr = inc_ring_buff_ptr(ptr, diff, self.bitmap_size);
                    continue;
                }
            }

            // Check if all entries are available. If at least one isn't, go next
            for i in 0..page_count {
                if self.bitmap_get(ptr + i) != 0 {
                    ptr = inc_ring_buff_ptr(ptr, ptr + i + 1, self.bitmap_size);
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

        self.ptr = inc_ring_buff_ptr(ptr, ptr + page_count, self.bitmap_size);

        return Ok(PhysAddr(page_id_to_addr(ptr)));
    }

    /// Tries to free a block of pages of size `page_count` starting at `addr`.
    /// NOTE: `addr` must be a page (4096 bytes) aligned address, otherwise an `PmmError::InvalidAddressAlignment` error is returned.
    pub fn free(&mut self, addr: PhysAddr, page_count: usize) -> Result<(), PmmError> {
        let id = addr_to_page_id(addr.0).ok_or(PmmError::InvalidAddressAlignment)?;

        if id + page_count >= self.bitmap_size {
            return Err(PmmError::NoAvailableBlock);
        }

        for i in 0..page_count {
            if self.bitmap_get(id + i) == 0 {
                return Err(PmmError::FreeOfAlreadyFree); // can't free already freed page
            }
        }

        // Now that we know we can free them, free them :)
        for i in 0..page_count {
            self.bitmap_unset(id + i);
        }

        Ok(())
    }
}


// TODO: Move this to a more fitting place
const fn inc_ring_buff_ptr(ring_buff: PageId, amount: usize, ring_buff_size: usize) -> PageId {
    (ring_buff + amount) % ring_buff_size
}
