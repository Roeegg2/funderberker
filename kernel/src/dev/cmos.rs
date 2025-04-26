//! Handling of CMOS data

use crate::arch::x86_64::{
    cpu::{inb_8, outb_8},
    interrupts,
};

/// List of available CMOS indices
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum CmosIndex {
    // A bunch of RTC stuff
    /// RTC current second
    Seconds = 0x00,
    /// RTC current minute
    Minutes = 0x02,
    /// RTC current hour
    Hours = 0x04,
    /// RTC current day of week
    DayOfWeek = 0x06,
    /// RTC current day of month
    DayOfMonth = 0x07,
    /// RTC current month
    Month = 0x08,
    /// RTC current year
    Year = 0x09,
    /// RTC current century
    Century = 0x32,

    /// RTC status register A
    StatusA = 0x0A,
    /// RTC status register B
    StatusB = 0x0B,
    /// RTC status register C
    StatusC = 0x0C,

    /// Getting info about the floppy disks connected to the system; Not used by us though
    _FloppyDiskDrive = 0x10,
    // The rest of the CMOS isn't standardized
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum NmiStatus {
    /// NMI is enabled
    Enabled = 0x80,
    /// NMI is disabled
    Disabled = 0x00,
}

#[derive(Debug, Clone, Copy)]
#[repr(u16)]
enum CmosPort {
    /// Port to read from
    Read = 0x70,
    /// Port to write to
    Write = 0x71,
}

/// Read a byte from the CMOS
pub fn read_cmos(index: CmosIndex, nmi_status: NmiStatus) -> u8 {
    interrupts::do_inside_interrupts_disabled_window(|| unsafe {
        outb_8(CmosPort::Read as u16, index as u8 | nmi_status as u8);

        inb_8(CmosPort::Write as u16)
    })
}

/// Write a byte to the CMOS
pub fn write_cmos(index: CmosIndex, value: u8, nmi_status: NmiStatus) {
    interrupts::do_inside_interrupts_disabled_window(|| unsafe {
        outb_8(CmosPort::Read as u16, index as u8 | nmi_status as u8);

        outb_8(CmosPort::Write as u16, value);
    })
}
