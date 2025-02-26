pub mod cpu;
mod interrupts;

#[repr(C, packed)]
#[derive(Debug)]
pub(super) struct DescriptorTablePtr {
    limit: u16,
    base: u64,
}

pub(super) unsafe fn init() {
    unsafe {
        // make sure no pesky interrupt interrupt us
        cpu::cli();
        interrupts::load_idt();
        // now pesky interrupts can interrupt us
        cpu::sti();
    };
}
