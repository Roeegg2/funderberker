//! Assembly instruction wrappers & other low level CPU operations
//!
//! `NOTE:` 'core::arch::x86_64' already implements __cpuid, so use that when needed

use core::arch::asm;

#[allow(unused_imports)]
pub use core::arch::x86_64;

/// Wrapper for the 'outb' instruction
#[cfg(feature = "serial")]
#[inline]
pub unsafe fn outb(port: u16, offset: u16, value: u8) {
    unsafe {
        asm! (
            "out dx, al",
            in("dx") port + offset,
            in("al") value,
            options(nomem, nostack),
        )
    };
}

/// Wrapper for the 'in' instruction
#[cfg(feature = "serial")]
#[inline]
pub unsafe fn inb(port: u16, offset: u16) -> u8 {
    let res: u8;
    unsafe {
        asm! (
            "in al, dx",
            out("al") res,
            in("dx") port + offset,
            options(nomem, nostack),
        )
    };

    res
}

/// Clear `RFLAGS` interrupt flag to mask all maskable external interrupts
#[inline]
pub(super) unsafe fn cli() {
    unsafe { asm!("cli", options(nostack, nomem)) };
}

/// Set `RFLAGS` interrupt flag to enable handling of external interrupts
#[inline]
pub(super) unsafe fn sti() {
    unsafe { asm!("sti", options(nostack, nomem)) };
}

/// Read the time stap counter.
///
/// `NOTES:`
/// 1. Some time is passed between the actually read value and the return of this function,
///    since the shifting and oring takes some time. This function is marked as `inline(always)` to
///    reduce this time as much as possible. (plus it's probably more efficient)
/// 2. This function is unsafe, soley because when the TSD flag in CR4 is set access to the TSC
///    results in a #GP exception.
#[inline(always)]
pub(super) unsafe fn rdtsc() -> u64 {
    // XXX: Need to make sure timer isn't locked on CR4 before calling this
    let val: u64;
    unsafe {
        asm!(
            "rdtsc",
            "shl rdx, 32",
            "or rax, rdx",
            out("rax") val,
            options(nostack, nomem),
        )
    };

    val
}

/// Read the value of a model specific register (MSR)
#[inline]
pub(super) unsafe fn rdmsr() -> (u32, u32) {
    let low: u32;
    let high: u32;
    unsafe {
        asm!(
            "rdmsr",
            out("eax") low,
            out("edx") high,
            options(nostack, nomem),
        )
    };

    (low, high)
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
