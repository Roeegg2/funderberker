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

mod boot;
#[macro_use]
#[cfg(any(feature = "serial", feature = "framebuffer"))]
mod print;
mod acpi;
mod arch;
mod dev;
mod mem;
mod sync;
#[cfg(test)]
mod test;

/// After all early booting stuff have been sorted out, it's time to start Funderberker main operation!
pub fn funderberker_main(rsdp: *const ()) {
    #[cfg(test)]
    test_main();

    unsafe { crate::acpi::init(rsdp).expect("Failed to initialize ACPI") };

    unsafe {
        crate::arch::init_cores();
    }

    let time = crate::arch::cycles_since_boot();
    let second_time = crate::arch::cycles_since_boot();
    println!("Time since boot: {} cycles", time);
    println!("Time since boot: {} cycles", second_time);

    log_info!("Starting Funderberker main operation!");
}
