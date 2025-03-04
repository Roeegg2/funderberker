///! Serial port driver for logging stuff
use crate::arch::x86_64::cpu;

pub static mut SERIAL_WRITER: SerialWriter = SerialWriter {
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
pub enum SerialError {
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

impl SerialPort {
    /// Initilize serial port. MUST call this before using any serial port
    unsafe fn init(self) -> Result<(), SerialError> {
        unsafe {
            cpu::outb(self as u16, 1, 0x00); // Disable all interrupts
            cpu::outb(self as u16, 3, 0x80); // Enable DLAB (set baud rate divisor)
            cpu::outb(self as u16, 0, 0x03); // Set divisor to 3 (lo byte) 38400 baud
            cpu::outb(self as u16, 1, 0x00); //                  (hi byte)
            cpu::outb(self as u16, 3, 0x03); // 8 bits, no parity, one stop bit
            cpu::outb(self as u16, 2, 0xC7); // Enable FIFO, clear them, with 14-byte threshold
            cpu::outb(self as u16, 4, 0x0B); // IRQs enabled, RTS/DSR set
            cpu::outb(self as u16, 4, 0x1E); // Set in loopback mode, test the serial chip
            cpu::outb(self as u16, 0, 0xAE); // Test serial chip (send byte 0xAE and check if serial returns same byte)
        }

        if unsafe { cpu::inb(self as u16, 0) } != 0xae {
            return Err(SerialError::FaultySerialPort);
        }

        // If serial port is fine, set it to normal operation mode
        unsafe { cpu::outb(self as u16, 4, 0xf) };

        Ok(())
    }

    /// Write a byte to serial
    pub(super) fn write_byte(self, byte: u8) {
        if byte == b'\n' {
            unsafe { cpu::outb(self as u16, 0, b'\r') };
        }
        unsafe { cpu::outb(self as u16, 0, byte) };
    }
}

pub struct SerialWriter {
    ports: [Option<SerialPort>; 8],
}

impl SerialWriter {
    /// Initilize each of the enabled serial ports. If an error occured, mark them as unwriteable
    #[inline(always)]
    pub fn init(&mut self) -> Result<(), SerialError> {
        for ref mut port_wrapper in self.ports {
            if let Some(port) = port_wrapper
                && unsafe { port.init().is_err() }
            {
                *port_wrapper = None;
            }
        }

        Ok(())
    }

    /// Write a byte to all available serial ports
    pub(super) fn write_byte_all(&self, byte: u8) {
        for port in self.ports {
            if let Some(val) = port {
                val.write_byte(byte);
            }
        }
    }
}
