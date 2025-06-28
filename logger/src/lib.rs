//! Simple module to provide logging & printing utils

#![no_std]
// TODO: Remove this once you fix the `as` conversion warnings
#![allow(clippy::cast_possible_truncation)]

// #![cfg(not(any(feature = "framebuffer", feature = "serial")))]
// compile_error!("At least one of the 'framebuffer' or 'serial' features must be enabled for the logger module.");

use core::fmt::{self, Write};
#[cfg(feature = "framebuffer")]
pub mod framebuffer;
#[cfg(feature = "serial")]
pub mod serial;

/// Empty struct to implement 'Write' on
pub struct Writer;

/// A macro to print to the serial port or framebuffer with a newline
#[macro_export]
macro_rules! println {
    ($($arg:tt)*) => {{
        let _ = core::fmt::Write::write_fmt(&mut $crate::Writer, format_args!("{}\n", format_args!($($arg)*)));
    }}
}

/// A macro to print a warning to the serial port or framebuffer
#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        $crate::println!("-> INFO: {}", format_args!($($arg)*));
    }
}

/// A macro to print an error to the serial port or framebuffer
#[macro_export]
macro_rules! err {
    ($($arg:tt)*) => {
        $crate::println!("-> ERROR: {}", format_args!($($arg)*));
    }
}

/// A macro to print a warning to the serial port or framebuffer
#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {
        $crate::println!("-> WARNING: {}", format_args!($($arg)*));
    }
}

/// A macro to print a debug message to the serial port or framebuffer
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        #[cfg(debug_assertions)]
        $crate::println!("-> DEBUG: {}", format_args!($($arg)*));
    }
}

impl Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            #[cfg(feature = "serial")]
            #[allow(static_mut_refs)]
            unsafe {
                serial::SERIAL_WRITER.write_byte_all(byte);
            };
            #[cfg(feature = "framebuffer")]
            #[allow(static_mut_refs)]
            unsafe {
                framebuffer::FRAMEBUFFER_WRITER.draw_char(byte).unwrap();
            };
        }

        Ok(())
    }
}
