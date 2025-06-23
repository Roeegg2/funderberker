//! (Will be) safe, general arch abstractions so kernel doesn't need to deal with all the nitty gritty

#![no_std]

#![feature(sync_unsafe_cell)]

#[cfg(feature = "limine")]
use limine::memory_map;
use paging::{Flags, PageSize, PagingError};
use pmm::PmmAllocator;
use utils::mem::{VirtAddr, PhysAddr};
use vaa::VAA;

#[cfg(target_arch = "x86_64")]
pub mod x86_64;
pub mod paging;
pub mod vaa;

#[cfg(target_arch = "x86_64")]
pub const BASIC_PAGE_SIZE: usize = x86_64::X86_64::BASIC_PAGE_SIZE.size();

/// A trait that every arch should implement
// TODO: Make this internal
pub trait Arch: Sized {
    const BASIC_PAGE_SIZE: PageSize<Self>;
    /// Initilize everything arch related
    ///
    /// SHOULD ONLY BE CALLED ONCE DURING BOOT!
    unsafe fn early_boot_init();

    /// Initialize the paging system when booting from Limine
    #[cfg(feature = "limine")]
    unsafe fn init_paging_from_limine(
        mem_map: &[&memory_map::Entry],
        kernel_virt: VirtAddr,
        kernel_phys: PhysAddr,
    );

    unsafe fn map_page_to(
            phys_addr: PhysAddr,
            virt_addr: VirtAddr,
            flags: Flags<Self>,
            page_size: PageSize<Self>,
        ) -> Result<(), PagingError>;

    unsafe fn unmap_page(virt_addr: VirtAddr, page_size: PageSize<Self>) -> Result<(), PagingError>;

    fn translate(virt_addr: VirtAddr) -> Option<PhysAddr>;
}

/// Wrapper to call the arch specific `init` function
#[inline]
pub unsafe fn early_boot_init() {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        x86_64::X86_64::early_boot_init();
    }
}

#[inline]
#[cfg(feature = "limine")]
pub unsafe fn init_paging_from_limine(
    mem_map: &[&memory_map::Entry],
    kernel_virt: VirtAddr,
    kernel_phys: PhysAddr,
) {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        x86_64::X86_64::init_paging_from_limine(mem_map, kernel_virt, kernel_phys);
    }
}

#[inline]
pub unsafe fn map_page_to<A: Arch>(
    phys_addr: PhysAddr,
    virt_addr: VirtAddr,
    flags: Flags<A>,
    page_size: PageSize<A>,
) -> Result<(), PagingError> {
    unsafe {
        A::map_page_to(phys_addr, virt_addr, flags, page_size)
    }
}

pub unsafe fn map_page<A: Arch>(
    phys_addr: PhysAddr,
    flags: Flags<A>,
    page_size: PageSize<A>) -> Result<VirtAddr, PagingError> {
    let virt_addr = {
        let mut vaa = VAA.lock();
        vaa.handout(1, page_size.alignment())
    };
    
    unsafe {
        A::map_page_to(phys_addr, virt_addr, flags, page_size)?;
    };

    Ok(virt_addr)
}

#[inline]
pub unsafe fn unmap_page<A: Arch>(
    virt_addr: VirtAddr,
    page_size: PageSize<A>,
) -> Result<(), PagingError> {
    unsafe {
        A::unmap_page(virt_addr, page_size)
    }
}

pub fn allocate_pages<A: Arch>(
    count: usize,
    flags: Flags<A>,
    page_size: PageSize<A>,
) -> Result<VirtAddr, PagingError> {
    let base_virt_addr = {
        let mut vaa = VAA.lock();
        vaa.handout(count, page_size.alignment())
    };

    let basic_page_count = page_size.to_default_page_count();
    for i in 0..count {
        let virt_addr = base_virt_addr + (i * page_size.size());
        let phys_addr = pmm::get().allocate(1, basic_page_count).unwrap();

        unsafe {
            A::map_page_to(phys_addr, virt_addr, flags, page_size)?;
        }
    }

    Ok(base_virt_addr)
}


pub unsafe fn free_pages<A: Arch>(
    virt_addr: VirtAddr,
    count: usize,
    page_size: PageSize<A>,
) -> Result<(), PagingError> {
    for i in 0..count {
        let addr = virt_addr + (i * page_size.size());
        let phys_addr = A::translate(virt_addr).ok_or(PagingError::PageNotPresent)?;
        // XXX: need to make sure we uunmap and then free
        unsafe {
            A::unmap_page(addr, page_size)?;

            pmm::get().free(phys_addr, page_size.to_default_page_count())
                .map_err(|_| PagingError::PageNotPresent)?;

        };
    }

    Ok(())
}

pub fn translate<A: Arch>(virt_addr: VirtAddr) -> Option<PhysAddr> {
    A::translate(virt_addr)
}
