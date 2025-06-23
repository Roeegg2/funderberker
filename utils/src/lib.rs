#![cfg_attr(not(test), no_std)]
#![feature(let_chains)]
#![feature(box_vec_non_null)]
#![feature(sync_unsafe_cell)]

pub mod collections;
pub mod mem;
pub mod sync;

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

/// For assertions that are so obvious, they should never fail in production code.
/// These assertions are only checked in debug builds.
#[macro_export]
macro_rules! sanity_assert {
    ($cond:expr) => {
        debug_assert!($cond, "Sanity check failed!");
    };
    ($cond:expr, $msg:expr) => {
        debug_assert!($cond, "Sanity check failed: {}", $msg);
    };
    ($cond:expr, $fmt:expr, $($arg:tt)*) => {
        debug_assert!($cond, "Sanity check failed: {}", format!($fmt, $($arg)*));
    };
}

#[macro_export]
macro_rules! sanity_assert_eq {
    ($left:expr, $right:expr) => {
        debug_assert_eq!($left, $right, "Sanity check failed!");
    };
    ($left:expr, $right:expr, $msg:expr) => {
        debug_assert_eq!($left, $right, "Sanity check failed: {}", $msg);
    };
    ($left:expr, $right:expr, $fmt:expr, $($arg:tt)*) => {
        debug_assert_eq!($left, $right, "Sanity check failed: {}", format!($fmt, $($arg)*));
    };
}

#[macro_export]
macro_rules! sanity_assert_ne {
    ($left:expr, $right:expr) => {
        debug_assert_ne!($left, $right, "Sanity check failed!");
    };
    ($left:expr, $right:expr, $msg:expr) => {
        debug_assert_ne!($left, $right, "Sanity check failed: {}", $msg);
    };
    ($left:expr, $right:expr, $fmt:expr, $($arg:tt)*) => {
        debug_assert_ne!($left, $right, "Sanity check failed: {}", format!($fmt, $($arg)*));
    };
}
