#![no_std]
#![no_main]
#![feature(let_chains)]
#![feature(nonnull_provenance)]
#![feature(allocator_api)]
#![feature(pointer_is_aligned_to)]
#![feature(box_vec_non_null)]
#![feature(non_null_from_ref)]
#![feature(custom_test_frameworks)]
#![feature(ptr_as_ref_unchecked)]
#![test_runner(crate::test::test_runner)]
#![reexport_test_harness_main = "test_main"]
#![feature(stmt_expr_attributes)]
#![feature(sync_unsafe_cell)]
#![feature(naked_functions)]

use core::arch::asm;
use dev::timer::{self, apic::ApicTimer};

mod boot;
#[macro_use]
#[cfg(any(feature = "serial", feature = "framebuffer"))]
mod print;
mod acpi;
mod arch;
mod dev;
mod mem;
#[macro_use]
mod sync;
#[cfg(test)]
mod test;

/// After all early booting stuff have been sorted out, it's time to start Funderberker main operation!
pub fn funderberker_main() -> ! {
    #[cfg(test)]
    test_main();

    timer::enable_secondary_timer();

    let _timer = ApicTimer::new();

    unsafe {
        crate::arch::init_cores();
    }

    log_info!("Starting Funderberker main operation!");

    hcf();
}

#[panic_handler]
pub fn rust_panic(info: &core::panic::PanicInfo) -> ! {
    use crate::println;

    println!("{}", info);

    hcf();
}

pub fn hcf() -> ! {
    loop {
        unsafe {
            #[cfg(target_arch = "x86_64")]
            asm!("hlt");
            #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
            asm!("wfi");
        }
    }
}
