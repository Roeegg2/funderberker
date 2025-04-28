//! Multiprocessor (MP) support for the system

use core::{arch::{asm, global_asm, x86_64::__cpuid_count}, num::NonZero, time::Duration};

use utils::{mem, spin_until};

use crate::{
    arch::{
        x86_64::apic::{
            lapic::DeliveryStatus, DeliveryMode, Destination, DestinationShorthand, Level, TriggerMode
        }, BASIC_PAGE_SIZE, CORE_STACK_PAGE_COUNT
    }, dev::timer::{hpet::{HpetTimer, TimerMode}, Timer}, mem::{vmm::alloc_pages_any, PhysAddr, VirtAddr}, sync::spinlock::SpinLock
};

use super::apic::lapic;

#[unsafe(no_mangle)]
static mut NEXT_CORE_STACK_SPINLOCK: u8 = 0;

#[unsafe(no_mangle)]
static mut NEXT_CORE_STACK: VirtAddr = VirtAddr(0);

/// Initialize the APs on the system using Brendan's method found on the OSDev wiki.
///
/// SHOULD BE CALLED ONLY ONCE DURING BOOT!
pub(super) fn init_cores() {
    let bsp_id = unsafe { __cpuid_count(1, 0).ebx } >> 24;
    println!("BSP ID: {}", bsp_id);

    let (phys_addr, page_count) = setup_trampoline_and_stack();
    unsafe {
        NEXT_CORE_STACK = PhysAddr(phys_addr.0 + (page_count * BASIC_PAGE_SIZE)).add_hhdm_offset();
    }
    // XXX: You need to handle the freeing of the ID beforehand so you can actually use it
    let mut hpet_timer = HpetTimer::new().unwrap();
    // NOTE: The interrupt should be sent from the BSP, not from the AP?
    // TODO: Maybe just send an IPI to `all excluding self`? That could be easier
    
    for apic in unsafe {
        #[allow(static_mut_refs)]
        &mut lapic::LOCAL_APICS
    }
    .iter()
    {
        // The BSP is already initialized
        if apic.apic_id() == bsp_id {
            continue;
        }

        // Sanity clear the error status register
        apic.read_errors();

        let destination =
            Destination::new(apic.apic_id() as u8, false).expect("Possibly invalid APIC ID");
        // XXX: Not sure about the trigger mode
        unsafe {
            apic.send_ipi(
                0,
                DeliveryMode::Init,
                destination,
                Level::Assert,
                TriggerMode::EdgeTriggered,
                DestinationShorthand::NoShorthand,
            );
        }

        println!("here!");

        // We should do a deassert beforehand, but we shouldn't do this on Intel Xeon or pentium
        // processors
        spin_until!(apic.ipi_status() == DeliveryStatus::Idle);

        hpet_timer.configure(Duration::from_millis(10), TimerMode::OneShot)
            .unwrap();
        hpet_timer.set_disabled(false);

        spin_until!(hpet_timer.get_status() == true);

        // XXX: Not sure about the trigger mode
        // XXX: Not sure about this interrupt vector, I just copied whatever from the osdev wiki
        // XXX: Are you sure there is no danger of the page ID wrapping over?
        for _ in 0..2 {
            unsafe {
                apic.send_ipi(
                    (phys_addr.0 >> 12) as u8,
                    DeliveryMode::StartUp,
                    destination,
                    Level::Assert,
                    TriggerMode::EdgeTriggered,
                    DestinationShorthand::NoShorthand,
                );
            }

            hpet_timer.configure(Duration::from_micros(200), TimerMode::OneShot)
                .unwrap();
            hpet_timer.set_disabled(false);

            spin_until!(hpet_timer.get_status() == true);

            spin_until!(apic.ipi_status() == DeliveryStatus::Idle);
        }
    }
}

/// Setup the trampoline and stack memory for the APs
fn setup_trampoline_and_stack() -> (PhysAddr, usize) {
    // TODO XXX: Make this allocation executable
    // TODO XXX: We can't just rely on HHDM mapping here. Add a function to get physical address
    // for a given virtual address
    let apic_count = unsafe {
        #[allow(static_mut_refs)]
        &mut lapic::LOCAL_APICS
    }.len();

    // We allocate a 64KB stack for each AP + 1 page for the trampoline
    // TODO: In some cases it might be better to just allocate a 2MB stack?
    let page_count = (CORE_STACK_PAGE_COUNT.get() * apic_count) + 1;
    let initialization_vector_page = unsafe {
        alloc_pages_any(NonZero::new_unchecked(1),
            NonZero::new_unchecked(page_count)).unwrap()
    };

    // We copy over the trampoline code to the first page
    unsafe {
        mem::memcpy(
            initialization_vector_page.as_ptr().cast::<u8>(),
            ap_trampoline as *const u8,
            BASIC_PAGE_SIZE,
        )
    };

    (VirtAddr(initialization_vector_page.addr().into()).subtract_hhdm_offset(), page_count)
}

unsafe extern "C" {
    fn ap_trampoline();
}

global_asm! {
    r#"
    .code16
    .section .text
    .global ap_trampoline
ap_trampoline:
    hlt
    cli
    cld
    jmp _continue
    .align 16
_L8010_GDT_table:
    .long 0, 0
    .long 0x0000FFFF, 0x00CF9A00
    .long 0x0000FFFF, 0x008F9200
    .long 0x00000068, 0x00CF8900
_L8030_GDT_value:
    .word _L8030_GDT_value - _L8010_GDT_table - 1
    .long 0x8010
    .long 0, 0
    .align 64
_continue:
    mov ax, 16
    mov ds, ax
    mov ss, ax
    hlt
    "#
}


// 1:
//     pause
//     lock bts QWORD PTR NEXT_CORE_STACK_SPINLOCK, 0
//     jc 1b
//     mov rax, NEXT_CORE_STACK
//     mov rsp, rax
//     sub QWORD PTR NEXT_CORE_STACK, 0x64000
//     jmp ap_statup

#[unsafe(no_mangle)]
fn ap_statup() -> ! {
    let apic_id = unsafe { __cpuid_count(1, 0).ebx } >> 24;
    log_info!("Initialized core with APIC ID {}", apic_id);

    loop {
        unsafe {
            asm!("hlt");
        }
    }
}
