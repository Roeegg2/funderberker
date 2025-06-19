//! Interface and driver for the IO APIC

use super::{DeliveryMode, Destination};
use crate::arch::x86_64::cpu::outb_8;
use crate::arch::x86_64::paging::Entry;
use crate::mem::PhysAddr;
use crate::mem::mmio::MmioCell;
use crate::mem::vmm::map_page;
use crate::sync::spinlock::{SpinLock, SpinLockable};
use alloc::vec::Vec;
use core::cell::SyncUnsafeCell;
use modular_bitfield::prelude::*;
use utils::collections::id::Id;
use utils::collections::id::tracker::{IdTracker, IdTrackerError};

/// Errors the IO APIC might encounter
#[derive(Debug, Copy, Clone)]
pub enum IoApicError {
    /// The GSI passed in is invalid
    InvalidGsi,
    /// IDT Error
    IdtError,
}

/// An IRQ override mapping
#[derive(Debug, Clone, Copy)]
struct IrqOverride {
    /// The original IRQ pin
    irq: u8,
    /// The GSI that the IRQ is mapped to
    gsi: u32,
}

/// An IRQ allocator, to keep track of used/unused IRQs efficiently
pub static IRQ_ALLOCATOR: SpinLock<IdTracker> = SpinLock::new(IdTracker::uninit());

/// The registered IRQ overrides
static IRQ_OVERRIDES: SyncUnsafeCell<Vec<IrqOverride>> = SyncUnsafeCell::new(Vec::new());

/// All of the IO APICs on the system
pub static IO_APICS: SyncUnsafeCell<Vec<SpinLock<IoApic>>> = SyncUnsafeCell::new(Vec::new());

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

/// A ZST to for the IO APIC registers
struct IoApicReg;

/// The IO APIC's redirection table entry, which configure the behaviour and mapping of the
/// external interrupts
#[bitfield(bits = 64)]
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
    reserved: B39,
    /// The destination of the interrupt
    destination: B8,
}

#[allow(unused)]
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
        (irq_index * 2) + Self::APIC_REDIRACTION_TABLE_BASE
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
    fn new(base: *mut u32, gsi_base: u32, apic_id: u8) -> Self {
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

        ret.gsi_count = max_gsi + 1;

        ret
    }

    /// Utility wrapper for reading from the RED TBL entries as reading from there is a bit error prone
    #[inline]
    unsafe fn read_redirection_entry(&self, offset: u32) -> RedirectionEntry {
        // TODO: Return error if the offset is invalid
        unsafe {
            self.io_sel.write(offset);
            let mut raw: u64 = self.io_win.read().into();
            self.io_sel.write(offset + IoApicReg::red_tbl_to_index(1));
            raw |= (self.io_win.read() as u64) << 32;

            RedirectionEntry::from(raw)
        }
    }

    /// Utility wrapper for writing to the RED TBL entries as writing there is a bit error prone
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

fn get_ioapics() -> &'static Vec<SpinLock<IoApic>> {
    unsafe { IO_APICS.get().as_ref().unwrap() }
}

/// Adds an IO APIC to the global list of IO APICs
pub unsafe fn add(phys_addr: PhysAddr, gsi_base: u32, apic_id: u8) {
    // SAFETY: This should be OK since we're mapping a physical address that is marked as
    // reserved, so the kernel shouldn't be tracking it
    let virt_addr = unsafe { map_page(phys_addr, Entry::FLAG_RW) };

    let ioapics = unsafe { IO_APICS.get().as_mut().unwrap() };
    ioapics.push(SpinLock::new(IoApic::new(
        virt_addr.into(),
        gsi_base,
        apic_id,
    )));
}

/// Initialize the IRQ allocator.
///
/// NOTE: THIS SHOULD BE CALLED ONLY ONCE DURING BOOT!
pub fn init_irq_allocator() {
    let mut irq_allocator = IRQ_ALLOCATOR.lock();
    let gsi_count = get_ioapics()
        .iter()
        .map(|ioapic| ioapic.lock())
        .fold(0, |acc, ioapic| acc + ioapic.gsi_count as usize);

    *irq_allocator = IdTracker::new(Id(0)..Id(gsi_count));
}

/// Mark the given IRQ as used.
///
/// SAFETY: This function is unsafe since some IRQ lines are hardwired to certain devices, and so
/// cannot be used for other purposes.
/// It is up to the caller to make sure that the IRQ line requested is legal for it's use case
pub unsafe fn allocate_irq_at(irq: u8) -> Result<(), IdTrackerError> {
    // TODO: Possibly perform a small check to make sure we aren't overriding some known devices?
    // (PIT, RTC, RTC)
    let mut irq_allocator = IRQ_ALLOCATOR.lock();

    irq_allocator.allocate_at(Id(irq as usize))
}

