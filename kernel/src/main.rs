#![no_std]
#![no_main]
#![feature(let_chains)]
#![feature(nonnull_provenance)]
#![feature(allocator_api)]
#![feature(pointer_is_aligned_to)]
#![feature(box_vec_non_null)]
#![feature(non_null_from_ref)]
#![feature(generic_const_exprs)]

mod boot;
#[macro_use]
#[cfg(any(feature = "serial", feature = "framebuffer"))]
mod print;
mod arch;
mod mem;
#[cfg(feature = "test")]
mod test;

#[cfg(not(feature = "test"))]
/// After all early booting stuff have been sorted out, it's time to start Funderberker main operation!
pub fn funderberker_main() {
    log!("Funderberker operation completed!");
}
