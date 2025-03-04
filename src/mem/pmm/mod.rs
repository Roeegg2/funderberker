//! A simple ring-buffer style bump allocator physical memory manager

use core::slice::from_raw_parts_mut;

use super::{PageId, PhysAddr, addr_to_page_id, page_id_to_addr};

// TODO: Definitely use an UnsafeCell with some locking mechanism here
/// Singleton instance of the bump allocator
pub static mut BUMP_ALLOCATOR: BumpAllocator = BumpAllocator {
    bitmap: &mut [],
    ptr: 0,
    used_bitmap_size: 0,
};

/// Errors that the bump allocator might encounter
#[derive(Debug, PartialEq)]
pub enum PmmError {
    NoAvailableBlock,
    FreeOfAlreadyFree,
    InvalidAddressAlignment,
}

pub struct BumpAllocator<'a> {
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

impl<'a> BumpAllocator<'a> {
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
        let mut ptr = inc_ring_buff_ptr(self.ptr, 1, self.used_bitmap_size);

        if alignment >= self.used_bitmap_size || alignment == 0 {
            return Err(PmmError::InvalidAddressAlignment);
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
                if self.bitmap_get(ptr + i) != 0 {
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

    /// Tries to free a block of pages of size `page_count` starting at `addr`.
    /// NOTE: `addr` must be a page (4096 bytes) aligned address, otherwise an `PmmError::InvalidAddressAlignment` error is returned.
    pub fn _free(&mut self, addr: PhysAddr, page_count: usize) -> Result<(), PmmError> {
        let id = addr_to_page_id(addr.0).ok_or(PmmError::InvalidAddressAlignment)?;

        if id + page_count >= self.used_bitmap_size {
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

#[cfg(feature = "limine")]
impl<'a> BumpAllocator<'a> {
    unsafe fn set_for_mem_map_entry(&mut self, base: usize, len: usize) {
        {
            let start_id = addr_to_page_id(base).unwrap();
            let end_id = start_id + ((len + 0xfff) / 0x1000);

            start_id..end_id
        }
        .into_iter()
        .for_each(|page_id| self.bitmap_set(page_id));
    }

    unsafe fn fill_bitmap(
        &mut self,
        mem_map: &[&limine::memory_map::Entry],
        bitmap_entry: &limine::memory_map::Entry,
        bitmap_alloc_size: usize,
    ) {
        for entry in mem_map {
            match entry.entry_type {
                limine::memory_map::EntryType::USABLE
                | limine::memory_map::EntryType::BOOTLOADER_RECLAIMABLE => unsafe {
                    self.set_for_mem_map_entry(entry.base as usize, entry.length as usize)
                },
                _ => (),
            }
        }

        unsafe { self.set_for_mem_map_entry(bitmap_entry.base as usize, bitmap_alloc_size) };
    }

    pub unsafe fn init_from_limine(mem_map: &[&limine::memory_map::Entry]) {
        unsafe fn new_bitmap_from_limine<'b, 'c>(
            mem_map: &[&'c limine::memory_map::Entry],
            bitmap_alloc_size: u64,
        ) -> (&'b mut [u8], &'c limine::memory_map::Entry) {
            let bitmap_entry = mem_map
                .iter()
                .find(|&entry| match entry.entry_type {
                    limine::memory_map::EntryType::USABLE
                    | limine::memory_map::EntryType::BOOTLOADER_RECLAIMABLE
                        if entry.length >= bitmap_alloc_size =>
                    {
                        true
                    }
                    _ => false,
                })
                .expect("Unreachable, can't find entry for bitmap");

            let bitmap = unsafe {
                let bitmap_virt_addr = PhysAddr(bitmap_entry.base as usize).add_hhdm_offset();
                let ptr = core::ptr::without_provenance_mut::<u8>(bitmap_virt_addr.0);

                crate::utils::memset(ptr, 1, bitmap_alloc_size as usize);

                from_raw_parts_mut(ptr, bitmap_alloc_size as usize)
            };

            (bitmap, bitmap_entry)
        }

        let page_count = {
            let last_descr = mem_map
                .iter()
                .rev()
                .find(|&entry| entry.base != 0xfd00000000)
                .unwrap();
            (last_descr.base + last_descr.length) as usize
        } / 0x1000;

        unsafe {
            #[allow(static_mut_refs)]
            let bitmap_alloc_size = (page_count + 7) / 8;
            BUMP_ALLOCATOR.used_bitmap_size = page_count;

            let bitmap_entry: &limine::memory_map::Entry;
            (BUMP_ALLOCATOR.bitmap, bitmap_entry) =
                new_bitmap_from_limine(mem_map, bitmap_alloc_size as u64);

            #[allow(static_mut_refs)]
            BUMP_ALLOCATOR.fill_bitmap(mem_map, bitmap_entry, bitmap_alloc_size);
        }
    }
}

// TODO: Move this to a more fitting place
const fn inc_ring_buff_ptr(ring_buff: PageId, amount: usize, ring_buff_size: usize) -> PageId {
    (ring_buff + amount) % ring_buff_size
}
