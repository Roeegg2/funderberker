//! Various drivers and driver interfaces

use crate::arch::x86_64::{
    apic::ioapic::{self, map_irq_to_vector},
    cpu::Register,
    gdt::Cs,
    interrupts::{self, Dpl, GateType, IsrStub, Present},
};

pub mod clock;
// #[cfg(feature = "legacy_timers")]
// pub mod cmos;
pub mod bus;
#[cfg(feature = "framebuffer")]
pub mod framebuffer;
#[cfg(feature = "serial")]
pub mod serial;
pub mod timer;

// TODO: Return an error instead of panicking here
/// A wrapper for easier installing of IRQ ISRs
unsafe fn register_irq(irq: u8, isr_stub: IsrStub) {
    unsafe {
        // Make sure the interrupt is masked off before we do any fiddiling with the
        // IO APIC and IDT
        ioapic::set_disabled(irq, true).unwrap();

        // Install the new ISR
        let vector = interrupts::install_isr(
            isr_stub,
            Cs::read().0,
            0,
            GateType::Interrupt,
            Dpl::Kernel,
            Present::Present,
        );

        // Tell the IO APIC to map `irq` to the given `vector`
        // XXX: Change the flags here!
        map_irq_to_vector(vector, irq).unwrap();

        // Now we can unmask the IRQ in the IO APIC
        //
        // NOTE: No interrupt should be triggered yet, since the timer is still
        // disabled internally.
        ioapic::set_disabled(irq, false).unwrap();
    };
}

// TODO: unregister_isr
