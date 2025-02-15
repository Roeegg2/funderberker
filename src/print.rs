/// Empty struct to implement 'core::fmt::Write' on
use crate::arch::x86_64::serial;

pub struct SerialWriter;

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {{
        let _ = core::fmt::Write::write_fmt(&mut $crate::print::SerialWriter, format_args!($($arg)*));
    }}
}

#[macro_export]
macro_rules! println {
    ($($arg:tt)*) => {{
        let _ = core::fmt::Write::write_fmt(&mut $crate::print::SerialWriter, format_args!("{}\n", format_args!($($arg)*)));
    }}
}

#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {{
        println!(" -> LOG: {}", $($arg)*);
    }}
}

impl core::fmt::Write for SerialWriter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        // TODO: Implement a global state for all ports, and query ports from there
        // TODO: Add a mechanism to make sure we called port.init()
        let port = serial::SerialPort::Comm1;
        for byte in s.bytes() {
            port.write_byte(byte);
        }
        Ok(())
    }
}
