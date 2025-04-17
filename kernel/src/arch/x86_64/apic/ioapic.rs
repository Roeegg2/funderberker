use super::{DeliveryMode, Destination, Mask, PinPolarity, RemoteIrr, TriggerMode};
use crate::arch::x86_64::paging::{Entry, PageSize, PageTable};
use crate::mem::{PhysAddr, VirtAddr, mmio::RwReg};
use alloc::vec::Vec;

static mut IO_APICS: Vec<IoApic> = Vec::new();

/// Struct representing the IO APIC, containing everything needed to interact with it
#[derive(Debug)]
pub struct IoApic {
    /// The select register. The index of the register is written here in order for it to be
    /// accessible in the `win` register
    io_sel: RwReg<u32>,
    /// The window register. This is where the data is read from and written to
    io_win: RwReg<u32>,
    /// The base of the global system interrupts (GSIs) that this IO APIC is responsible for
    gsi_base: u32,
}

/// The IO APIC's MMIO registers that can be written to
#[derive(Debug, Clone, Copy)]
pub enum IoApicReg {
    /// The index of the ID register
    ApicId = 0x0,
    /// The index of the version register
    ApicVer = 0x1,
    /// The index of the arbitration register
    ApicArb = 0x2,
    /// The base index of the redirection table registers
    RedTbl = 0x10,
}

/// The IO APIC's redirection table entry, which configure the behaviour and mapping of the
/// external interrupts
#[derive(Debug, Clone, Copy)]
struct RedirectionEntry(u64);

impl IoApic {
    /// The offset that needs to be added from the `win` MMIO register address to get `sel` MMIO
    /// address
    const OFFSET_FROM_SEL_TO_WIN: usize = 0x10;

    /// Creates a new IO APIC
    unsafe fn new(io_apic_addr: VirtAddr, gsi_base: u32) -> Self {
        let io_sel = unsafe { RwReg::new(io_apic_addr.into()) };
        let io_win = unsafe { RwReg::new(VirtAddr(io_apic_addr.0 + Self::OFFSET_FROM_SEL_TO_WIN)) };

        IoApic {
            io_sel,
            io_win,
            gsi_base,
        }
    }

    /// Convert a GSI to the corresponding redirection table index
    #[inline]
    const fn red_tbl_index(irq_index: u32) -> u32 {
        (irq_index * 2) as u32 + IoApicReg::RedTbl as u32
    }
}

#[allow(dead_code)]
impl RedirectionEntry {
    /// Sets the vector field
    #[inline]
    const fn set_vector(&mut self, vector: u8) {
        // XXX: Need to check if vector is valid?
        self.0 = (self.0 & !0xff) | (vector as u64 + 0x10);
    }

    /// Sets the delivery mode field
    #[inline]
    const fn set_delivery_mode(&mut self, delivery_mode: DeliveryMode) {
        self.0 = (self.0 & !(0b111 << 8)) | ((delivery_mode as u64) << 8);
    }

    /// Sets the trigger mode field
    #[inline]
    const fn set_trigger_mode(&mut self, trigger_mode: TriggerMode) {
        self.0 = (self.0 & !(0b1 << 15)) | ((trigger_mode as u64) << 15);
    }

    /// Sets the pin polarity field
    #[inline]
    const fn set_pin_polarity(&mut self, pin_polarity: PinPolarity) {
        self.0 = (self.0 & !(0b1 << 13)) | ((pin_polarity as u64) << 13);
    }

    /// Sets the remote IRR field
    #[inline]
    const fn set_remote_irr(&mut self, remote_irr: RemoteIrr) {
        self.0 = (self.0 & !(0b1 << 14)) | ((remote_irr as u64) << 14);
    }

    /// Sets the mask field
    #[inline]
    const fn set_mask(&mut self, mask: Mask) {
        self.0 = (self.0 & !(0b1 << 16)) | ((mask as u64) << 16);
    }

    /// Sets the destination field
    #[inline]
    const fn set_dest(&mut self, dest: Destination) {
        let (mode, dest) = dest.get();
        self.0 = (self.0 & !(0b1 << 11)) | ((mode as u64) << 11);
        self.0 = (self.0 & !(0xff << 56)) | ((dest as u64) << 56);
    }

    /// Get the low 32 bits of the entry
    #[inline]
    const fn get_low(&self) -> u32 {
        (self.0 & 0xffff_ffff) as u32
    }

    /// Get the high 32 bits of the entry
    #[inline]
    const fn get_high(&self) -> u32 {
        ((self.0 >> 32) & 0xffff_ffff) as u32
    }
}

impl From<IoApicReg> for u32 {
    fn from(reg: IoApicReg) -> u32 {
        reg as u32
    }
}

/// Adds an IO APIC to the global list of IO APICs
pub unsafe fn add(io_apic_addr: PhysAddr, gsi_base: u32) {
    let virt_addr = io_apic_addr.add_hhdm_offset();
    // XXX: This might fuck things up very badly, since we're mapping without letting the
    // allocator know
    PageTable::map_page_specific(
        virt_addr,
        io_apic_addr,
        Entry::FLAG_P | Entry::FLAG_RW | Entry::FLAG_PCD,
        PageSize::Size4KB,
    )
    .unwrap();

    unsafe {
        #[allow(static_mut_refs)]
        IO_APICS.push(IoApic::new(virt_addr, gsi_base))
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
                let offset = IoApic::red_tbl_index(gsi - io_apic.gsi_base);

                let mut entry = {
                    io_apic.io_sel.write(offset);
                    let mut raw: u64 = io_apic.io_win.read() as u64;
                    io_apic.io_sel.write(offset + 1);
                    raw |= (io_apic.io_win.read() as u64) << 32;

                    RedirectionEntry(raw)
                };

                entry.set_vector(irq_source);
                entry.set_pin_polarity((flags & 0b11).try_into().unwrap());
                entry.set_trigger_mode(((flags >> 2) & 0b11).try_into().unwrap());
                entry.set_delivery_mode(delivery_mode);

                io_apic.io_sel.write(offset);
                io_apic.io_win.write(entry.get_low());
                io_apic.io_sel.write(offset + 1);
                io_apic.io_win.write(entry.get_high());
            });
    }

    Ok(())
}
