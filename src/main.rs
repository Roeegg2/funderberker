#![no_std]
#![no_main]
#![feature(let_chains)]
#![feature(nonnull_provenance)]

mod boot;
#[macro_use]
#[cfg(any(feature = "serial", feature = "framebuffer"))]
mod print;
mod arch;
mod mem;
pub mod lib;

/// After all early booting stuff have been sorted out, it's time to start Funderberker main operation!
pub fn funderberker_main() {
    log!("Starting Funderberker operation...");
}
