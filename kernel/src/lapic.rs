
use crate::mem::mmio::{RwReg, RoReg};

/// Interrrupt Command Register. Used to send inter-processor interrupts.
#[derive(Debug)]
struct Icr {
    /// The low part of the ICR
    low: RwReg<u32>,
    /// The high part of the ICR
    high: RwReg<u32>,
}

#[derive(Debug)]
struct ApicTimer {
    /// The initial count of the timer. When the timer reaches 0, it will generate an interrupt.
    initial_count: RwReg<u32>,
    /// The current count of the timer. It will be decremented by 1 every time the timer is enabled.
    current_count: RoReg<u32>,
    /// The divide configuration of the timer. It will be used to divide the input frequency of the
    /// timer.
    divide: RwReg<u32>,
}

// TODO: Maybe restucture this, since RwReg & RoReg both hold the address, and it takes up quite a
// bit of extra mem
#[derive(Debug)]
struct LocalApic {
    /// Holds the APIC ID assigned to this core.
    /// It could be changed by us if we wanted to, but it's bad practice
    lapic_id: RwReg<u32>,
    /// Holds the version and a bunch of other info about the Local APIC
    lapic_version: RoReg<u32>,
    task_priority: RwReg<u32>,
    arbitration_priority: RwReg<u32>,
    processor_priority: RwReg<u32>,
    /// "End Of Interrupt" register. It's only ever used to signal the end of an interrupt, by
    /// writing 0 to it. UB if you write anything else.
    eoi: RwReg<u32>,
    remote_read: RwReg<u32>,
    logical_destination: RwReg<u32>,
    destination_format: RwReg<u32>,
    spi_vector: [RwReg<u32>; 8],
    isr: [RoReg<u32>; 8],
    tmr: [RwReg<u32>; 8],
    irr: [RoReg<u32>; 8],
    /// Records any errors detected by the local APIC
    esr: RoReg<u32>,
    /// Icr of the local APIC
    icr: Icr,
    /// A bunch of registers that let us configure the way local interrupts are delivered & handled
    /// by this core
    lvt_timer: RwReg<u32>,
    lvt_thermal: RwReg<u32>,
    lvt_perf_monitor: RwReg<u32>,
    lvt_lint0: RwReg<u32>,
    lvt_lint1: RwReg<u32>,
    lvt_error: RwReg<u32>,
    timer: ApicTimer,
}

impl LocalApic {
    pub fn init() {

    }
}
