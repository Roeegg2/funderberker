#![feature(naked_functions)]
#![no_std]
#![no_main]

#[macro_use]
mod print;
mod acpi;
mod arch;
mod mem;
mod uefi;
mod util;

fn funderberker_main(
    mem_map: *mut uefi::MemoryDescriptor,
    mem_map_size: usize,
    mem_descr_size: usize,
    config_tables: &mut uefi::ConfigurationTable,
    num_of_config_tables: usize,
) {
    log!("starting Funderberker main operation...");

    arch::x86_64::load_idt_and_gdt();

    mem::init(mem_map, mem_map_size as u64, mem_descr_size as u64);
    //acpi::parse_acpi(config_tables, num_of_config_tables);
}
