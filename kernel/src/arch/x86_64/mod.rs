//! Everything specific to x86_64 arch

use interrupts::Idt;

use super::Architecture;

#[macro_use]
pub mod cpu;
pub mod apic;
mod interrupts;
#[cfg(feature = "mp")]
mod mp;
pub mod paging;

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
        unsafe {
            // Make sure no pesky interrupt interrupt us
            cpu::cli();
            Idt::init();
        };
    }

    /// Initialize the other cores on an MP system
    #[cfg(feature = "mp")]
    #[inline]
    unsafe fn init_cores() {
        // mp::init_cores();
        // make sure we are on an MP system, otherwise return
        //
    }
}
