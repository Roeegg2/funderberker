//! Simple module to provide logging & printing utils

#[cfg(all(feature = "serial", feature = "gop"))]
compile_error!("Both 'serial' and UEFI 'GOP' logging options are enabled. Please choose only one");

/// Empty struct to implement 'core::fmt::Write' on
#[cfg(feature = "serial")]
mod serial;

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
        println!(" -> {}", $($arg)*);
    }}
}

#[macro_export]
macro_rules! dbg {
    ($($arg:tt)*) => {{
        #[cfg(debug_assertions)]
        println!(" -> (DEBUG) {}", $($arg)*);
    }}
}

#[cfg(feature = "serial")]
impl core::fmt::Write for Writer {
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

pub unsafe fn init() {
    #[cfg(feature = "serial")]
    {
        unsafe { serial::SerialPort::Comm1.init().unwrap() };
        log!("initilized serial port COMM1 successfully!");
    }
}
