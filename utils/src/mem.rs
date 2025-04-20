//! Mem related usefull wrappers and utility functions

/// Wrapper to memset some region of memory to some value
pub unsafe fn memset(ptr: *mut u8, value: u8, len: usize) {
    unsafe {
        for i in 0..len {
            core::ptr::write_volatile(ptr.add(i), value);
        }
    };
}

#[macro_export]
macro_rules! sanity_assert {
    ($cond:expr) => {
        debug_assert!($cond, "Sanity check failed!");
    };
}

#[macro_export]
macro_rules! sanity_assert_eq {
    ($left:expr, $right:expr) => {
        debug_assert_eq!($left, $right, "Sanity check failed!");
    };
}

#[macro_export]
macro_rules! sanity_assert_ne {
    ($left:expr, $right:expr) => {
        debug_assert_ne!($left, $right, "Sanity check failed!");
    };
}
