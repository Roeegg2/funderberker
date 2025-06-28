#![no_std]
#![no_main]
#![feature(pointer_is_aligned_to)]
// TODO: Remove this once the modular_bitfield errors are taken care of
#![allow(dead_code)]
// TODO: Remove this once you fix the `as` conversion warnings
#![allow(clippy::cast_possible_truncation)]

// TODO: Some boot sanity checks to make sure basic features that are expected are available on
// this CPU.

use core::arch::asm;
use slab::heap::Heap;

mod acpi;
mod boot;

/// The global instance of the kernel heap allocator
#[global_allocator]
static HEAP: Heap = Heap::new();

fn funderberker_start() -> ! {
    logger::info!("Funderberker kernel started!");
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

#[panic_handler]
pub fn panic_handler(info: &core::panic::PanicInfo) -> ! {
    logger::err!("{}", info);

    hcf();
}
