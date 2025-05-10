//! Assembly instruction wrappers & other low level CPU operations
//!
//! `NOTE:` `core::arch::x86_64` already implements `__cpuid`, `rdtsc` and many others, so use them when needed

use core::arch::asm;

#[repr(u32)]
pub enum Msr {
    /// Address of the `IA32_APIC_BASE` MSR
    Ia32ApicBase = 0x1B,
    /// Address of the `IA32_FEATURE_CONTROL` MSR
    Ia32FeatureControl = 0x3A,
    /// FOR AMD CPUs!
    ///
    /// Extended feature enable
    Efer = 0xC000_0080,
    /// Control and status bits for HAV
    VmCr = 0xC001_0114,
}

#[allow(unused)]
/// Wrapper for the 'outb' instruction, accessing a `u32` port
#[inline]
pub unsafe fn outb_32(port: u16, value: u32) {
    unsafe {
        asm! (
            "out dx, eax",
            in("dx") port,
            in("eax") value,
            options(nomem, nostack),
        );
    };
}

/// Wrapper for the 'out' instruction, accessing a `u8` port
#[inline]
pub unsafe fn outb_8(port: u16, value: u8) {
    unsafe {
        asm! (
            "out dx, al",
            in("dx") port,
            in("al") value,
            options(nomem, nostack),
        );
    };
}

#[allow(unused)]
/// Wrapper for the 'in' instruction, accessing a `u32` port
#[inline]
pub unsafe fn inb_32(port: u16) -> u32 {
    let res: u32;
    unsafe {
        asm! (
            "in eax, dx",
            out("eax") res,
            in("dx") port,
            options(nomem, nostack),
        );
    };

    res
}

/// Wrapper for the 'in' instruction, accessing a `u8` port
#[inline]
pub unsafe fn inb_8(port: u16) -> u8 {
    let res: u8;
    unsafe {
        asm! (
            "in al, dx",
            out("al") res,
            in("dx") port,
            options(nomem, nostack),
        );
    };

    res
}

/// Clear `RFLAGS` interrupt flag to mask all maskable external interrupts
#[inline]
pub fn cli() {
    unsafe {
        asm!("cli", options(nostack, nomem));
    };
}

/// Set `RFLAGS` interrupt flag to enable handling of external interrupts
#[inline]
pub fn sti() {
    unsafe {
        asm!("sti", options(nostack, nomem));
    };
}

/// Read the value of a model specific register (MSR)
#[inline]
pub unsafe fn rdmsr(msr: Msr) -> (u32, u32) {
    let low: u32;
    let high: u32;
    unsafe {
        asm!(
            "rdmsr",
            out("eax") low,
            out("edx") high,
            in("ecx") msr as u32,
            options(nostack, nomem),
        );
    };
    (low, high)
}

/// Write a value to a model specific register (MSR)
#[inline]
pub unsafe fn wrmsr(msr: Msr, low: u32, high: u32) {
    unsafe {
        asm!(
            "wrmsr",
            in("eax") low,
            in("edx") high,
            in("ecx") msr as u32,
            options(nostack, nomem),
        );
    };
}

/// Read the value of the current CS register
#[inline]
pub fn get_cs() -> u16 {
    let cs: u16;
    unsafe {
        asm! (
            "mov {:x}, cs",
            out(reg) cs,
        );
    };
    cs
}

// TODO: Maybe implement these as functions with enums for the fields you can write to make it
// safer?

/// Macro to check whether a CR register number is valid during compile time
#[macro_export]
macro_rules! validate_cr {
    ($cr:literal) => {
        const _: () = assert!(
            matches!($cr, 2 | 3 | 4 | 8),
            "Invalid control register number. Must be 2, 3, 4, or 8.",
        );
    };
}

/// Wrapper to read the value of a control register
#[macro_export]
macro_rules! read_cr {
    ($cr:literal) => {{
        $crate::validate_cr!($cr);
        unsafe {
            let value: usize;
            core::arch::asm!(
                concat!("mov {}, cr", $cr),
                out(reg) value,
                options(nostack, nomem)
            );
            value
        }
    }};
}

/// Wrapper to write a value to a control register
#[macro_export]
macro_rules! write_cr {
    ($cr:literal, $val:expr) => {
        $crate::validate_cr!($cr);

        #[allow(clippy::macro_metavars_in_unsafe)]
        unsafe {
            core::arch::asm!(
                concat!("mov cr", $cr, ", {}"),
                in(reg) $val,
                options(nostack, nomem)
            );
        }
    };
}
