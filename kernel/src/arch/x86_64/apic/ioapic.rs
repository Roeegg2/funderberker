use alloc::vec::Vec;

use crate::arch::x86_64::paging::{PageSize, PageTable};
use super::{DeliveryMode, PinPolarity, Mask, RemoteIrr, TriggerMode};
use crate::mem::{mmio::RwReg, PhysAddr, VirtAddr};

static mut IO_APICS: Vec<IoApic> = Vec::new();

// TODO: Move away from RwReg and RoReg, since we don't need to store the address for each reg. We
// can just store the base and then write the offset in the read/write functions.
#[derive(Debug)]
pub struct IoApic {
    io_sel: RwReg<u32>,
    io_win: RwReg<u32>,
    gsi_base: u32,
}

#[derive(Debug, Clone, Copy)]
pub enum IoApicReg {
    ApicId = 0x0,
    ApicVer = 0x1,
    ApicArb = 0x2,
    RedTbl = 0x10,
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum Destination {
    Physical(u8),
    Logical(u8),
}

#[derive(Debug, Clone, Copy)]
struct RedirectionEntry(u64);

impl IoApic {
    const OFFSET_FROM_SEL_TO_WIN: usize = 0x10;

    unsafe fn new(io_apic_addr: VirtAddr, gsi_base: u32) -> Self {


        let io_sel = unsafe { RwReg::new(io_apic_addr.into()) };
        let io_win = unsafe {
            RwReg::new(VirtAddr(
                io_apic_addr.0 + Self::OFFSET_FROM_SEL_TO_WIN
            ))
        };

        IoApic {
            io_sel,
            io_win,
            gsi_base,
        }
    }

    #[inline]
    const fn red_tbl_index(irq_index: u32) -> u32 {
        (irq_index * 2) as u32 + IoApicReg::RedTbl as u32
    }
}

impl Destination {
    const PHYSICAL_MODE: u8 = 0b0;
    const LOGICAL_MODE: u8 = 0b1;

    #[inline]
    pub const fn new(dest: u8, is_logical: bool) -> Result<Self, ()> {
        if is_logical {
            if dest | 0x0f != 0 {
                // TODO: Add error message here.
                // In this mode, the destination is a 4-bit logical destination ID.
                return Err(());
            }
            Ok(Destination::Logical(dest))
        } else {
            Ok(Destination::Physical(dest))
        }
    }

    #[inline]
    pub const fn get(&self) -> (u8, u8) {
        match self {
            Destination::Physical(dest) => (Self::PHYSICAL_MODE, *dest),
            Destination::Logical(dest) => (Self::LOGICAL_MODE, *dest),
        }
    }
}

impl RedirectionEntry {
    #[inline]
    const fn set_vector(&mut self, vector: u8) -> Result<(), ()> {
        // Only vectors in the range 0x10-0xfe are legal
        if vector > 0xee {
            return Err(());
        }

        self.0 = (self.0 & !0xff) | (vector as u64 + 0x10);

        Ok(())
    }

    #[inline]
    const fn set_delivery_mode(&mut self, delivery_mode: DeliveryMode) {
        self.0 = (self.0 & !(0b111 << 8)) | ((delivery_mode as u64) << 8);
    }

    #[inline]
    const fn set_trigger_mode(&mut self, trigger_mode: TriggerMode) {
        self.0 = (self.0 & !(0b1 << 15)) | ((trigger_mode as u64) << 15);
    }

    #[inline]
    const fn set_pin_polarity(&mut self, pin_polarity: PinPolarity) {
        self.0 = (self.0 & !(0b1 << 13)) | ((pin_polarity as u64) << 13);
    }

    #[inline]
    const fn set_remote_irr(&mut self, remote_irr: RemoteIrr) {
        self.0 = (self.0 & !(0b1 << 14)) | ((remote_irr as u64) << 14);
    }

    #[inline]
    const fn set_mask(&mut self, mask: Mask) {
        self.0 = (self.0 & !(0b1 << 16)) | ((mask as u64) << 16);
    }

    #[inline]
    const fn set_dest(&mut self, dest: Destination) {
        let (mode, dest) = dest.get();
        self.0 = (self.0 & !(0b1 << 11)) | ((mode as u64) << 11);
        self.0 = (self.0 & !(0xff << 56)) | ((dest as u64) << 56);
    }

    #[inline]
    const fn get_low(&self) -> u32 {
        (self.0 & 0xffff_ffff) as u32
    }

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

pub unsafe fn add(io_apic_addr: PhysAddr, gsi_base: u32) {
    let virt_addr = io_apic_addr.add_hhdm_offset();
    // XXX: This might fuck things up very badly, since we're mapping without letting the
    // allocator know
    PageTable::map_page_specific(virt_addr, io_apic_addr, 0b11, PageSize::Size4KB).unwrap();

    unsafe {
        #[allow(static_mut_refs)]
        IO_APICS.push(IoApic::new(virt_addr, gsi_base))
    };
}

/// Set the redirection table entry for the given GSI.
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

                println!("gsi: {gsi}, offset: {offset}, entry: {entry:?} irq_source: {irq_source}");
                entry.set_vector(irq_source).unwrap();
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
