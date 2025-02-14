#![feature(naked_functions)]
#![no_std]
#![no_main]

#[macro_use]
mod print;
mod acpi;
mod arch;
mod uefi;

fn funderberker_main(
    mem_map: &mut uefi::MemoryDescriptor,
    config_tables: &mut uefi::ConfigurationTable,
    num_of_config_tables: usize,
) {
    log!("starting Funderberker main operation...");

    arch::x86_64::load_idt_and_gdt();

    //acpi::parse_acpi(config_tables, num_of_config_tables);
}
