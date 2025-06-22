#![no_std]
#![no_main]
#![feature(let_chains)]
#![feature(allocator_api)]
#![feature(pointer_is_aligned_to)]
#![feature(box_vec_non_null)]
#![feature(custom_test_frameworks)]
#![feature(ptr_as_ref_unchecked)]
#![test_runner(crate::test::test_runner)]
#![reexport_test_harness_main = "test_main"]
#![feature(stmt_expr_attributes)]
#![feature(sync_unsafe_cell)]
#![feature(concat_idents)]
// TODO: Remove these after code is stable!!!
#![allow(clippy::cast_possible_truncation)]
#![allow(unused)]

use core::arch::asm;

mod boot;
#[macro_use]
// #[cfg(any(feature = "serial", feature = "framebuffer"))]
mod print;
mod acpi;
mod arch;
mod virt;
#[cfg(test)]
mod test;

/// After all early booting stuff have been sorted out, it's time to start Funderberker main operation!
pub fn funderberker_main() -> ! {
    let pcie_manager = pcie::PCIE_MANAGER.lock();
    pcie_manager.load_device_drivers();
    virt::start();

    #[cfg(test)]
    test_main();

    log_info!("Starting Funderberker main operation!");

    hcf();
}

