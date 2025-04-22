//! Local APIC driver and interface

use super::{
    DeliveryMode, Destination, DestinationShorthand, Level, PinPolarity,
    TriggerMode,
};
use crate::{
    arch::x86_64::{cpu::{wrmsr, Msr}, paging::{Entry, PageSize, PageTable}},
    mem::{mmio::{MmioArea, Offsetable}, PhysAddr},
};
use alloc::vec::Vec;
use modular_bitfield::prelude::*;

pub static mut LOCAL_APICS: Vec<LocalApic> = Vec::new();

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
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum DeliveryStatus {
    Idle = 0b0,
    SendPending = 0b1,
}

#[bitfield]
#[derive(Debug, Clone, Copy)]
#[repr(u32)]
/// Represents the LVT register
struct LvtReg {
    /// The vector to be used for this interrupt
    vector: B8,
    /// The delivery mode of the interrupt
    delivery_mode: B3,
    _reserved: B1,
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
    _reserved2: B15,
}


/// Represents a Local APIC on the system, and contains all the info needed to manage it
#[derive(Debug)]
pub struct LocalApic {
    /// Pointer to the base address of the MMIO of the APIC
    area: MmioArea<ReadableRegs, WriteableRegs, u32>,
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

impl LocalApic {
    // #[inline]
    // unsafe fn hardware_enable(&self) {
    //     unsafe {wrmsr(Msr::Ia32ApicBase, low, high)};
    // }

    #[inline]
    unsafe fn init(&self) {
        unsafe {
            self.area.write(WriteableRegs::SpuriousInterruptVector, 0xff);
        }
    }

    /// Creates a new Local APIC instance
    #[inline]
    unsafe fn new(base: *mut u32, acpi_processor_id: u32, apic_id: u32, flags: ApicFlags) -> Self {
        let apic = Self {
            area: MmioArea::new(base),
            acpi_processor_id,
            apic_id,
            flags: ApicFlags::from(flags),
        };

        unsafe {
            apic.init();
        };

        apic
    }

    /// Sets the LVT register for the passed LINT as an NMI
    #[inline]
    unsafe fn set_lint_as_nmi(
        &self,
        lint: u8,
        pin_polarity: PinPolarity,
        trigger_mode: TriggerMode,
    ) -> Result<(), ()> {
        let mut entry: LvtReg = match lint {
            0 => unsafe {self.area.read(ReadableRegs::LvtLint0).into()},
            1 => unsafe {self.area.read(ReadableRegs::LvtLint1).into()},
            _ => return Err(()),
        };

        entry.set_delivery_mode(DeliveryMode::Nmi as u8);
        entry.set_trigger_mode(trigger_mode as u8);
        entry.set_pin_polarity(pin_polarity as u8);

        match lint {
            0 => unsafe {self.area.write(WriteableRegs::LvtLint0, entry.into())},
            1 => unsafe {self.area.write(WriteableRegs::LvtLint1, entry.into())},
            _ => return Err(()),
        }

        Ok(())
    }
}

impl LocalApic {
    pub fn apic_id(&self) -> u32 {
        self.apic_id
    }

    pub fn flags(&self) -> ApicFlags {
        self.flags
    }

    /// Read the error status register
    pub fn read_errors(&self) -> u32 {
        // We should write to the ESR before reading from it to discard any stale data
        unsafe { 
            self.area.write(WriteableRegs::ErrorStatus, 0);

            self.area.read(ReadableRegs::ErrorStatus)
        }
    }

    /// Configure and send an inter-processor interrupt.
    ///
    /// This function is unsafe for 2 reasons:
    /// 1. Sending an interrupt to some other processor could result in UB.
    /// 2. Not all flag combinations are legal, and so using an invalid combination could result in
    ///    UB.
    ///
    /// NOTE: `level` and `trigger_mode` are both not used in Pentium 4 and Intel Xeon processors,
    /// and should always be set to 1 and 0 respectively.
    pub unsafe fn send_ipi(
        &self,
        vector: u8,
        delivery_mode: DeliveryMode,
        destination: Destination,
        level: Level,
        trigger_mode: TriggerMode,
        destination_shorthand: DestinationShorthand,
    ) {
        let destination = destination.get();
        unsafe {
            self.area.write(
                WriteableRegs::InterruptCommand1,
                (destination.1 as u32) << 24,
            )
        }

        let lower_part_data = (vector as u32)
            | ((delivery_mode as u32) << 8)
            | ((destination.0 as u32) << 11)
            | ((level as u32) << 14)
            | ((trigger_mode as u32) << 15)
            | ((destination_shorthand as u32) << 18);

        unsafe { self.area.write(WriteableRegs::InterruptCommand0, lower_part_data) }
    }

    pub fn ipi_status(&self) -> DeliveryStatus {
        let status = unsafe {self.area.read(ReadableRegs::InterruptCommand0)};
        if status & (1 << 12) == 0 {
            DeliveryStatus::Idle
        } else {
            DeliveryStatus::SendPending
        }
    }
}

/// Adds a new Local APIC to the systems global list of Local APICs
pub unsafe fn add(base: PhysAddr, acpi_processor_id: u32, apic_id: u32, flags: ApicFlags) {
    let virt_addr = base.add_hhdm_offset();
    // XXX: This might fuck things up very badly, since we're mapping without letting the
    // allocator know, but AFAIK the address the local APIC is mapped to never appears on the
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
            apic.area.override_base(base);
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

impl Offsetable for ReadableRegs {
    fn offset(self) -> usize {
        self as usize
    }
}

impl Offsetable for WriteableRegs {
    fn offset(self) -> usize {
        self as usize
    }
}
