use alloc::vec::Vec;

use crate::{
    arch::x86_64::paging::{Entry, PageSize, PageTable},
    mem::PhysAddr,
};

use super::{DeliveryMode, Mask, PinPolarity, RemoteIrr, TriggerMode};

static mut LOCAL_APICS: Vec<LocalApic> = Vec::new();

/// The local APICs' MMIO registers that can be written to
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
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

/// The local APICs' MMIO registers that can be read from
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
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

/// Represents the flags of the local APIC
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum ApicFlags {
    Enabled = 0x0,
    OnlineCapable = 0x1,
}

/// Represents the delivery mode of the interrupt
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum DeliveryStatus {
    Idle = 0b0,
    SendPending = 0b1,
}

/// Wrapper for an LVT register.
#[derive(Debug, Clone, Copy)]
struct LvtReg(u32);

/// Represents a Local APIC on the system, and contains all the info needed to manage it
#[derive(Debug)]
pub struct LocalApic {
    /// Pointer to the base address of the MMIO of the APIC
    base: *mut u32,
    // NOTE: This is a 32-bit value, since on x2APIC systems the ID is 32 bit instead of 8 bit
    /// The APIC ID assigned to the processor
    apic_id: u32,
    // NOTE: This is a 32-bit value, since on x2APIC systems the ID is 32 bit instead of 8 bit
    /// The ACPI processor ID assigned to the processor
    acpi_processor_id: u32,
    /// The flags this local APIC is configured with
    flags: ApicFlags,
    // TODO: Maybe also store ACPI ID?
}

#[allow(dead_code)]
impl LvtReg {
    /// Sets the vector field of the LVT register
    #[inline]
    fn set_vector(&mut self, vector: u8) {
        self.0 = (self.0 & !0xff) | vector as u32;
    }

    /// Sets the delivery mode field of the LVT register
    #[inline]
    fn set_delivery_mode(&mut self, delivery_mode: DeliveryMode) {
        self.0 = (self.0 & !(0b111 << 8)) | ((delivery_mode as u32) << 8);
    }

    /// Sets the delivery status field of the LVT register
    #[inline]
    const fn set_trigger_mode(&mut self, trigger_mode: TriggerMode) {
        self.0 = (self.0 & !(0b1 << 15)) | ((trigger_mode as u32) << 15);
    }

    /// Sets the pin polarity field of the LVT register
    #[inline]
    const fn set_pin_polarity(&mut self, pin_polarity: PinPolarity) {
        self.0 = (self.0 & !(0b1 << 13)) | ((pin_polarity as u32) << 13);
    }

    /// Sets the remote IRR field of the LVT register
    #[inline]
    fn set_remote_irr(&mut self, remote_irr: RemoteIrr) {
        self.0 = (self.0 & !(0b1 << 14)) | ((remote_irr as u32) << 14);
    }

    /// Sets the mask field of the LVT register
    #[inline]
    fn set_mask(&mut self, mask: Mask) {
        self.0 = (self.0 & !(0b1 << 16)) | ((mask as u32) << 16);
    }
}

impl LocalApic {
    /// Creates a new Local APIC instance
    #[inline]
    unsafe fn new(base: *mut u32, acpi_processor_id: u32, apic_id: u32, flags: ApicFlags) -> Self {
        Self {
            base,
            acpi_processor_id,
            apic_id,
            flags: ApicFlags::from(flags),
        }
    }

    /// Reads from the passed APICs MMIO register
    #[inline]
    fn read(&self, reg: ReadableRegs) -> u32 {
        unsafe { core::ptr::read_volatile(self.base.add(reg as usize)) }
    }

    /// Writes to the passed APICs MMIO register
    #[inline]
    unsafe fn write(&self, reg: WriteableRegs, data: u32) {
        unsafe { core::ptr::write_volatile(self.base.add(reg as usize), data) }
    }

    /// Sets the LVT register for the passed LINT as an NMI
    #[inline]
    unsafe fn set_lint_as_nmi(
        &self,
        lint: u8,
        pin_polarity: PinPolarity,
        trigger_mode: TriggerMode,
    ) -> Result<(), ()> {
        let mut entry = match lint {
            0 => LvtReg(self.read(ReadableRegs::LvtLint0)),
            1 => LvtReg(self.read(ReadableRegs::LvtLint1)),
            _ => return Err(()),
        };

        entry.set_delivery_mode(DeliveryMode::Nmi);
        entry.set_trigger_mode(trigger_mode);
        entry.set_pin_polarity(pin_polarity);

        unsafe {
            match lint {
                0 => self.write(WriteableRegs::LvtLint0, entry.0),
                1 => self.write(WriteableRegs::LvtLint1, entry.0),
                _ => return Err(()),
            }
        }

        Ok(())
    }
}

/// Adds a new Local APIC to the systems global list of Local APICs
pub unsafe fn add(base: PhysAddr, acpi_processor_id: u32, apic_id: u32, flags: ApicFlags) {
    let virt_addr = base.add_hhdm_offset();
    // XXX: This might fuck things up very badly, since we're mapping without letting the
    // allocator know, but IIRC the address the local APIC is mapped to never appears on the
    // memory map
    PageTable::map_page_specific(
        virt_addr,
        base,
        Entry::FLAG_P | Entry::FLAG_RW | Entry::FLAG_PCD,
        PageSize::Size4KB,
    )
    .unwrap();

    unsafe {
        #[allow(static_mut_refs)]
        LOCAL_APICS.push(LocalApic::new(
            virt_addr.into(),
            acpi_processor_id,
            apic_id,
            flags,
        ));
    }
}

/// Marks the matching processor's LINT as NMI with the passed flags
pub unsafe fn set_as_nmi(acpi_processor_id: u32, lint: u8, flags: u16) {
    const ALL_PROCESSORS_ID: u32 = 0xff;

    unsafe {
        // If the acpi_processor_id is 0xff, we set all processors
        if acpi_processor_id == ALL_PROCESSORS_ID {
            #[allow(static_mut_refs)]
            LOCAL_APICS.iter().for_each(|apic| {
                apic.set_lint_as_nmi(
                    lint,
                    (flags & 0b11).try_into().unwrap(),
                    ((flags >> 2) & 0b11).try_into().unwrap(),
                )
                .unwrap();
            });
        } else {
            #[allow(static_mut_refs)]
            LOCAL_APICS
                .iter()
                .find(|&apic| apic.acpi_processor_id == acpi_processor_id)
                .map(|apic| {
                    apic.set_lint_as_nmi(
                        lint,
                        (flags & 0b11).try_into().unwrap(),
                        ((flags >> 2) & 0b11).try_into().unwrap(),
                    )
                    .unwrap();
                });
        }
    }
}

/// Overrides the base address of the local APICs MMIO registers
pub unsafe fn override_base(base: *mut u32) {
    unsafe {
        #[allow(static_mut_refs)]
        LOCAL_APICS.iter_mut().for_each(|apic| {
            apic.base = base;
        });
    }
}

impl TryFrom<u32> for ApicFlags {
    type Error = ();

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0x0 => Ok(ApicFlags::Enabled),
            0x1 => Ok(ApicFlags::OnlineCapable),
            _ => Err(()),
        }
    }
}
