//! Everything specific to the `x86_64` architecture

use core::arch::x86_64::__cpuid_count;

#[cfg(feature = "limine")]
use limine::memory_map;

use logger::*;
use paging::get_pml;
use utils::mem::VirtAddr;
use utils::mem::PhysAddr;

use crate::paging::Flags;
use crate::paging::PageSize;
use crate::paging::PagingError;
use crate::Arch;

use interrupts::Idt;
use utils::collections::fast_lazy_static::FastLazyStatic;

#[macro_use]
pub mod cpu;
pub mod apic;
pub mod event;
pub mod gdt;
pub mod interrupts;
pub mod paging;

/// A static variable to store the CPU vendor we are running on
pub static CPU_VENDOR: FastLazyStatic<CpuVendor> = FastLazyStatic::new(CpuVendor::Invalid);

#[derive(Debug, Clone, Copy, PartialEq)]
/// The `x86_64` CPU vendors Funderberker supports
pub enum CpuVendor {
    /// We're running on an AMD CPU
    Amd,
    /// We're running on an AMD CPU
    Intel,
    /// Invalid vendor. This is the default start value
    Invalid,
}

/// a ZST to implement the Arch trait on
pub struct X86_64;

/// Pointer to some descriptor table (IDTR, GDTR, etc)
#[repr(C, packed)]
#[derive(Debug, Default)]
pub struct DescriptorTablePtr {
    limit: u16,
    base: u64,
}

impl Arch for X86_64 {
    const BASIC_PAGE_SIZE: PageSize<Self> = PageSize::<Self>::size_4kb(); // 4KB page size

    unsafe fn early_boot_init() {
        // Make sure no pesky interrupt interrupt us
        Idt::init();

        find_cpu_vendor();
    }

    #[cfg(feature = "limine")]
    unsafe fn init_paging_from_limine(
        mem_map: &[&memory_map::Entry],
        kernel_virt: VirtAddr,
        kernel_phys: PhysAddr,
    ) {
        use paging::init_from_limine;

        unsafe {
            init_from_limine(mem_map, kernel_virt, kernel_phys);
        }
    }

    unsafe fn map_page_to(
            phys_addr: PhysAddr,
            virt_addr: VirtAddr,
            flags: Flags<Self>,
            page_size: PageSize<Self>,
        ) -> Result<(), PagingError> {
        let pml = get_pml();
        unsafe {
            pml.map(virt_addr, phys_addr, page_size, flags)
        }
    }

    unsafe fn unmap_page(virt_addr: VirtAddr, page_size: PageSize<Self>) -> Result<(), PagingError> {
        let pml = get_pml();
        // TODO: Change this to unmap_page
        unsafe {
            pml.unmap(virt_addr, 1, page_size)
        }
    }

    fn translate(virt_addr: VirtAddr) -> Option<PhysAddr> {
        let pml = get_pml();

        pml.translate(virt_addr)
    }
}

// TODO: Possibly remove these asserts here? Could slow things down

#[inline]
fn find_cpu_vendor() {
    type CpuidVendorString = (u32, u32, u32);

    // The strings (broken down into parts) we should compare to to find out the vendor.
    //
    // The order is EBX:EDX:ECX
    const INTEL_STRING: CpuidVendorString = (
        u32::from_le_bytes(*b"Genu"),
        u32::from_le_bytes(*b"ineI"),
        u32::from_le_bytes(*b"ntel"),
    );
    const AMD_STRING: CpuidVendorString = (
        u32::from_le_bytes(*b"Auth"),
        u32::from_le_bytes(*b"enti"),
        u32::from_le_bytes(*b"cAMD"),
    );

    // Making sure we're not executing this for nothing
    assert!(
        CPU_VENDOR.get() == CpuVendor::Invalid,
        "CPU vendor is already set. Did you forget you called `find_cpu_vendor`?",
    );

    let string = unsafe {
        let res = __cpuid_count(0, 0);
        (res.ebx, res.edx, res.ecx)
    };

    unsafe {
        CPU_VENDOR.set(match string {
            INTEL_STRING => CpuVendor::Intel,
            AMD_STRING => CpuVendor::Amd,
            _ => panic!("Invalid CPU vendor found"),
        });
    };

    log_info!("CPU Vendor found: `{:?}`", CPU_VENDOR.get());
}

impl From<DescriptorTablePtr> for VirtAddr {
    fn from(value: DescriptorTablePtr) -> Self {
        // SAFETY: The value stored here should be the linear address, so we just put it into
        // `VirtAddr`
        Self(value.base as usize)
    }
}
