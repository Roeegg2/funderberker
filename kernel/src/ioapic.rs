use crate::mem::{mmio::RwReg, VirtAddr};

// TODO: Move away from RwReg and RoReg, since we don't need to store the address for each reg. We
// can just store the base and then write the offset in the read/write functions.
#[derive(Debug)]
pub struct IoApic {
    io_sel: RwReg<u32>,
    io_win: RwReg<u32>,
    gsi_base: u32,
}

enum IoApicReg {
    ApicId = 0x00,
    ApicVer = 0x01,
    ApicArb = 0x02,
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

    fn write_red_tbl(&self, index: u32, data: u32) {
    }
}
