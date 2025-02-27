#![no_std]
#![no_main]

#![feature(let_chains)]

mod boot;
#[macro_use]
#[cfg(any(feature = "serial", feature = "framebuffer"))]
mod print;
mod arch;

pub fn funderberker_main() {
    unsafe { arch::init() };
    log!("Starting Funderberker operation...");
}
