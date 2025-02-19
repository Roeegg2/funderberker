#![cfg(target_arch = "x86_64")]

mod cpu;
mod gdt;
mod interrupts;
pub mod paging;
pub mod serial;

#[repr(C, packed)]
#[derive(Debug, PartialEq)]
pub struct DescriptorTablePtr {
    limit: u16,
    base: u64,
}

pub fn load_idt_and_gdt() {
    unsafe { cpu::cli() }; // make sure no pesky interrupts interrupt us

    gdt::load_gdt();
    log!("loaded GDT successfully");

    interrupts::load_idt();
    log!("loaded IDT successfully");

    unsafe { cpu::sti() }; // now pesky interrupts are free to interrupt us

    paging::setup_paging();
    log!("paging setup successfully");

}
