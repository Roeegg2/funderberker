//! Local APIC driver and interface

use core::{arch::x86_64::__cpuid_count, cell::SyncUnsafeCell, mem::transmute};

use super::{DeliveryMode, Destination, DestinationShorthand, Level, PinPolarity, TriggerMode};
use crate::{
    arch::x86_64::{
        cpu::{IntelMsr, rdmsr, wrmsr},
        paging::Entry,
    },
    dev::timer::apic::{TimerDivisor, TimerMode},
    mem::{
        PhysAddr,
        mmio::{MmioArea, Offsetable},
        vmm::map_page,
    },
    sync::spinlock::{SpinLock, SpinLockDropable, SpinLockGuard},
};
use alloc::vec::Vec;
use modular_bitfield::prelude::*;

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

/// Represents the LVT register
#[bitfield]
#[derive(Debug, Clone, Copy)]
#[repr(u32)]
pub struct LvtReg {
    /// The vector to be used for this interrupt
    vector: B8,
    /// The delivery mode of the interrupt
    delivery_mode: B3,
    reserved: B1,
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
    reserved2: B15,
}

pub static LOCAL_APICS: SyncUnsafeCell<Vec<SpinLock<LocalApic>>> = SyncUnsafeCell::new(Vec::new());

/// Represents a Local APIC on the system, and contains all the info needed to manage it
#[derive(Debug)]
pub struct LocalApic {
    /// Pointer to the base address of the MMIO of the APIC
    area: MmioArea<ReadableRegs, WriteableRegs, u32>,
    /// The ACPI ID assigned to this core (to which the APIC belongs to)
    acpi_processor_id: u32,
    /// The APIC ID assigned to this APIC
    apic_id: u32,
}

impl LocalApic {
    /// Verifies that the CPU actually supports APIC
    #[inline]
    fn check_support() {
        const CPUID_APIC_BIT: u32 = 0x1 << 9;

        let cpuid = unsafe { core::arch::x86_64::__cpuid(1) };

        assert!(
            cpuid.edx & CPUID_APIC_BIT != 0,
            "Local APIC not supported on this CPU"
        );
    }

    /// Enables the local APIC in the IA32_APIC_BASE MSR, in case firmware didn't do it already
    #[inline]
    fn hardware_enable() {
        const APIC_ENABLE: u32 = 1 << 11;

        Self::check_support();

        let mut value = unsafe { rdmsr(IntelMsr::Ia32ApicBase) };
        value.0 |= APIC_ENABLE;

        unsafe { wrmsr(IntelMsr::Ia32ApicBase, value.0, value.1) };
    }

    /// Creates a new Local APIC instance
    #[inline]
    fn new(base: *mut u32, acpi_processor_id: u32, apic_id: u32) -> Self {
        let apic = Self {
            area: MmioArea::new(base),
            acpi_processor_id,
            apic_id,
        };

        // Initialize the local APIC
        unsafe {
            // Make sure the APIC enable bit on the IA32_APIC_BASE MSR is set
            Self::hardware_enable();

            // Configure the SIV and software enable the APIC
            apic.area
                .write(WriteableRegs::SpuriousInterruptVector, 0x1ff);
            apic.area.write(WriteableRegs::TaskPriority, 0x0);
        }

        apic
    }

