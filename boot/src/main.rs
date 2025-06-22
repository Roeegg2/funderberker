#![no_std]
#![no_main]

#![feature(pointer_is_aligned_to)]

// TODO: Some boot sanity checks to make sure basic features that are expected are available on
// this CPU.

use logger::*;
use core::panic::PanicInfo;
use core::arch::asm;
use core::format_args;

mod boot;
mod acpi;

fn funderberker_start() -> ! {
    hcf();
}

#[panic_handler]
pub fn rust_panic(info: &PanicInfo) -> ! {
    println!("{}", info);

    hcf();
}

/// Halt the CPU forever
fn hcf() -> ! {
    loop {
        unsafe {
            #[cfg(target_arch = "x86_64")]
            asm!("hlt");
            #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
            asm!("wfi");
        }
    }
}
