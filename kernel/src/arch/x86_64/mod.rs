//! Everything specific to the `x86_64` architecture

use core::arch::x86_64::__cpuid_count;

use super::Architecture;
use interrupts::Idt;

pub mod hav;
#[macro_use]
pub mod cpu;
pub mod apic;
pub mod event;
pub mod interrupts;
pub mod paging;

/// A static variable to store the CPU vendor we are running on
static mut CPU_VENDOR: CpuVendor = CpuVendor::Invalid;

#[derive(Debug, Clone, Copy, PartialEq)]
/// The x86_64 CPU vendors Funderberker supports
pub enum CpuVendor {
    /// We're running on an AMD CPU
    Amd,
    /// We're running on an AMD CPU
    Intel,
    /// Invalid vendor. This is the default start value
    Invalid,
}

/// a ZST to implement the Arch trait on
pub(super) struct X86_64;

/// Pointer to some descriptor table (IDTR, GDTR, etc)
#[repr(C, packed)]
#[derive(Debug)]
pub(super) struct DescriptorTablePtr {
    limit: u16,
    base: u64,
}

impl Architecture for X86_64 {
    unsafe fn init() {
        // Make sure no pesky interrupt interrupt us
        Idt::init();

        find_cpu_vendor();
    }
}

// TODO: Possibly remove these asserts here? Could slow things down

#[inline]
fn find_cpu_vendor() {
    type CpuidVendorString = (u32, u32, u32);

    // Making sure we're not executing this for nothing
    unsafe {
        assert!(
            CPU_VENDOR == CpuVendor::Invalid,
            "CPU vendor is already set. Did you forget you called `find_cpu_vendor`?",
        );
    }

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

    let string = unsafe {
        let res = __cpuid_count(0, 0);
        (res.ebx, res.edx, res.ecx)
    };

    unsafe {
        CPU_VENDOR = match string {
            INTEL_STRING => CpuVendor::Intel,
            AMD_STRING => CpuVendor::Amd,
            _ => panic!("Invalid CPU vendor found"),
        };
    };

    log_info!("CPU Vendor found: `{:?}`", get_cpu_vendor());
}

#[inline]
fn get_cpu_vendor() -> CpuVendor {
    unsafe {
        assert!(
            CPU_VENDOR != CpuVendor::Invalid,
            "CPU vendor is not set. Did you forget to call `find_cpu_vendor`?",
        );
        CPU_VENDOR
    }
}
