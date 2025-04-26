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

use dev::timer::{self, Timer};

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
pub fn funderberker_main(rsdp: *const ()) {
    #[cfg(test)]
    test_main();

    unsafe { crate::acpi::init(rsdp).expect("Failed to initialize ACPI") };

    timer::init_secondary_timer();

    // let mut pit = dev::timer::pit::PIT.lock();
    // pit.start(
    //     core::time::Duration::from_millis(1),
    //     dev::timer::pit::OperatingMode::InterruptOnTerminalCount,
    // )
    // .unwrap();

    // let mut timer = dev::timer::hpet::HpetTimer::new().unwrap();
    // timer.start(core::time::Duration::from_secs(1), dev::timer::hpet::TimerMode::Periodic).unwrap();
    // timer.start(core::time::Duration::from_millis(1), dev::timer::pit::OperatingMode::_RateGenerator2).unwrap();

    // let mut timer = dev::timer::apic::ApicTimer::new();
    // timer.start(core::time::Duration::from_millis(1), dev::timer::apic::TimerMode::Periodic).unwrap();

    let mut rtc = dev::clock::rtc::RTC.lock();
    rtc.new_periodic_interrupts(dev::cmos::NmiStatus::Enabled);

    println!("Timer started!");

    loop {}

    unsafe {
        crate::arch::init_cores();
    }

    log_info!("Starting Funderberker main operation!");
}
