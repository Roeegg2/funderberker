//! Simple module to provide logging & printing utils

#[cfg(feature = "framebuffer")]
use crate::dev::framebuffer;
#[cfg(feature = "serial")]
use crate::dev::serial;

/// Empty struct to implement 'Write' on
pub struct Writer;

/// A macro to print to the serial port or framebuffer
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {{
        let _ = core::fmt::Write::write_fmt(&mut $crate::print::Writer, format_args!($($arg)*));
    }}
}

/// A macro to print to the serial port or framebuffer with a newline
#[macro_export]
macro_rules! println {
    ($($arg:tt)*) => {{
        let _ = core::fmt::Write::write_fmt(&mut $crate::print::Writer, format_args!("{}\n", format_args!($($arg)*)));
    }}
}

/// A macro to print a warning to the serial port or framebuffer
#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        println!("-> INFO: {}", format_args!($($arg)*));
    }
}

/// A macro to print an error to the serial port or framebuffer
#[macro_export]
macro_rules! log_err {
    ($($arg:tt)*) => {
        println!("-> ERROR: {}", format_args!($($arg)*));
    }
}

/// A macro to print a warning to the serial port or framebuffer
#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        println!("-> WARNING: {}", format_args!($($arg)*));
    }
}

impl core::fmt::Write for Writer {
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
