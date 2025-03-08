//! VMM which provides high level paging wrappers

pub mod slab;

// TODO: Remove x86_64 dependencies here

/// Allocation of pages for the kernel
pub mod kalloc {

    use core::{ffi::c_void, num::NonZero, ptr::NonNull};

    use crate::arch::x86_64::paging::PagingError;

    use crate::mem::{PhysAddr, pmm::PmmAllocator};

    /// Tries to allocate a block of pages of `page_count` amount, satisfying `alignment` page alignment (e.g. is `alignment = 2` then allocaitons will happen in multiples of 2 * BASE_PAGE_SIZE)
    pub fn kalloc_pages_any(
        alignment: usize,
        page_count: usize,
    ) -> Result<NonNull<c_void>, PagingError> {
        // WHEN GETTING BACK FROM BREAK:
        // 1. you were in the middle of finishing this function. check comment at the bottom of this
        // function
        // 2. finish alloc_pages_at
        // 3. continue slab implementation
        // 4. implement kheap[]
        let phys_addr = crate::mem::pmm::get()
            .alloc_any(alignment, page_count)
            .map_err(|e| PagingError::AllocationError(e))?;
        let virt_addr = phys_addr.add_hhdm_offset();

        // TODO: Add a way to customize the flags being set. 3 => x86_64 Present + Read & Write
        crate::arch::x86_64::paging::PageTable::map_page_specific(virt_addr, phys_addr, 3)?;

        // We can gurantee that `virt_addr` is not null since physical page 0 should always be marked as taken
        Ok(NonNull::without_provenance(
            NonZero::new(virt_addr.0).unwrap(),
        ))
    }

    /// Tries to allocates a block of pages of `page_count` amount, at the given `addr` virtual
    /// address
    pub fn kalloc_pages_at(phys_addr: PhysAddr, page_count: usize) -> Result<(), PagingError> {
        crate::mem::pmm::get()
            .alloc_at(phys_addr, page_count)
            .map_err(|e| PagingError::AllocationError(e))?;

        let virt_addr = phys_addr.add_hhdm_offset();
        // TODO: Add a way to customize the flags being set. 3 => x86_64 Present + Read & Write
        crate::arch::x86_64::paging::PageTable::map_page_specific(virt_addr, phys_addr, 3)
    }
}
