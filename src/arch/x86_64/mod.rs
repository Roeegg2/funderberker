//! Everything specific to x86_64 arch

pub mod cpu;
mod interrupts;
pub mod paging;

/// Pointer to some descriptor table (IDTR, GDTR, etc)
#[repr(C, packed)]
#[derive(Debug)]
pub(super) struct DescriptorTablePtr {
    limit: u16,
    base: u64,
}

/// Initilize everything arch related!
pub(super) unsafe fn init() {
    unsafe {
        // make sure no pesky interrupt interrupt us
        cpu::cli();
        interrupts::load_idt();
        // now pesky interrupts can interrupt us
        cpu::sti();
    };
}
