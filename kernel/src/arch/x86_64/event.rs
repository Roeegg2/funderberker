//! Various x86_64 specific events handling

use macros::isr;

use crate::arch::x86_64::apic::lapic::LocalApic;

pub const GENERIC_ISR_VECTOR: u8 = 255;

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

// static EXCEPTION_MESSAGES: &[&str] = &[
//     "Divide-by-zero Error",
//     "Debug",
//     "Non-maskable Interrupt",
//     "Breakpoint",
//     "Overflow",
//     "Bound Range Exceeded",
//     "Invalid Opcode",
//     "Device Not Available",
//     "Double Fault",
//     "Coprocessor Segment Overrun",
//     "Invalid TSS",
//     "Segment Not Present",
//     "Stack-Segment Fault",
//     "General Protection Fault",
//     "Page Fault",
//     "Unknown",
//     "x87 Floating-Point Exception",
//     "Alignment Check",
//     "Machine Check",
//     "SIMD Floating-Point Exception",
//     "Virtualization Exception",
//     "Unknown",
//     "Unknown",
//     "Unknown",
//     "Unknown",
//     "Unknown",
//     "Unknown",
//     "Unknown",
//     "Unknown",
//     "Unknown",
//     "Security Exception",
//     "Unknown",
// ];

/// Utility macro to define an exception ISR that just prints the error to the screen.
macro_rules! generic_exception_isr {
    ($isr_name:ident, $vec:expr) => {
        #[isr]
        fn $isr_name() {
            panic!("Exception: {}", EXCEPTION_MESSAGES[$vec as usize]);
        }
    };
}

generic_exception_isr!(exception_0, 0);
generic_exception_isr!(exception_1, 1);
generic_exception_isr!(exception_2, 2);
generic_exception_isr!(exception_3, 3);
generic_exception_isr!(exception_4, 4);
generic_exception_isr!(exception_5, 5);
generic_exception_isr!(exception_6, 6);
generic_exception_isr!(exception_7, 7);
generic_exception_isr!(exception_8, 8);
generic_exception_isr!(exception_9, 9);
generic_exception_isr!(exception_10, 10);
generic_exception_isr!(exception_11, 11);
generic_exception_isr!(exception_12, 12);
generic_exception_isr!(exception_13, 13);
generic_exception_isr!(exception_14, 14);
generic_exception_isr!(exception_15, 15);
generic_exception_isr!(exception_16, 16);
generic_exception_isr!(exception_17, 17);
generic_exception_isr!(exception_18, 18);
generic_exception_isr!(exception_19, 19);
generic_exception_isr!(exception_20, 20);
generic_exception_isr!(exception_21, 21);
generic_exception_isr!(exception_22, 22);
generic_exception_isr!(exception_23, 23);
generic_exception_isr!(exception_24, 24);
generic_exception_isr!(exception_25, 25);
generic_exception_isr!(exception_26, 26);
generic_exception_isr!(exception_27, 27);
generic_exception_isr!(exception_28, 28);
generic_exception_isr!(exception_29, 29);
generic_exception_isr!(exception_30, 30);
generic_exception_isr!(exception_31, 31);

#[isr]

pub fn generic_irq_isr() {
    println!("GENERIC IRQ ISR CALLED!");
    // TODO: Possibly rewrite this
    let this_lapic_id = LocalApic::get_this_apic_id();
    let lapic = LocalApic::get_apic(this_lapic_id);
    lapic.signal_eoi();
}
