#![no_std]
#![no_main]
#![feature(let_chains)]
#![feature(nonnull_provenance)]
#![feature(allocator_api)]
#![feature(box_vec_non_null)]

mod boot;
#[macro_use]
#[cfg(any(feature = "serial", feature = "framebuffer"))]
mod print;
mod arch;
mod mem;
#[cfg(feature = "test")]
mod test;

use alloc::boxed::Box;

/// After all early booting stuff have been sorted out, it's time to start Funderberker main operation!
pub fn funderberker_main() {
    #[cfg(feature = "test")]
    {
        log!("Starting testing...");
        test::start_testing();
        log!("Testing completed!");
        return;
    }

    log!("Starting Funderberker operation...");

    //{
    //
    //let a = Box::new(5);
    //println!("a = {}", a);
    //}
    //
    //{
    //
    //let b = Box::new([10, 5, 1, 4]);
    //println!("b = {:?}", b);
    //}

    log!("Funderberker operation completed!");

}
