//! Interface and driver for the IO APIC

use super::{DeliveryMode, Destination};
use crate::arch::x86_64::cpu::outb_8;
use crate::arch::x86_64::interrupts;
use crate::arch::x86_64::paging::{Entry, PageSize, PageTable};
use crate::mem::PhysAddr;
use crate::mem::mmio::MmioCell;
use alloc::vec::Vec;
use modular_bitfield::prelude::*;

#[derive(Debug, Copy, Clone)]
pub enum IoApicError {
    InvalidGsi,
}

static mut IRQ_OVERRIDES: Vec<(u8, u32)> = Vec::new();

pub static mut IO_APICS: Vec<IoApic> = Vec::new();

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
    /// The number of GSIs that this IO APIC is responsible for
    gsi_count: u32,
    /// The ID of the IO APIC
    apic_id: u8,
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
    const fn red_tbl_to_index(irq_index: u32) -> u32 {
        (irq_index * 2) as u32 + Self::APIC_REDIRACTION_TABLE_BASE as u32
    }
}

impl IoApic {
    /// The offset that needs to be added from the `win` MMIO register address to get `sel` MMIO
    /// address
    const OFFSET_FROM_SEL_TO_WIN: usize = 0x10;

    /// Mask all of the PICs interrupt, effectively disabling it. 
    /// This is required so the PICs don't interfere with the APIC stack 
    pub unsafe fn mask_off_pic() {
        unsafe {
            outb_8(0x21, 0xff); // ICW1
            outb_8(0xa1, 0xff); // ICW2
        }
    }

    /// Creates a new IO APIC
    unsafe fn new(base: *mut u32, gsi_base: u32, apic_id: u8) -> Self {
        let io_sel = MmioCell::new(base);
        let io_win = MmioCell::new(unsafe { base.byte_add(Self::OFFSET_FROM_SEL_TO_WIN) });
        let mut ret = IoApic {
            io_sel,
            io_win,
            gsi_base,
            gsi_count: 0,
            apic_id,
        };

        let max_gsi = unsafe {
            // Read the version register to get the number of GSIs
            ret.io_sel.write(IoApicReg::APIC_VERSION);
            let version = ret.io_win.read();
            (version >> 16) & 0xff
        };
        utils::sanity_assert!(0 < max_gsi && max_gsi < 256);

        ret.gsi_count = max_gsi as u32 + 1;

        println!("this is the max gsi: {}", ret.gsi_count);

        ret
    }

    #[inline]
    unsafe fn read_redirection_entry(&self, offset: u32) -> RedirectionEntry {
        // TODO: Return error if the offset is invalid
        unsafe {
            self.io_sel.write(offset);
            let mut raw: u64 = self.io_win.read() as u64;
            self.io_sel.write(offset + IoApicReg::red_tbl_to_index(1));
            raw |= (self.io_win.read() as u64) << 32;

            RedirectionEntry::from(raw)
        }
    }

    #[inline]
    unsafe fn write_redirection_entry(&self, offset: u32, entry: RedirectionEntry) {
        // TODO: Return error if the offset is invalid
        unsafe {
            self.io_sel.write(offset);
            self.io_win.write(entry.get_low());
            self.io_sel.write(offset + IoApicReg::red_tbl_to_index(1));
            self.io_win.write(entry.get_high());
        }
    }


    /// Get the GSI base of this IO APIC
    #[inline]
    pub fn gsi_base(&self) -> u32 {
        self.gsi_base
    }

    /// Get the ID of this IO APIC
    #[inline]
    pub fn apic_id(&self) -> u8 {
        self.apic_id
    }

    /// Get the number of GSIs that this IO APIC is responsible for
    #[inline]
    pub fn gsi_count(&self) -> u32 {
        self.gsi_count
    }
}

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
pub unsafe fn add(phys_addr: PhysAddr, gsi_base: u32, apic_id: u8) {
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
        IO_APICS.push(IoApic::new(phys_addr.add_hhdm_offset().into(), gsi_base, apic_id))
    };
}

/// Overrides the identity mapping of a specific IRQ in the system
#[inline]
pub unsafe fn override_irq(
    irq_source: u8,
    gsi: u32,
    flags: u16,
    delivery_mode: Option<DeliveryMode>,
) -> Result<(), IoApicError> {
    let vector = interrupts::irq_to_vector(irq_source);

    unsafe {
        #[allow(static_mut_refs)]
        IO_APICS
            .iter()
            .find(|&ioapic| ioapic.gsi_base <= gsi && gsi < (ioapic.gsi_base + ioapic.gsi_count))
            .map(|io_apic| {
                let offset = IoApicReg::red_tbl_to_index(gsi - io_apic.gsi_base);

                let mut entry: RedirectionEntry = io_apic.read_redirection_entry(offset);

                entry.set_vector(vector as u8);
                entry.set_pin_polarity(((flags & 2) >> 1) as u8);
                entry.set_trigger_mode(((flags & 8) >> 3) as u8);
                // XXX: I think I should change the delivery mode?
                if let Some(delivery_mode) = delivery_mode {
                    entry.set_delivery_mode(delivery_mode as u8)
                }
                // XXX: Are the following 2 correct?
                entry.set_destination_mode(Destination::PHYSICAL_MODE);
                entry.set_destination(io_apic.apic_id);
                entry.set_mask(true.into());

                io_apic.write_redirection_entry(offset, entry);

                IRQ_OVERRIDES.push((irq_source, gsi));
            })
            .ok_or(IoApicError::InvalidGsi)
    }
}

pub fn irq_to_gsi(irq: u8) -> u32 {
    // Try finding a matching, and return the matching GSI. If no match is found that means that
    // the IRQ is identity mapped, so just return it back
    unsafe {
        #[allow(static_mut_refs)]
        IRQ_OVERRIDES
            .iter()
            .find(|(irq_source, _)| *irq_source == irq)
            .map(|(_, gsi)| *gsi)
            .unwrap_or(irq as u32)
    }
}

/// Disable a certain IRQ that belongs to this IO APIC by masking it.
///
/// SAFETY: This function is unsafe because any fiddling with interrupts can cause UB
pub unsafe fn set_disabled(irq: u8, status: bool) -> Result<(), IoApicError> {
    let gsi = irq_to_gsi(irq as u8);

    // TODO: Perhaps remove this check? It can be checked beforehand during initialization or
    // something
    let ioapic = unsafe {
        #[allow(static_mut_refs)]
        IO_APICS
            .iter()
            .find(|&ioapic| ioapic.gsi_base <= gsi && gsi < (ioapic.gsi_base + ioapic.gsi_count))
            .ok_or(IoApicError::InvalidGsi)?
    };

    let offset = IoApicReg::red_tbl_to_index(gsi - ioapic.gsi_base);

    let mut entry: RedirectionEntry = unsafe { ioapic.read_redirection_entry(offset) };
    entry.set_mask(status.into());
    unsafe { ioapic.write_redirection_entry(offset, entry) };

    Ok(())
}
