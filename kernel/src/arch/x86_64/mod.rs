//! Everything specific to x86_64 arch

use interrupts::Idt;

#[macro_use]
pub mod cpu;
pub mod apic;
mod interrupts;
pub mod paging;

/// Pointer to some descriptor table (IDTR, GDTR, etc)
#[repr(C, packed)]
#[derive(Debug)]
pub(super) struct DescriptorTablePtr {
    limit: u16,
    base: u64,
}

/// Initilize everything arch related
#[inline(always)]
pub(super) unsafe fn init() {
    unsafe {
        // make sure no pesky interrupt interrupt us
        cpu::cli();
        Idt::init();
        // now pesky interrupts can interrupt us
        cpu::sti();
    };
}
