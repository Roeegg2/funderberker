//! This module contains implementations of drivers for various timers.

#[cfg(all(target_arch = "x86_64", feature = "hpet"))]
pub mod hpet;
#[cfg(all(target_arch = "x86_64", feature = "pit"))]
pub mod pit;
