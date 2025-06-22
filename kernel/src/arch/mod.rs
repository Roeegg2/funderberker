//! (Will be) safe, general arch abstractions so kernel doesn't need to deal with all the nitty gritty

#[cfg(feature = "limine")]
use limine::memory_map;
use utils::mem::VirtAddr;
use utils::mem::PhysAddr;

#[cfg(target_arch = "x86_64")]
pub mod x86_64;

/// The size of the smallest page that can be allocated
pub const BASIC_PAGE_SIZE: usize = 0x1000; // 4KB page size

// /// The size of the allocated stack for each core in `BASIC_PAGE_SIZE` pages
// pub const CORE_STACK_PAGE_COUNT: usize = 64; // 64KB stack for each core

/// A trait that every arch should implement
trait Architecture {
    /// Initilize everything arch related
    ///
    /// SHOULD ONLY BE CALLED ONCE DURING BOOT!
    unsafe fn init();

    /// Initialize the paging system when booting from Limine
    #[cfg(feature = "limine")]
    unsafe fn init_paging_from_limine(
        mem_map: &[&memory_map::Entry],
        kernel_virt: VirtAddr,
        kernel_phys: PhysAddr,
    );
}

/// Wrapper to call the arch specific `init` function
#[inline]
pub unsafe fn init() {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        x86_64::X86_64::init();
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
