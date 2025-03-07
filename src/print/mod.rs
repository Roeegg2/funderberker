//! Simple module to provide logging & printing utils

#[cfg(feature = "framebuffer")]
pub mod framebuffer;
#[cfg(feature = "serial")]
pub mod serial;

/// Empty struct to implement 'core::fmt::Write' on
pub struct Writer;

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {{
        let _ = core::fmt::Write::write_fmt(&mut $crate::print::Writer, format_args!($($arg)*));
    }}
}

#[macro_export]
macro_rules! println {
    ($($arg:tt)*) => {{
        let _ = core::fmt::Write::write_fmt(&mut $crate::print::Writer, format_args!("{}\n", format_args!($($arg)*)));
    }}
}

#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {{
        println!("-> {}", $($arg)*);
    }}
}

impl core::fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        #[cfg(feature = "serial")]
        for byte in s.bytes() {
            #[allow(static_mut_refs)]
            unsafe {
                serial::SERIAL_WRITER.write_byte_all(byte)
            };
        }
        #[cfg(feature = "framebuffer")]
        for byte in s.as_bytes() {
            #[allow(static_mut_refs)]
            unsafe {
                framebuffer::FRAMEBUFFER_WRITER.draw_char(*byte).unwrap()
            };
        }

        Ok(())
    }
}
