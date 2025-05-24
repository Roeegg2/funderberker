#![cfg_attr(not(test), no_std)]
#![feature(let_chains)]
#![feature(box_vec_non_null)]
#![feature(sync_unsafe_cell)]

pub mod collections;
pub mod mem;

#[cfg(not(test))]
extern crate alloc;

/// Returns the maximum of two values (potentially) at compile time.
///
/// NOTE: This requires the 2 operands to be able to be evaluated at compile time.
#[macro_export]
macro_rules! const_max {
    ($a:expr, $b:expr) => {
        if $a > $b { $a } else { $b }
    };
}

/// Returns the minimum of two values (potentially) at compile time.
///
/// NOTE: This requires the 2 operands to be able to be evaluated at compile time.
#[macro_export]
macro_rules! const_min {
    ($a:expr, $b:expr) => {
        if $a < $b { $a } else { $b }
    };
}

/// Spins until the given condition evaluates to `true`.
#[macro_export]
macro_rules! spin_until {
    ($condition:expr) => {
        loop {
            core::hint::spin_loop();
            if $condition {
                break;
            }
        }
    };
}
