//! Simple module to provide logging & printing utils

#![no_std]
// TODO: Remove this once you fix the `as` conversion warnings
#![allow(clippy::cast_possible_truncation)]

use core::fmt::Write;
#[cfg(feature = "limine")]
use limine::framebuffer::Framebuffer;
#[cfg(feature = "framebuffer")]
mod framebuffer;
#[cfg(feature = "serial")]
mod serial;

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

impl Writer {
    #[cfg(feature = "limine")]
    pub fn init_from_limine(fb: Option<&Framebuffer<'static>>) {
        #[cfg(feature = "serial")]
        {
            #[allow(static_mut_refs)]
            unsafe {
                serial::SERIAL_WRITER.init();
            }
        }

        #[cfg(feature = "framebuffer")]
        {
            if let Some(fb) = fb {
                #[allow(static_mut_refs)]
                unsafe {
                    framebuffer::FRAMEBUFFER_WRITER.init_from_limine(fb);
                }
            } else {
                panic!("Framebuffer is not available, but framebuffer feature is enabled.");
            }
        }
    }
}

impl Write for Writer {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        #[cfg(feature = "serial")]
        for byte in s.bytes() {
            #[allow(static_mut_refs)]
            unsafe {
                serial::SERIAL_WRITER.write_byte_all(byte);
            };
        }
        #[cfg(feature = "framebuffer")]
        for byte in s.as_bytes() {
            #[allow(static_mut_refs)]
            unsafe {
                framebuffer::FRAMEBUFFER_WRITER.draw_char(*byte).unwrap();
            };
        }

        Ok(())
    }
}
