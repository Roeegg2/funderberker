//! Virtual memory manager for handing out virtual pages

use crate::
    arch::{
        BASIC_PAGE_SIZE,
        x86_64::paging::{self, PageSize},
    };
use logger::*;
use utils::{
    collections::id::{hander::IdHander, Id}, mem::{PhysAddr, VirtAddr, HHDM_OFFSET}, sanity_assert, sync::spinlock::{SpinLock, SpinLockable}
};

#[cfg(feature = "limine")]
use limine::memory_map;

static VIRTUAL_ADDRESS_ALLOCATOR: SpinLock<VirtualAddressAllocator> =
    SpinLock::new(VirtualAddressAllocator::uninit());

struct VirtualAddressAllocator(IdHander);

impl VirtualAddressAllocator {
    fn new(start_addr: VirtAddr) -> Self {
        // The minimal memory range we demand
        const MIN_MEM_SPAN: usize = 8 * 0x1000 * 0x1000 * 0x1000 * 0x1000; // 8TB

        // Making sure address is page aligned
        sanity_assert!(start_addr.0 % BASIC_PAGE_SIZE == 0);

        // Make sure we have enough virtual memory space
        assert!(
            HHDM_OFFSET.get() - start_addr.0 >= MIN_MEM_SPAN,
            "Cannot find enough virtual memory space"
        );

        log_info!("VAA initialized with start address of {:?}", start_addr);

        {
            let start_id = Id(start_addr.0 / BASIC_PAGE_SIZE);
            Self(IdHander::new_starting_from(start_id, Id::MAX_ID))
        }
    }

    #[inline]
    const fn uninit() -> Self {
        Self(IdHander::new(Id(0)))
    }

    #[inline]
    fn handout(&mut self, count: usize) -> VirtAddr {
        let page_id = unsafe { self.0.handout_and_skip(count) };

        VirtAddr(page_id.0 * BASIC_PAGE_SIZE)
    }
}

#[cfg(feature = "limine")]
pub fn init_from_limine(mem_map: &[&memory_map::Entry]) {
    // Get the last entry in the memory map
    let last_entry = mem_map.last().unwrap();
    let addr = VirtAddr(last_entry.base as usize + last_entry.length as usize);

    let mut vaa = VIRTUAL_ADDRESS_ALLOCATOR.lock();
    *vaa = VirtualAddressAllocator::new(addr);
}

/// Map the given physical address to the given virtual address
///
/// NOTE: This doesn't allocate a page from the PMM, it just maps the given physical address to
/// some virtual address.
/// If you want to allocate a page, use `allocate_pages` instead.
///
/// This function is unsafe for 2 reasons:
/// 1. The mapped physical page may not be valid, thus accessing is UB
pub unsafe fn map_page_to(phys_addr: PhysAddr, virt_addr: VirtAddr, flags: usize) {
    assert!(
        virt_addr.0 % BASIC_PAGE_SIZE == 0,
        "Virtual address wanted to map isn't page aligned"
    );

    let pml = paging::get_pml();
    unsafe {
        pml.map(virt_addr, phys_addr, PageSize::Size4KB, flags);
    }
}

/// Map the given physical address to some virtual address
///
/// NOTE: This doesn't allocate a page from the PMM, it just maps the given physical address to
/// some virtual address.
/// If you want to allocate a page, use `allocate_pages` instead.
pub unsafe fn map_page(phys_addr: PhysAddr, flags: usize) -> VirtAddr {
    let virt_addr = {
        let mut vaa = VIRTUAL_ADDRESS_ALLOCATOR.lock();
        vaa.handout(1)
    };

    let pml = paging::get_pml();
    unsafe {
        pml.map(virt_addr, phys_addr, PageSize::Size4KB, flags);
    }

    virt_addr
}

/// Allocate `count` virtually contiguous block of 4KB pages
///
/// NOTE: This function might use 2MB or 1GB pages if the allocation is large enough for it, OR if
/// it takes up less memory
#[must_use = "Not freeing the pages will cause a memory leak"]
pub fn allocate_pages(count: usize, flags: usize) -> VirtAddr {
    let virt_addr = {
        let mut vaa = VIRTUAL_ADDRESS_ALLOCATOR.lock();
        vaa.handout(count)
    };

    // TODO: Support multiple page sizes

    {
        let pml = paging::get_pml();
        pml.map_allocate(virt_addr, count, PageSize::Size4KB, flags);
    }

    virt_addr
}

/// Free a virtually contiguous block of 4KB pages
pub unsafe fn free_pages(base_addr: VirtAddr, count: usize) {
    assert!(
        base_addr.0 % BASIC_PAGE_SIZE == 0,
        "Base address wanted to free isn't page aligned"
    );

    let pml = paging::get_pml();
    unsafe {
        pml.unmap(base_addr, count, PageSize::Size4KB);
    }
}

pub unsafe fn unmap_page(base_addr: VirtAddr) {
    assert!(
        base_addr.0 % BASIC_PAGE_SIZE == 0,
        "Base address wanted to unmap isn't page aligned"
    );

    let pml = paging::get_pml();
    unsafe {
        pml.unmap(base_addr, 1, PageSize::Size4KB);
    }
}

/// Returns the physical address mapped to the given virtual address.
///
/// If the virtual address isn't mapped, `None` is returned
pub fn translate(base_addr: VirtAddr, page_size: PageSize) -> Option<PhysAddr> {
    let pml = paging::get_pml();

    // XXX: This might cause problem when using 2MB or 1GB pages
    let offset = base_addr.0 % page_size.size();

    pml.translate(base_addr - offset)
        .map(|phys_addr| PhysAddr(phys_addr.0 + offset))
}

impl SpinLockable for VirtualAddressAllocator {}
