//! A simple ring-buffer style bump allocator physical memory manager

use super::{PageId, addr_to_page_id, page_id_to_addr};

// TODO: Definitely use an UnsafeCell with some locking mechanism here
/// Singleton instance of the bump allocator
static mut BUMP_ALLOCATOR: BumpAllocator = BumpAllocator {
    bitmap: &mut [],
    ptr: 0,
};

/// Errors that the bump allocator might encounter
#[derive(Debug)]
pub enum PmmError {
    NoAvailableBlock,
    FreeOfAlreadyFree,
    InvalidAddressAlignment,
}

struct BumpAllocator {
    /// The bitmap representing the status of each page (`1` meaning used, `0` meaning free)
    bitmap: &'static mut [u8],
    /// The ring buffer ptr for finding new pages to allocate
    ptr: PageId,
}

impl BumpAllocator {
    /// Initilizes the singleton page bump allocator. SHOULD ONLY BE CALLED ONCE EARLY AT BOOT!
    pub unsafe fn init(bitmap: &'static mut [u8]) {
        unsafe {
            BUMP_ALLOCATOR.bitmap = bitmap;
            // NOTE: `ptr` is already 0, so it's not changed
        }
    }

    /// Index into the bitmap and set the status of a page
    const fn bitmap_set(&mut self, index: PageId, val: u8) {
        self.bitmap[index / 8] |= val & (1 << index);
    }

    /// Index into the bitmap and get the status of a page
    const fn bitmap_get(&self, index: PageId) -> u8 {
        self.bitmap[index / 8] & (1 << index)
    }

    /// Tries to allocates a contiguious block of pages of size `page_count` which satisfy the passed `alignment`. If allocation if successfull, the physical address of the start of the block is returned.
    pub fn allocate(&mut self, alignment: usize, page_count: usize) -> Result<usize, PmmError> {
        let mut ptr = self.ptr;

        'main: loop {
            ptr = (ptr + 1) % self.bitmap.len();

            if ptr == self.ptr {
                return Err(PmmError::NoAvailableBlock); // couldn't find suitable block
            }

            // If alignment doesn't match, go next
            if ptr % alignment != 0 {
                continue;
            }

            for i in 0..page_count {
                if self.bitmap_get(i) != 0 {
                    continue 'main;
                }
            }

            // TODO: find an easier way to do this using iterators or something
            for i in 0..page_count {
                self.bitmap_set(i, 1);
            }

            // Advance ring buffer
            self.ptr = (ptr + page_count) % self.bitmap.len();

            return Ok(page_id_to_addr(ptr));
        }
    }

    /// Tries to free a block of pages of size `page_count` starting at `addr`.
    /// NOTE: `addr` must be a page (4096 bytes) aligned address, otherwise an `PmmError::InvalidAddressAlignment` error is returned.
    pub fn free(&mut self, addr: usize, page_count: usize) -> Result<(), PmmError> {
        let id = addr_to_page_id(addr).ok_or(PmmError::InvalidAddressAlignment)?;

        for i in 0..page_count {
            if self.bitmap[id + i] != 1 {
                return Err(PmmError::FreeOfAlreadyFree); // can't free already freed page
            }
        }

        // Now that we know we can free them, free them :)
        for i in 0..page_count {
            self.bitmap[id + i] = 0;
        }

        Ok(())
    }
}
