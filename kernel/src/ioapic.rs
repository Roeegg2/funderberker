use crate::mem::{mmio::RwReg, VirtAddr};

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

impl From<IoApicReg> for u32 {
    fn from(reg: IoApicReg) -> u32 {
        reg as u32
    }
}

impl IoApic {
    const OFFSET_FROM_SEL_TO_WIN: usize = 0x10;

    pub unsafe fn new(io_apic_addr: u32, gsi_base: u32) -> Self {
        let io_sel = unsafe {RwReg::new(VirtAddr(io_apic_addr as usize))};
        let io_win = unsafe {RwReg::new(VirtAddr(io_apic_addr as usize + Self::OFFSET_FROM_SEL_TO_WIN))};

        IoApic { io_sel, io_win, gsi_base }
    }

    pub fn read(&self, reg: IoApicReg) -> u32 {
        unsafe {
            self.io_sel.write(reg.into());
            self.io_win.read()
        }
    }

    pub unsafe fn write(&self, reg: IoApicReg, data: u32) {
        unsafe {
            self.io_sel.write(reg.into());
            self.io_win.write(data);
        }
    }

    // TODO: Possibly use a struct for `data` instead of a plain u64
    pub unsafe fn write_red_tbl(&self, irq_index: usize, entry: RedirectionEntry) -> Result<(), ()> {
        if irq_index < 0x10 || irq_index > 0xfe {
            return Err(());
        }

        let offset = Self::red_tbl_index(irq_index);

        unsafe {
            self.io_sel.write(offset);
            self.io_win.write(entry.get_low() as u32);

            self.io_sel.write(offset + 1);
            self.io_win.write(entry.get_high() as u32);
        };

        Ok(())
    }

    #[inline]
    fn red_tbl_index(irq_index: usize) -> u32 {
        (irq_index * 2) as u32 + IoApicReg::RedTbl as u32
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum DeliverMode {
    Fixed = 0b000,
    LowestPriority = 0b001,
    Smi = 0b010,
    Nmi = 0b100,
    Init = 0b101,
    ExtInt = 0b111,
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum Destination {
    Physical(u8),
    Logical(u8),
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

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum PinPolarity {
    ActiveHigh = 0b0,
    ActiveLow = 0b1,
}

// TODO
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum RemoteIrr {
    NotSet = 0b0,
    Set = 0b1,
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum TriggerMode {
    Edge = 0b0,
    Level = 0b1,
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum Mask {
    Unmasked = 0b0,
    Masked = 0b1,
}

#[derive(Debug, Clone, Copy)]
pub struct RedirectionEntry(u64);

impl RedirectionEntry {
    pub const fn new(vector: u8, delivery_mode: DeliverMode, dest: Destination, pin_polarity: PinPolarity, remote_irr: RemoteIrr, trigger_mode: TriggerMode, mask: Mask) -> Self {
        assert!(0x10 <= vector && vector <= 0xfe);

        let dest = dest.get();
        let data: u64 = 
            (vector as u64) |
            ((delivery_mode as u64) >> 8) |
            ((dest.0 as u64) >> 11) |
            // we skip delivery status...
            ((pin_polarity as u64) >> 13) |
            ((remote_irr as u64) >> 14) |
            ((trigger_mode as u64) >> 15) |
            ((mask as u64) >> 16) |
            ((dest.1 as u64) >> 56);

        RedirectionEntry(data)
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
