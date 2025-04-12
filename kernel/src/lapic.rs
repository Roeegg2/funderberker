
use crate::mem::mmio::{RwReg, RoReg};

pub enum WriteableRegs {
    Id = 0x20,
    TaskPriority = 0x80,
    EndOfInterrupt = 0xb0,
    LogicalDestination = 0xd0,
    DestinationFormat = 0xe0,
    SpuriousInterruptVector = 0xf0,
    LvtCmci = 0x2f0,
    InterruptCommand0 = 0x300,
    InterruptCommand1 = 0x310,
    LvtTimer = 0x320,
    LvtThermal = 0x330,
    LvtPerformance = 0x340,
    LvtLint0 = 0x350,
    LvtLint1 = 0x360,
    LvtError = 0x370,
    TimerInitialCount = 0x380,
    TimerDivideConfig = 0x3e0,
}

pub enum ReadableRegs {
    Id = 0x20,
    Version = 0x30,
    TaskPriority = 0x80,
    ArbitrationPriority = 0x90,
    ProcessorPriority = 0xa0,
    RemoteRead = 0xc0,
    LogicalDestination = 0xd0,
    DestinationFormat = 0xe0,
    SpuriousInterruptVector = 0xf0,
    InService0 = 0x100,
    InService1 = 0x110,
    InService2 = 0x120,
    InService3 = 0x130,
    InService4 = 0x140,
    InService5 = 0x150,
    InService6 = 0x160,
    InService7 = 0x170,
    TriggerMode0 = 0x180,
    TriggerMode1 = 0x190,
    TriggerMode2 = 0x1a0,
    TriggerMode3 = 0x1b0,
    TriggerMode4 = 0x1c0,
    TriggerMode5 = 0x1d0,
    TriggerMode6 = 0x1e0,
    TriggerMode7 = 0x1f0,
    InterruptRequest0 = 0x200,
    InterruptRequest1 = 0x210,
    InterruptRequest2 = 0x220,
    InterruptRequest3 = 0x230,
    InterruptRequest4 = 0x240,
    InterruptRequest5 = 0x250,
    InterruptRequest6 = 0x260,
    InterruptRequest7 = 0x270,
    ErrorStatus = 0x280,
    LvtCmci = 0x2f0,
    InterruptCommand0 = 0x300,
    InterruptCommand1 = 0x310,
    LvtTimer = 0x320,
    LvtThermal = 0x330,
    LvtPerformance = 0x340,
    LvtLint0 = 0x350,
    LvtLint1 = 0x360,
    LvtError = 0x370,
    TimerInitialCount = 0x380,
    TimerCurrentCount = 0x390,
    TimerDivideConfig = 0x3e0,
}

#[repr(u8)]
pub enum ApicFlags {
    Enabled = 0x0,
    OnlineCapable = 0x1,
}

pub struct LocalApic {
    base: *mut u32,
    // NOTE: This is a 32-bit value, since on x2APIC systems the ID is 32 bit isntead of 8 bit
    apic_id: u32,
    flags: ApicFlags,
    // TODO: Maybe also store ACPI ID?
}

impl LocalApic {
    #[inline]
    pub unsafe fn new(base: *mut u32, apic_id: u32, flags: ApicFlags) -> Self {
        Self {
            base,
            apic_id,
            flags: ApicFlags::from(flags),
        }
    }

    #[inline]
    pub fn read(&self, reg: ReadableRegs) -> u32 {
        unsafe { core::ptr::read_volatile(self.base.add(reg as usize)) }
    }

    #[inline]
    pub unsafe fn write(&self, reg: WriteableRegs, data: u32) {
        unsafe { core::ptr::write_volatile(self.base.add(reg as usize), data) }
    }
}
