//! Interface and driver for the IO APIC

use super::DeliveryMode;
use crate::arch::x86_64::paging::{Entry, PageSize, PageTable};
use crate::mem::mmio::MmioCell;
use crate::mem::PhysAddr;
use alloc::vec::Vec;
use modular_bitfield::prelude::*;

static mut IO_APICS: Vec<IoApic> = Vec::new();

/// Struct representing the IO APIC, containing everything needed to interact with it
#[derive(Debug)]
pub struct IoApic {
    /// The select register. The index of the register is written here in order for it to be
    /// accessible in the `win` register
    io_sel: MmioCell<u32>,
    /// The window register. This is where the data is read from and written to
    io_win: MmioCell<u32>,
    /// The base of the global system interrupts (GSIs) that this IO APIC is responsible for
    gsi_base: u32,
}

struct IoApicReg;


/// The IO APIC's redirection table entry, which configure the behaviour and mapping of the
/// external interrupts
#[bitfield]
#[derive(Debug, Clone, Copy)]
#[repr(u64)]
struct RedirectionEntry {
    /// The vector to be used for this interrupt
    vector: B8,
    /// The delivery mode of the interrupt
    delivery_mode: B3,
    /// The destination mode of the interrupt
    destination_mode: B1,
    /// The delivery status of the interrupt
    delivery_status: B1,
    /// The pin polarity of the interrupt
    pin_polarity: B1,
    /// The remote IRR of the interrupt
    remote_irr: B1,
    /// The trigger mode of the interrupt
    trigger_mode: B1,
    /// The mask of the interrupt
    mask: B1,
    _reserved: B39,
    /// The destination of the interrupt
    destination: B8,
}

impl IoApicReg {
    /// The index of the ID register
    const APIC_ID: u32 = 0x0;
    /// The index of the version register
    const APIC_VERSION: u32 = 0x1;
    /// The index of the arbitration register
    const APIC_ARBITRATION: u32 = 0x2;
    /// The base index of the redirection table registers
    const APIC_REDIRACTION_TABLE_BASE: u32 = 0x10;

    /// Convert a GSI to the corresponding redirection table index
    #[inline]
    const fn red_tbl_index(irq_index: u32) -> u32 {
        (irq_index * 2) as u32 + Self::APIC_REDIRACTION_TABLE_BASE as u32
    }
}

impl IoApic {
    /// The offset that needs to be added from the `win` MMIO register address to get `sel` MMIO
    /// address
    const OFFSET_FROM_SEL_TO_WIN: usize = 0x10;

    /// Creates a new IO APIC
    unsafe fn new(base: *mut u32, gsi_base: u32) -> Self {
        let io_sel = MmioCell::new(base);
        let io_win = MmioCell::new(unsafe {base.byte_add(Self::OFFSET_FROM_SEL_TO_WIN)});
        IoApic {
            io_sel,
            io_win,
            gsi_base,
        }
    }
}

#[allow(dead_code)]
impl RedirectionEntry {
    /// Get the low 32 bits of the entry
    #[inline]
    fn get_low(self) -> u32 {
        let value: u64 = self.into();
        (value & 0xffff_ffff) as u32
    }

    /// Get the high 32 bits of the entry
    #[inline]
    fn get_high(self) -> u32 {
        let value: u64 = self.into();
        ((value >> 32) & 0xffff_ffff) as u32
    }
}

/// Adds an IO APIC to the global list of IO APICs
pub unsafe fn add(phys_addr: PhysAddr, gsi_base: u32) {
    // XXX: This might fuck things up very badly, since we're mapping without letting the
    // allocator know
    PageTable::map_page_specific(
        phys_addr.add_hhdm_offset(),
        phys_addr,
        Entry::FLAG_P | Entry::FLAG_RW | Entry::FLAG_PCD,
        PageSize::Size4KB,
    )
    .unwrap();

    unsafe {
        #[allow(static_mut_refs)]
        IO_APICS.push(IoApic::new(phys_addr.add_hhdm_offset().into(), gsi_base))
    };
}

/// Overrides the identity mapping of a specific IRQ in the system
#[inline]
pub unsafe fn override_irq(
    irq_source: u8,
    gsi: u32,
    flags: u16,
    delivery_mode: DeliveryMode,
) -> Result<(), ()> {
    unsafe {
        #[allow(static_mut_refs)]
        IO_APICS
            .iter()
            .find(|&io_apic| io_apic.gsi_base <= gsi)
            .map(|io_apic| {
                let offset = IoApicReg::red_tbl_index(gsi - io_apic.gsi_base);

                let mut entry: RedirectionEntry = {
                    io_apic.io_sel.write(offset);
                    let mut raw: u64 = io_apic.io_win.read() as u64;
                    io_apic.io_sel.write(offset + 1);
                    raw |= (io_apic.io_win.read() as u64) << 32;

                    raw.into()
                };

                entry.set_vector(irq_source);
                entry.set_pin_polarity(((flags & 2) >> 1) as u8);
                entry.set_trigger_mode(((flags & 8) >> 3) as u8);
                entry.set_delivery_mode(delivery_mode as u8);

                io_apic.io_sel.write(offset);
                io_apic.io_win.write(entry.get_low());
                io_apic.io_sel.write(offset + 1);
                io_apic.io_win.write(entry.get_high());
            });
    }

    Ok(())
}