/// Map the given IRQ to the given vector.
///
/// NOTE: Make sure you pass in the IRQ, and NOT the GSI.
pub unsafe fn map_irq_to_vector(vector: u8, irq: u8) -> Result<(), IoApicError> {
    let gsi = irq_to_gsi(irq);

    let ioapics = get_ioapics();

    // Find the IO APIC that matches this GSI
    ioapics
        .iter()
        .map(|ioapic| ioapic.lock())
        .find(|ioapic| ioapic.gsi_base <= gsi && gsi < (ioapic.gsi_base + ioapic.gsi_count))
        .map(|io_apic| {
            // Get the offset to the redirection entry
            let offset = IoApicReg::red_tbl_to_index(gsi - io_apic.gsi_base);
            // Read the redirection entry
            let mut entry: RedirectionEntry = unsafe { io_apic.read_redirection_entry(offset) };

            // Set the vector
            entry.set_vector(vector);
            // Mask of the interrupt for now; When we want it enabled we'll unmask it manually
            // later
            entry.set_mask(true.into());

            // Write the entry back
            unsafe {
                io_apic.write_redirection_entry(offset, entry);
            };
        })
        .ok_or(IoApicError::InvalidGsi)
}

/// Overrides the identity mapping of a specific IRQ in the system, as well as sets some settings
/// as specified by the MADT entry.
#[inline]
pub unsafe fn override_irq(irq: u8, gsi: u32, flags: u16, delivery_mode: Option<DeliveryMode>) {
    let ioapics = get_ioapics();
    // Find the IO APIC that matches this GSI
    ioapics
        .iter()
        .map(|ioapic| ioapic.lock())
        .find(|ioapic| ioapic.gsi_base <= gsi && gsi < (ioapic.gsi_base + ioapic.gsi_count))
        .map(|io_apic| {
            // Get the offset to the redirection entry
            let offset = IoApicReg::red_tbl_to_index(gsi - io_apic.gsi_base);
            // Read the redirection entry
            let mut entry: RedirectionEntry = unsafe { io_apic.read_redirection_entry(offset) };

            entry.set_pin_polarity(((flags & 2) >> 1) as u8);
            entry.set_trigger_mode(((flags & 8) >> 3) as u8);
            // XXX: FIX THESE! Make this be all LOCAL APICS and not just the one setting this
            // up
            entry.set_destination_mode(Destination::PHYSICAL_MODE);
            entry.set_destination(io_apic.apic_id);

            // XXX: I think I should change the delivery mode?
            if let Some(delivery_mode) = delivery_mode {
                entry.set_delivery_mode(delivery_mode as u8);
            }

            // Write the entry back
            unsafe {
                io_apic.write_redirection_entry(offset, entry);
            };
        })
        .expect("Invalid GSI for override");

    // TODO: Use hashmap? And check to make sure such an entry doesn't exist yet

    // Record the IRQ to GSI override mapping
    let irq_overrides = unsafe { IRQ_OVERRIDES.get().as_mut().unwrap() };

    irq_overrides.push(IrqOverride { irq, gsi });
}

/// Pass in a GSI, get the IRQ mapped to it.
/// If no override exists for the GSI, the GSI is assumed to be identity-mapped to the same IRQ value.
pub fn gsi_to_irq(gsi: u32) -> u8 {
    // Access the IRQ overrides
    let irq_overrides = unsafe { IRQ_OVERRIDES.get().as_mut().unwrap() };

    // Find an override where the GSI matches, and return the corresponding IRQ
    irq_overrides
        .iter()
        .find(|&irq_override| irq_override.gsi == gsi)
        .map_or(gsi as u8, |&irq_override| irq_override.irq)
}

/// Pass in an IRQ, get the GSI mapped to it (Most IRQs aren't overriden and so the returned GSI
/// will just be the same as the IRQ passed in)
pub fn irq_to_gsi(irq: u8) -> u32 {
    // Try finding a matching, and return the matching GSI. If no match is found that means that
    // the IRQ is identity mapped, so just return it back
    let irq_overrides = unsafe { IRQ_OVERRIDES.get().as_mut().unwrap() };

    irq_overrides
        .iter()
        .find(|&irq_override| irq_override.irq == irq)
        .map_or(irq as u32, |&irq_override| irq_override.gsi)
}

/// Masks/unmasks a certain IRQ effectively enabling/disabling it.
///
/// NOTE: Make sure you pass in the IRQ and NOT the GSI.
///
/// SAFETY: This function is unsafe because any fiddling with interrupts can cause UB
pub unsafe fn set_disabled(irq: u8, status: bool) -> Result<(), IoApicError> {
    let gsi = irq_to_gsi(irq);
    // TODO: Perhaps remove this check? It can be checked beforehand during initialization or
    // something
    let ioapics = get_ioapics();
    let ioapic = ioapics
        .iter()
        .map(|ioapic| ioapic.lock())
        .find(|ioapic| ioapic.gsi_base <= gsi && gsi < (ioapic.gsi_base + ioapic.gsi_count))
        .ok_or(IoApicError::InvalidGsi)?;

    let offset = IoApicReg::red_tbl_to_index(gsi - ioapic.gsi_base);

    let mut entry: RedirectionEntry = unsafe { ioapic.read_redirection_entry(offset) };
    entry.set_mask(status.into());
    unsafe { ioapic.write_redirection_entry(offset, entry) };

    Ok(())
}

unsafe impl Send for IoApic {}
unsafe impl Sync for IoApic {}

impl SpinLockable for IoApic {}

// TODO: Move this some place else
impl SpinLockable for IdTracker {}