    /// Sets the `LVT` register for the passed `LINT` as an `NMI` and the other as an `ExtInt`, and enabled
    /// the error `LVT`
    #[inline]
    unsafe fn setup_lvts(
        &self,
        nmi_lint: u8,
        nmi_lint_pin_polarity: PinPolarity,
        nmi_lint_trigger_mode: TriggerMode,
    ) {
        let (nmi_lint, ext_int_lint) = match nmi_lint {
            0 => (ReadableRegs::LvtLint0, ReadableRegs::LvtLint1),
            1 => (ReadableRegs::LvtLint1, ReadableRegs::LvtLint0),
            _ => panic!("Invalid LINT number"),
        };

        let (mut nmi_lint_entry, mut ext_int_lint_entry): (LvtReg, LvtReg) = unsafe {
            (
                self.area.read(nmi_lint).into(),
                self.area.read(ext_int_lint).into(),
            )
        };

        // Configure the NMI LINT
        nmi_lint_entry.set_delivery_mode(DeliveryMode::Nmi as u8);
        nmi_lint_entry.set_trigger_mode(nmi_lint_trigger_mode as u8);
        nmi_lint_entry.set_pin_polarity(nmi_lint_pin_polarity as u8);
        nmi_lint_entry.set_mask(false.into());

        // Configure the other LINT to be for external interrupts
        ext_int_lint_entry.set_delivery_mode(DeliveryMode::ExtInt as u8);
        // XXX: IIRC Some newer devices need this to be level, so this might cause trouble
        ext_int_lint_entry.set_trigger_mode(TriggerMode::EdgeTriggered as u8);
        // ext_int_lint_entry.set_pin_polarity(PinPolarity::ActiveHigh as u8);
        ext_int_lint_entry.set_mask(false.into());

        // Enable the error LVT
        let mut error: LvtReg = unsafe { self.area.read(ReadableRegs::LvtError).into() };
        error.set_mask(false.into());

        // Write the results back
        unsafe {
            // XXX: Perhaps get rid of the transmute here?
            self.area.write(transmute(nmi_lint), nmi_lint_entry.into());
            self.area
                .write(transmute(ext_int_lint), ext_int_lint_entry.into());
            self.area.write(WriteableRegs::LvtError, error.into());
        }
    }

    /// Read the error status register
    pub fn read_errors(&self) -> u32 {
        unsafe {
            // We write to the ESR before reading from it to discard any stale data
            self.area.write(WriteableRegs::ErrorStatus, 0);

            self.area.read(ReadableRegs::ErrorStatus)
        }
    }

    /// Reads and calculates the frequency divider
    pub fn get_timer_divide_config(&self) -> TimerDivisor {
        unsafe {
            let ret = self.area.read(ReadableRegs::TimerDivideConfig);
            // Bits 0,1 and 3
            let pow = ((ret & 0b1000) >> 1) | (ret & 0b11);

            // SAFETY: This is OK, we can't get a value over 0b111. All possible values are valid
            transmute(pow as u8)
        }
    }

    /// Set the timer's frequency divider
    pub fn set_timer_divider_config(&self, divider: TimerDivisor) {
        let val = (divider as u32) & 0b11 | ((divider as u32) << 1);

        unsafe {
            self.area.write(WriteableRegs::TimerDivideConfig, val);
        }
    }

    /// Configure the timer
    pub fn configure_timer(&self, cycle_count: u32, timer_mode: TimerMode) {
        // Set the timer to be periodic
        let mut lvtt: LvtReg = unsafe { self.area.read(ReadableRegs::LvtTimer).into() };
        // This field is reserved on all LVT registers except for the timer
        lvtt.set_reserved2(timer_mode as u16);

        unsafe {
            self.area.write(WriteableRegs::LvtTimer, lvtt.into());

            self.area
                .write(WriteableRegs::TimerInitialCount, cycle_count);
        }
    }

    /// Disable the timer
    pub fn set_timer_disabled(&self, disable: bool) {
        unsafe {
            let mut lvtt: LvtReg = self.area.read(ReadableRegs::LvtTimer).into();
            lvtt.set_mask(disable.into());
            self.area.write(WriteableRegs::LvtTimer, lvtt.into());
        }
    }

    /// Read the timer's current count
    #[inline]
    pub fn read_current_timer_count(&self) -> u32 {
        unsafe { self.area.read(ReadableRegs::TimerCurrentCount) }
    }

