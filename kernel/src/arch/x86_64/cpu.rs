//! Assembly instruction wrappers & other low level CPU operations
//!
//! `NOTE:` 'core::arch::x86_64' already implements `__cpuid`, `rdtsc` and many others, so use them when needed

use core::arch::asm;

#[repr(u32)]
pub enum Msr {
    Ia32ApicBase = 0x1b,
}

/// Wrapper for the 'outb' instruction, accessing a `u32` port
#[inline]
pub unsafe fn outb_32(port: u16, value: u32) {
    unsafe {
        asm! (
            "out dx, eax",
            in("dx") port,
            in("eax") value,
            options(nomem, nostack),
        )
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
        )
    };
}

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
        )
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
        )
    };

    res
}

/// Clear `RFLAGS` interrupt flag to mask all maskable external interrupts
#[inline]
pub unsafe fn cli() {
    unsafe { asm!("cli", options(nostack, nomem)) };
}

/// Set `RFLAGS` interrupt flag to enable handling of external interrupts
#[inline]
pub unsafe fn sti() {
    unsafe { asm!("sti", options(nostack, nomem)) };
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
        )
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
        )
    };
}

// TODO: Maybe implement these as functions with enums for the fields you can write to make it
// safer?

/// Wrapper to read the value of a control register
#[macro_export]
macro_rules! read_cr {
    ($cr:ident) => {{
        #[allow(unused_unsafe)]
        unsafe {
            let value: usize;
            core::arch::asm!(
                concat!("mov {}, ", stringify!($cr)),
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
    ($cr:ident, $val:expr) => {{
        #[allow(unused_unsafe)]
        unsafe {
            core::arch::asm!(
                concat!("mov ", stringify!($cr), ", {}"),
                in(reg) $val,
                options(nostack, nomem)
            );
        }
    }};
}
