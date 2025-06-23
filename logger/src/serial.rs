//! Simple serial driver for logging purposes

use core::arch::asm;

pub(super) static mut SERIAL_WRITER: SerialWriter = SerialWriter {
    ports: [
        Some(SerialPort::Comm1),
        Some(SerialPort::Comm2),
        Some(SerialPort::Comm3),
        Some(SerialPort::Comm4),
        Some(SerialPort::Comm5),
        Some(SerialPort::Comm6),
        Some(SerialPort::Comm7),
        Some(SerialPort::Comm8),
    ],
};

/// Possible errors serial driver could encounter
#[derive(Debug, Clone, Copy)]
pub(super) enum SerialError {
    /// Serial port isn't available/isn't working
    FaultySerialPort,
}

/// Serial port addresses
#[allow(unused)]
#[derive(Debug, Clone, Copy)]
#[repr(u16)]
enum SerialPort {
    Comm1 = 0x3f8,
    Comm2 = 0x2f8,
    Comm3 = 0x3e8,
    Comm4 = 0x2e8,
    Comm5 = 0x5f8,
    Comm6 = 0x4f8,
    Comm7 = 0x5e8,
    Comm8 = 0x4e8,
}

/// A serial writer that writes to all available serial ports
pub struct SerialWriter {
    ports: [Option<SerialPort>; 8],
}

impl SerialPort {
    /// Initilize serial port. MUST call this before using any serial port
    unsafe fn init(self) -> Result<(), SerialError> {
        unsafe {
            outb_8(self as u16 + 1, 0x00); // Disable all interrupts
            outb_8(self as u16 + 3, 0x80); // Enable DLAB (set baud rate divisor)
            outb_8(self as u16, 0x03); // Set divisor to 3 (lo byte) 38400 baud
            outb_8(self as u16 + 1, 0x00); //                  (hi byte)
            outb_8(self as u16 + 3, 0x03); // 8 bits, no parity, one stop bit
            outb_8(self as u16 + 2, 0xC7); // Enable FIFO, clear them, with 14-byte threshold
            outb_8(self as u16 + 4, 0x0B); // IRQs enabled, RTS/DSR set
            outb_8(self as u16 + 4, 0x1E); // Set in loopback mode, test the serial chip
            outb_8(self as u16, 0xAE); // Test serial chip (send byte 0xAE and check if serial returns same byte)
        }

        if unsafe { inb_8(self as u16) } != 0xae {
            return Err(SerialError::FaultySerialPort);
        }

        // If serial port is fine, set it to normal operation mode
        unsafe { outb_8(self as u16 + 4, 0xf) };

        Ok(())
    }

    /// Write a byte to serial
    fn write_byte(self, byte: u8) {
        if byte == b'\n' {
            unsafe { outb_8(self as u16, b'\r') };
        }
        unsafe { outb_8(self as u16, byte) };
    }
}

impl SerialWriter {
    /// Initilize each of the enabled serial ports. If an error occured, mark them as unwriteable
    #[inline]
    pub fn init(&mut self) {
        for ref mut port_wrapper in self.ports {
            if let Some(port) = port_wrapper
                && unsafe { port.init().is_err() }
            {
                *port_wrapper = None;
            }
        }
    }

    /// Write a byte to all available serial ports
    pub(super) fn write_byte_all(&self, byte: u8) {
        self.ports.iter().filter_map(|port| *port).for_each(|port| {
            port.write_byte(byte);
        });
    }
}

// TODO: Remove these and use a arch lib crate

/// Wrapper for the 'out' instruction, accessing a `u8` port
#[inline]
unsafe fn outb_8(port: u16, value: u8) {
    unsafe {
        asm! (
            "out dx, al",
            in("dx") port,
            in("al") value,
            options(nomem, nostack),
        );
    };
}

/// Wrapper for the 'in' instruction, accessing a `u8` port
#[inline]
unsafe fn inb_8(port: u16) -> u8 {
    let res: u8;
    unsafe {
        asm! (
            "in al, dx",
            out("al") res,
            in("dx") port,
            options(nomem, nostack),
        );
    };

    res
}