    /// Read the timer's initial count
    #[inline]
    pub fn read_initial_timer_count(&self) -> u32 {
        unsafe { self.area.read(ReadableRegs::TimerInitialCount) }
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
            );
        };

        let lower_part_data = (vector as u32)
            | ((delivery_mode as u32) << 8)
            | ((destination.0 as u32) << 11)
            | ((level as u32) << 14)
            | ((trigger_mode as u32) << 15)
            | ((destination_shorthand as u32) << 18);

        unsafe {
            self.area
                .write(WriteableRegs::InterruptCommand0, lower_part_data);
        };
    }

    /// Checks the status of the IPI sent
    ///
    /// NOTE: This function is marked as inline since it's usually used with spinning
    #[inline]
    pub fn ipi_status(&self) -> DeliveryStatus {
        let status = unsafe { self.area.read(ReadableRegs::InterruptCommand0) };
        if status & (1 << 12) == 0 {
            DeliveryStatus::Idle
        } else {
            DeliveryStatus::SendPending
        }
    }

    /// Writes to the EOI register, signaling the end of an interrupt.
    ///
    /// This function **MUST** be called from every IRQ ISR so new interrupts can be processed.
    pub fn signal_eoi(&self) {
        unsafe { self.area.write(WriteableRegs::EndOfInterrupt, 0) };
    }

    /// Get the APIC ID of this core's local APIC
    pub fn get_this_apic_id() -> u32 {
        unsafe { (__cpuid_count(1, 0).ebx >> 24) & 0xff_u32 }
    }

    /// Spin and lock the local APIC matching the given APIC ID
    pub fn get_apic(apic_id: u32) -> SpinLockGuard<'static, LocalApic> {
        let lapics = unsafe { LOCAL_APICS.get().as_ref().unwrap() };
        // TODO: Change this
        lapics
            .iter()
            .map(|lapic| lapic.lock())
            .find(|lapic| lapic.apic_id == apic_id)
            .expect("No APIC matching this APIC ID")
    }
}

fn get_lapics() -> &'static Vec<SpinLock<LocalApic>> {
    unsafe { LOCAL_APICS.get().as_ref().unwrap() }
}

/// Adds a new Local APIC to the systems global list of Local APICs
pub unsafe fn add(base: PhysAddr, acpi_processor_id: u32, apic_id: u32, flags: u32) {
    if flags & 0x1 != 1 && flags & 0x2 != 0x2 {
        log_warn!(
            "LOCAL APIC with ID {:#x} is not enabled or online capable",
            apic_id
        );
        return;
    }

    // SAFETY: This should be OK since we're mapping a physical address that is marked as
    // reserved, so the kernel shouldn't be tracking it
    let virt_addr = unsafe { map_page(base, Entry::FLAG_RW) };

    let lapics = unsafe { LOCAL_APICS.get().as_mut().unwrap() };

    lapics.push(SpinLock::new(LocalApic::new(
        virt_addr.into(),
        acpi_processor_id,
        apic_id,
    )));
}

/// Marks the matching processor's LINT as NMI with the passed flags, making the other ExtInt
///
/// SAFTEY:
pub unsafe fn config_lints(acpi_processor_id: u32, lint: u8, flags: u16) {
    const ALL_PROCESSORS_ID: u32 = 0xff;

    let lapics = get_lapics();
    // If the acpi_processor_id is 0xff, we set all processors
    if acpi_processor_id == ALL_PROCESSORS_ID {
        for apic in lapics {
            let apic = apic.lock();
            unsafe {
                apic.setup_lvts(
                    lint,
                    (flags & 0b11).try_into().unwrap(),
                    ((flags >> 2) & 0b11).try_into().unwrap(),
                );
            }
        }
    } else {
        for apic in lapics {
            let apic = apic.lock();
            if apic.acpi_processor_id == acpi_processor_id {
                unsafe {
                    apic.setup_lvts(
                        lint,
                        (flags & 0b11).try_into().unwrap(),
                        ((flags >> 2) & 0b11).try_into().unwrap(),
                    );
                }
            }
        }
    }
}

/// Overrides the base address of the local APICs MMIO registers
pub unsafe fn override_base(base: *mut u32) {
    let lapics = get_lapics();

    lapics.iter().for_each(|apic| unsafe {
        let mut apic = apic.lock();
        apic.area.change_base(base);
    });
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

impl Default for LvtReg {
    fn default() -> Self {
        LvtReg::new()
    }
}

// XXX: This is safe only when the address for each local APIC is the same
unsafe impl Send for LocalApic {}
unsafe impl Sync for LocalApic {}

impl SpinLockDropable for LocalApic {}
