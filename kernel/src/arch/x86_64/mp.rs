//! Multiprocessor (MP) support for the system

use core::{arch::x86_64::__cpuid_count, hint, num::NonZero};

use crate::{
    arch::x86_64::apic::{
        DeliveryMode, Destination, DestinationShorthand, Level, TriggerMode, lapic::DeliveryStatus,
    },
    mem::{VirtAddr, vmm::alloc_pages_any},
};

use super::apic::lapic;

/// Initialize the APs on the system
/// NOTE: Based off the code from the OSDev wiki
pub(super) fn init_cores() {
    let bsp_id = unsafe { __cpuid_count(1, 0).ebx } >> 24;
    println!("BSP ID: {}", bsp_id);

    // TODO XXX: Make this allocation executable
    // TODO XXX: We can't just rely on HHDM mapping here. Add a function to get physical address
    // for a given virtual address
    let initialization_vector_page =
        alloc_pages_any(unsafe { NonZero::new_unchecked(1) }, unsafe {
            NonZero::new_unchecked(1)
        })
        .unwrap();
    let phys_addr = VirtAddr(initialization_vector_page.addr().into()).subtract_hhdm_offset();

    // NOTE: The interrupt should be sent from the BSP, not from the AP?
    // TODO: Maybe just send an IPI to `all excluding self`? That could be easier
    for apic in lapic::LOCAL_APICS.iter_mut() {
        // The BSP is already initialized
        if apic.apic_id() == bsp_id {
            continue;
        }

        // Sanity clear the error status register
        apic.read_errors();

        unsafe {
            apic.send_ipi(
                0,
                DeliveryMode::Init,
                Destination::new(apic.apic_id() as u8, false).expect("Possibly invalid APIC ID"),
                Level::Assert,
                TriggerMode::BusDefault,
                DestinationShorthand::NoShorthand,
            );
        }

        // We should do a deassert beforehand, but we shouldn't do this on Intel Xeon or pentium
        // processors
        while apic.ipi_status() == DeliveryStatus::SendPending {
            hint::spin_loop()
        }

        // TODO: Sleep for 10ms

        // XXX: Not sure about this interrupt vector, I just copied whatever from the osdev wiki
        unsafe {
            apic.send_ipi(
                0,
                delivery_mode,
                destination,
                level,
                trigger_mode,
                destination_shorthand,
            );
        }
    }
}

fn ap_statup_foo(apic_id: usize) -> ! {
    log_info!("Initialized core with APIC ID {}", apic_id);

    loop {}
}
