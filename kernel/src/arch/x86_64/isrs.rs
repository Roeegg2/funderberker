//! Interrupt Service Routines defintions

use core::arch::{global_asm, x86_64::__cpuid_count};

use crate::dev::cmos::{self, CmosIndex, NmiStatus};

use super::apic::lapic::LOCAL_APICS;

// TODO: Rewrite this macro
macro_rules! print_n_die {
    ($vec:ident, $vec_num:expr) => {
        #[unsafe(no_mangle)]
        fn $vec() {
            panic!("Exception: {}", EXCEPTION_MESSAGES[$vec_num as usize]);
        }
    };
}

print_n_die!(handler_vec_0, 0);
print_n_die!(handler_vec_1, 1);
print_n_die!(handler_vec_2, 2);
print_n_die!(handler_vec_3, 3);
print_n_die!(handler_vec_4, 4);
print_n_die!(handler_vec_5, 5);
print_n_die!(handler_vec_6, 6);
print_n_die!(handler_vec_7, 7);
print_n_die!(handler_vec_8, 8);
print_n_die!(handler_vec_9, 9);
print_n_die!(handler_vec_10, 10);
print_n_die!(handler_vec_11, 11);
print_n_die!(handler_vec_12, 12);
print_n_die!(handler_vec_13, 13);
print_n_die!(handler_vec_14, 14);
print_n_die!(handler_vec_15, 15);
print_n_die!(handler_vec_16, 16);
print_n_die!(handler_vec_17, 17);
print_n_die!(handler_vec_18, 18);
print_n_die!(handler_vec_19, 19);
print_n_die!(handler_vec_20, 20);
print_n_die!(handler_vec_21, 21);
print_n_die!(handler_vec_22, 22);
print_n_die!(handler_vec_23, 23);
print_n_die!(handler_vec_24, 24);
print_n_die!(handler_vec_25, 25);
print_n_die!(handler_vec_26, 26);
print_n_die!(handler_vec_27, 27);
print_n_die!(handler_vec_28, 28);
print_n_die!(handler_vec_29, 29);
print_n_die!(handler_vec_30, 30);
print_n_die!(handler_vec_31, 31);

/// List of error messages for each exception
static EXCEPTION_MESSAGES: &[&str] = &[
    "Divide-by-zero Error",
    "Debug",
    "Non-maskable Interrupt",
    "Breakpoint",
    "Overflow",
    "Bound Range Exceeded",
    "Invalid Opcode",
    "Device Not Available",
    "Double Fault",
    "Coprocessor Segment Overrun",
    "Invalid TSS",
    "Segment Not Present",
    "Stack-Segment Fault",
    "General Protection Fault",
    "Page Fault",
    "Unknown",
    "x87 Floating-Point Exception",
    "Alignment Check",
    "Machine Check",
    "SIMD Floating-Point Exception",
    "Virtualization Exception",
    "Unknown",
    "Unknown",
    "Unknown",
    "Unknown",
    "Unknown",
    "Unknown",
    "Unknown",
    "Unknown",
    "Unknown",
    "Security Exception",
    "Unknown",
];

#[unsafe(no_mangle)]
fn signal_eoi() {
    // TODO: Rewrite this whole thing
    let this_apic_id = unsafe { (__cpuid_count(1, 0).ebx >> 24) & 0xff } as u32;
    unsafe {
        #[allow(static_mut_refs)]
        let lapic = LOCAL_APICS
            .iter()
            .find(|&lapic| lapic.apic_id() == this_apic_id)
            .unwrap();
        lapic.signal_eoi();
    };
}

global_asm! {
    r#"
    .section .text
    .macro define_exception_stub vec
    .global stub_vec_\vec
    stub_vec_\vec:
        call handler_vec_\vec
        iretq
    .endm

    .macro define_irq_stub vec
    .global stub_vec_\vec
    stub_vec_\vec:
        call handler_vec_\vec
        call signal_eoi
        iretq
    .endm

    define_exception_stub 0
    define_exception_stub 1
    define_exception_stub 2
    define_exception_stub 3
    define_exception_stub 4
    define_exception_stub 5
    define_exception_stub 6
    define_exception_stub 7
    define_exception_stub 8
    define_exception_stub 9
    define_exception_stub 10
    define_exception_stub 11
    define_exception_stub 12
    define_exception_stub 13
    define_exception_stub 14
    define_exception_stub 15
    define_exception_stub 16
    define_exception_stub 17
    define_exception_stub 18
    define_exception_stub 19
    define_exception_stub 20
    define_exception_stub 21
    define_exception_stub 22
    define_exception_stub 23
    define_exception_stub 24
    define_exception_stub 25
    define_exception_stub 26
    define_exception_stub 27
    define_exception_stub 28
    define_exception_stub 29
    define_exception_stub 30
    define_exception_stub 31

    define_irq_stub 32
    define_irq_stub 33
    define_irq_stub 34
    define_irq_stub 254
    "#
}

#[unsafe(no_mangle)]
fn handler_vec_32() {
    println!("LOCAL APIT TIMER INTERRUPT!!!!");
}

#[unsafe(no_mangle)]
fn handler_vec_33() {
    println!("PIT/HPET TIMER INTERRUPT!!!!");
}

#[unsafe(no_mangle)]
fn handler_vec_34() {
    cmos::read_cmos(CmosIndex::StatusC, NmiStatus::Enabled);

    println!("RTC TIMER INTERRUPT!!!!");
}

#[unsafe(no_mangle)]
fn handler_vec_254() {
    println!("UNHANDELED INTERRUPT!!!!");
}
