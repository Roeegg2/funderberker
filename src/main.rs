#![no_std]
#![no_main]

mod boot;
#[macro_use]
mod print;
mod arch;

pub fn funderberker_main() {
    unsafe {
        print::init();
        arch::init();
    };

    log!("Starting Funderberker operation...");
}
