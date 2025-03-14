//! VMM which provides high level paging wrappers

pub mod slab;

// TODO: Remove x86_64 dependencies here

/// Allocation of pages for the kernel
pub mod kalloc {

    use core::{ffi::c_void, num::NonZero, ptr::NonNull};

    use crate::arch::x86_64::paging::PagingError;

    use crate::mem::{page_id_to_addr, VirtAddr};
    use crate::mem::pmm::PmmAllocator;

    /// Tries to allocate a block of pages of `page_count` amount, satisfying `alignment` page alignment (e.g. If `alignment == 2` then allocaitons will happen in multiples of 2 * BASE_PAGE_SIZE)
    pub fn kalloc_pages_any(
        alignment: usize,
        page_count: usize,
    ) -> Result<NonNull<c_void>, PagingError> {

        let phys_addr = crate::mem::pmm::get()
            .alloc_any(alignment, page_count)
            .map_err(|e| PagingError::AllocationError(e))?;
        let virt_addr = phys_addr.add_hhdm_offset();

        // TODO: Add a way to customize the flags being set. 3 => x86_64 Present + Read & Write
        crate::arch::x86_64::paging::PageTable::map_page_specific(virt_addr, phys_addr, 3)?;

        // SAFETY: `virt_addr` is not null since physical page 0 should always be marked as taken
        Ok(NonNull::without_provenance(
            NonZero::new(virt_addr.0).unwrap(),
        ))
    }

    /// Tries to allocates a block of pages of `page_count` amount, at the given `virt_addr` virtual
    /// address
    pub fn kalloc_pages_at(virt_addr: VirtAddr, page_count: usize) -> Result<(), PagingError> {
        let phys_addr = virt_addr.subtract_hhdm_offset();
        crate::mem::pmm::get()
            .alloc_at(phys_addr, page_count)
            .map_err(|e| PagingError::AllocationError(e))?;

        // TODO: Add a way to customize the flags being set. 3 => x86_64 Present + Read & Write
        crate::arch::x86_64::paging::PageTable::map_page_specific(virt_addr, phys_addr, 3)
    }

    /// Tries to unmap and free a contigious block of pages of `page_count` amount, at the given `virt_addr`
    pub unsafe fn kfree_pages(ptr: NonNull<c_void>, page_count: usize) -> Result<(), PagingError> {
        let virt_addr: VirtAddr = ptr.into();
        for i in 0..page_count {
            let virt_addr = VirtAddr(virt_addr.0 + page_id_to_addr(i));
            unsafe {crate::arch::x86_64::paging::PageTable::unmap_page(virt_addr)}?;
        }

        let phys_addr = virt_addr.subtract_hhdm_offset();
        unsafe {crate::mem::pmm::get().free(phys_addr, page_count)}
            .map_err(|e| PagingError::AllocationError(e))?;

        Ok(())
    }
}
