//! VMM which provides high level paging wrappers

mod heap;
pub mod slab;

// TODO: Remove x86_64 dependencies here
// TODO: Rewrite all of this shit well

use core::{num::NonZero, ptr::NonNull};

use crate::arch::x86_64::paging::{PageSize, PageTable, PagingError};

use crate::mem::pmm::{self, PmmAllocator};
use crate::mem::{VirtAddr, page_id_to_addr};

/// Tries to allocate a block of pages of `page_count` amount, satisfying `alignment` page alignment
/// (e.g. If `alignment == 2` then allocaitons will happen in multiples of 2 * BASE_PAGE_SIZE)
pub fn alloc_pages_any(
    alignment: NonZero<usize>,
    page_count: NonZero<usize>,
) -> Result<NonNull<()>, PagingError> {
    let phys_addr = pmm::get()
        .alloc_any(alignment, page_count)
        .map_err(|e| PagingError::AllocationError(e))?;
    let virt_addr = phys_addr.add_hhdm_offset();

    // TODO: Add a way to customize the flags being set. 3 => x86_64 Present + Read & Write
    PageTable::map_page_specific(virt_addr, phys_addr, 3, PageSize::Size4KB)?;

    // SAFETY: `virt_addr` is not null since physical page 0 should always be marked as taken
    Ok(NonNull::new(virt_addr.0 as *mut ()).unwrap())
}

/// Tries to allocates a block of pages of `page_count` amount, at the given `virt_addr` virtual
/// address
#[allow(dead_code)]
pub fn alloc_pages_at(virt_addr: VirtAddr, page_count: NonZero<usize>) -> Result<(), PagingError> {
    let phys_addr = virt_addr.subtract_hhdm_offset();

    pmm::get()
        .alloc_at(phys_addr, page_count)
        .map_err(|e| PagingError::AllocationError(e))?;

    // TODO: Add a way to customize the flags being set. 3 => x86_64 Present + Read & Write
    PageTable::map_page_specific(virt_addr, phys_addr, 3, PageSize::Size4KB)
}

/// Tries to unmap and free a contigious block of pages of `page_count` amount, at the given `virt_addr`
pub unsafe fn free_pages(ptr: NonNull<()>, page_count: NonZero<usize>) -> Result<(), PagingError> {
    let virt_addr: VirtAddr = ptr.into();
    for i in 0..page_count.get() {
        let virt_addr = VirtAddr(virt_addr.0 + page_id_to_addr(i));
        unsafe { PageTable::unmap_page(virt_addr, PageSize::Size4KB) }?;
    }

    let phys_addr = virt_addr.subtract_hhdm_offset();

    unsafe { pmm::get().free(phys_addr, page_count) }
        .map_err(|e| PagingError::AllocationError(e))?;

    Ok(())
}
